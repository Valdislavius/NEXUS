use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};

// Reports are constructed from an allowlist. Never serialize the full snapshot:
// it contains host names, network addresses, adapter IDs and OEM specifications.
fn safe_status(metric: &Value) -> &'static str {
    if !metric["value"].is_null() {
        return "available";
    }
    match metric["diagnostic"]["status"].as_str().unwrap_or("") {
        "unsupported" => "unsupported",
        "permission_denied" => "permission_denied",
        "dependency_missing" => "dependency_missing",
        "device_unresponsive" => "device_unresponsive",
        "invalid_response" => "invalid_response",
        "transient_error" => "transient_error",
        _ => "driver_no_data",
    }
}

fn safe_code(raw: &str) -> &str {
    const CODES: &[&str] = &[
        "VALUE_INVALID",
        "ATA_SHORT_RESPONSE",
        "ATA_INVALID_LAYOUT",
        "ATA_INVALID_STATUS",
        "ATA_SMART_CHECKSUM",
        "ATA_SMART_REVISION",
        "ATA_SMART_DUPLICATE_ID",
        "ATA_SMART_EMPTY",
        "ATA_IDENTIFY_INVALID",
        "ATA_SMART_DISABLED",
        "ATA_SMART_UNSUPPORTED",
        "ATA_BUS_UNSUPPORTED",
        "NVME_INVALID_RESPONSE",
        "COLLECTOR_STOPPED",
        "SNAPSHOT_STALE",
        "GENERIC_NO_DATA",
        "CPU_VENDOR_UNSUPPORTED",
        "CPU_ARCHITECTURE_UNSUPPORTED",
        "CPU_PROCESSOR_GROUPS_UNSUPPORTED",
        "CPU_AFFINITY_QUERY_FAILED",
        "CPU_AFFINITY_SET_FAILED",
        "CPU_READ_FAILED",
        "CPU_MULTIPLE_PACKAGES_UNSUPPORTED",
        "CPU_MSR_NOT_PERMITTED",
        "INTEL_CPUID_UNSUPPORTED",
        "INTEL_PACKAGE_SENSOR_ABSENT",
        "INTEL_PACKAGE_INVALID",
        "PAWNIO_MSR_RESPONSE_LENGTH",
        "PAWNIO_OPEN_FAILED",
        "PAWNIO_LOAD_FAILED",
        "PAWNIO_READ_TARGET_FAILED",
        "PAWNIO_READ_STATUS_FAILED",
        "AMD_SMN_SELECTOR_WRITE_REQUIRED",
        "AMD_FAMILY_UNVERIFIED",
        "ATA_READ_FAILED",
        "ATA_COMMAND_NOT_ALLOWED",
        "DISK_NO_CURRENT_DATA",
        "NVML_VALUE_INVALID",
    ];
    if CODES.contains(&raw) {
        return raw;
    }
    if let Some(n) = raw.strip_prefix("NVML_") {
        if let Ok(n) = n.parse::<u32>() {
            if (1..=30).contains(&n) || n == 999 {
                return raw;
            }
        }
    }
    if let Some(n) = raw.strip_prefix("WIN32_") {
        if !n.is_empty()
            && n.len() <= 10
            && n.bytes().all(|c| c.is_ascii_digit())
            && n.parse::<u32>().is_ok()
        {
            return raw;
        }
    }
    if raw.is_ascii()
        && raw.len() == 22
        && raw.starts_with("ATA_STATUS_")
        && &raw[13..20] == "_ERROR_"
        && raw[11..13]
            .bytes()
            .chain(raw[20..].bytes())
            .all(|c| c.is_ascii_hexdigit())
    {
        return raw;
    }
    "PROVIDER_ERROR"
}

fn safe_sensor(raw: &str) -> &'static str {
    match raw {
        "temperature" => "temperature",
        "load" => "load",
        "frequency" => "frequency",
        "used" | "memoryUsed" => "memory_used",
        "total" | "memoryTotal" => "memory_total",
        "available" | "free" => "memory_or_storage_free",
        "read" => "read",
        "write" => "write",
        "power" => "power",
        "fan" => "fan",
        "uptime" => "uptime",
        "smart-failure" => "smart_failure",
        "smart-warning" => "smart_warning",
        "ata-smart" => "ata_smart",
        "smart-data" => "smart_data",
        "smart-thresholds" => "smart_thresholds",
        "power-hours" => "power_hours",
        "power-cycles" => "power_cycles",
        "wear-leveling" => "wear_leveling",
        "lifetime-written" => "lifetime_written",
        "lifetime-read" => "lifetime_read",
        "reallocated" | "reallocated-blocks" => "reallocated",
        "pending-sectors" => "pending_sectors",
        "uncorrectable-errors" | "offline-uncorrectable" => "uncorrectable_errors",
        "sata-crc-errors" => "sata_crc_errors",
        "remaining-life" | "wear-indicator" | "wear" => "wear_indicator",
        _ if raw.starts_with("smart-") || raw.starts_with("ata-") => "smart_attribute",
        _ => "other",
    }
}

fn timestamp(snapshot: Option<&Value>) -> String {
    let raw = snapshot.and_then(|s| s["at"].as_str()).unwrap_or("");
    if (20..=40).contains(&raw.len())
        && raw
            .bytes()
            .all(|c| c.is_ascii_digit() || b"-:.TZ+".contains(&c))
    {
        raw.into()
    } else {
        "unavailable".into()
    }
}

pub fn sensors(snapshot: Option<&Value>, running: bool, fresh: bool) -> Vec<Value> {
    let mut counts = HashMap::<&str, usize>::new();
    let mut result = Vec::new();
    if let Some(components) = snapshot.and_then(|s| s["components"].as_array()) {
        for component in components {
            let kind = match component["kind"].as_str().unwrap_or("") {
                "cpu" => "cpu",
                "gpu" => "gpu",
                "storage" => "storage",
                "ram" => "ram",
                "motherboard" => "motherboard",
                "network" => "network",
                "system" => "system",
                _ => "other",
            };
            let ordinal = counts.entry(kind).or_default();
            *ordinal += 1;
            if let Some(metrics) = component["metrics"].as_array() {
                for metric in metrics {
                    let sensor = safe_sensor(metric["id"].as_str().unwrap_or(""));
                    let source = match metric["source"].as_str().unwrap_or("") {
                        "ATA SMART" => "ATA SMART",
                        "Intel MSR через PawnIO" | "Intel CPU Package · PawnIO / MSR 0x1B1" => {
                            "Intel MSR / PawnIO"
                        }
                        "AMD CPU adapter" | "AMD CPU · CPUID / SMN compatibility" => {
                            "AMD CPU adapter"
                        }
                        "NVMe SMART" => "NVMe SMART",
                        "NVIDIA NVML" => "NVIDIA NVML",
                        _ => match kind {
                            "cpu" => "CPU provider",
                            "gpu" => "GPU provider",
                            "storage" => "Windows storage provider",
                            _ => "Windows provider",
                        },
                    };
                    let status = if !running || !fresh {
                        "transient_error"
                    } else {
                        safe_status(metric)
                    };
                    let code = if !running {
                        "COLLECTOR_STOPPED"
                    } else if !fresh {
                        "SNAPSHOT_STALE"
                    } else {
                        safe_code(
                            metric["diagnostic"]["code"]
                                .as_str()
                                .unwrap_or("GENERIC_NO_DATA"),
                        )
                    };
                    let mut entry = json!({"component":format!("{kind}-{ordinal}"),"sensor":sensor,"source":source,"status":status});
                    if status != "available" {
                        entry["code"] = json!(code);
                    }
                    result.push(entry);
                }
            }
        }
    }
    result
}

#[derive(Default)]
pub struct DiagnosticLog {
    entries: VecDeque<Value>,
    previous: Vec<Value>,
}
impl DiagnosticLog {
    pub fn record(&mut self, snapshot: &Value) {
        let current = sensors(Some(snapshot), true, true);
        for (index, sensor) in current.iter().enumerate() {
            if sensor["status"] != "available" && self.previous.get(index) != Some(sensor) {
                let mut entry = sensor.clone();
                entry["at"] = json!(timestamp(Some(snapshot)));
                if self.entries.len() == 128 {
                    self.entries.pop_front();
                }
                self.entries.push_back(entry);
            }
        }
        self.previous = current;
    }
    pub fn report(&self, snapshot: Option<&Value>, running: bool, fresh: bool) -> Value {
        json!({"version":1,"type":"diagnostics","appVersion":env!("CARGO_PKG_VERSION"),
            "at":timestamp(snapshot),"privacy":{"serialNumbers":false,"userIdentifiers":false,"ipAddresses":false},
            "collector":{"running":running,"snapshotReady":running && fresh && snapshot.is_some_and(|s| s["ready"] == true)},
            "sensors":sensors(snapshot,running,fresh),"log":self.entries})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_excludes_identifiers_and_provider_text() {
        let snapshot = json!({"at":"2026-10-08T10:00:00Z","host":"PRIVATE_HOST","ready":true,"warnings":["PRIVATE_WARNING"],
            "components":[{"kind":"cpu","id":"PRIVATE_ID","name":"PRIVATE_MODEL","specs":[{"value":"PRIVATE_SERIAL"}],
            "metrics":[{"id":"PRIVATE_SENSOR","value":null,"source":"PRIVATE_SOURCE","reason":"PRIVATE_IP",
                "diagnostic":{"status":"permission_denied","code":"PRIVATE_CODE"}}]}]});
        let mut log = DiagnosticLog::default();
        log.record(&snapshot);
        log.record(&snapshot);
        let report = log.report(Some(&snapshot), true, true);
        assert!(!report.to_string().contains("PRIVATE"));
        assert_eq!(report["log"].as_array().unwrap().len(), 1);
        assert_eq!(report["sensors"][0]["code"], "PROVIDER_ERROR");
    }
    #[test]
    fn stopped_collector_never_exports_available_sensor() {
        let snapshot = json!({"ready":true,"components":[{"kind":"storage","metrics":[{"id":"temperature","value":34}]}]});
        let report = DiagnosticLog::default().report(Some(&snapshot), false, true);
        assert_eq!(report["sensors"][0]["status"], "transient_error");
        assert_eq!(report["collector"]["snapshotReady"], false);
    }
    #[test]
    fn codes_are_bounded_and_log_is_capped() {
        assert_eq!(safe_code("WIN32_5"), "WIN32_5");
        assert_eq!(safe_code("WIN32_4294967296"), "PROVIDER_ERROR");
        assert_eq!(
            safe_code("ATA_STATUS_51_ERROR_04"),
            "ATA_STATUS_51_ERROR_04"
        );
        let mut log = DiagnosticLog::default();
        for n in 0..150 {
            log.record(&json!({"components":[{"kind":"cpu","metrics":[{"id":"temperature","value":null,"diagnostic":{"status":"transient_error","code":format!("WIN32_{n}")}}]}]}));
        }
        assert_eq!(log.entries.len(), 128);
    }
}
