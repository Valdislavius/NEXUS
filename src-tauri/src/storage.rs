//! Native, bounded history. Windows ships winsqlite3; no daemon or extra runtime.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_int, c_void, CStr, CString},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn parse_ms(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|t| t.timestamp_millis())
}

#[link(name = "winsqlite3")]
unsafe extern "C" {
    fn sqlite3_open_v2(
        name: *const c_char,
        db: *mut *mut c_void,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_errmsg(db: *mut c_void) -> *const c_char;
    fn sqlite3_exec(
        db: *mut c_void,
        sql: *const c_char,
        callback: *const c_void,
        arg: *mut c_void,
        err: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        size: c_int,
        stmt: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_reset(stmt: *mut c_void) -> c_int;
    fn sqlite3_clear_bindings(stmt: *mut c_void) -> c_int;
    fn sqlite3_bind_null(stmt: *mut c_void, index: c_int) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut c_void, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_double(stmt: *mut c_void, index: c_int, value: f64) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut c_void,
        index: c_int,
        value: *const c_char,
        len: c_int,
        destructor: isize,
    ) -> c_int;
    fn sqlite3_column_count(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_type(stmt: *mut c_void, index: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut c_void, index: c_int) -> i64;
    fn sqlite3_column_double(stmt: *mut c_void, index: c_int) -> f64;
    fn sqlite3_column_text(stmt: *mut c_void, index: c_int) -> *const u8;
    fn sqlite3_busy_timeout(db: *mut c_void, milliseconds: c_int) -> c_int;
}
pub struct Database(*mut c_void);
// Connections are used only through History's Mutex; FULLMUTEX is an extra guard.
unsafe impl Send for Database {}
impl Drop for Database {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close(self.0);
        }
    }
}
struct Statement(*mut c_void);
impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.0);
        }
    }
}
impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        let name =
            CString::new(path.to_string_lossy().as_bytes()).map_err(|_| "Invalid database path")?;
        let mut raw = std::ptr::null_mut();
        let code = unsafe {
            sqlite3_open_v2(
                name.as_ptr(),
                &mut raw,
                0x0002 | 0x0004 | 0x10000,
                std::ptr::null(),
            )
        };
        if raw.is_null() {
            return Err(format!("SQLite open failed ({code})"));
        }
        let db = Self(raw);
        if code != 0 {
            return Err(db.error());
        }
        unsafe {
            sqlite3_busy_timeout(db.0, 250);
        }
        db.exec("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA cache_size=-2048; PRAGMA wal_autocheckpoint=256;")?;
        Ok(db)
    }
    fn error(&self) -> String {
        unsafe {
            CStr::from_ptr(sqlite3_errmsg(self.0))
                .to_string_lossy()
                .into_owned()
        }
    }
    pub fn exec(&self, sql: &str) -> Result<(), String> {
        let text = CString::new(sql).map_err(|_| "Invalid SQL")?;
        if unsafe {
            sqlite3_exec(
                self.0,
                text.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0
        {
            return Err(self.error());
        }
        Ok(())
    }
    fn prepare(&self, sql: &str) -> Result<Statement, String> {
        let text = CString::new(sql).map_err(|_| "Invalid SQL")?;
        let mut raw = std::ptr::null_mut();
        if unsafe { sqlite3_prepare_v2(self.0, text.as_ptr(), -1, &mut raw, std::ptr::null_mut()) }
            != 0
        {
            return Err(self.error());
        }
        Ok(Statement(raw))
    }
    fn bind(&self, stmt: &Statement, args: &[Value]) -> Result<(), String> {
        for (i, value) in args.iter().enumerate() {
            let index = (i + 1) as i32;
            let code = unsafe {
                match value {
                    Value::Null => sqlite3_bind_null(stmt.0, index),
                    Value::Number(n) => {
                        if let Some(n) = n.as_i64() {
                            sqlite3_bind_int64(stmt.0, index, n)
                        } else {
                            sqlite3_bind_double(stmt.0, index, n.as_f64().ok_or("Invalid number")?)
                        }
                    }
                    Value::String(s) => {
                        sqlite3_bind_text(stmt.0, index, s.as_ptr().cast(), s.len() as i32, -1)
                    }
                    _ => return Err("Unsupported SQL argument".into()),
                }
            };
            if code != 0 {
                return Err(self.error());
            }
        }
        Ok(())
    }
    pub fn execute(&self, sql: &str, args: &[Value]) -> Result<(), String> {
        let stmt = self.prepare(sql)?;
        self.bind(&stmt, args)?;
        if unsafe { sqlite3_step(stmt.0) } != 101 {
            return Err(self.error());
        }
        Ok(())
    }
    pub fn each(
        &self,
        sql: &str,
        args: &[Value],
        mut visit: impl FnMut(Vec<Value>) -> Result<(), String>,
    ) -> Result<(), String> {
        let stmt = self.prepare(sql)?;
        self.bind(&stmt, args)?;
        loop {
            match unsafe { sqlite3_step(stmt.0) } {
                101 => return Ok(()),
                100 => {
                    let mut row = Vec::new();
                    for i in 0..unsafe { sqlite3_column_count(stmt.0) } {
                        let value = unsafe {
                            match sqlite3_column_type(stmt.0, i) {
                                1 => json!(sqlite3_column_int64(stmt.0, i)),
                                2 => json!(sqlite3_column_double(stmt.0, i)),
                                3 => {
                                    let p = sqlite3_column_text(stmt.0, i);
                                    json!(if p.is_null() {
                                        String::new()
                                    } else {
                                        CStr::from_ptr(p.cast()).to_string_lossy().into_owned()
                                    })
                                }
                                _ => Value::Null,
                            }
                        };
                        row.push(value);
                    }
                    visit(row)?;
                }
                _ => return Err(self.error()),
            }
        }
    }
    pub fn query(&self, sql: &str, args: &[Value]) -> Result<Vec<Vec<Value>>, String> {
        let mut rows = Vec::new();
        self.each(sql, args, |r| {
            rows.push(r);
            Ok(())
        })?;
        Ok(rows)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub at_ms: i64,
    pub source: String,
    pub component_id: String,
    pub severity: String,
    pub title: String,
    pub details: String,
    pub data: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub component_id: Option<String>,
    pub metric_ids: Option<Vec<String>>,
    pub from_ms: i64,
    pub to_ms: i64,
    pub max_points: Option<usize>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: Option<usize>,
}
#[derive(Clone)]
struct Sample {
    at: i64,
    component: String,
    kind: String,
    metric: String,
    unit: String,
    source: String,
    value: Option<f64>,
    status: String,
}
pub struct History {
    pub db: Database,
    pending: Vec<Sample>,
    last_flush: i64,
    last_maintenance: i64,
    pub recovery: Option<String>,
    pub dropped_samples: u64,
}
impl History {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut recovery = None;
        let db = match Database::open(path) {
            Ok(db) => db,
            Err(e) if e.contains("not a database") || e.contains("malformed") => {
                let suffix = format!("corrupt-{}", now_ms());
                let quarantined = path.with_extension(&suffix);
                std::fs::rename(path, &quarantined).map_err(|e| e.to_string())?;
                for extension in ["-wal", "-shm"] {
                    let side = std::path::PathBuf::from(format!("{}{extension}", path.display()));
                    if side.exists() {
                        let _ = std::fs::rename(&side, format!("{}.{}", side.display(), suffix));
                    }
                }
                recovery =
                    Some("A damaged database was preserved; a new history was created.".into());
                Database::open(path)?
            }
            Err(e) => return Err(e),
        };
        let version = db.query("PRAGMA user_version", &[])?[0][0]
            .as_i64()
            .unwrap_or(0);
        if version > 2 {
            return Err("History database schema is newer than this NEXUS build".into());
        }
        db.exec("CREATE TABLE IF NOT EXISTS samples(at_ms INTEGER NOT NULL,component_id TEXT NOT NULL,kind TEXT NOT NULL,metric_id TEXT NOT NULL,unit TEXT NOT NULL,source TEXT NOT NULL,value REAL,status TEXT NOT NULL,PRIMARY KEY(at_ms,component_id,metric_id)) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS samples_series ON samples(component_id,metric_id,at_ms);
        CREATE TABLE IF NOT EXISTS aggregates(bucket_ms INTEGER NOT NULL,component_id TEXT NOT NULL,kind TEXT NOT NULL,metric_id TEXT NOT NULL,unit TEXT NOT NULL,source TEXT NOT NULL,min REAL,max REAL,total REAL,count INTEGER NOT NULL,PRIMARY KEY(bucket_ms,component_id,metric_id,unit,source)) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS aggregates_series ON aggregates(component_id,metric_id,bucket_ms);
        CREATE TABLE IF NOT EXISTS timeline(id TEXT PRIMARY KEY,at_ms INTEGER NOT NULL,source TEXT NOT NULL,component_id TEXT NOT NULL,severity TEXT NOT NULL,title TEXT NOT NULL,details TEXT NOT NULL,data TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS timeline_time ON timeline(at_ms);
        CREATE TABLE IF NOT EXISTS benchmarks(id TEXT PRIMARY KEY,started_at_ms INTEGER NOT NULL,result TEXT NOT NULL);
        ")?;
        if version == 1 {
            db.exec("BEGIN IMMEDIATE; CREATE TABLE aggregates_v2(bucket_ms INTEGER NOT NULL,component_id TEXT NOT NULL,kind TEXT NOT NULL,metric_id TEXT NOT NULL,unit TEXT NOT NULL,source TEXT NOT NULL,min REAL,max REAL,total REAL,count INTEGER NOT NULL,PRIMARY KEY(bucket_ms,component_id,metric_id,unit,source)) WITHOUT ROWID; INSERT INTO aggregates_v2 SELECT * FROM aggregates; DROP TABLE aggregates; ALTER TABLE aggregates_v2 RENAME TO aggregates; CREATE INDEX aggregates_series ON aggregates(component_id,metric_id,bucket_ms); PRAGMA user_version=2; COMMIT;")?;
        } else {
            db.exec("PRAGMA user_version=2")?;
        }
        Ok(Self {
            db,
            pending: Vec::new(),
            last_flush: now_ms(),
            last_maintenance: 0,
            recovery,
            dropped_samples: 0,
        })
    }
    pub fn record(&mut self, snapshot: &Value, retention_days: u32) -> Result<(), String> {
        let now = now_ms();
        if let Some(components) = snapshot["components"].as_array() {
            for component in components.iter().take(128) {
                if let Some(metrics) = component["metrics"].as_array() {
                    for metric in metrics.iter().take(256) {
                        let Some(at) = metric["at"].as_str().and_then(parse_ms) else {
                            continue;
                        };
                        if at > now + 60_000 || at < now - 86_400_000 {
                            continue;
                        }
                        let value = metric["value"].as_f64().filter(|v| v.is_finite());
                        if self.pending.len() >= 8192 {
                            self.pending.drain(..4096);
                            self.dropped_samples += 4096;
                        }
                        self.pending.push(Sample {
                            at,
                            component: component["id"].as_str().unwrap_or("").to_string(),
                            kind: component["kind"].as_str().unwrap_or("other").to_string(),
                            metric: metric["id"].as_str().unwrap_or("").to_string(),
                            unit: metric["unit"].as_str().unwrap_or("").to_string(),
                            source: metric["source"].as_str().unwrap_or("").to_string(),
                            value,
                            status: if value.is_some() {
                                "available".into()
                            } else {
                                metric["status"]
                                    .as_str()
                                    .unwrap_or("unavailable")
                                    .to_string()
                            },
                        });
                    }
                }
            }
        }
        if now - self.last_flush >= 15_000 || self.pending.len() >= 4096 {
            self.flush()?;
        }
        if now - self.last_maintenance >= 3_600_000 {
            self.flush()?;
            self.maintain(now, retention_days)?;
            self.last_maintenance = now;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.db.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let stmt = self
                .db
                .prepare("INSERT OR IGNORE INTO samples VALUES(?,?,?,?,?,?,?,?)")?;
            for s in &self.pending {
                self.db.bind(
                    &stmt,
                    &[
                        json!(s.at),
                        json!(s.component),
                        json!(s.kind),
                        json!(s.metric),
                        json!(s.unit),
                        json!(s.source),
                        json!(s.value),
                        json!(s.status),
                    ],
                )?;
                if unsafe { sqlite3_step(stmt.0) } != 101 {
                    return Err(self.db.error());
                }
                unsafe {
                    sqlite3_reset(stmt.0);
                    sqlite3_clear_bindings(stmt.0);
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            let _ = self.db.exec("ROLLBACK");
            return Err(e);
        }
        if let Err(e) = self.db.exec("COMMIT") {
            let _ = self.db.exec("ROLLBACK");
            return Err(e);
        }
        self.pending.clear();
        self.last_flush = now_ms();
        Ok(())
    }
    pub fn maintain(&self, now: i64, retention_days: u32) -> Result<(), String> {
        let raw_cutoff = (now - 86_400_000) / 60_000 * 60_000;
        let old = now - (retention_days.clamp(1, 90) as i64) * 86_400_000;
        self.db.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.db.execute("INSERT OR REPLACE INTO aggregates SELECT (at_ms/60000)*60000,component_id,kind,metric_id,unit,source,MIN(value),MAX(value),SUM(value),COUNT(value) FROM samples WHERE at_ms<? GROUP BY (at_ms/60000),component_id,metric_id,unit,source",&[json!(raw_cutoff)])?;
            self.db
                .execute("DELETE FROM samples WHERE at_ms<?", &[json!(raw_cutoff)])?;
            self.db
                .execute("DELETE FROM aggregates WHERE bucket_ms<?", &[json!(old)])?;
            self.db
                .execute("DELETE FROM timeline WHERE at_ms<?", &[json!(old)])?;
            self.db.exec("DELETE FROM benchmarks WHERE id NOT IN(SELECT id FROM benchmarks ORDER BY started_at_ms DESC LIMIT 500)")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.db.exec("ROLLBACK");
            return result;
        }
        if let Err(e) = self.db.exec("COMMIT") {
            let _ = self.db.exec("ROLLBACK");
            return Err(e);
        }
        Ok(())
    }
    pub fn history(&mut self, q: &HistoryQuery) -> Result<Value, String> {
        if q.from_ms < 0 || q.to_ms <= q.from_ms || q.to_ms - q.from_ms > 91 * 86_400_000 {
            return Err("Choose a valid period up to 90 days".into());
        }
        if q.metric_ids.as_ref().is_some_and(|m| m.len() > 32) {
            return Err("Select at most 32 metrics".into());
        }
        self.flush()?;
        let max = q.max_points.unwrap_or(300).clamp(16, 1200) as i64;
        let bucket = ((q.to_ms - q.from_ms) / max + 1).max(1);
        let mut filters = "t>=? AND t<=?".to_string();
        let mut args = vec![json!(q.from_ms), json!(q.to_ms)];
        if let Some(c) = &q.component_id {
            filters.push_str(" AND component_id=?");
            args.push(json!(c));
        }
        if let Some(ids) = &q.metric_ids {
            if !ids.is_empty() {
                filters.push_str(&format!(
                    " AND metric_id IN({})",
                    vec!["?"; ids.len()].join(",")
                ));
                args.extend(ids.iter().map(|id| json!(id)));
            }
        }
        // Both stages aggregate in SQLite. No history-sized allocation in Rust/UI.
        let sql=format!("SELECT component_id,metric_id,unit,source,((t-{})/{bucket})*{bucket}+{},MIN(lo),MAX(hi),SUM(total),SUM(n) FROM (SELECT at_ms t,component_id,metric_id,unit,source,value lo,value hi,value total,CASE WHEN value IS NULL THEN 0 ELSE 1 END n FROM samples UNION ALL SELECT bucket_ms t,component_id,metric_id,unit,source,min lo,max hi,total,count n FROM aggregates) WHERE {filters} GROUP BY component_id,metric_id,unit,source,((t-{})/{bucket}) ORDER BY component_id,metric_id,unit,source,t LIMIT 40000",q.from_ms,q.from_ms,q.from_ms);
        let mut series = BTreeMap::<String, Value>::new();
        let mut sums = BTreeMap::<String, (f64, i64)>::new();
        self.db.each(&sql,&args,|r|{let key=format!("{}\0{}\0{}\0{}",r[0].as_str().unwrap_or(""),r[1].as_str().unwrap_or(""),r[2].as_str().unwrap_or(""),r[3].as_str().unwrap_or(""));
            if !series.contains_key(&key)&&series.len()>=32{return Ok(());}
            let s=series.entry(key.clone()).or_insert_with(||json!({"componentId":r[0],"metricId":r[1],"unit":r[2],"source":r[3],"points":[],"min":null,"max":null,"avg":null}));
            let count=r[8].as_i64().unwrap_or(0);let total=r[7].as_f64().unwrap_or(0.);let avg=if count>0{Some(total/count as f64)}else{None};
            s["points"].as_array_mut().unwrap().push(json!({"atMs":r[4],"value":avg,"min":r[5],"max":r[6],"count":count}));
            if let Some(lo)=r[5].as_f64(){s["min"]=json!(s["min"].as_f64().map_or(lo,|v|v.min(lo)));}if let Some(hi)=r[6].as_f64(){s["max"]=json!(s["max"].as_f64().map_or(hi,|v|v.max(hi)));}
            let sum=sums.entry(key).or_default();sum.0+=total;sum.1+=count;Ok(())})?;
        for (k, s) in &mut series {
            let (sum, n) = sums.get(k).copied().unwrap_or_default();
            if n > 0 {
                s["avg"] = json!(sum / n as f64);
            }
        }
        Ok(Value::Array(series.into_values().collect()))
    }
    pub fn event(&self, event: &Event) -> Result<(), String> {
        self.db.execute(
            "INSERT OR IGNORE INTO timeline VALUES(?,?,?,?,?,?,?,?)",
            &[
                json!(event.id),
                json!(event.at_ms),
                json!(event.source),
                json!(event.component_id),
                json!(event.severity),
                json!(event.title),
                json!(event.details),
                json!(event.data.to_string()),
            ],
        )
    }
    pub fn timeline(&self, q: &TimelineQuery) -> Result<Vec<Event>, String> {
        let rows=self.db.query("SELECT id,at_ms,source,component_id,severity,title,details,data FROM timeline WHERE at_ms>=? AND at_ms<=? ORDER BY at_ms DESC LIMIT ?",&[json!(q.from_ms.unwrap_or(0)),json!(q.to_ms.unwrap_or(i64::MAX)),json!(q.limit.unwrap_or(200).clamp(1,1000))])?;
        Ok(rows
            .into_iter()
            .map(|r| Event {
                id: r[0].as_str().unwrap_or("").into(),
                at_ms: r[1].as_i64().unwrap_or(0),
                source: r[2].as_str().unwrap_or("").into(),
                component_id: r[3].as_str().unwrap_or("").into(),
                severity: r[4].as_str().unwrap_or("").into(),
                title: r[5].as_str().unwrap_or("").into(),
                details: r[6].as_str().unwrap_or("").into(),
                data: serde_json::from_str(r[7].as_str().unwrap_or("{}")).unwrap_or(json!({})),
            })
            .collect())
    }
    pub fn save_benchmark(&self, result: &Value) -> Result<(), String> {
        self.db.execute(
            "INSERT OR REPLACE INTO benchmarks VALUES(?,?,?)",
            &[
                result["id"].clone(),
                result["startedAtMs"].clone(),
                json!(result.to_string()),
            ],
        )
    }
    pub fn benchmarks(&self) -> Result<Vec<Value>, String> {
        Ok(self
            .db
            .query(
                "SELECT result FROM benchmarks ORDER BY started_at_ms DESC LIMIT 100",
                &[],
            )?
            .iter()
            .filter_map(|r| serde_json::from_str(r[0].as_str()?).ok())
            .collect())
    }
}
impl Drop for History {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn memory() -> History {
        History::open(Path::new(":memory:")).unwrap()
    }
    #[test]
    fn timestamps_use_sensor_time_and_unavailable_stays_null() {
        let mut h = memory();
        let at = now_ms() - 3000;
        h.pending.push(Sample {
            at,
            component: "cpu".into(),
            kind: "cpu".into(),
            metric: "temperature".into(),
            unit: "°C".into(),
            source: "test".into(),
            value: None,
            status: "unavailable".into(),
        });
        let v = h
            .history(&HistoryQuery {
                component_id: None,
                metric_ids: None,
                from_ms: at - 1,
                to_ms: at + 1,
                max_points: Some(16),
            })
            .unwrap();
        assert_eq!(v[0]["points"][0]["value"], Value::Null);
        assert_eq!(v[0]["points"][0]["count"], 0);
    }
    #[test]
    fn history_is_bounded_and_rollup_preserves_statistics() {
        let mut h = memory();
        let old = (now_ms() - 2 * 86_400_000) / 60000 * 60000;
        for n in 0..10000 {
            h.pending.push(Sample {
                at: old + n * 1000,
                component: "cpu".into(),
                kind: "cpu".into(),
                metric: "load".into(),
                unit: "%".into(),
                source: "test".into(),
                value: Some((n % 100) as f64),
                status: "available".into(),
            });
        }
        h.flush().unwrap();
        h.maintain(now_ms(), 7).unwrap();
        let v = h
            .history(&HistoryQuery {
                component_id: Some("cpu".into()),
                metric_ids: None,
                from_ms: old - 1,
                to_ms: old + 10_000_000,
                max_points: Some(100),
            })
            .unwrap();
        assert!(v[0]["points"].as_array().unwrap().len() <= 101);
        assert_eq!(v[0]["min"], 0.);
        assert_eq!(v[0]["max"], 99.);
        assert!((v[0]["avg"].as_f64().unwrap() - 49.5).abs() < 0.001);
    }
    #[test]
    fn cached_samples_are_deduplicated_and_sql_parameters_are_bound() {
        let mut h = memory();
        for _ in 0..2 {
            h.pending.push(Sample {
                at: 10,
                component: "x'; DROP TABLE samples;--".into(),
                kind: "cpu".into(),
                metric: "load".into(),
                unit: "%".into(),
                source: "test".into(),
                value: Some(10.),
                status: "available".into(),
            });
        }
        h.flush().unwrap();
        assert_eq!(
            h.db.query("SELECT COUNT(*) FROM samples", &[]).unwrap()[0][0],
            1
        );
    }
    #[test]
    fn timeline_persists_and_limit_is_capped() {
        let h = memory();
        let event = Event {
            id: "test".into(),
            at_ms: 42,
            source: "test".into(),
            component_id: "cpu".into(),
            severity: "warning".into(),
            title: "Temperature".into(),
            details: "Observed".into(),
            data: json!({"value":91}),
        };
        h.event(&event).unwrap();
        h.event(&event).unwrap();
        let events = h
            .timeline(&TimelineQuery {
                from_ms: None,
                to_ms: None,
                limit: Some(100000),
            })
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data["value"], 91);
    }
    #[test]
    fn failed_transactions_keep_a_bounded_queue_and_recover() {
        let mut h = memory();
        let at = now_ms();
        let sample = Sample {
            at,
            component: "cpu".into(),
            kind: "cpu".into(),
            metric: "load".into(),
            unit: "%".into(),
            source: "test".into(),
            value: Some(10.),
            status: "available".into(),
        };
        h.pending.resize(8190, sample);
        let timestamp = chrono::DateTime::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339();
        let metrics:Vec<Value>=(0..256).map(|n|json!({"id":format!("load-{n}"),"at":timestamp,"value":12,"unit":"%","source":"test"})).collect();
        h.db.exec("BEGIN IMMEDIATE").unwrap();
        assert!(h
            .record(
                &json!({"components":[{"id":"cpu","kind":"cpu","metrics":metrics}]}),
                7
            )
            .is_err());
        assert!(h.pending.len() <= 8192);
        assert!(h.dropped_samples > 0);
        h.db.exec("ROLLBACK").unwrap();
        h.flush().unwrap();
        assert!(h.pending.is_empty());
        assert!(
            h.db.query("SELECT COUNT(*) FROM samples", &[]).unwrap()[0][0]
                .as_i64()
                .unwrap()
                > 0
        );
    }
    #[test]
    fn sources_and_units_are_never_averaged_together() {
        let mut h = memory();
        let at = (now_ms() - 2 * 86_400_000) / 60000 * 60000;
        for (n, unit, source) in [(0, "%", "A"), (1, "W", "B")] {
            h.pending.push(Sample {
                at: at + n,
                component: "cpu".into(),
                kind: "cpu".into(),
                metric: "load".into(),
                unit: unit.into(),
                source: source.into(),
                value: Some(10. + n as f64),
                status: "available".into(),
            });
        }
        h.flush().unwrap();
        h.maintain(now_ms(), 7).unwrap();
        let values = h
            .history(&HistoryQuery {
                component_id: None,
                metric_ids: None,
                from_ms: at - 1,
                to_ms: at + 60000,
                max_points: Some(16),
            })
            .unwrap();
        assert_eq!(values.as_array().unwrap().len(), 2);
        assert_ne!(values[0]["unit"], values[1]["unit"]);
    }
    #[test]
    fn corrupted_database_is_preserved_and_results_survive_reopen() {
        let folder = std::env::temp_dir().join(format!("nexus-storage-recovery-{}", now_ms()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("history.sqlite");
        std::fs::write(&path, b"deliberately corrupt fixture").unwrap();
        let h = History::open(&path).unwrap();
        assert!(h.recovery.is_some());
        h.save_benchmark(
            &json!({"id":"test","startedAtMs":42,"kind":"memory","status":"cancelled"}),
        )
        .unwrap();
        drop(h);
        let reopened = History::open(&path).unwrap();
        assert_eq!(reopened.benchmarks().unwrap()[0]["status"], "cancelled");
        drop(reopened);
        assert!(std::fs::read_dir(&folder).unwrap().any(|file| file
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("corrupt-")));
        for entry in std::fs::read_dir(&folder).unwrap() {
            let _ = std::fs::remove_file(entry.unwrap().path());
        }
        let _ = std::fs::remove_dir(folder);
    }
}
