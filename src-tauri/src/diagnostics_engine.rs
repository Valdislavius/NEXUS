//! Diagnostics consume collected metrics only; they never probe devices.
use crate::{
    settings::Thresholds,
    storage::{now_ms, parse_ms, Event},
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub struct Engine {
    active: HashMap<String, Value>,
    seen_devices: HashSet<String>,
    previous: HashMap<String, (i64, f64)>,
    last: Value,
}
impl Engine {
    pub fn evaluate(&mut self, snapshot: &Value, thresholds: &Thresholds) -> Vec<Event> {
        let now = now_ms();
        let mut findings = Vec::new();
        let mut events = Vec::new();
        let mut devices = HashSet::new();
        let mut available = 0;
        let mut total = 0;
        if let Some(components) = snapshot["components"].as_array() {
            for c in components {
                let component = c["id"].as_str().unwrap_or("");
                let kind = c["kind"].as_str().unwrap_or("other");
                devices.insert(component.to_string());
                if let Some(metrics) = c["metrics"].as_array() {
                    for m in metrics {
                        total += 1;
                        let metric = m["id"].as_str().unwrap_or("");
                        let key = format!("{component}.{metric}");
                        let at = m["at"].as_str().and_then(parse_ms).unwrap_or(0);
                        let ttl = m["maxAgeMs"].as_i64().unwrap_or(if kind == "storage" {
                            180_000
                        } else {
                            20_000
                        });
                        let Some(value) = m["value"]
                            .as_f64()
                            .filter(|v| v.is_finite() && now - at <= ttl && at <= now + 60000)
                        else {
                            continue;
                        };
                        available += 1;
                        let threshold = if metric == "temperature" {
                            match kind {
                                "cpu" => Some(thresholds.cpu_temperature),
                                "gpu" => Some(thresholds.gpu_temperature),
                                "storage" => Some(thresholds.storage_temperature),
                                _ => None,
                            }
                        } else if kind == "ram" && metric == "load" {
                            Some(thresholds.memory_load)
                        } else {
                            None
                        };
                        if let Some(limit) = threshold.filter(|limit| value >= *limit) {
                            let severity = if metric == "temperature" && value >= limit + 10. {
                                "critical"
                            } else {
                                "warning"
                            };
                            findings.push(json!({"id":format!("threshold:{key}"),"componentId":component,"componentKind":kind,"severity":severity,"title":if metric=="temperature"{"Повышенная температура"}else{"Недостаточно свободной памяти"},"atMs":at,"actual":{"value":value,"unit":m["unit"],"threshold":limit},"reason":format!("Показание {value:.1} достигло установленного порога {limit:.1}"),"explanations":["Нагрузка или условия охлаждения могут объяснять показание; аппаратная неисправность этим не подтверждается."],"actions":if metric=="temperature"{vec!["Снизьте нагрузку и проверьте охлаждение.","Сопоставьте температуру с частотой и загрузкой."]}else{vec!["Проверьте потребление памяти приложениями.","Закройте ненужные программы."]}}));
                        }
                        if [
                            "smart-failure",
                            "smart-warning",
                            "pending-sectors",
                            "uncorrectable-errors",
                            "offline-uncorrectable",
                        ]
                        .contains(&metric)
                            && value > 0.
                        {
                            findings.push(json!({"id":format!("health:{key}"),"componentId":component,"componentKind":kind,"severity":if metric=="smart-failure"{"critical"}else{"warning"},"title":"Предупреждение состояния накопителя","atMs":at,"actual":{"value":value,"unit":m["unit"]},"reason":format!("Провайдер сообщил ненулевой показатель {metric}"),"explanations":["Значение требует проверки документации конкретного накопителя; счётчик не доказывает текущую неисправность."],"actions":["Создайте резервную копию важных данных.","Проверьте накопитель утилитой производителя."]}));
                        }
                        if metric == "temperature" {
                            if let Some((previous_at, previous)) = self.previous.get(&key) {
                                if at > *previous_at
                                    && at - previous_at < 15_000
                                    && (value - previous).abs() > 35.
                                {
                                    findings.push(json!({"id":format!("jump:{key}"),"componentId":component,"componentKind":kind,"severity":"warning","title":"Резкое изменение показания датчика","atMs":at,"actual":{"value":value,"previous":previous,"unit":m["unit"]},"reason":"Изменение температуры более 35 °C менее чем за 15 секунд","explanations":["Возможны изменение нагрузки, переключение источника или нестабильная телеметрия."],"actions":["Сравните с другими датчиками и повторными показаниями."]}));
                                }
                            }
                            self.previous.insert(key, (at, value));
                        }
                    }
                }
            }
        }
        // Preserve unresolved findings while their source disappears. Missing
        // data cannot turn an overheating warning into a reassuring green state.
        for (id, previous) in &self.active {
            if !findings.iter().any(|f| f["id"] == *id) {
                let component = previous["componentId"].as_str().unwrap_or("");
                let metric = id.rsplit('.').next().unwrap_or("");
                let readable = snapshot["components"].as_array().is_some_and(|cs| {
                    cs.iter().any(|c| {
                        c["id"] == component
                            && c["metrics"].as_array().is_some_and(|ms| {
                                ms.iter().any(|m| {
                                    m["id"] == metric
                                        && m["value"].as_f64().is_some()
                                        && m["at"].as_str().and_then(parse_ms).is_some_and(|at| {
                                            now - at <= m["maxAgeMs"].as_i64().unwrap_or(20_000)
                                        })
                                })
                            })
                    })
                });
                if !readable {
                    let mut pending = previous.clone();
                    pending["telemetryUnavailable"] = json!(true);
                    findings.push(pending);
                }
            }
        }
        let current: HashMap<String, Value> = findings
            .iter()
            .filter_map(|f| Some((f["id"].as_str()?.into(), f.clone())))
            .collect();
        for (id, f) in &current {
            if self
                .active
                .get(id)
                .is_none_or(|old| old["severity"] != f["severity"])
            {
                events.push(Event {
                    id: format!("diag:{id}:{now}"),
                    at_ms: now,
                    source: "diagnostics".into(),
                    component_id: f["componentId"].as_str().unwrap_or("").into(),
                    severity: f["severity"].as_str().unwrap_or("warning").into(),
                    title: f["title"].as_str().unwrap_or("").into(),
                    details: f["reason"].as_str().unwrap_or("").into(),
                    data: f["actual"].clone(),
                });
            }
        }
        // Missing telemetry is not recovery. Resolve only while that exact sensor
        // is currently available; never turn a missing temperature into "normal".
        for (id, f) in &self.active {
            if !current.contains_key(id) {
                let component = f["componentId"].as_str().unwrap_or("");
                let metric = id
                    .split_once(':')
                    .map(|(_, k)| k.rsplit('.').next().unwrap_or(""))
                    .unwrap_or("");
                let readable = snapshot["components"].as_array().is_some_and(|cs| {
                    cs.iter().any(|c| {
                        c["id"] == component
                            && c["metrics"].as_array().is_some_and(|ms| {
                                ms.iter().any(|m| {
                                    m["id"] == metric
                                        && m["value"].as_f64().is_some()
                                        && m["at"]
                                            .as_str()
                                            .and_then(parse_ms)
                                            .is_some_and(|at| now - at <= 20_000)
                                })
                            })
                    })
                });
                if readable {
                    events.push(Event {
                        id: format!("resolved:{id}:{now}"),
                        at_ms: now,
                        source: "diagnostics".into(),
                        component_id: component.into(),
                        severity: "info".into(),
                        title: "Показатель вернулся в допустимый диапазон".into(),
                        details: f["title"].as_str().unwrap_or("").into(),
                        data: json!({"rule":id}),
                    });
                }
            }
        }
        if !self.seen_devices.is_empty() {
            for id in devices.symmetric_difference(&self.seen_devices) {
                events.push(Event {
                    id: format!("device:{id}:{now}"),
                    at_ms: now,
                    source: "hardware".into(),
                    component_id: id.clone(),
                    severity: "info".into(),
                    title: if devices.contains(id) {
                        "Устройство обнаружено"
                    } else {
                        "Устройство больше не представлено в телеметрии"
                    }
                    .into(),
                    details: "Изменился состав собранных компонентов".into(),
                    data: json!({}),
                });
            }
        }
        self.seen_devices = devices;
        self.active = current;
        let state = if available == 0 {
            "insufficient_data"
        } else if findings
            .iter()
            .any(|f| f["severity"] == "critical" && f["telemetryUnavailable"] != true)
        {
            "critical"
        } else if findings.iter().any(|f| f["telemetryUnavailable"] != true) {
            "warning"
        } else if !findings.is_empty() {
            "insufficient_data"
        } else {
            "normal"
        };
        self.last = json!({"atMs":now,"state":state,"findings":findings,"availableSensors":available,"totalSensors":total,"fresh":true});
        events
    }
    pub fn current(&self, fresh: bool) -> Value {
        if self.last.is_null() {
            return json!({"atMs":now_ms(),"state":"insufficient_data","findings":[],"availableSensors":0,"totalSensors":0,"fresh":false});
        }
        let mut last = self.last.clone();
        last["fresh"] = json!(fresh);
        if !fresh {
            last["state"] = json!("insufficient_data");
            last["availableSensors"] = json!(0);
        }
        last
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_events_are_transitions_and_missing_is_not_recovery() {
        let mut e = Engine::default();
        let at = chrono::DateTime::from_timestamp_millis(now_ms())
            .unwrap()
            .to_rfc3339();
        let mut s = json!({"components":[{"id":"cpu","kind":"cpu","metrics":[{"id":"temperature","at":at,"value":95,"unit":"°C"}]}]});
        assert_eq!(e.evaluate(&s, &Thresholds::default()).len(), 1);
        assert!(e.evaluate(&s, &Thresholds::default()).is_empty());
        s["components"][0]["metrics"][0]["value"] = Value::Null;
        assert!(e.evaluate(&s, &Thresholds::default()).is_empty());
        assert_eq!(e.current(true)["findings"].as_array().unwrap().len(), 1);
        assert_eq!(e.current(true)["state"], "insufficient_data");
        assert_eq!(e.current(false)["state"], "insufficient_data");
        s["components"][0]["metrics"][0]["value"] = json!(45.);
        assert_eq!(e.evaluate(&s, &Thresholds::default()).len(), 1);
        assert_eq!(e.current(true)["state"], "normal");
    }
    #[test]
    fn stale_values_cannot_trigger_findings() {
        let mut e = Engine::default();
        let s = json!({"components":[{"id":"cpu","kind":"cpu","metrics":[{"id":"temperature","at":"2020-01-01T00:00:00Z","value":150,"unit":"°C"}]}]});
        assert!(e.evaluate(&s, &Thresholds::default()).is_empty());
        assert_eq!(e.current(true)["availableSensors"], 0);
    }
}
