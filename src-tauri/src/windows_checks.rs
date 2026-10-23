//! Opt-in Windows checks. Never called by collection, startup or diagnostics.
//! Commands, destinations and queries are fixed; raw route/event data never
//! leaves this module. No register writes, device changes or log clearing.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_HOPS: u8 = 8;
const MAX_EVENTS: usize = 20;
const TRACE_DESTINATION: &str = "1.1.1.1";
const EVENT_QUERY: &str = "/q:*[System[Provider[@Name='Microsoft-Windows-WHEA-Logger'] and TimeCreated[timediff(@SystemTime) <= 86400000]]]";
// Select only numeric status. Neither device IDs, names, paths nor host/IP
// addresses are included in stdout. A failure is unavailable, never zero errors.
const CONTROLLER_SCRIPT: &str = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); Import-Module -Name ($PSHOME+'\\Modules\\CimCmdlets\\CimCmdlets.psd1') -ErrorAction Stop; $d=@(CimCmdlets\\Get-CimInstance -Query \"SELECT ConfigManagerErrorCode FROM Win32_PnPEntity WHERE PNPClass='System' AND Present=TRUE\" -ErrorAction Stop); $problems=0; $unknown=0; foreach($device in $d){if($null -eq $device.ConfigManagerErrorCode){$unknown++}elseif($device.ConfigManagerErrorCode -ne 0){$problems++}}; [Console]::WriteLine('{\"total\":'+$d.Count+',\"problems\":'+$problems+',\"unknown\":'+$unknown+'}')";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSummary {
    pub status: String,
    pub details: String,
    pub measurements: Value,
}

#[derive(Clone, Copy)]
enum Check {
    Traceroute,
    Events,
    Controllers,
}
impl Check {
    fn program(self) -> &'static str {
        match self {
            Self::Traceroute => "tracert.exe",
            Self::Events => "wevtutil.exe",
            Self::Controllers => "WindowsPowerShell/v1.0/powershell.exe",
        }
    }
    fn args(self) -> Vec<&'static str> {
        match self {
            Self::Traceroute => vec!["/d", "/4", "/h", "8", "/w", "500", TRACE_DESTINATION],
            Self::Events => vec![
                "qe",
                "System",
                EVENT_QUERY,
                "/c:20",
                "/rd:true",
                "/f:xml",
                "/e:Events",
                "/uni:true",
            ],
            Self::Controllers => vec![
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                CONTROLLER_SCRIPT,
            ],
        }
    }
    fn timeout(self) -> Duration {
        Duration::from_secs(match self {
            Self::Traceroute => 15,
            Self::Events => 5,
            Self::Controllers => 6,
        })
    }
}

struct CommandOutput {
    stdout: Vec<u8>,
    truncated: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    elapsed_ms: f64,
}

/// Manual public-endpoint trace: at most 8 hops / 24 probes, 500 ms per probe,
/// 15 seconds total. ICMP filtering is not diagnosed as hardware failure.
pub fn traceroute(cancel: &AtomicBool) -> Result<CheckSummary, String> {
    let output = run(Check::Traceroute, cancel)?;
    Ok(trace_summary(output))
}
fn trace_summary(output: CommandOutput) -> CheckSummary {
    let counts = parse_trace(&String::from_utf8_lossy(&output.stdout));
    let usable = !output.truncated && counts.observed > 0;
    let status = if usable && counts.reached && !output.timed_out && output.exit_code == Some(0) {
        "completed"
    } else if usable {
        "partial"
    } else {
        "unavailable"
    };
    CheckSummary {
        status: status.into(),
        details: if usable {
            format!("Manual IPv4 trace: observed {} of at most 8 hops; {} responded, {} did not respond; destination reached={}; timed out={}. Max 24 probes, 500 ms each, 15 s total. Route addresses are excluded. Missing ICMP replies can reflect filtering or rate limits; no latency or hardware-health score is inferred.", counts.observed, counts.responded, counts.unanswered, counts.reached, output.timed_out)
        } else {
            "Traceroute unavailable: Windows returned no complete supported hop rows, an error, or truncated output. No route, latency or connectivity result is inferred. Maximum 8 hops / 24 probes / 15 seconds; route addresses are excluded.".into()
        },
        measurements: json!({"observedHops": if usable { Some(counts.observed) } else { None }, "respondingHops": if usable { Some(counts.responded) } else { None }, "unansweredHops": if usable { Some(counts.unanswered) } else { None }, "destinationReached": if usable { Some(counts.reached) } else { None }, "maxHops":MAX_HOPS,"maxProbes":24,"probeTimeoutMs":500,"timedOut":output.timed_out,"elapsedMs":output.elapsed_ms}),
    }
}

/// Read-only WHEA events from the last 24 h (at most 20), plus present System
/// class PnP status counts. These observations do not certify a physical board.
pub fn motherboard(cancel: &AtomicBool) -> Result<CheckSummary, String> {
    let event_output = run(Check::Events, cancel);
    if cancel.load(Ordering::Relaxed) {
        return Err("Test cancelled".into());
    }
    let events = event_output
        .as_ref()
        .ok()
        .filter(|o| o.exit_code == Some(0) && !o.timed_out && !o.truncated)
        .and_then(|o| decode_xml(&o.stdout))
        .and_then(|s| count_events(&s));
    let controller_output = run(Check::Controllers, cancel);
    if cancel.load(Ordering::Relaxed) {
        return Err("Test cancelled".into());
    }
    let controllers = controller_output
        .as_ref()
        .ok()
        .filter(|o| o.exit_code == Some(0) && !o.timed_out && !o.truncated)
        .and_then(|o| parse_controller_counts(&o.stdout));
    let status = if events.is_none() && controllers.is_none() {
        "unavailable"
    } else if events.is_none()
        || controllers.is_none()
        || controllers
            .as_ref()
            .is_some_and(|c| c.total == 0 || c.unknown > 0)
    {
        "partial"
    } else {
        "completed"
    };
    let event_note = match events {
        Some(n) if n == MAX_EVENTS => {
            "at least 20 matching WHEA events (query limit reached)".into()
        }
        Some(n) => format!("{n} matching WHEA events"),
        None => "WHEA log unavailable (access, timeout, query or output format)".into(),
    };
    let device_note = controllers
        .as_ref()
        .map(|c| {
            format!(
                "{} present System-class PnP devices, {} nonzero problem codes, {} unknown codes",
                c.total, c.problems, c.unknown
            )
        })
        .unwrap_or_else(|| {
            "System-class PnP status unavailable (access, timeout, query or output format)".into()
        });
    Ok(CheckSummary {
        status: status.into(),
        details: format!("Manual read-only Windows check: {event_note} in the last 24 hours; {device_note}. WHEA query is capped at 20 records / 5 s; one local WMI status query is capped at 6 s. Raw event records, device IDs, names and host addresses are excluded. No stress, driver changes or controller test commands were issued. Events may be corrected or historical; their presence is not proof of a motherboard fault, and absence does not certify hardware health."),
        measurements: json!({"wheaEventsObserved":events,"wheaLimit":MAX_EVENTS,"wheaLimitReached":events.map(|n|n==MAX_EVENTS),"lookbackHours":24,"systemDevicesObserved":controllers.as_ref().map(|c|c.total),"deviceProblemCodes":controllers.as_ref().map(|c|c.problems),"unknownDeviceCodes":controllers.as_ref().map(|c|c.unknown)}),
    })
}

#[derive(Default, Debug)]
struct TraceCounts {
    observed: usize,
    responded: usize,
    unanswered: usize,
    reached: bool,
}
fn parse_trace(text: &str) -> TraceCounts {
    let mut hops = BTreeMap::new();
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let Some(hop) = tokens
            .next()
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|n| (1..=MAX_HOPS).contains(n))
        else {
            continue;
        };
        let mut addresses = Vec::new();
        let mut stars = 0;
        for token in tokens {
            if token == "*" {
                stars += 1;
            }
            if let Ok(ip) = token.trim_matches(['[', ']']).parse::<std::net::Ipv4Addr>() {
                addresses.push(ip);
            }
        }
        if !addresses.is_empty() || stars >= 3 {
            hops.insert(
                hop,
                (
                    !addresses.is_empty(),
                    addresses
                        .iter()
                        .any(|ip| ip.to_string() == TRACE_DESTINATION),
                ),
            );
        }
    }
    TraceCounts {
        observed: hops.len(),
        responded: hops.values().filter(|(response, _)| *response).count(),
        unanswered: hops.values().filter(|(response, _)| !*response).count(),
        reached: hops.values().any(|(_, reached)| *reached),
    }
}
fn decode_xml(bytes: &[u8]) -> Option<String> {
    let utf16 =
        bytes.starts_with(&[0xff, 0xfe]) || (bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0);
    if utf16 {
        let bytes = if bytes.starts_with(&[0xff, 0xfe]) {
            &bytes[2..]
        } else {
            bytes
        };
        if bytes.len() % 2 != 0 {
            return None;
        }
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()
    } else {
        std::str::from_utf8(bytes)
            .ok()
            .map(|s| s.trim_start_matches('\u{feff}').to_owned())
    }
}
fn count_events(xml: &str) -> Option<usize> {
    // wevtutil's XML writer escapes event data; count actual Event elements only.
    // Require the wrapper and all closings so truncated output cannot imply zero.
    if !xml.contains("<Events>") || !xml.contains("</Events>") || xml.contains("<!DOCTYPE") {
        return None;
    }
    let count = xml.matches("<Event ").count() + xml.matches("<Event>").count();
    if count > MAX_EVENTS || count != xml.matches("</Event>").count() {
        None
    } else {
        Some(count)
    }
}
struct ControllerCounts {
    total: u64,
    problems: u64,
    unknown: u64,
}
fn parse_controller_counts(bytes: &[u8]) -> Option<ControllerCounts> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let total = value["total"].as_u64()?;
    let problems = value["problems"].as_u64()?;
    let unknown = value["unknown"].as_u64()?;
    if total > 10000 || problems > total || unknown > total - problems {
        return None;
    }
    Some(ControllerCounts {
        total,
        problems,
        unknown,
    })
}
fn capture(buffer: &mut Vec<u8>, bytes: &[u8], truncated: &mut bool) {
    let remaining = OUTPUT_LIMIT.saturating_sub(buffer.len());
    buffer.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
    *truncated |= bytes.len() > remaining;
}

#[cfg(windows)]
fn run(check: Check, cancel: &AtomicBool) -> Result<CommandOutput, String> {
    use std::{
        ffi::c_void,
        io::Read,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Command, Stdio},
        time::Instant,
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
        fn PeekNamedPipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Test cancelled".into());
    }
    let mut directory = [0u16; 32768];
    let length =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    if length == 0 || length >= directory.len() {
        return Err("Windows system directory unavailable".into());
    }
    use std::os::windows::ffi::OsStringExt;
    let system_directory =
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&directory[..length]));
    let executable = system_directory.join(check.program());
    let mut child = Command::new(executable)
        .args(check.args())
        .current_dir(system_directory)
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Windows check could not start (missing tool or access denied)".to_string())?;
    let Some(mut pipe) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Windows check output unavailable".into());
    };
    let started = Instant::now();
    let mut output = CommandOutput {
        stdout: Vec::with_capacity(8192),
        truncated: false,
        exit_code: None,
        timed_out: false,
        elapsed_ms: 0.,
    };
    let outcome = (|| {
        let mut exit = None;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Test cancelled".into());
            }
            if started.elapsed() >= check.timeout() {
                output.timed_out = true;
                return Ok(());
            }
            // Poll and drain only available bytes: no blocking reader thread and
            // no wait-for-exit while the child can be blocked on a full stdout.
            let mut drained = 0;
            loop {
                let mut available = 0u32;
                let ok = unsafe {
                    PeekNamedPipe(
                        pipe.as_raw_handle(),
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    if matches!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(109 | 232)
                    ) {
                        break;
                    }
                    return Err("Windows check output could not be read".into());
                }
                if available == 0 || drained >= OUTPUT_LIMIT {
                    break;
                }
                let mut chunk = [0u8; 8192];
                let count = pipe
                    .read(&mut chunk[..(available as usize).min(8192)])
                    .map_err(|_| "Windows check output could not be read")?;
                if count == 0 {
                    break;
                }
                capture(&mut output.stdout, &chunk[..count], &mut output.truncated);
                drained += count;
            }
            if let Some(status) = exit {
                if drained == 0 {
                    output.exit_code = status;
                    return Ok(());
                }
            } else {
                exit = child
                    .try_wait()
                    .map_err(|_| "Windows check process status unavailable")?
                    .map(|s| s.code());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
    }
    let _ = child.wait();
    output.elapsed_ms = started.elapsed().as_secs_f64() * 1000.;
    outcome.map(|_| output)
}
#[cfg(not(windows))]
fn run(_: Check, cancel: &AtomicBool) -> Result<CommandOutput, String> {
    Err(if cancel.load(Ordering::Relaxed) {
        "Test cancelled"
    } else {
        "Windows system checks are unavailable on this platform"
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_trace_is_anonymized_and_timeout_is_partial() {
        let output = CommandOutput {
            stdout: b"1 1 ms 1 ms 1 ms 192.168.0.1\n2 * * * Request timed out.\n".to_vec(),
            truncated: false,
            exit_code: None,
            timed_out: true,
            elapsed_ms: 15001.,
        };
        let summary = trace_summary(output);
        assert_eq!(summary.status, "partial");
        assert_eq!(summary.measurements["respondingHops"], 1);
        assert_eq!(summary.measurements["timedOut"], true);
        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(!serialized.contains("192.168.0.1"));
        assert!(!serialized.contains("1.1.1.1"));
        assert!(!serialized.contains("averageRtt"));
        let unknown = trace_summary(CommandOutput {
            stdout: b"Permission denied".to_vec(),
            truncated: false,
            exit_code: Some(1),
            timed_out: false,
            elapsed_ms: 10.,
        });
        assert_eq!(unknown.status, "unavailable");
        assert!(unknown.measurements["observedHops"].is_null());
        assert!(unknown.measurements["destinationReached"].is_null());
    }
    #[test]
    fn trace_counts_do_not_store_routes_or_invent_latency() {
        let text="Tracing route to 1.1.1.1\r\n 1 <1 ms 1 ms 1 ms 192.168.0.1\r\n 2 * * * Request timed out.\r\n 3 10 ms * 11 ms 1.1.1.1\r\n";
        let count = parse_trace(text);
        assert_eq!(
            (
                count.observed,
                count.responded,
                count.unanswered,
                count.reached
            ),
            (3, 2, 1, true)
        );
        let empty = parse_trace("Tracing route to 1.1.1.1\nMaximum of 8 hops");
        assert_eq!((empty.observed, empty.reached), (0, false));
        assert_eq!(parse_trace("9 1 ms 1 ms 1 ms 1.1.1.1").observed, 0);
    }
    #[test]
    fn command_policy_is_fixed_read_only_and_bounded() {
        assert_eq!(
            Check::Traceroute.args(),
            vec!["/d", "/4", "/h", "8", "/w", "500", "1.1.1.1"]
        );
        assert_eq!(Check::Events.args()[0], "qe");
        assert!(EVENT_QUERY.contains("86400000"));
        assert!(Check::Events.args().contains(&"/c:20"));
        for check in [Check::Traceroute, Check::Events, Check::Controllers] {
            assert!(check.timeout() <= Duration::from_secs(15));
            assert!(!check.program().contains("cmd.exe"));
        }
        assert!(CONTROLLER_SCRIPT.contains("SELECT ConfigManagerErrorCode FROM"));
        assert!(!CONTROLLER_SCRIPT.contains("DeviceID"));
    }
    #[test]
    fn capture_remains_bounded_while_accepting_drain_chunks() {
        let mut bytes = vec![];
        let mut truncated = false;
        for _ in 0..100 {
            capture(&mut bytes, &[b'x'; 8192], &mut truncated);
        }
        assert_eq!(bytes.len(), OUTPUT_LIMIT);
        assert!(truncated);
    }
    #[test]
    fn event_query_empty_is_distinct_from_inaccessible_or_truncated() {
        assert_eq!(count_events("<Events></Events>"), Some(0));
        assert_eq!(
            count_events("<Events><Event xmlns='x'><System/></Event></Events>"),
            Some(1)
        );
        assert_eq!(count_events("<Events><Event xmlns='x'>"), None);
        assert_eq!(count_events("Access denied"), None);
        let data = "<Events><Event><Data>&lt;Event&gt;PRIVATE_HOST</Data></Event></Events>";
        assert_eq!(count_events(data), Some(1));
        assert_eq!(
            count_events(&format!(
                "<Events>{}</Events>",
                "<Event></Event>".repeat(21)
            )),
            None
        );
    }
    #[test]
    fn unicode_event_output_is_decoded_without_retaining_identifiers() {
        let text = "<Events><Event><Computer>PRIVATE_HOST</Computer></Event></Events>";
        let bytes: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(count_events(&decode_xml(&bytes).unwrap()), Some(1));
        assert!(decode_xml(&[0xff, 0xfe, 0x3c]).is_none());
    }
    #[test]
    fn invalid_controller_counts_are_unavailable_instead_of_healthy() {
        let count = parse_controller_counts(br#"{"total":12,"problems":2,"unknown":1}"#).unwrap();
        assert_eq!((count.total, count.problems, count.unknown), (12, 2, 1));
        for bytes in [
            br#"{"total":1,"problems":2,"unknown":0}"#.as_slice(),
            b"Access denied",
            br#"{"total":1,"problems":0}"#,
            br#"{"total":-1,"problems":0,"unknown":0}"#,
        ] {
            assert!(parse_controller_counts(bytes).is_none());
        }
    }
    #[test]
    fn pre_cancel_never_starts_a_command() {
        assert!(traceroute(&AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled"));
        assert!(motherboard(&AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled"));
    }
}
