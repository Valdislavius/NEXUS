#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{io::{BufRead, BufReader, Write}, process::{Child, ChildStdin, Command, Stdio}, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
use tauri::{Emitter, Manager, State};
use serde_json::{json, Value};
mod diagnostics;
mod instance;
mod elevation;

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
impl Drop for Collector { fn drop(&mut self) { self.stop(); } }

fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let folder = exe.parent().ok_or("Application directory is unavailable")?;
    let binary = folder.join(if cfg!(windows) { "nexus-collector.exe" } else { "nexus-collector" });
    let mut command = Command::new(binary);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|e| format!("Не удалось запустить сборщик: {e}"))?;
    let output = child.stdout.take().ok_or("Collector stdout is unavailable")?;
    let state = app.state::<Collector>();
    *state.input.lock().unwrap() = child.stdin.take();
    *state.child.lock().unwrap() = Some(child);
    let generation = state.generation.load(Ordering::SeqCst);
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            if handle.state::<Collector>().generation.load(Ordering::SeqCst) != generation { return; }
            // Bound a message even if a future collector contains a framing bug.
            let mut line = Vec::new();
            let result = std::io::Read::take(&mut reader, 2 * 1024 * 1024).read_until(b'\n', &mut line);
            match result {
                Ok(0) | Err(_) => break,
                Ok(_) if line.last() != Some(&b'\n') => break,
                Ok(_) => {
                    let state = handle.state::<Collector>();
                    let _lifecycle = state.lifecycle.lock().unwrap();
                    if handle.state::<Collector>().generation.load(Ordering::SeqCst) != generation { return; }
                    if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                        if value["version"] == 1 && value["type"] == "snapshot" && value["components"].is_array() {
                            state.diagnostics.lock().unwrap().record(&value);
                            *state.received_at.lock().unwrap() = Some(std::time::Instant::now());
                            *handle.state::<Collector>().latest.lock().unwrap() = Some(value.clone());
                            let _ = handle.emit("nexus:snapshot", value);
                        } else { break; }
                    } else { break; }
                }
            }
        }
        let state = handle.state::<Collector>();
        let _lifecycle = state.lifecycle.lock().unwrap();
        if handle.state::<Collector>().generation.load(Ordering::SeqCst) != generation { return; }
        state.stop();
        let message = "Сборщик остановлен. Нажмите «Повторить», чтобы возобновить мониторинг.".to_string();
        *handle.state::<Collector>().error.lock().unwrap() = Some(message.clone());
        let _ = handle.emit("nexus:error", message);
    });
    Ok(())
}
#[tauri::command]
fn current_snapshot(state: State<Collector>) -> Value {
    json!({"snapshot": *state.latest.lock().unwrap(), "error": *state.error.lock().unwrap()})
}
#[tauri::command]
fn export_diagnostics(state: State<Collector>) -> Value {
    let running = state.child.lock().unwrap().as_mut().is_some_and(|child| matches!(child.try_wait(), Ok(None)));
    let age_limit = std::time::Duration::from_millis(15000.max(state.interval_ms.load(Ordering::Relaxed) * 3));
    let fresh = state.error.lock().unwrap().is_none()
        && state.received_at.lock().unwrap().is_some_and(|at| at.elapsed() <= age_limit);
    let latest = state.latest.lock().unwrap();
    state.diagnostics.lock().unwrap().report(latest.as_ref(), running, fresh)
}
#[tauri::command]
fn set_interval(milliseconds: u64, state: State<Collector>) -> Result<(), String> {
    if ![2000, 5000, 10000].contains(&milliseconds) { return Err("Unsupported interval".into()); }
    let mut input = state.input.lock().unwrap();
    let pipe = input.as_mut().ok_or("Сборщик не запущен")?;
    writeln!(pipe, "{}", json!({"command":"interval","intervalMs":milliseconds})).map_err(|e| e.to_string())?;
    state.interval_ms.store(milliseconds, Ordering::Relaxed);
    Ok(())
}
#[tauri::command]
async fn restart_collector(app: tauri::AppHandle, milliseconds: u64) -> Result<(), String> {
    if ![2000,5000,10000].contains(&milliseconds) { return Err("Unsupported interval".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Collector>();
        let _lifecycle = state.lifecycle.lock().unwrap();
        state.stop();
        *state.latest.lock().unwrap() = None;
        *state.error.lock().unwrap() = None;
        start(&app)?;
        set_interval(milliseconds, app.state::<Collector>())
    }).await.map_err(|e| e.to_string())?
}
fn main() {
    if !elevation::ensure_elevated() { return; }
    let Some(_instance) = instance::Instance::acquire() else { return; };
    let app = tauri::Builder::default()
        .manage(Collector::default())
        .invoke_handler(tauri::generate_handler![current_snapshot, set_interval, restart_collector, export_diagnostics])
        .setup(|app| {
            if let Err(e) = start(app.handle()) { *app.state::<Collector>().error.lock().unwrap() = Some(e); }
            Ok(())
        })
        .build(tauri::generate_context!()).expect("Unable to initialize NEXUS Desktop");
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event { let state=handle.state::<Collector>();let _lifecycle=state.lifecycle.lock().unwrap();state.stop(); }
    });
}
