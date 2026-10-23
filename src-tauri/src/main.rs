#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(panic = "abort")]
compile_error!(
    "NEXUS requires panic=unwind so benchmark worker failures cannot abort the desktop process"
);
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager, State};
mod benchmark;
mod desktop;
mod diagnostics;
mod diagnostics_engine;
#[cfg(test)]
#[allow(dead_code)]
mod elevation;
mod instance;
mod report;
mod settings;
mod storage;
mod windows_checks;

struct Services {
    history: Mutex<Option<storage::History>>,
    history_path: std::path::PathBuf,
    storage_error: Mutex<Option<String>>,
    engine: Mutex<diagnostics_engine::Engine>,
}

#[derive(Default)]
struct Collector {
    generation: AtomicU64,
    lifecycle: Mutex<()>,
    child: Mutex<Option<Child>>,
    input: Mutex<Option<ChildStdin>>,
    latest: Mutex<Option<Value>>,
    error: Mutex<Option<String>>,
    diagnostics: Mutex<diagnostics::DiagnosticLog>,
    received_at: Mutex<Option<std::time::Instant>>,
    interval_ms: AtomicU64,
}
impl Collector {
    fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        // Closing stdin is also an EOF signal if the shell crashes or is terminated.
        self.input.lock().unwrap().take();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.stop();
    }
}

fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let folder = exe.parent().ok_or("Application directory is unavailable")?;
    let binary = folder.join(if cfg!(windows) {
        "nexus-collector.exe"
    } else {
        "nexus-collector"
    });
    let mut command = Command::new(binary);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Не удалось запустить сборщик: {e}"))?;
    let output = child
        .stdout
        .take()
        .ok_or("Collector stdout is unavailable")?;
    let state = app.state::<Collector>();
    *state.input.lock().unwrap() = child.stdin.take();
    *state.child.lock().unwrap() = Some(child);
    let generation = state.generation.load(Ordering::SeqCst);
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            if handle
                .state::<Collector>()
                .generation
                .load(Ordering::SeqCst)
                != generation
            {
                return;
            }
            // Bound a message even if a future collector contains a framing bug.
            let mut line = Vec::new();
            let result =
                std::io::Read::take(&mut reader, 2 * 1024 * 1024).read_until(b'\n', &mut line);
            match result {
                Ok(0) | Err(_) => break,
                Ok(_) if line.last() != Some(&b'\n') => break,
                Ok(_) => {
                    let state = handle.state::<Collector>();
                    let _lifecycle = state.lifecycle.lock().unwrap();
                    if handle
                        .state::<Collector>()
                        .generation
                        .load(Ordering::SeqCst)
                        != generation
                    {
                        return;
                    }
                    if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                        if value["version"] == 1
                            && value["type"] == "snapshot"
                            && value["components"].is_array()
                        {
                            state.diagnostics.lock().unwrap().record(&value);
                            if let Some(services) = handle.try_state::<Services>() {
                                let settings = handle
                                    .state::<settings::Preferences>()
                                    .value
                                    .lock()
                                    .unwrap()
                                    .clone();
                                let events = services
                                    .engine
                                    .lock()
                                    .unwrap()
                                    .evaluate(&value, &settings.thresholds);
                                if let Some(history) = services.history.lock().unwrap().as_mut() {
                                    let result = history
                                        .record(&value, settings.retention_days)
                                        .and_then(|_| {
                                            for event in events {
                                                history.event(&event)?;
                                            }
                                            Ok(())
                                        });
                                    if let Err(error) = result {
                                        *services.storage_error.lock().unwrap() = Some(error);
                                    }
                                }
                            }
                            *state.received_at.lock().unwrap() = Some(std::time::Instant::now());
                            *handle.state::<Collector>().latest.lock().unwrap() =
                                Some(value.clone());
                            desktop::on_snapshot(&value);
                            // There is no WebView to wake while running in the tray.
                            if handle.get_webview_window("main").is_some() {
                                let _ = handle.emit("nexus:snapshot", value);
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        let state = handle.state::<Collector>();
        let _lifecycle = state.lifecycle.lock().unwrap();
        if handle
            .state::<Collector>()
            .generation
            .load(Ordering::SeqCst)
            != generation
        {
            return;
        }
        state.stop();
        let message =
            "Сборщик остановлен. Нажмите «Повторить», чтобы возобновить мониторинг.".to_string();
        *handle.state::<Collector>().error.lock().unwrap() = Some(message.clone());
        let _ = handle.emit("nexus:error", message);
    });
    Ok(())
}
#[tauri::command]
fn current_snapshot(state: State<Collector>) -> Value {
    json!({"snapshot": *state.latest.lock().unwrap(), "error": *state.error.lock().unwrap(), "intervalMs":state.interval_ms.load(Ordering::Relaxed)})
}
fn snapshot_is_fresh(state: &Collector) -> bool {
    state.error.lock().unwrap().is_none()
        && state.received_at.lock().unwrap().is_some_and(|at| {
            at.elapsed()
                <= std::time::Duration::from_millis(
                    15000.max(state.interval_ms.load(Ordering::Relaxed) * 3),
                )
        })
}
#[tauri::command]
async fn history_query(
    app: tauri::AppHandle,
    query: storage::HistoryQuery,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Services>()
            .history
            .lock()
            .unwrap()
            .as_mut()
            .ok_or("History storage unavailable")?
            .history(&query)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn timeline_query(
    app: tauri::AppHandle,
    query: storage::TimelineQuery,
) -> Result<Vec<storage::Event>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Services>()
            .history
            .lock()
            .unwrap()
            .as_ref()
            .ok_or("History storage unavailable")?
            .timeline(&query)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn diagnostics_current(state: State<Services>, collector: State<Collector>) -> Value {
    let mut value = state
        .engine
        .lock()
        .unwrap()
        .current(snapshot_is_fresh(&collector));
    value["storageError"] = json!(*state.storage_error.lock().unwrap());
    value["historyDroppedSamples"] = json!(state
        .history
        .lock()
        .unwrap()
        .as_ref()
        .map_or(0, |h| h.dropped_samples));
    value
}
#[tauri::command]
fn benchmark_start(
    app: tauri::AppHandle,
    request: benchmark::Request,
) -> Result<benchmark::ResultRecord, String> {
    app.state::<benchmark::Engine>().start(app.clone(), request)
}
#[tauri::command]
fn benchmark_status(state: State<benchmark::Engine>) -> Option<benchmark::ResultRecord> {
    state.status()
}
#[tauri::command]
fn benchmark_cancel(state: State<benchmark::Engine>) -> bool {
    state.cancel()
}
#[tauri::command]
fn benchmark_history(state: State<Services>) -> Result<Vec<Value>, String> {
    state
        .history
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("History storage unavailable")?
        .benchmarks()
}
#[tauri::command]
async fn report_generate(app: tauri::AppHandle, request: report::Request) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Services>();
        {
            state
                .history
                .lock()
                .unwrap()
                .as_mut()
                .ok_or("History storage unavailable")?
                .flush()?;
        }
        let folder = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("reports");
        report::generate(&state.history_path, &folder, request)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn desktop_settings(state: State<settings::Preferences>) -> settings::Settings {
    state.value.lock().unwrap().clone()
}
#[tauri::command]
async fn desktop_settings_save(
    app: tauri::AppHandle,
    settings: settings::Settings,
) -> Result<settings::Settings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        settings.validate()?;
        let prefs = app.state::<settings::Preferences>();
        let mut value = prefs.value.lock().unwrap();
        let previous = value.clone();
        desktop::apply(&settings)?;
        if settings.autostart != previous.autostart {
            if let Err(e) = settings::set_autostart(settings.autostart) {
                let _ = desktop::apply(&previous);
                return Err(e);
            }
        }
        if let Err(e) = prefs.persist(&settings) {
            let _ = desktop::apply(&previous);
            if settings.autostart != previous.autostart {
                let _ = settings::set_autostart(previous.autostart);
            }
            return Err(e);
        }
        *value = settings.clone();
        drop(value);
        let _ = send_profile(&app, &settings);
        Ok(settings)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn overlay_toggle() -> Result<bool, String> {
    desktop::overlay_toggle()
}
#[tauri::command]
fn desktop_show(app: tauri::AppHandle, page: Option<String>) -> Result<(), String> {
    desktop::show(&app, page)
}
fn send_profile(app: &tauri::AppHandle, settings: &settings::Settings) -> Result<(), String> {
    let collector = app.state::<Collector>();
    let mut input = collector.input.lock().unwrap();
    let pipe = input.as_mut().ok_or("Collector not running")?;
    writeln!(
        pipe,
        "{}",
        json!({"command":"profile","profile":settings.profile})
    )
    .map_err(|e| e.to_string())?;
    collector
        .interval_ms
        .store(settings.interval_ms(), Ordering::Relaxed);
    let _ = app.emit("nexus:interval", settings.interval_ms());
    Ok(())
}
#[tauri::command]
fn export_diagnostics(state: State<Collector>) -> Value {
    let running = state
        .child
        .lock()
        .unwrap()
        .as_mut()
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
    let age_limit =
        std::time::Duration::from_millis(15000.max(state.interval_ms.load(Ordering::Relaxed) * 3));
    let fresh = state.error.lock().unwrap().is_none()
        && state
            .received_at
            .lock()
            .unwrap()
            .is_some_and(|at| at.elapsed() <= age_limit);
    let latest = state.latest.lock().unwrap();
    state
        .diagnostics
        .lock()
        .unwrap()
        .report(latest.as_ref(), running, fresh)
}
#[tauri::command]
fn set_interval(
    app: tauri::AppHandle,
    milliseconds: u64,
    state: State<Collector>,
) -> Result<(), String> {
    if ![2000, 5000, 10000].contains(&milliseconds) {
        return Err("Unsupported interval".into());
    }
    let mut input = state.input.lock().unwrap();
    let pipe = input.as_mut().ok_or("Сборщик не запущен")?;
    writeln!(
        pipe,
        "{}",
        json!({"command":"interval","intervalMs":milliseconds})
    )
    .map_err(|e| e.to_string())?;
    state.interval_ms.store(milliseconds, Ordering::Relaxed);
    let _ = app.emit("nexus:interval", milliseconds);
    Ok(())
}
#[tauri::command]
async fn restart_collector(app: tauri::AppHandle, milliseconds: u64) -> Result<(), String> {
    if ![2000, 5000, 10000].contains(&milliseconds) {
        return Err("Unsupported interval".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Collector>();
        let _lifecycle = state.lifecycle.lock().unwrap();
        state.stop();
        *state.latest.lock().unwrap() = None;
        *state.error.lock().unwrap() = None;
        start(&app)?;
        let settings = app
            .state::<settings::Preferences>()
            .value
            .lock()
            .unwrap()
            .clone();
        send_profile(&app, &settings)?;
        set_interval(app.clone(), milliseconds, app.state::<Collector>())
    })
    .await
    .map_err(|e| e.to_string())?
}
fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--self-check") {
        let folder = args
            .next()
            .map(std::path::PathBuf::from)
            .expect("--self-check requires an output directory");
        match self_check(&folder) {
            Ok(value) => println!("{value}"),
            Err(error) => {
                eprintln!("Self-check failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let Some(_instance) = instance::Instance::acquire() else {
        return;
    };
    let app = tauri::Builder::default()
        .manage(Collector::default())
        .manage(benchmark::Engine::default())
        .invoke_handler(tauri::generate_handler![
            current_snapshot,
            set_interval,
            restart_collector,
            export_diagnostics,
            history_query,
            timeline_query,
            diagnostics_current,
            benchmark_start,
            benchmark_status,
            benchmark_cancel,
            benchmark_history,
            report_generate,
            desktop_settings,
            desktop_settings_save,
            overlay_toggle,
            desktop_show
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let prefs = window.app_handle().state::<settings::Preferences>();
                    let settings = prefs.value.lock().unwrap().clone();
                    if settings.background_mode && settings.close_behavior == "tray" {
                        api.prevent_close();
                        let _ = window.destroy();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .setup(|app| {
            let folder = app.path().app_data_dir()?;
            std::fs::create_dir_all(&folder)?;
            let prefs = settings::Preferences::load(folder.join("desktop-settings.json"));
            let mut settings = prefs.value.lock().unwrap().clone();
            app.manage(prefs);
            let history_path = folder.join("nexus-history.sqlite");
            let (history, error) = match storage::History::open(&history_path) {
                Ok(h) => {
                    let recovery = h.recovery.clone();
                    (Some(h), recovery)
                }
                Err(e) => (None, Some(e)),
            };
            app.manage(Services {
                history: Mutex::new(history),
                history_path,
                storage_error: Mutex::new(error),
                engine: Mutex::new(diagnostics_engine::Engine::default()),
            });
            let tray = desktop::setup(app.handle());
            let tray_ready = tray.is_ok();
            if let Err(error) = tray {
                *app.state::<Services>().storage_error.lock().unwrap() =
                    Some(format!("Native desktop: {error}"));
                // A failed tray must never leave an unreachable background
                // process. Keep the user's persisted choice for a later retry.
                settings.background_mode = false;
                settings.close_behavior = "exit".into();
                *app.state::<settings::Preferences>().value.lock().unwrap() = settings.clone();
            }
            if let Err(error) = desktop::apply(&settings) {
                *app.state::<Services>().storage_error.lock().unwrap() = Some(error);
            }
            if let Err(e) = start(app.handle()) {
                *app.state::<Collector>().error.lock().unwrap() = Some(e);
            }
            let _ = send_profile(app.handle(), &settings);
            if !std::env::args().any(|arg| arg == "--background") || !tray_ready {
                desktop::show(app.handle(), None).map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Unable to initialize NEXUS Desktop");
    app.run(|handle, event| match event {
        tauri::RunEvent::ExitRequested { api, code, .. } => {
            if code.is_none()
                && handle
                    .state::<settings::Preferences>()
                    .value
                    .lock()
                    .unwrap()
                    .background_mode
            {
                api.prevent_exit();
            }
        }
        tauri::RunEvent::Exit => {
            handle.state::<benchmark::Engine>().cancel();
            desktop::shutdown();
            let state = handle.state::<Collector>();
            let _lifecycle = state.lifecycle.lock().unwrap();
            state.stop();
            if let Some(history) = handle.state::<Services>().history.lock().unwrap().as_mut() {
                let _ = history.flush();
            }
        }
        _ => {}
    });
}

fn self_check(folder: &std::path::Path) -> Result<Value, String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let path = folder.join("self-check-fixture.sqlite");
    let mut history = storage::History::open(&path)?;
    let now = storage::now_ms();
    let mut engine = diagnostics_engine::Engine::default();
    for n in 0..120 {
        let at = chrono::DateTime::from_timestamp_millis(now - 120_000 + n * 1000)
            .unwrap()
            .to_rfc3339();
        let snapshot = json!({"version":1,"type":"snapshot","ready":true,"at":at,"host":"PRIVATE_HOST","components":[{"id":"PRIVATE_CPU_SERIAL","kind":"cpu","name":"Mock CPU","metrics":[{"id":"load","at":at,"value":n as f64/2.,"unit":"%","source":"PRIVATE_SOURCE"},{"id":"temperature","at":at,"value":if n>115{95.}else{45.},"unit":"°C","source":"Fixture"}]},{"id":"PRIVATE_RAM","kind":"ram","metrics":[{"id":"load","at":at,"value":null,"unit":"%","source":"Fixture"}]}]});
        history.record(&snapshot, 7)?;
        for event in engine.evaluate(&snapshot, &settings::Thresholds::default()) {
            history.event(&event)?;
        }
    }
    history.flush()?;
    let finished = storage::now_ms() + 1;
    let series = history.history(&storage::HistoryQuery {
        component_id: None,
        metric_ids: None,
        from_ms: now - 130000,
        to_ms: finished,
        max_points: Some(60),
    })?;
    let mut outputs = Vec::new();
    for format in ["csv", "xlsx", "pdf"] {
        outputs.push(report::generate(
            &path,
            folder,
            report::Request {
                format: format.into(),
                from_ms: Some(now - 130000),
                to_ms: Some(finished),
                components: None,
                anonymized: Some(true),
            },
        )?);
    }
    Ok(
        json!({"kind":"synthetic-integration-check","hardwareTest":false,"series":series.as_array().map_or(0,Vec::len),"maxPoints":series.as_array().unwrap().iter().map(|s|s["points"].as_array().map_or(0,Vec::len)).max(),"diagnostics":engine.current(true),"reports":outputs}),
    )
}
