//! Stream report tables directly from SQLite. Identifiers are excluded by default.
use crate::storage::{now_ms, Database};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Seek, Write},
    path::Path,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub format: String,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub components: Option<Vec<String>>,
    pub anonymized: Option<bool>,
    pub sections: Option<Sections>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sections {
    pub summary: bool,
    pub metrics: bool,
    pub diagnostics: bool,
    pub benchmarks: bool,
}
impl Default for Sections {
    fn default() -> Self {
        Self {
            summary: true,
            metrics: true,
            diagnostics: true,
            benchmarks: true,
        }
    }
}
impl Sections {
    fn validate(self) -> Result<(), String> {
        if self.summary || self.metrics || self.diagnostics || self.benchmarks {
            Ok(())
        } else {
            Err("Select at least one report section".into())
        }
    }
}
pub fn generate(db_path: &Path, folder: &Path, request: Request) -> Result<Value, String> {
    if !["csv", "xlsx", "pdf"].contains(&request.format.as_str()) {
        return Err("Unsupported report format".into());
    }
    let sections = request.sections.unwrap_or_default();
    sections.validate()?;
    let now = now_ms();
    let from = request.from_ms.unwrap_or(now - 3_600_000);
    let to = request.to_ms.unwrap_or(now);
    if from < 0 || to <= from || to - from > 90 * 86_400_000 {
        return Err("Choose a report period of up to 90 days".into());
    }
    if request
        .components
        .as_ref()
        .is_some_and(|ids| ids.len() > 128)
    {
        return Err("Too many components".into());
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!("NEXUS-report-{now}.{}", request.format));
    let temp = path.with_extension(format!("{}.partial", request.format));
    let db = Database::open(db_path)?;
    let filter = Filter::new(from, to, request.components.as_deref());
    let anon = request.anonymized.unwrap_or(true);
    let aliases = aliases(&db, &filter)?;
    let result = (|| {
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let rows = match request.format.as_str() {
            "csv" => csv(file, &db, &filter, &aliases, anon, sections)?,
            "xlsx" => xlsx(file, &db, &filter, &aliases, anon, sections)?,
            _ => pdf(file, &db, &filter, &aliases, anon, sections)?,
        };
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())?;
        Ok(
            json!({"path":path,"rows":rows,"format":request.format,"anonymized":anon,"sections":sections}),
        )
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
struct Filter {
    where_sql: String,
    args: Vec<Value>,
    from: i64,
    to: i64,
}
impl Filter {
    fn new(from: i64, to: i64, components: Option<&[String]>) -> Self {
        let mut where_sql = "t>=? AND t<=?".to_string();
        let mut args = vec![json!(from), json!(to)];
        if let Some(ids) = components.filter(|ids| !ids.is_empty()) {
            where_sql.push_str(&format!(
                " AND component_id IN({})",
                vec!["?"; ids.len()].join(",")
            ));
            args.extend(ids.iter().map(|id| json!(id)));
        }
        Self {
            where_sql,
            args,
            from,
            to,
        }
    }
    fn table(&self) -> String {
        format!("(SELECT at_ms t,component_id,kind,metric_id,unit,source,value lo,value hi,value total,CASE WHEN value IS NULL THEN 0 ELSE 1 END n FROM samples UNION ALL SELECT bucket_ms t,component_id,kind,metric_id,unit,source,min lo,max hi,total,count n FROM aggregates) WHERE {}",self.where_sql)
    }
    fn metrics(&self) -> String {
        self.metrics_limit(1_000_001)
    }
    fn metrics_limit(&self, limit: usize) -> String {
        format!("SELECT t,component_id,kind,metric_id,unit,source,lo,hi,CASE WHEN n=0 THEN NULL ELSE total/n END,n FROM {} ORDER BY t LIMIT {limit}",self.table())
    }
    fn stats(&self) -> String {
        format!("SELECT component_id,kind,metric_id,unit,MIN(lo),MAX(hi),SUM(total)/NULLIF(SUM(n),0),SUM(n),source FROM {} GROUP BY component_id,metric_id,unit,source ORDER BY kind,component_id,metric_id,unit,source LIMIT 8192",self.table())
    }
    fn timeline(&self, columns: &str, limit: usize) -> String {
        format!(
            "SELECT {columns} FROM timeline WHERE {} ORDER BY at_ms LIMIT {limit}",
            self.where_sql
                .replace("t>=? AND t<=?", "at_ms>=? AND at_ms<=?")
        )
    }
}
fn benchmark_allowed(kind: &Value, f: &Filter, aliases: &HashMap<String, String>) -> bool {
    if f.args.len() <= 2 {
        return true;
    }
    let kind = kind.as_str().unwrap_or("");
    let component_kind = if kind == "memory" {
        "ram"
    } else {
        kind.split('_').next().unwrap_or("system")
    };
    aliases
        .values()
        .any(|alias| alias.starts_with(&format!("{component_kind}-")))
}
fn aliases(db: &Database, filter: &Filter) -> Result<HashMap<String, String>, String> {
    let rows=db.query(&format!("SELECT component_id,kind FROM {} GROUP BY component_id ORDER BY kind,component_id LIMIT 128",filter.table()),&filter.args)?;
    let mut counts = HashMap::<String, usize>::new();
    Ok(rows
        .iter()
        .map(|r| {
            let kind = r[1].as_str().unwrap_or("component").to_string();
            let count = counts.entry(kind.clone()).or_default();
            *count += 1;
            (
                r[0].as_str().unwrap_or("").into(),
                format!("{kind}-{count}"),
            )
        })
        .collect())
}
fn alias(id: &Value, aliases: &HashMap<String, String>, anonymized: bool) -> String {
    let id = id.as_str().unwrap_or("");
    if anonymized {
        aliases
            .get(id)
            .cloned()
            .unwrap_or_else(|| "component".into())
    } else {
        id.into()
    }
}
fn source(kind: &Value) -> String {
    format!("{} provider", kind.as_str().unwrap_or("system"))
}
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
fn safe_csv(s: &str) -> String {
    let mut value = s.to_string();
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        value.insert(0, '\'');
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn csv(
    file: File,
    db: &Database,
    f: &Filter,
    aliases: &HashMap<String, String>,
    anon: bool,
    sections: Sections,
) -> Result<usize, String> {
    let mut out = BufWriter::new(file);
    out.write_all(
        b"\xEF\xBB\xBFtimestamp_utc,component,kind,metric,unit,source,min,max,average,samples,record_type,severity,title,details,status,score\r\n",
    )
    .map_err(|e| e.to_string())?;
    let mut count = 0;
    if sections.summary {
        let mut fields = vec![String::new(); 16];
        fields[0] = utc(f.to);
        fields[5] = "NEXUS".into();
        fields[10] = "summary".into();
        fields[12] = "Report context".into();
        fields[13] = format!("Desktop {}; from {} to {}; identifiers excluded={anon}; raw retention 24h, older values are minute aggregates; observations do not certify hardware health.", env!("CARGO_PKG_VERSION"), utc(f.from), utc(f.to));
        csv_row(&mut out, &fields)?;
        count += 1;
        db.each(&f.stats(), &f.args, |r| {
            let mut fields = vec![
                utc(f.to),
                alias(&r[0], aliases, anon),
                text(&r[1]),
                text(&r[2]),
                text(&r[3]),
                if anon { source(&r[1]) } else { text(&r[8]) },
            ];
            fields.extend(r[4..8].iter().map(text));
            fields.extend([
                "summary".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            count += 1;
            csv_row(&mut out, &fields)
        })?;
    }
    if sections.metrics {
        let mut metrics = 0;
        db.each(&f.metrics(), &f.args, |r| {
            metrics += 1;
            if metrics > 1_000_000 {
                return Err("Report exceeds 1,000,000 metric rows; choose a shorter period".into());
            }
            let mut fields = vec![
                utc(r[0].as_i64().unwrap_or(0)),
                alias(&r[1], aliases, anon),
                text(&r[2]),
                text(&r[3]),
                text(&r[4]),
                if anon { source(&r[2]) } else { text(&r[5]) },
            ];
            fields.extend(r[6..10].iter().map(text));
            fields.extend([
                "metrics".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            count += 1;
            csv_row(&mut out, &fields)
        })?;
    }
    if sections.diagnostics {
        db.each(
            &f.timeline("at_ms,component_id,severity,source,title,details", 100000),
            &f.args,
            |r| {
                let mut fields = vec![String::new(); 16];
                fields[0] = utc(r[0].as_i64().unwrap_or(0));
                fields[1] = alias(&r[1], aliases, anon);
                fields[5] = text(&r[3]);
                fields[10] = "diagnostics".into();
                fields[11] = text(&r[2]);
                fields[12] = text(&r[4]);
                fields[13] = text(&r[5]);
                count += 1;
                csv_row(&mut out, &fields)
            },
        )?;
    }
    if sections.benchmarks {
        each_benchmark(db, f, aliases, |r| {
            let mut fields = vec![String::new(); 16];
            fields[0] = utc(r["startedAtMs"].as_i64().unwrap_or(0));
            fields[2] = text(&r["kind"]);
            fields[4] = text(&r["unit"]);
            fields[5] = "benchmark".into();
            fields[10] = "benchmarks".into();
            fields[13] = text(&r["details"]);
            fields[14] = text(&r["status"]);
            fields[15] = text(&r["score"]);
            count += 1;
            csv_row(&mut out, &fields)
        })?;
    }
    out.flush().map_err(|e| e.to_string())?;
    out.get_ref().sync_all().map_err(|e| e.to_string())?;
    Ok(count)
}
fn csv_row(out: &mut impl Write, fields: &[String]) -> Result<(), String> {
    writeln!(
        out,
        "{}\r",
        fields
            .iter()
            .map(|s| safe_csv(s))
            .collect::<Vec<_>>()
            .join(",")
    )
    .map_err(|e| e.to_string())
}
fn each_benchmark(
    db: &Database,
    f: &Filter,
    aliases: &HashMap<String, String>,
    mut visit: impl FnMut(&Value) -> Result<(), String>,
) -> Result<(), String> {
    db.each("SELECT result FROM benchmarks WHERE started_at_ms>=? AND started_at_ms<=? ORDER BY started_at_ms LIMIT 500",&[json!(f.from),json!(f.to)],|row| {
        let result:Value=serde_json::from_str(row[0].as_str().unwrap_or("{}")).map_err(|e|e.to_string())?;
        if benchmark_allowed(&result["kind"], f, aliases) { visit(&result)?; }
        Ok(())
    })
}
fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| *c == '\t' || *c == '\n' || *c == '\r' || *c >= ' ')
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn column(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}
fn row(out: &mut impl Write, index: usize, values: &[Value], header: bool) -> Result<(), String> {
    write!(
        out,
        "<row r=\"{index}\" ht=\"{}\" customHeight=\"1\">",
        if header { 24 } else { 34 }
    )
    .map_err(|e| e.to_string())?;
    for (i, v) in values.iter().enumerate() {
        let cell = format!("{}{index}", column(i));
        let style = if header { 1 } else { 0 };
        match v{Value::Number(n)=>write!(out,"<c r=\"{cell}\" s=\"{style}\"><v>{n}</v></c>"),Value::Null=>Ok(()),_=>write!(out,"<c r=\"{cell}\" s=\"{style}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",xml(&text(v))) }.map_err(|e|e.to_string())?;
    }
    out.write_all(b"</row>").map_err(|e| e.to_string())
}
const SHEET_START:&str="<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" state=\"frozen\"/></sheetView></sheetViews><cols><col min=\"1\" max=\"1\" width=\"40\" customWidth=\"1\"/><col min=\"2\" max=\"2\" width=\"40\" customWidth=\"1\"/><col min=\"3\" max=\"3\" width=\"18\" customWidth=\"1\"/><col min=\"4\" max=\"4\" width=\"30\" customWidth=\"1\"/><col min=\"5\" max=\"5\" width=\"46\" customWidth=\"1\"/><col min=\"6\" max=\"6\" width=\"90\" customWidth=\"1\"/><col min=\"7\" max=\"20\" width=\"16\" customWidth=\"1\"/></cols><sheetData>";
fn xlsx(
    file: File,
    db: &Database,
    f: &Filter,
    aliases: &HashMap<String, String>,
    anon: bool,
    sections: Sections,
) -> Result<usize, String> {
    let mut zip = Zip::new(file);
    let stats = if sections.summary {
        db.query(&f.stats(), &f.args)?
    } else {
        vec![]
    };
    let chart = if let Some(cpu) = stats
        .iter()
        .find(|r| sections.metrics && r[1] == "cpu" && r[2] == "load")
    {
        let bucket = ((f.to - f.from + 119) / 120).max(1);
        let mut args = f.args.clone();
        args.push(cpu[0].clone());
        args.push(cpu[8].clone());
        args.push(cpu[3].clone());
        db.query(&format!("SELECT ((t-{})/{bucket})*{bucket}+{},SUM(total)/NULLIF(SUM(n),0) FROM {} AND component_id=? AND source=? AND unit=? AND metric_id='load' GROUP BY ((t-{})/{bucket}) ORDER BY t LIMIT 120",f.from,f.from,f.table(),f.from),&args)?
    } else {
        vec![]
    };
    let mut names = Vec::new();
    if sections.summary {
        names.push("Summary");
    }
    if sections.metrics {
        names.push("Metrics");
    }
    if sections.diagnostics {
        names.push("Diagnostics");
    }
    if sections.benchmarks {
        names.push("Tests");
    }
    if sections.summary {
        names.extend([
            "CPU",
            "GPU",
            "RAM",
            "Storage",
            "Motherboard",
            "Network",
            "System",
        ]);
    }
    let sheet_path = |name: &str| {
        format!(
            "xl/worksheets/sheet{}.xml",
            names.iter().position(|n| *n == name).unwrap() + 1
        )
    };
    let mut types=String::from("<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    for i in 1..=names.len() {
        types.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
    }
    if !chart.is_empty() {
        types.push_str("<Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/><Override PartName=\"/xl/charts/chart1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>");
    }
    types.push_str("</Types>");
    zip.entry("[Content_Types].xml", |w| {
        w.write_all(types.as_bytes()).map_err(|e| e.to_string())
    })?;
    zip.entry("_rels/.rels",|w|w.write_all(b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>").map_err(|e|e.to_string()))?;
    let mut workbook=String::from("<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>");
    let mut rels=String::from("<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (i, name) in names.iter().enumerate() {
        workbook.push_str(&format!(
            "<sheet name=\"{name}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
            i + 1,
            i + 1
        ));
        rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",i+1,i+1));
    }
    workbook.push_str("</sheets></workbook>");
    rels.push_str("<Relationship Id=\"styles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>");
    zip.entry("xl/workbook.xml", |w| {
        w.write_all(workbook.as_bytes()).map_err(|e| e.to_string())
    })?;
    zip.entry("xl/_rels/workbook.xml.rels", |w| {
        w.write_all(rels.as_bytes()).map_err(|e| e.to_string())
    })?;
    zip.entry("xl/styles.xml",|w|w.write_all(b"<?xml version=\"1.0\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><color rgb=\"FFFFFFFF\"/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF12734A\"/><bgColor indexed=\"64\"/></patternFill></fill></fills><borders count=\"1\"><border/></borders><cellStyleXfs count=\"1\"><xf/></cellStyleXfs><cellXfs count=\"2\"><xf fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyAlignment=\"1\"><alignment vertical=\"top\" wrapText=\"1\"/></xf><xf fontId=\"1\" fillId=\"2\" borderId=\"0\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyAlignment=\"1\"><alignment vertical=\"center\" wrapText=\"1\"/></xf></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>").map_err(|e|e.to_string()))?;
    let mut count = 0;
    if sections.summary {
        zip.entry(&sheet_path("Summary"), |w| {
        w.write_all(SHEET_START.replace("<worksheet xmlns=","<worksheet xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns=").as_bytes())
            .map_err(|e| e.to_string())?;
        for (i, r) in [
            vec![
                json!("NEXUS Desktop diagnostic report"),
                json!(env!("CARGO_PKG_VERSION")),
            ],
            vec![json!("From UTC"), json!(utc(f.from))],
            vec![json!("To UTC"), json!(utc(f.to))],
            vec![json!("Identifiers excluded"), json!(anon.to_string())],
            vec![
                json!("Raw retention"),
                json!("24 hours; older values are minute aggregates"),
            ],
            vec![
                json!("Missing values"),
                json!("Blank cells; never interpolated or replaced with zero"),
            ],
            vec![
                json!("Interpretation"),
                json!(
                    "Observations and user-mode benchmark results do not certify hardware health"
                ),
            ],
        ]
        .iter()
        .enumerate()
        {
            row(w, i + 1, r, i == 0)?;
            count += 1;
        }
        if !chart.is_empty(){row(w,10,&[json!("Time UTC"),json!("CPU load (%)")],true)?;for(i,r)in chart.iter().enumerate(){row(w,i+11,&[json!(utc(r[0].as_i64().unwrap_or(0))),r[1].clone()],false)?;}}
        w.write_all(if chart.is_empty(){b"</sheetData></worksheet>"}else{b"</sheetData><drawing r:id=\"cpu-chart\"/></worksheet>"})
            .map_err(|e| e.to_string())
    })?;
    }
    if sections.metrics {
        zip.entry(&sheet_path("Metrics"), |w| {
            w.write_all(SHEET_START.as_bytes())
                .map_err(|e| e.to_string())?;
            row(
                w,
                1,
                &[
                    "Time UTC",
                    "Component",
                    "Kind",
                    "Metric",
                    "Unit",
                    "Source",
                    "Min",
                    "Max",
                    "Average",
                    "Samples",
                ]
                .map(|s| json!(s)),
                true,
            )?;
            let mut metrics = 0;
            db.each(&f.metrics(), &f.args, |r| {
                metrics += 1;
                if metrics > 1_000_000 {
                    return Err("Report exceeds 1,000,000 rows; choose a shorter period".into());
                }
                let mut values = vec![
                    json!(utc(r[0].as_i64().unwrap_or(0))),
                    json!(alias(&r[1], aliases, anon)),
                    r[2].clone(),
                    r[3].clone(),
                    r[4].clone(),
                    if anon {
                        json!(source(&r[2]))
                    } else {
                        r[5].clone()
                    },
                ];
                values.extend(r[6..10].iter().cloned());
                count += 1;
                row(w, metrics + 1, &values, false)
            })?;
            w.write_all(b"</sheetData></worksheet>")
                .map_err(|e| e.to_string())
        })?;
    }
    if sections.diagnostics {
        zip.entry(&sheet_path("Diagnostics"), |w| {
            w.write_all(SHEET_START.as_bytes())
                .map_err(|e| e.to_string())?;
            row(
                w,
                1,
                &[
                    "Time UTC",
                    "Component",
                    "Severity",
                    "Source",
                    "Title",
                    "Reason",
                ]
                .map(|s| json!(s)),
                true,
            )?;
            let mut i = 1;
            db.each(
                &f.timeline("at_ms,component_id,severity,source,title,details", 100000),
                &f.args,
                |r| {
                    i += 1;
                    count += 1;
                    row(
                        w,
                        i,
                        &[
                            json!(utc(r[0].as_i64().unwrap_or(0))),
                            json!(alias(&r[1], aliases, anon)),
                            r[2].clone(),
                            r[3].clone(),
                            r[4].clone(),
                            r[5].clone(),
                        ],
                        false,
                    )
                },
            )?;
            w.write_all(b"</sheetData></worksheet>")
                .map_err(|e| e.to_string())
        })?;
    }
    if sections.benchmarks {
        zip.entry(&sheet_path("Tests"), |w| {
            w.write_all(SHEET_START.as_bytes())
                .map_err(|e| e.to_string())?;
            row(
                w,
                1,
                &["Time UTC", "Kind", "Status", "Score", "Unit", "Details"].map(|s| json!(s)),
                true,
            )?;
            let mut i = 1;
            each_benchmark(db, f, aliases, |result| {
                i += 1;
                count += 1;
                row(
                    w,
                    i,
                    &[
                        json!(utc(result["startedAtMs"].as_i64().unwrap_or(0))),
                        result["kind"].clone(),
                        result["status"].clone(),
                        result["score"].clone(),
                        result["unit"].clone(),
                        result["details"].clone(),
                    ],
                    false,
                )
            })?;
            w.write_all(b"</sheetData></worksheet>")
                .map_err(|e| e.to_string())
        })?;
    }
    if !chart.is_empty() {
        write_chart(&mut zip, &chart)?;
    }
    if sections.summary {
        for (name, kind) in [
            "cpu",
            "gpu",
            "ram",
            "storage",
            "motherboard",
            "network",
            "system",
        ]
        .iter()
        .zip(
            [
                "CPU",
                "GPU",
                "RAM",
                "Storage",
                "Motherboard",
                "Network",
                "System",
            ]
            .iter(),
        )
        .map(|(kind, name)| (*name, *kind))
        {
            zip.entry(&sheet_path(name), |w| {
                w.write_all(SHEET_START.as_bytes())
                    .map_err(|e| e.to_string())?;
                row(
                    w,
                    1,
                    &[
                        "Component",
                        "Metric",
                        "Unit",
                        "Min",
                        "Max",
                        "Average",
                        "Samples",
                    ]
                    .map(|s| json!(s)),
                    true,
                )?;
                let mut n = 1;
                for r in stats.iter().filter(|r| r[1] == kind) {
                    n += 1;
                    count += 1;
                    row(
                        w,
                        n,
                        &[
                            json!(alias(&r[0], aliases, anon)),
                            r[2].clone(),
                            r[3].clone(),
                            r[4].clone(),
                            r[5].clone(),
                            r[6].clone(),
                            r[7].clone(),
                        ],
                        false,
                    )?;
                }
                w.write_all(b"</sheetData></worksheet>")
                    .map_err(|e| e.to_string())
            })?;
        }
    }
    zip.finish()?;
    Ok(count)
}
fn write_chart(zip: &mut Zip, rows: &[Vec<Value>]) -> Result<(), String> {
    let parts=[
        ("xl/worksheets/_rels/sheet1.xml.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"cpu-chart\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>"),
        ("xl/drawings/drawing1.xml", "<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><xdr:twoCellAnchor><xdr:from><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>9</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>11</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>28</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"1\" name=\"CPU load history\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"chart\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"),
        ("xl/drawings/_rels/drawing1.xml.rels", "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"chart\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart1.xml\"/></Relationships>")
    ];
    for (name, content) in parts {
        zip.entry(name, |w| {
            w.write_all(content.as_bytes()).map_err(|e| e.to_string())
        })?;
    }
    let end = rows.len() + 10;
    let chart=format!("<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/><c:lineChart><c:grouping val=\"standard\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:v>CPU load (%)</c:v></c:tx><c:spPr><a:ln w=\"25400\"><a:solidFill><a:srgbClr val=\"39A96B\"/></a:solidFill></a:ln></c:spPr><c:cat><c:strRef><c:f>'Summary'!$A$11:$A${end}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>'Summary'!$B$11:$B${end}</c:f></c:numRef></c:val><c:smooth val=\"0\"/></c:ser><c:axId val=\"10\"/><c:axId val=\"20\"/></c:lineChart><c:catAx><c:axId val=\"10\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:axPos val=\"b\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"20\"/><c:crosses val=\"autoZero\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:tickLblSkip val=\"20\"/></c:catAx><c:valAx><c:axId val=\"20\"/><c:scaling><c:orientation val=\"minMax\"/><c:max val=\"100\"/><c:min val=\"0\"/></c:scaling><c:axPos val=\"l\"/><c:majorGridlines/><c:numFmt formatCode=\"0.0\" sourceLinked=\"0\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"10\"/><c:crosses val=\"autoZero\"/></c:valAx></c:plotArea><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>");
    let mut categories = format!("<c:strCache><c:ptCount val=\"{}\"/>", rows.len());
    let mut values = format!(
        "<c:numCache><c:formatCode>0.0</c:formatCode><c:ptCount val=\"{}\"/>",
        rows.len()
    );
    for (i, r) in rows.iter().enumerate() {
        categories.push_str(&format!(
            "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>",
            xml(&utc(r[0].as_i64().unwrap_or(0)))
        ));
        if let Some(value) = r[1].as_f64() {
            values.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{value}</c:v></c:pt>"));
        }
    }
    categories.push_str("</c:strCache>");
    values.push_str("</c:numCache>");
    let chart = chart
        .replace("</c:strRef>", &format!("{categories}</c:strRef>"))
        .replace("</c:numRef>", &format!("{values}</c:numRef>"));
    zip.entry("xl/charts/chart1.xml", |w| {
        w.write_all(chart.as_bytes()).map_err(|e| e.to_string())
    })
}
fn utc(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
}

// ZIP method 0 + data descriptors allow genuinely streaming worksheets. CRC and
// central directory sizes are verified by automated Python/openpyxl validation.
struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}
struct Zip {
    file: BufWriter<File>,
    entries: Vec<Entry>,
}
struct ZipWriter<'a> {
    file: &'a mut BufWriter<File>,
    crc: u32,
    size: u64,
}
fn crc_table() -> &'static [u32; 256] {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0; 256];
        for (i, value) in table.iter_mut().enumerate() {
            let mut crc = i as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb88320
                } else {
                    crc >> 1
                };
            }
            *value = crc;
        }
        table
    })
}
impl Write for ZipWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(bytes)?;
        for b in &bytes[..n] {
            self.crc = (self.crc >> 8) ^ crc_table()[((self.crc ^ *b as u32) & 255) as usize];
        }
        self.size += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Zip {
    fn new(file: File) -> Self {
        Self {
            file: BufWriter::with_capacity(64 * 1024, file),
            entries: Vec::new(),
        }
    }
    fn entry(
        &mut self,
        name: &str,
        write: impl FnOnce(&mut ZipWriter) -> Result<(), String>,
    ) -> Result<(), String> {
        let offset = self.file.stream_position().map_err(|e| e.to_string())?;
        if offset > u32::MAX as u64 {
            return Err("ZIP exceeds 4 GiB limit".into());
        }
        self.file
            .write_all(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 8, 0, 0, 0, 0, 0, 0, 0])
            .map_err(|e| e.to_string())?;
        self.file.write_all(&[0; 12]).map_err(|e| e.to_string())?;
        self.file
            .write_all(&(name.len() as u16).to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.file.write_all(&[0, 0]).map_err(|e| e.to_string())?;
        self.file
            .write_all(name.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut writer = ZipWriter {
            file: &mut self.file,
            crc: !0,
            size: 0,
        };
        write(&mut writer)?;
        let size = u32::try_from(writer.size).map_err(|_| "Worksheet exceeds 4 GiB ZIP limit")?;
        let crc = !writer.crc;
        self.file
            .write_all(&0x08074b50u32.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.file
            .write_all(&crc.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.file
            .write_all(&size.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.file
            .write_all(&size.to_le_bytes())
            .map_err(|e| e.to_string())?;
        self.entries.push(Entry {
            name: name.into(),
            crc,
            size,
            offset: offset as u32,
        });
        Ok(())
    }
    fn finish(mut self) -> Result<(), String> {
        let start = self.file.stream_position().map_err(|e| e.to_string())? as u32;
        for e in &self.entries {
            let mut record = vec![];
            record.extend(0x02014b50u32.to_le_bytes());
            record.extend([20, 0, 20, 0, 8, 0, 0, 0, 0, 0, 0, 0]);
            record.extend(e.crc.to_le_bytes());
            record.extend(e.size.to_le_bytes());
            record.extend(e.size.to_le_bytes());
            record.extend((e.name.len() as u16).to_le_bytes());
            record.extend([0; 12]);
            record.extend(e.offset.to_le_bytes());
            record.extend(e.name.as_bytes());
            self.file.write_all(&record).map_err(|e| e.to_string())?;
        }
        let end = self.file.stream_position().map_err(|e| e.to_string())? as u32;
        let mut record = vec![];
        record.extend(0x06054b50u32.to_le_bytes());
        record.extend([0; 4]);
        record.extend((self.entries.len() as u16).to_le_bytes());
        record.extend((self.entries.len() as u16).to_le_bytes());
        record.extend((end - start).to_le_bytes());
        record.extend(start.to_le_bytes());
        record.extend([0; 2]);
        self.file.write_all(&record).map_err(|e| e.to_string())?;
        self.file.flush().map_err(|e| e.to_string())?;
        self.file.get_ref().sync_all().map_err(|e| e.to_string())
    }
}

fn pdf(
    mut file: File,
    db: &Database,
    f: &Filter,
    aliases: &HashMap<String, String>,
    anon: bool,
    sections: Sections,
) -> Result<usize, String> {
    let mut lines = vec![
        format!(
            "NEXUS Desktop {} | Diagnostic report",
            env!("CARGO_PKG_VERSION")
        ),
        format!("Period: {} / {}", utc(f.from), utc(f.to)),
        format!(
            "Privacy: identifiers {}",
            if anon {
                "excluded"
            } else {
                "included for component IDs only"
            }
        ),
    ];
    let mut count = 0;
    if sections.summary {
        let stats = db.query(&f.stats(), &f.args)?;
        lines.extend([
            "".into(),
            "SUMMARY / COMPONENT AGGREGATES".into(),
            "Raw samples: 24h; older samples: minute aggregates. Missing values are N/A.".into(),
            "Observations do not certify hardware health. Benchmarks are bounded user-mode checks."
                .into(),
            format!(
                "{:<15} | {:<22} | {:<8} | {:>9} | {:>9} | {:>9} | {:>6}",
                "COMPONENT", "METRIC", "UNIT", "MIN", "MAX", "AVG", "N"
            ),
        ]);
        for r in &stats {
            lines.push(format!(
                "{:<15} | {:<22} | {:<8} | {:>9} | {:>9} | {:>9} | {:>6}",
                pdf_cell(&alias(&r[0], aliases, anon), 15),
                pdf_cell(&text(&r[2]), 22),
                pdf_cell(&text(&r[3]), 8),
                numeric(&r[4]),
                numeric(&r[5]),
                numeric(&r[6]),
                text(&r[7])
            ));
            count += 1;
        }
    }
    if sections.metrics {
        lines.extend([
            "".into(),
            "METRIC HISTORY (at most 1000 rows; use CSV/XLSX for complete selected history)".into(),
            "Long or non-ASCII labels are shortened; CSV/XLSX retain their complete text.".into(),
            format!(
                "{:<29} | {:<15} | {:<22} | {:<8} | {:>9} | {:>9} | {:>9} | {:>6}",
                "TIME UTC", "COMPONENT", "METRIC", "UNIT", "MIN", "MAX", "AVG", "N"
            ),
        ]);
        let mut history_rows = 0;
        db.each(&f.metrics_limit(1001), &f.args, |r| {
            history_rows += 1;
            if history_rows <= 1000 {
                lines.push(format!(
                    "{:<29} | {:<15} | {:<22} | {:<8} | {:>9} | {:>9} | {:>9} | {:>6}",
                    utc(r[0].as_i64().unwrap_or(0)),
                    pdf_cell(&alias(&r[1], aliases, anon), 15),
                    pdf_cell(&text(&r[3]), 22),
                    pdf_cell(&text(&r[4]), 8),
                    numeric(&r[6]),
                    numeric(&r[7]),
                    numeric(&r[8]),
                    text(&r[9])
                ));
                count += 1;
            }
            Ok(())
        })?;
        if history_rows > 1000 {
            lines.push(
                "History shortened at 1000 rows. Export CSV/XLSX for the full selected period."
                    .into(),
            );
        }
    }
    if sections.diagnostics {
        lines.push("".into());
        lines.push(
        "TIMELINE (up to 100 events; severity / component / source; localized explanations in XLSX/CSV)".into(),
    );
        db.each(
            &f.timeline("at_ms,component_id,severity,source,id,data", 100),
            &f.args,
            |r| {
                count += 1;
                let data: Value =
                    serde_json::from_str(r[5].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
                lines.push(format!(
                    "{} | {} | {} | {}",
                    utc(r[0].as_i64().unwrap_or(0)),
                    match r[2].as_str() {
                        Some("critical") => "critical",
                        Some("warning") => "warning",
                        _ => "info",
                    },
                    pdf_cell(&alias(&r[1], aliases, anon), 20),
                    pdf_event_description(
                        r[4].as_str().unwrap_or(""),
                        r[3].as_str().unwrap_or(""),
                        &data
                    )
                ));
                Ok(())
            },
        )?;
    }
    if sections.benchmarks {
        lines.push("".into());
        lines.push(
            "BENCHMARK RESULTS (partial tests do not certify physical component health)".into(),
        );
        each_benchmark(db, f, aliases, |r| {
            count += 1;
            lines.push(format!(
                "{} | {} | {} | {} {}",
                utc(r["startedAtMs"].as_i64().unwrap_or(0)),
                pdf_cell(&text(&r["kind"]), 24),
                pdf_cell(&text(&r["status"]), 16),
                numeric(&r["score"]),
                pdf_cell(&text(&r["unit"]), 16)
            ));
            Ok(())
        })?;
    }
    let pages: Vec<_> = lines.chunks(46).collect();
    let mut objects = vec![String::new(); 3 + pages.len() * 2];
    objects[0] = "<< /Type /Catalog /Pages 2 0 R >>".into();
    objects[1] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        pages.len(),
        (0..pages.len())
            .map(|i| format!("{} 0 R", 4 + i * 2))
            .collect::<Vec<_>>()
            .join(" ")
    );
    objects[2] = "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into();
    for (i, lines) in pages.iter().enumerate() {
        let page = 4 + i * 2;
        let mut stream = String::from("0.08 0.45 0.29 rg 0 583 842 12 re f\nBT /F1 16 Tf 34 550 Td (NEXUS / DIAGNOSTICS) Tj ET\n0.15 0.17 0.18 rg BT /F1 8 Tf 10.5 TL 34 524 Td\n");
        for line in *lines {
            let safe = ascii(line)
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            stream.push_str(&format!("({safe}) Tj T*\n"));
        }
        stream.push_str(&format!(
            "ET\n0.5 0.5 0.5 rg BT /F1 8 Tf 34 18 Td (Local report / Page {} of {}) Tj ET",
            i + 1,
            pages.len()
        ));
        objects[page-1]=format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",page+1);
        objects[page] = format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        );
    }
    file.write_all(b"%PDF-1.4\n").map_err(|e| e.to_string())?;
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(file.stream_position().map_err(|e| e.to_string())?);
        write!(file, "{} 0 obj\n{}\nendobj\n", i + 1, object).map_err(|e| e.to_string())?;
    }
    let xref = file.stream_position().map_err(|e| e.to_string())?;
    write!(file, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1)
        .map_err(|e| e.to_string())?;
    for offset in &offsets[1..] {
        writeln!(file, "{offset:010} 00000 n ").map_err(|e| e.to_string())?;
    }
    write!(
        file,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
        objects.len() + 1
    )
    .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(count)
}
fn pdf_event_description(id: &str, source: &str, data: &Value) -> String {
    let description = if id.starts_with("diag:threshold:") {
        "Configured threshold exceeded"
    } else if id.starts_with("diag:health:") {
        "Device health counter warning"
    } else if id.starts_with("diag:jump:") {
        "Abrupt sensor reading change"
    } else if id.starts_with("resolved:") {
        "Reading returned to the accepted range"
    } else if source == "hardware" {
        "Device inventory changed"
    } else if source == "benchmark" {
        "Manual test state changed"
    } else {
        "Registered event; localized explanation is available in CSV/XLSX"
    };
    let mut details = description.to_string();
    if data["value"].is_number() {
        let unit = ascii(data["unit"].as_str().unwrap_or(""));
        details.push_str(&format!(
            "; observed={} {}",
            numeric(&data["value"]),
            if unit.contains('?') {
                "unit unavailable"
            } else {
                &unit
            }
        ));
    }
    for (key, label) in [("threshold", "limit"), ("previous", "previous")] {
        if data[key].is_number() {
            details.push_str(&format!("; {label}={}", numeric(&data[key])));
        }
    }
    details
}
fn ascii(s: &str) -> String {
    let s = match s {
        "ГиБ" => "GiB",
        "МиБ" => "MiB",
        "МБ/с" => "MB/s",
        "КБ/с" => "KB/s",
        "Б/с" => "B/s",
        "МГц" => "MHz",
        "ГГц" => "GHz",
        "Гц" => "Hz",
        "с" => "s",
        "мс" => "ms",
        "Вт" => "W",
        "В" => "V",
        "об/мин" => "RPM",
        _ => s,
    };
    s.replace('°', "deg")
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}
fn numeric(v: &Value) -> String {
    v.as_f64()
        .map(|v| {
            if v.abs() >= 100_000_000. || (v != 0. && v.abs() < 0.01) {
                format!("{v:.2e}")
            } else {
                format!("{v:.2}")
            }
        })
        .unwrap_or_else(|| "N/A".into())
}
fn pdf_cell(value: &str, width: usize) -> String {
    let mut value = ascii(value);
    if value.contains('?') {
        value = if width < 12 {
            "CSV/XLSX"
        } else {
            "See CSV/XLSX"
        }
        .into();
    }
    if value.len() > width {
        value.truncate(width.saturating_sub(3));
        value.push_str("...");
    }
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_and_xml_escape_injection() {
        assert_eq!(safe_csv("=1+1"), "\"'=1+1\"");
        assert_eq!(xml("<x>&\u{1}"), "&lt;x&gt;&amp;");
        assert_eq!(column(26), "AA");
    }
    #[test]
    fn pdf_cells_keep_nonzero_values_readable_and_bound_unavailable_labels() {
        assert_ne!(numeric(&json!(0.001)), "0.00");
        assert!(numeric(&json!(1.0e100)).len() <= 10);
        assert_eq!(pdf_cell("°C", 8), "degC");
        let missing = pdf_cell("Русское пояснение", 15);
        assert!(missing.contains("CSV/XLSX") && !missing.contains('?'));
        assert!(pdf_cell("a".repeat(100).as_str(), 22).len() <= 22);
    }
    #[test]
    fn filter_binds_component_ids() {
        let f = Filter::new(0, 100, Some(&["private';DROP TABLE samples".into()]));
        assert!(!f.metrics().contains("private"));
        assert_eq!(f.args.len(), 3);
    }
    #[test]
    fn omitted_sections_keep_all_content_and_empty_selection_creates_no_files() {
        let legacy: Request = serde_json::from_value(json!({"format":"csv"})).unwrap();
        let sections = legacy.sections.unwrap_or_default();
        assert!(
            sections.summary && sections.metrics && sections.diagnostics && sections.benchmarks
        );
        let partial: Sections = serde_json::from_value(json!({"metrics":false})).unwrap();
        assert!(partial.summary && !partial.metrics && partial.diagnostics && partial.benchmarks);
        let folder = std::env::temp_dir().join(format!(
            "nexus-empty-report-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let result = generate(
            Path::new("missing-database.sqlite"),
            &folder,
            Request {
                format: "csv".into(),
                from_ms: Some(1),
                to_ms: Some(2),
                components: None,
                anonymized: None,
                sections: Some(Sections {
                    summary: false,
                    metrics: false,
                    diagnostics: false,
                    benchmarks: false,
                }),
            },
        );
        assert!(result.unwrap_err().contains("at least one"));
        assert!(
            !folder.exists(),
            "An empty selection must fail before file or folder creation"
        );
    }
    #[test]
    fn selected_content_is_respected_in_every_report_format() {
        let folder = std::env::temp_dir().join(format!(
            "nexus-report-sections-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let db_path = folder.join("fixture.sqlite");
        let mut history = crate::storage::History::open(&db_path).unwrap();
        let now = now_ms();
        let at = chrono::DateTime::from_timestamp_millis(now)
            .unwrap()
            .to_rfc3339();
        history.record(&json!({"components":[{"id":"PRIVATE_COMPONENT","kind":"cpu","metrics":[{"id":"load","value":42,"at":at,"unit":"%","source":"PRIVATE_SOURCE"}]}]}),7).unwrap();
        history
            .event(&crate::storage::Event {
                id: "diag:threshold:PRIVATE_COMPONENT.temperature:1".into(),
                at_ms: now,
                component_id: "PRIVATE_COMPONENT".into(),
                source: "diagnostics".into(),
                severity: "warning".into(),
                title: "=1+1".into(),
                details: "@SUM(1,2)".into(),
                data: json!({"value":95,"unit":"°C","threshold":90}),
            })
            .unwrap();
        history.save_benchmark(&json!({"id":"fixture","startedAtMs":now,"kind":"cpu_single","status":"completed","score":0,"unit":"operations/s","details":"+synthetic only"})).unwrap();
        history.flush().unwrap();
        drop(history);
        for section in ["summary", "metrics", "diagnostics", "benchmarks"] {
            for format in ["csv", "xlsx", "pdf"] {
                let target = folder.join(format!("{section}-{format}"));
                let result = generate(
                    &db_path,
                    &target,
                    Request {
                        format: format.into(),
                        from_ms: Some(now - 1),
                        to_ms: Some(now + 1),
                        components: None,
                        anonymized: None,
                        sections: Some(Sections {
                            summary: section == "summary",
                            metrics: section == "metrics",
                            diagnostics: section == "diagnostics",
                            benchmarks: section == "benchmarks",
                        }),
                    },
                )
                .unwrap();
                let bytes = std::fs::read(result["path"].as_str().unwrap()).unwrap();
                let content = String::from_utf8_lossy(&bytes);
                assert!(
                    !content.contains("PRIVATE"),
                    "{format}/{section} leaked an identifier"
                );
                match format {
                    "csv" => {
                        for candidate in ["summary", "metrics", "diagnostics", "benchmarks"] {
                            assert_eq!(
                                content.contains(&format!("\"{candidate}\"")),
                                section == candidate,
                                "CSV unexpectedly exported {candidate} in {section}"
                            );
                        }
                        if section == "diagnostics" {
                            assert!(content.contains("\"'=1+1\""));
                            assert!(content.contains("\"'@SUM(1,2)\""));
                        }
                        if section == "benchmarks" {
                            assert!(content.contains("\"'+synthetic only\""));
                        }
                    }
                    "xlsx" => {
                        for (candidate, name) in [
                            ("summary", "Summary"),
                            ("metrics", "Metrics"),
                            ("diagnostics", "Diagnostics"),
                            ("benchmarks", "Tests"),
                        ] {
                            assert_eq!(
                                content.contains(&format!("name=\"{name}\"")),
                                section == candidate,
                                "XLSX unexpectedly exported {candidate} in {section}"
                            );
                        }
                        assert!(
                            !content.contains("xl/charts/chart1.xml"),
                            "Chart must not include unchecked history"
                        );
                    }
                    _ => {
                        for (candidate, heading) in [
                            ("summary", "SUMMARY / COMPONENT AGGREGATES"),
                            ("metrics", "METRIC HISTORY"),
                            ("diagnostics", "TIMELINE"),
                            ("benchmarks", "BENCHMARK RESULTS"),
                        ] {
                            assert_eq!(
                                content.contains(heading),
                                section == candidate,
                                "PDF unexpectedly exported {candidate} in {section}"
                            );
                        }
                        if section == "diagnostics" {
                            assert!(content.contains("Configured threshold exceeded"));
                            assert!(content.contains("observed=95.00 degC; limit=90.00"));
                            assert!(!content.contains("????"));
                        }
                    }
                }
            }
        }
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                for report in std::fs::read_dir(&path).unwrap() {
                    std::fs::remove_file(report.unwrap().path()).unwrap();
                }
                std::fs::remove_dir(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
        std::fs::remove_dir(folder).unwrap();
    }
    #[test]
    fn component_selection_applies_before_timeline_limit_and_filters_tests() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        db.exec("CREATE TABLE timeline(at_ms INTEGER,component_id TEXT); INSERT INTO timeline VALUES(1,'network'),(2,'cpu');").unwrap();
        let filter = Filter::new(0, 10, Some(&["cpu".into()]));
        let rows = db
            .query(&filter.timeline("component_id", 1), &filter.args)
            .unwrap();
        assert_eq!(rows[0][0], "cpu");
        let aliases = HashMap::from([("cpu".into(), "cpu-1".into())]);
        assert!(benchmark_allowed(&json!("cpu_single"), &filter, &aliases));
        assert!(!benchmark_allowed(&json!("memory"), &filter, &aliases));
        assert!(!benchmark_allowed(
            &json!("network_ping"),
            &filter,
            &aliases
        ));
    }
    #[test]
    fn zip_stream_has_valid_central_directory() {
        let path = std::env::temp_dir().join(format!("nexus-zip-test-{}.zip", now_ms()));
        let file = File::create(&path).unwrap();
        let mut zip = Zip::new(file);
        zip.entry("test.xml", |w| {
            w.write_all(b"123456789").map_err(|e| e.to_string())
        })
        .unwrap();
        zip.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.windows(4).any(|v| v == 0xcbf43926u32.to_le_bytes()));
        assert_eq!(
            &bytes[bytes.len() - 22..bytes.len() - 18],
            &0x06054b50u32.to_le_bytes()
        );
        let _ = std::fs::remove_file(path);
    }
}
