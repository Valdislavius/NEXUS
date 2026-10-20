//! Manually requested, bounded user-mode workloads. No writes to physical disks.
use crate::storage::{now_ms, parse_ms, Event};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResultRecord {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub progress: f64,
    pub score: Option<f64>,
    pub unit: Option<String>,
    pub details: String,
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub kind: String,
    pub duration_seconds: Option<u64>,
}
#[derive(Default)]
pub struct Engine {
    pub current: Mutex<Option<Arc<Mutex<ResultRecord>>>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    last_network_at: AtomicI64,
}
impl Engine {
    pub fn status(&self) -> Option<ResultRecord> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.lock().unwrap().clone())
    }
    pub fn cancel(&self) -> bool {
        if self.status().is_some_and(|r| r.status == "running") {
            if let Some(flag) = self.cancel.lock().unwrap().as_ref() {
                flag.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }
    pub fn start(&self, app: tauri::AppHandle, request: Request) -> Result<ResultRecord, String> {
        const KINDS: &[&str] = &[
            "cpu_single",
            "cpu_multi",
            "memory",
            "storage_read",
            "network_dns",
            "network_ping",
            "network_traceroute",
            "motherboard",
        ];
        if !KINDS.contains(&request.kind.as_str()) {
            return Err("This benchmark is not available; GPU/VRAM tests require a compatible documented backend".into());
        }
        let mut current = self.current.lock().unwrap();
        if current
            .as_ref()
            .is_some_and(|r| r.lock().unwrap().status == "running")
        {
            return Err("A test is already running".into());
        }
        let duration = request.duration_seconds.unwrap_or(5);
        if !(1..=30).contains(&duration) {
            return Err("Test duration must be 1–30 seconds".into());
        }
        let now = now_ms();
        if ["cpu_single", "cpu_multi", "memory", "storage_read"].contains(&request.kind.as_str()) {
            if app
                .state::<crate::Collector>()
                .latest
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|snapshot| !dangerous_temperatures(snapshot).is_empty())
            {
                return Err("Тест не запущен: доступное свежее показание температуры уже достигло защитного лимита (CPU/GPU 100 °C, накопитель 80 °C).".into());
            }
        }
        if request.kind.starts_with("network_") && !self.network_ready(now) {
            return Err("Wait 15 seconds between network tests".into());
        }
        if request.kind.starts_with("network_") {
            self.last_network_at.store(now, Ordering::Relaxed);
        }
        let initial = ResultRecord {
            id: format!("benchmark-{now}"),
            kind: request.kind.clone(),
            status: "running".into(),
            started_at_ms: now,
            finished_at_ms: None,
            progress: 0.,
            score: None,
            unit: None,
            details: description(&request.kind).into(),
            error: None,
        };
        let record = Arc::new(Mutex::new(initial.clone()));
        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().unwrap() = Some(cancel.clone());
        *current = Some(record.clone());
        log_event(&app, &initial, "Начало проверки");
        std::thread::spawn(move || {
            let started = Instant::now();
            let work_cancel = cancel.clone();
            let kind = request.kind.clone();
            let work_app = app.clone();
            let worker =
                std::thread::spawn(move || run(&kind, duration, work_cancel, Some(&work_app)));
            let mut safety_samples = std::collections::HashMap::<String, (i64, u32)>::new();
            let mut stopped = false;
            while !worker.is_finished() {
                let status = app.state::<crate::Collector>();
                if let Some(snapshot) = status.latest.lock().unwrap().as_ref() {
                    let readings = dangerous_temperatures(snapshot);
                    safety_samples.retain(|key, _| readings.contains_key(key));
                    for (key, at) in readings {
                        let previous = safety_samples.entry(key).or_insert((at, 0));
                        if previous.1 == 0 || at > previous.0 {
                            previous.0 = at;
                            previous.1 += 1;
                        }
                    }
                }
                if safety_samples.values().any(|(_, count)| *count >= 2) {
                    stopped = true;
                    cancel.store(true, Ordering::Relaxed);
                }
                record.lock().unwrap().progress =
                    (started.elapsed().as_secs_f64() / duration as f64 * 100.).min(99.);
                std::thread::sleep(Duration::from_millis(100));
            }
            let outcome = worker.join();
            let mut r = record.lock().unwrap();
            r.finished_at_ms = Some(now_ms());
            r.progress = 100.;
            match outcome {
                Ok(Ok((score, unit, details))) => {
                    r.score = score;
                    r.unit = unit;
                    r.details = details;
                    r.status = if stopped {
                        "stopped"
                    } else if cancel.load(Ordering::Relaxed) {
                        "cancelled"
                    } else {
                        "completed"
                    }
                    .into();
                    if stopped {
                        r.error=Some("Тест остановлен: два свежих показания достигли защитного температурного лимита. Это не диагноз неисправности.".into());
                    }
                }
                Ok(Err(error)) => {
                    r.status = if cancel.load(Ordering::Relaxed) {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    r.error = Some(error);
                }
                Err(_) => {
                    r.status = "failed".into();
                    r.error = Some("Изолированный рабочий поток завершился с ошибкой".into());
                }
            }
            let finished = r.clone();
            drop(r);
            if let Some(history) = app
                .state::<crate::Services>()
                .history
                .lock()
                .unwrap()
                .as_mut()
            {
                let _ = history.save_benchmark(&json!(finished));
            }
            log_event(&app, &finished, "Проверка завершена");
            let _ = app.emit("nexus:benchmark", &finished);
        });
        Ok(initial)
    }
    fn network_ready(&self, now: i64) -> bool {
        let previous = self.last_network_at.load(Ordering::Relaxed);
        previous == 0 || now - previous >= 15000
    }
}
fn description(kind: &str) -> &'static str {
    match kind{"cpu_single"=>"Один поток, вычисления с паузами (до 50% времени). Результат относится к этому алгоритму.","cpu_multi"=>"До 16 потоков с паузами (до 50% времени); это сравнительная проверка, не стресс-тест.","memory"=>"64 МиБ собственной выделенной памяти: запись шаблона, чтение и проверка. Не проверяет всю RAM.","storage_read"=>"Чтение файла приложения: до 64 МиБ за проход; кэш Windows влияет на результат. Записей нет.","network_dns"=>"Один запрос DNS example.com через системный DNS. Ограничение 5 секунд.","network_ping"=>"Четыре ICMP-запроса к публичному узлу с интервалом 1 с. Блокировка ICMP не означает неисправность.",_=>"Анализ уже доступных датчиков и контроллеров без отправки тестовых команд устройствам."}
}
fn log_event(app: &tauri::AppHandle, r: &ResultRecord, title: &str) {
    if let Some(history) = app
        .state::<crate::Services>()
        .history
        .lock()
        .unwrap()
        .as_mut()
    {
        let _ = history.event(&Event {
            id: format!("{}:{}", r.id, r.status),
            at_ms: now_ms(),
            source: "benchmark".into(),
            component_id: r.kind.split('_').next().unwrap_or("system").into(),
            severity: if r.status == "failed" || r.status == "stopped" {
                "warning"
            } else {
                "info"
            }
            .into(),
            title: title.into(),
            details: r.details.clone(),
            data: json!(r),
        });
    }
}
type Outcome = Result<(Option<f64>, Option<String>, String), String>;
fn dangerous_temperatures(snapshot: &serde_json::Value) -> std::collections::HashMap<String, i64> {
    let mut result = std::collections::HashMap::new();
    if let Some(components) = snapshot["components"].as_array() {
        for c in components {
            if let Some(metrics) = c["metrics"].as_array() {
                for m in metrics {
                    if m["id"] == "temperature"
                        && m["value"]
                            .as_f64()
                            .is_some_and(|v| v >= if c["kind"] == "storage" { 80. } else { 100. })
                    {
                        if let Some(at) = m["at"]
                            .as_str()
                            .and_then(parse_ms)
                            .filter(|at| now_ms() - at < 15000 && *at <= now_ms() + 60000)
                        {
                            result.insert(c["id"].as_str().unwrap_or("").into(), at);
                        }
                    }
                }
            }
        }
    }
    result
}
fn run(
    kind: &str,
    seconds: u64,
    cancel: Arc<AtomicBool>,
    app: Option<&tauri::AppHandle>,
) -> Outcome {
    if cancel.load(Ordering::Relaxed) {
        return Err("Test cancelled before allocating resources".into());
    }
    match kind {
        "cpu_single" | "cpu_multi" => {
            let workers = if kind == "cpu_single" {
                1
            } else {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
                    .min(16)
            };
            let count = Arc::new(AtomicU64::new(0));
            let end = Instant::now() + Duration::from_secs(seconds);
            let start = Instant::now();
            let mut threads = Vec::new();
            for seed in 0..workers {
                let count = count.clone();
                let cancel = cancel.clone();
                threads.push(std::thread::spawn(move || {
                    let mut x = seed as u64 + 1;
                    while Instant::now() < end && !cancel.load(Ordering::Relaxed) {
                        let slice = Instant::now();
                        let mut n = 0;
                        while slice.elapsed() < Duration::from_millis(10)
                            && Instant::now() < end
                            && !cancel.load(Ordering::Relaxed)
                        {
                            for _ in 0..4096 {
                                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                                x ^= x.rotate_left(17);
                            }
                            std::hint::black_box(x);
                            n += 4096;
                        }
                        count.fetch_add(n, Ordering::Relaxed);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }));
            }
            for thread in threads {
                thread.join().map_err(|_| "CPU worker failed")?;
            }
            Ok((Some(count.load(Ordering::Relaxed)as f64/start.elapsed().as_secs_f64()),Some("operations/s".into()),format!("{workers} workers; duty cap 50%; algorithm NEXUS integer v1. Hardware health is not inferred.")))
        }
        "memory" => {
            let start = Instant::now();
            let end = start + Duration::from_secs(seconds);
            let mut memory = Vec::<u64>::new();
            memory
                .try_reserve_exact(8 * 1024 * 1024)
                .map_err(|_| "Cannot allocate a bounded 64 MiB block")?;
            memory.resize(8 * 1024 * 1024, 0);
            let mut bytes = 0u64;
            let mut pass = 0u64;
            while Instant::now() < end && !cancel.load(Ordering::Relaxed) {
                for chunk in memory.chunks_mut(8192) {
                    if cancel.load(Ordering::Relaxed) || Instant::now() >= end {
                        break;
                    }
                    for (i, v) in chunk.iter_mut().enumerate() {
                        *v = (i as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ pass;
                    }
                    std::hint::black_box(&mut *chunk);
                    let mut checksum = 0u64;
                    for (i, v) in chunk.iter().enumerate() {
                        if *v != (i as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ pass {
                            return Err("Pattern mismatch in allocated process memory".into());
                        }
                        checksum ^= *v;
                    }
                    std::hint::black_box(checksum);
                    bytes += (chunk.len() * 16) as u64;
                }
                pass += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok((Some(bytes as f64/(1024.*1024.)/start.elapsed().as_secs_f64()),Some("MiB/s".into()),"64 MiB process-local write/read pattern; no claim about unallocated memory, DIMMs or latency.".into()))
        }
        "storage_read" => {
            let path = std::env::current_exe().map_err(|e| e.to_string())?;
            let start = Instant::now();
            let mut total = 0usize;
            let mut buffer = vec![0u8; 1024 * 1024];
            let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            while total < 64 * 1024 * 1024
                && start.elapsed() < Duration::from_secs(seconds)
                && !cancel.load(Ordering::Relaxed)
            {
                let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                std::hint::black_box(&buffer[..n]);
                total += n;
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok((Some(total as f64/(1024.*1024.)/start.elapsed().as_secs_f64()),Some("MiB/s".into()),format!("Read {total} bytes of the executable only; OS cache and intentional 1 ms pauses affect throughput. This is not a physical-drive benchmark.")))
        }
        "network_dns" => {
            let output = bounded_command(
                "nslookup.exe",
                &["-timeout=2", "-retry=1", "example.com"],
                &cancel,
                Duration::from_secs(5),
            )?;
            if output.0 != 0 {
                return Err("System DNS query returned an error".into());
            }
            Ok((Some(output.1),Some("ms".into()),"One system DNS lookup of example.com completed. Response addresses are excluded from the result.".into()))
        }
        "network_ping" => icmp_ping(&cancel),
        "network_traceroute" => check_outcome(crate::windows_checks::traceroute(&cancel)?),
        "motherboard" => {
            let snapshot = app
                .ok_or("Desktop state is unavailable")?
                .state::<crate::Collector>()
                .latest
                .lock()
                .unwrap()
                .clone();
            let Some(snapshot) = snapshot else {
                return Err("No collected hardware inventory is available".into());
            };
            let motherboard = snapshot["components"]
                .as_array()
                .and_then(|cs| cs.iter().find(|c| c["kind"] == "motherboard"));
            let Some(board) = motherboard else {
                return Err("Motherboard inventory is unavailable".into());
            };
            let metrics = board["metrics"].as_array();
            let count =
                metrics.map_or(0, |ms| ms.iter().filter(|m| m["value"].is_number()).count());
            let summary = crate::windows_checks::motherboard(&cancel)?;
            let (score, unit, details) = check_outcome(summary)?;
            Ok((
                score,
                unit,
                format!("Motherboard inventory available; {count} sensor readings. {details}"),
            ))
        }
        _ => Err("Unsupported test".into()),
    }
}
fn check_outcome(summary: crate::windows_checks::CheckSummary) -> Outcome {
    if summary.status == "unavailable" {
        return Err(summary.details);
    }
    Ok((
        None,
        None,
        format!(
            "{}: {} Measurements: {}",
            summary.status, summary.details, summary.measurements
        ),
    ))
}
fn bounded_command(
    executable: &str,
    args: &[&str],
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<(i32, f64), String> {
    use std::process::{Command, Stdio};
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot unavailable")?;
    let path = std::path::PathBuf::from(system)
        .join("System32")
        .join(executable);
    let mut command = Command::new(path);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let start = Instant::now();
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) {
                "Test cancelled"
            } else {
                "Network test timed out"
            }
            .into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok((
                status.code().unwrap_or(-1),
                start.elapsed().as_secs_f64() * 1000.,
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn ping_summary(sent: usize, replies: &[u32]) -> Outcome {
    let loss = if sent == 0 {
        0.
    } else {
        (sent - replies.len()) as f64 / sent as f64 * 100.
    };
    let average = if replies.is_empty() {
        None
    } else {
        Some(replies.iter().map(|v| *v as f64).sum::<f64>() / replies.len() as f64)
    };
    Ok((average,Some("ms".into()),format!("ICMP public endpoint: sent={sent}, received={}, loss={loss:.1}%, min={}ms, max={}ms. Firewalls or rate limits can drop ICMP; this does not establish hardware failure. Target and response addresses are excluded.",replies.len(),replies.iter().min().map(u32::to_string).unwrap_or_else(||"N/A".into()),replies.iter().max().map(u32::to_string).unwrap_or_else(||"N/A".into()))))
}
#[cfg(windows)]
fn icmp_ping(cancel: &AtomicBool) -> Outcome {
    use std::ffi::c_void;
    #[repr(C)]
    struct Options {
        ttl: u8,
        tos: u8,
        flags: u8,
        size: u8,
        data: *mut u8,
    }
    #[repr(C)]
    struct Reply {
        address: u32,
        status: u32,
        round_trip: u32,
        data_size: u16,
        reserved: u16,
        data: *mut u8,
        options: Options,
    }
    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        fn IcmpCreateFile() -> *mut c_void;
        fn IcmpCloseHandle(handle: *mut c_void) -> i32;
        fn IcmpSendEcho(
            handle: *mut c_void,
            destination: u32,
            request: *const c_void,
            size: u16,
            options: *const Options,
            reply: *mut c_void,
            reply_size: u32,
            timeout: u32,
        ) -> u32;
    }
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                IcmpCloseHandle(self.0);
            }
        }
    }
    let raw = unsafe { IcmpCreateFile() };
    if raw.is_null() || raw as isize == -1 {
        return Err("Windows ICMP API is unavailable".into());
    }
    let handle = Handle(raw);
    let mut replies = Vec::new();
    let mut sent = 0;
    for _ in 0..4 {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let start = Instant::now();
        let payload = b"NEXUS health";
        let mut buffer = vec![0usize; 128];
        let count = unsafe {
            IcmpSendEcho(
                handle.0,
                u32::from_ne_bytes([1, 1, 1, 1]),
                payload.as_ptr().cast(),
                payload.len() as u16,
                std::ptr::null(),
                buffer.as_mut_ptr().cast(),
                (buffer.len() * std::mem::size_of::<usize>()) as u32,
                750,
            )
        };
        sent += 1;
        if count > 0 {
            let reply = unsafe { &*(buffer.as_ptr() as *const Reply) };
            if reply.status == 0 {
                replies.push(reply.round_trip);
            }
        }
        while start.elapsed() < Duration::from_secs(1) && !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    ping_summary(sent, &replies)
}
#[cfg(not(windows))]
fn icmp_ping(_: &AtomicBool) -> Outcome {
    Err("Windows ICMP API is unavailable".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_has_no_implicit_start() {
        let engine = Engine::default();
        assert!(!engine.cancel());
        assert!(engine.status().is_none());
    }
    #[test]
    fn network_cooldown_is_independent_of_last_test_kind() {
        let engine = Engine::default();
        engine.last_network_at.store(1000, Ordering::Relaxed);
        assert!(!engine.network_ready(1001));
        assert!(!engine.network_ready(15999));
        assert!(engine.network_ready(16000));
    }
    #[test]
    fn descriptions_distinguish_partial_checks() {
        assert!(description("memory").contains("64"));
        assert!(description("storage_read").contains("кэш"));
    }
    #[test]
    fn packet_loss_does_not_fabricate_latency() {
        let no_replies = ping_summary(4, &[]).unwrap();
        assert_eq!(no_replies.0, None);
        assert!(no_replies.2.contains("loss=100.0%"));
        let replies = ping_summary(4, &[10, 20]).unwrap();
        assert_eq!(replies.0, Some(15.));
        assert!(replies.2.contains("loss=50.0%"));
    }
    #[test]
    fn cpu_worker_observes_cancellation_instead_of_running_full_duration() {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let timer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            signal.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        let outcome = run("cpu_single", 30, cancel, None).unwrap();
        timer.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(outcome.1.as_deref(), Some("operations/s"));
    }
    #[test]
    fn pre_cancel_does_not_allocate_memory() {
        let result = run("memory", 30, Arc::new(AtomicBool::new(true)), None);
        assert!(result.unwrap_err().contains("before allocating"));
    }
    #[test]
    fn thermal_guard_requires_fresh_actual_sensor_reading() {
        let at = chrono::DateTime::from_timestamp_millis(now_ms())
            .unwrap()
            .to_rfc3339();
        let mut snapshot = json!({"components":[{"id":"cpu","kind":"cpu","metrics":[{"id":"temperature","value":105,"at":at}]}]});
        assert_eq!(dangerous_temperatures(&snapshot).len(), 1);
        snapshot["components"][0]["metrics"][0]["at"] = json!("2020-01-01T00:00:00Z");
        assert!(dangerous_temperatures(&snapshot).is_empty());
    }
}
