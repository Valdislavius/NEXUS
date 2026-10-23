//! Shared availability checks for already-collected sensor readings.
use serde_json::Value;

pub fn reading(metric: &Value, reference_ms: i64, default_ttl_ms: i64) -> Option<(f64, i64)> {
    if metric["status"].as_str().is_some_and(|s| s != "available") {
        return None;
    }
    let value = metric["value"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.)?;
    let at = metric["at"].as_str().and_then(crate::storage::parse_ms)?;
    let ttl = metric["maxAgeMs"].as_i64().unwrap_or(default_ttl_ms);
    if ttl <= 0 || at > reference_ms || reference_ms.saturating_sub(at) > ttl {
        return None;
    }
    Some((value, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_explicit_missing_values_and_future_or_expired_readings() {
        let now = 1_800_000_000_000;
        let mut metric = json!({"at":chrono::DateTime::from_timestamp_millis(now).unwrap().to_rfc3339(),"value":0.,"maxAgeMs":5000});
        assert_eq!(reading(&metric, now, 20000), Some((0., now)));
        for status in ["stale", "unavailable", "invalid"] {
            metric["status"] = json!(status);
            assert!(reading(&metric, now, 20000).is_none());
        }
        metric["status"] = json!("available");
        assert!(reading(&metric, now - 1, 20000).is_none());
        assert!(reading(&metric, now + 5001, 20000).is_none());
        metric["maxAgeMs"] = json!(120000);
        assert!(reading(&metric, now + 90000, 20000).is_some());
        metric["value"] = json!(-1.);
        assert!(reading(&metric, now, 20000).is_none());
    }
}
