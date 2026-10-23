//! Native tray/overlay/hotkeys. Closing the main window destroys its WebView.
use crate::settings::Settings;
use tauri::{Emitter, Manager};
pub fn show(app: &tauri::AppHandle, page: Option<String>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        if let Some(page) = page {
            let _ = window.emit("nexus:navigate", page);
        }
        return Ok(());
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or("Main window configuration missing")?;
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)
        .map_err(|e| e.to_string())?
        .visible(true);
    if let Some(page) = page {
        let serialized = serde_json::to_string(&page).map_err(|e| e.to_string())?;
        builder = builder.initialization_script(format!(
            "sessionStorage.setItem('nexus:initial-page',{serialized});location.hash={serialized};"
        ));
    }
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}
pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    };
    let open = MenuItem::with_id(app, "open", "Открыть NEXUS", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let overlay = MenuItem::with_id(
        app,
        "overlay",
        "Показать / скрыть оверлей",
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let diagnostic = MenuItem::with_id(app, "diagnostics", "Диагностика", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "quit", "Завершить NEXUS", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&open, &overlay, &diagnostic, &settings, &quit])
        .map_err(|e| e.to_string())?;
    let mut tray = TrayIconBuilder::with_id("nexus-tray")
        .tooltip("NEXUS Desktop · мониторинг оборудования")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                let _ = show(app, None);
            }
            "overlay" => {
                let _ = native::toggle();
            }
            "diagnostics" | "settings" => {
                let _ = show(app, Some(event.id().as_ref().to_string()));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                let _ = show(tray.app_handle(), None);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app).map_err(|e| e.to_string())?;
    native::start(app.clone())?;
    Ok(())
}
pub fn apply(settings: &Settings) -> Result<(), String> {
    native::apply(settings)
}
pub fn overlay_toggle() -> Result<bool, String> {
    native::toggle()
}
pub fn on_snapshot(snapshot: &serde_json::Value) {
    native::snapshot(snapshot)
}
pub fn shutdown() {
    native::shutdown()
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::{
        settings::{parse_hotkey, Hotkeys, Overlay},
        storage::{now_ms, parse_ms},
    };
    use std::{
        ffi::c_void,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc, Arc, Mutex, OnceLock,
        },
        time::Duration,
    };
    type Hwnd = *mut c_void;
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Message {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        point: Point,
        private: u32,
    }
    #[repr(C)]
    struct Paint {
        dc: Hwnd,
        erase: i32,
        rect: Rect,
        restore: i32,
        update: i32,
        reserved: [u8; 32],
    }
    #[repr(C)]
    struct Class {
        style: u32,
        procedure: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
        class_extra: i32,
        window_extra: i32,
        instance: Hwnd,
        icon: Hwnd,
        cursor: Hwnd,
        background: Hwnd,
        menu: *const u16,
        name: *const u16,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(class: *const Class) -> u16;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hwnd,
            instance: Hwnd,
            param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
        fn GetMessageW(message: *mut Message, hwnd: Hwnd, min: u32, max: u32) -> i32;
        fn TranslateMessage(message: *const Message) -> i32;
        fn DispatchMessageW(message: *const Message) -> isize;
        fn PostMessageW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> i32;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn PostQuitMessage(code: i32);
        fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
        fn SetWindowPos(
            hwnd: Hwnd,
            after: Hwnd,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn SetLayeredWindowAttributes(hwnd: Hwnd, color: u32, alpha: u8, flags: u32) -> i32;
        fn BeginPaint(hwnd: Hwnd, paint: *mut Paint) -> Hwnd;
        fn EndPaint(hwnd: Hwnd, paint: *const Paint) -> i32;
        fn FillRect(dc: Hwnd, rect: *const Rect, brush: Hwnd) -> i32;
        fn DrawTextW(dc: Hwnd, text: *const u16, count: i32, rect: *mut Rect, format: u32) -> i32;
        fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: i32) -> i32;
        fn RegisterHotKey(hwnd: Hwnd, id: i32, modifiers: u32, key: u32) -> i32;
        fn UnregisterHotKey(hwnd: Hwnd, id: i32) -> i32;
        fn SetTimer(hwnd: Hwnd, id: usize, milliseconds: u32, callback: *const c_void) -> usize;
        fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateSolidBrush(color: u32) -> Hwnd;
        fn DeleteObject(object: Hwnd) -> i32;
        fn SetTextColor(dc: Hwnd, color: u32) -> u32;
        fn SetBkMode(dc: Hwnd, mode: i32) -> i32;
        fn SelectObject(dc: Hwnd, object: Hwnd) -> Hwnd;
        fn CreateFontW(
            height: i32,
            width: i32,
            escapement: i32,
            orientation: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strike: u32,
            charset: u32,
            output: u32,
            clip: u32,
            quality: u32,
            pitch: u32,
            name: *const u16,
        ) -> Hwnd;
        fn CreatePen(style: i32, width: i32, color: u32) -> Hwnd;
        fn MoveToEx(dc: Hwnd, x: i32, y: i32, point: *mut Point) -> i32;
        fn LineTo(dc: Hwnd, x: i32, y: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Hwnd;
    }
    #[derive(Clone)]
    struct Reading {
        label: String,
        value: Option<f64>,
        unit: String,
        source: String,
        at: i64,
        ttl: i64,
        history: Vec<Option<f64>>,
    }
    impl Reading {
        fn update(
            label: &str,
            metric: Option<&serde_json::Value>,
            previous: Option<&Self>,
            now: i64,
        ) -> Self {
            let unit = metric
                .and_then(|m| m["unit"].as_str())
                .unwrap_or("")
                .to_string();
            let source = metric
                .and_then(|m| m["source"].as_str())
                .unwrap_or("")
                .to_string();
            let at = metric
                .and_then(|m| m["at"].as_str())
                .and_then(parse_ms)
                .unwrap_or(0);
            let ttl = metric.and_then(|m| m["maxAgeMs"].as_i64()).unwrap_or(15000);
            let value = metric
                .and_then(|m| crate::metrics::reading(m, now, 15000))
                .map(|(value, _)| value);
            let previous = previous.filter(|p| p.source == source && p.unit == unit && p.at <= at);
            let mut history = previous.map(|p| p.history.clone()).unwrap_or_default();
            if previous.is_none_or(|p| p.at != at || p.value != value) {
                if previous.is_some_and(|p| at.saturating_sub(p.at) > p.ttl) {
                    history.push(None);
                }
                history.push(value);
                if history.len() > 64 {
                    history.drain(..history.len() - 64);
                }
            }
            Self {
                label: label.into(),
                value,
                unit,
                source,
                at,
                ttl,
                history,
            }
        }
    }
    struct ConfigRequest {
        overlay: Overlay,
        hotkeys: Hotkeys,
        reply: mpsc::SyncSender<Result<(), String>>,
    }
    struct Shared {
        app: tauri::AppHandle,
        hwnd: AtomicUsize,
        enabled: AtomicBool,
        overlay: Mutex<Overlay>,
        hotkeys: Mutex<Hotkeys>,
        readings: Mutex<Vec<Reading>>,
        config: Mutex<Option<ConfigRequest>>,
    }
    static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
    const APPLY: u32 = 0x8001;
    const TOGGLE: u32 = 0x8002;
    const REDRAW: u32 = 0x8003;
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let shared = Arc::new(Shared {
            app,
            hwnd: AtomicUsize::new(0),
            enabled: AtomicBool::new(false),
            overlay: Mutex::new(Overlay::default()),
            hotkeys: Mutex::new(Hotkeys::default()),
            readings: Mutex::new(vec![]),
            config: Mutex::new(None),
        });
        SHARED
            .set(shared.clone())
            .map_err(|_| "Native desktop already initialized")?;
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || unsafe {
            let name = wide("NEXUS.Native.Overlay.1");
            let instance = GetModuleHandleW(std::ptr::null());
            let class = Class {
                style: 0,
                procedure: Some(procedure),
                class_extra: 0,
                window_extra: 0,
                instance,
                icon: std::ptr::null_mut(),
                cursor: std::ptr::null_mut(),
                background: std::ptr::null_mut(),
                menu: std::ptr::null(),
                name: name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                let _ = tx.send(Err("Unable to register native overlay".to_string()));
                return;
            }
            let hwnd = CreateWindowExW(
                0x00080000 | 0x00000008 | 0x00000080 | 0x08000000 | 0x20,
                name.as_ptr(),
                wide("NEXUS Overlay").as_ptr(),
                0x80000000,
                24,
                24,
                300,
                190,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null_mut(),
            );
            if hwnd.is_null() {
                let _ = tx.send(Err("Unable to create native overlay".to_string()));
                return;
            }
            shared.hwnd.store(hwnd as usize, Ordering::Release);
            let _ = tx.send(Ok(()));
            let mut msg = std::mem::zeroed::<Message>();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            shared.hwnd.store(0, Ordering::Release);
        });
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| "Native desktop initialization timed out")?
    }
    fn post(message: u32) -> Result<(), String> {
        let shared = SHARED.get().ok_or("Native overlay unavailable")?;
        let hwnd = shared.hwnd.load(Ordering::Acquire) as Hwnd;
        if hwnd.is_null() {
            return Err("Native overlay unavailable".into());
        }
        if unsafe { PostMessageW(hwnd, message, 0, 0) } == 0 {
            return Err("Native desktop message failed".into());
        }
        Ok(())
    }
    pub fn apply(settings: &Settings) -> Result<(), String> {
        let shared = SHARED.get().ok_or("Native desktop unavailable")?;
        let (tx, rx) = mpsc::sync_channel(1);
        let mut slot = shared.config.lock().unwrap();
        if slot.is_some() {
            return Err("Desktop settings are being applied".into());
        }
        *slot = Some(ConfigRequest {
            overlay: settings.overlay.clone(),
            hotkeys: settings.hotkeys.clone(),
            reply: tx,
        });
        drop(slot);
        post(APPLY)?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "Desktop settings timed out")?
    }
    pub fn toggle() -> Result<bool, String> {
        let shared = SHARED.get().ok_or("Overlay unavailable")?;
        let target = !shared.enabled.load(Ordering::Acquire);
        post(TOGGLE)?;
        Ok(target)
    }
    pub fn shutdown() {
        let _ = post(0x0010);
    }
    pub fn snapshot(snapshot: &serde_json::Value) {
        let Some(shared) = SHARED.get() else { return };
        if !shared.enabled.load(Ordering::Relaxed) {
            return;
        }
        let config = shared.overlay.lock().unwrap().clone();
        let mut readings = shared.readings.lock().unwrap();
        let previous = readings.clone();
        readings.clear();
        for key in &config.metric_ids {
            let (kind, id) = key.split_once('.').unwrap_or(("", ""));
            let id = if kind == "cpu" && id == "frequency" {
                "clock"
            } else {
                id
            };
            let metric = snapshot["components"]
                .as_array()
                .and_then(|cs| cs.iter().find(|c| c["kind"] == kind))
                .and_then(|c| c["metrics"].as_array())
                .and_then(|ms| ms.iter().find(|m| m["id"] == id));
            readings.push(Reading::update(
                key,
                metric,
                previous.iter().find(|r| r.label == *key),
                now_ms(),
            ));
        }
        drop(readings);
        let _ = post(REDRAW);
    }
    unsafe fn register(hwnd: Hwnd, keys: &Hotkeys) -> Result<(), String> {
        let mut registered = vec![];
        for (id, key) in [(1, &keys.overlay), (2, &keys.main)] {
            if let Some((mods, key)) = parse_hotkey(key)? {
                if RegisterHotKey(hwnd, id, mods, key) == 0 {
                    for id in registered {
                        UnregisterHotKey(hwnd, id);
                    }
                    return Err(
                        "Сочетание клавиш занято другой программой; выберите другое.".into(),
                    );
                }
                registered.push(id);
            }
        }
        Ok(())
    }
    unsafe fn dimensions(hwnd: Hwnd, settings: &Overlay) {
        let width = (if settings.mode == "advanced" {
            450.
        } else {
            300.
        } * settings.scale) as i32;
        let height = ((35.
            + settings.metric_ids.len() as f64
                * if settings.mode == "advanced" {
                    37.
                } else {
                    25.
                })
            * settings.scale) as i32;
        SetWindowPos(
            hwnd,
            -1isize as Hwnd,
            settings.x,
            settings.y,
            width,
            height,
            0x0010,
        );
        SetLayeredWindowAttributes(hwnd, 0, (settings.opacity * 255.) as u8, 2);
    }
    unsafe extern "system" fn procedure(
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        let Some(shared) = SHARED.get() else {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        };
        match message {
            APPLY => {
                if let Some(request) = shared.config.lock().unwrap().take() {
                    let old = shared.hotkeys.lock().unwrap().clone();
                    UnregisterHotKey(hwnd, 1);
                    UnregisterHotKey(hwnd, 2);
                    match register(hwnd, &request.hotkeys) {
                        Ok(()) => {
                            *shared.hotkeys.lock().unwrap() = request.hotkeys;
                            *shared.overlay.lock().unwrap() = request.overlay.clone();
                            dimensions(hwnd, &request.overlay);
                            shared.readings.lock().unwrap().clear();
                            let _ = request.reply.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = register(hwnd, &old);
                            let _ = request.reply.send(Err(e));
                        }
                    }
                }
                0
            }
            TOGGLE => {
                let enabled = !shared.enabled.fetch_xor(true, Ordering::AcqRel);
                if enabled {
                    dimensions(hwnd, &shared.overlay.lock().unwrap());
                    ShowWindow(hwnd, 4);
                    SetTimer(hwnd, 1, 1000, std::ptr::null());
                    if let Some(snapshot) = shared
                        .app
                        .state::<crate::Collector>()
                        .latest
                        .lock()
                        .unwrap()
                        .as_ref()
                    {
                        snapshot_update(snapshot);
                    }
                } else {
                    KillTimer(hwnd, 1);
                    ShowWindow(hwnd, 0);
                }
                0
            }
            0x0312 => {
                if wparam == 1 {
                    let _ = post(TOGGLE);
                } else if wparam == 2 {
                    let app = shared.app.clone();
                    let dispatch = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        let _ = super::show(&dispatch, None);
                    });
                }
                0
            }
            REDRAW | 0x0113 => {
                if shared.enabled.load(Ordering::Relaxed) {
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                0
            }
            0x000F => {
                paint(hwnd, shared);
                0
            }
            0x0014 => 1,
            0x0010 => {
                UnregisterHotKey(hwnd, 1);
                UnregisterHotKey(hwnd, 2);
                KillTimer(hwnd, 1);
                DestroyWindow(hwnd);
                0
            }
            0x0002 => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
    fn snapshot_update(snapshot: &serde_json::Value) {
        self::snapshot(snapshot);
    }
    unsafe fn draw(dc: Hwnd, text: &str, x: i32, y: i32, width: i32, height: i32, color: u32) {
        SetTextColor(dc, color);
        let mut rect = Rect {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        };
        let text = wide(text);
        DrawTextW(dc, text.as_ptr(), -1, &mut rect, 0x0020 | 0x0004 | 0x8000);
    }
    unsafe fn paint(hwnd: Hwnd, shared: &Shared) {
        let mut paint = std::mem::zeroed::<Paint>();
        let dc = BeginPaint(hwnd, &mut paint);
        let bg = CreateSolidBrush(0x001b1815);
        FillRect(dc, &paint.rect, bg);
        DeleteObject(bg);
        SetBkMode(dc, 1);
        let overlay = shared.overlay.lock().unwrap().clone();
        let scale = overlay.scale;
        let font = CreateFontW(
            (15. * scale) as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let old = SelectObject(dc, font);
        draw(
            dc,
            "NEXUS  /  LIVE",
            (14. * scale) as i32,
            (7. * scale) as i32,
            (270. * scale) as i32,
            (24. * scale) as i32,
            0x009bdd70,
        );
        for (i, r) in shared.readings.lock().unwrap().iter().enumerate() {
            let y = ((34. + i as f64 * if overlay.mode == "advanced" { 37. } else { 25. }) * scale)
                as i32;
            let age = now_ms().saturating_sub(r.at);
            let value = if age >= 0 && r.ttl > 0 && age <= r.ttl {
                r.value
                    .map(|v| format!("{v:.1} {}", r.unit))
                    .unwrap_or_else(|| "Недоступно".into())
            } else {
                "Нет свежих данных".into()
            };
            draw(
                dc,
                &format!("{}   {value}", r.label),
                (14. * scale) as i32,
                y,
                (275. * scale) as i32,
                (26. * scale) as i32,
                0x00eceae6,
            );
            if overlay.mode == "advanced" && r.history.len() > 1 {
                let min = r
                    .history
                    .iter()
                    .flatten()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
                let max = r
                    .history
                    .iter()
                    .flatten()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max);
                let pen = CreatePen(0, 1, 0x009bdd70);
                let old_pen = SelectObject(dc, pen);
                let mut connected = false;
                for (j, value) in r.history.iter().enumerate() {
                    let Some(value) = value else {
                        connected = false;
                        continue;
                    };
                    let x =
                        ((300. + j as f64 * 130. / (r.history.len() - 1) as f64) * scale) as i32;
                    let yy = y
                        + (25. * scale - (value - min) / (max - min).max(1.) * 20. * scale) as i32;
                    if !connected {
                        MoveToEx(dc, x, yy, std::ptr::null_mut());
                    } else {
                        LineTo(dc, x, yy);
                    }
                    connected = true;
                }
                SelectObject(dc, old_pen);
                DeleteObject(pen);
            }
        }
        SelectObject(dc, old);
        DeleteObject(font);
        EndPaint(hwnd, &paint);
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;
        fn metric(at: i64, source: &str, unit: &str) -> serde_json::Value {
            json!({"at":chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339(),"value":10.,"source":source,"unit":unit,"maxAgeMs":5000})
        }
        #[test]
        fn overlay_deduplicates_cached_readings_and_breaks_missing_spans() {
            let at = 1_800_000_000_000;
            let m = metric(at, "A", "%");
            let first = Reading::update("cpu.load", Some(&m), None, at);
            let cached = Reading::update("cpu.load", Some(&m), Some(&first), at + 2000);
            assert_eq!(cached.history, vec![Some(10.)]);
            let mut missing = m.clone();
            missing["status"] = json!("stale");
            let gap = Reading::update("cpu.load", Some(&missing), Some(&cached), at + 3000);
            assert!(gap.value.is_none());
            assert_eq!(gap.history, vec![Some(10.), None]);
            let restored = Reading::update(
                "cpu.load",
                Some(&metric(at + 10000, "A", "%")),
                Some(&gap),
                at + 10000,
            );
            assert_eq!(restored.history.last(), Some(&Some(10.)));
            assert!(restored.history.contains(&None));
            let future = Reading::update("cpu.load", Some(&m), None, at - 1);
            assert!(future.value.is_none());
        }
        #[test]
        fn overlay_never_mixes_sources_units_or_clock_epochs() {
            let at = 1_800_000_000_000;
            let first = Reading::update("cpu.load", Some(&metric(at, "A", "%")), None, at);
            for m in [
                metric(at + 1, "B", "%"),
                metric(at + 1, "A", "W"),
                metric(at - 1, "A", "%"),
            ] {
                let r = Reading::update("cpu.load", Some(&m), Some(&first), at + 1);
                assert_eq!(r.history, vec![Some(10.)]);
            }
            let mut previous = first;
            for n in 1..100 {
                previous = Reading::update(
                    "cpu.load",
                    Some(&metric(at + n, "A", "%")),
                    Some(&previous),
                    at + n,
                );
            }
            assert_eq!(previous.history.len(), 64);
        }
    }
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn start(_: tauri::AppHandle) -> Result<(), String> {
        Err("Native desktop is Windows-only".into())
    }
    pub fn apply(_: &Settings) -> Result<(), String> {
        Err("Windows desktop unavailable".into())
    }
    pub fn toggle() -> Result<bool, String> {
        Err("Windows overlay unavailable".into())
    }
    pub fn snapshot(_: &serde_json::Value) {}
    pub fn shutdown() {}
}
