"""Read-only independent validation of native --self-check exports.

Use the bundled Python runtime (openpyxl and pypdf). Does not author reports.
"""
import csv
from datetime import datetime
import json
import math
import sys
import zipfile
from pathlib import Path
import xml.etree.ElementTree as ET

import openpyxl
from pypdf import PdfReader

root = Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/native-self-check")
paths = {extension: max(root.glob(f"*.{extension}"), key=lambda p: p.stat().st_mtime) for extension in ("csv", "xlsx", "pdf")}
with paths["csv"].open(encoding="utf-8-sig", newline="") as handle:
    csv_rows = list(csv.DictReader(handle))
metric_rows = [row for row in csv_rows if row["record_type"] == "metrics"]
assert len(metric_rows) >= 360, "Expected the synthetic integration fixture, not an empty report"
assert {row["record_type"] for row in csv_rows} == {"summary", "metrics", "diagnostics", "benchmarks"}
assert all("PRIVATE" not in str(row) for row in csv_rows), "Anonymization leaked fixture identifiers"
assert all(not value.startswith(("=", "+", "-", "@", "\t", "\r")) for row in csv_rows for value in row.values()), "CSV cell can become a spreadsheet formula"
assert all(datetime.fromisoformat(row["timestamp_utc"]).utcoffset().total_seconds() == 0 for row in csv_rows)
assert all(row["unit"] == "%" for row in metric_rows if row["kind"] == "cpu" and row["metric"] == "load")
assert all(row["unit"] == "°C" for row in metric_rows if row["kind"] == "cpu" and row["metric"] == "temperature")
loads = [float(row["average"]) for row in metric_rows if row["kind"] == "cpu" and row["metric"] == "load"]
assert math.isclose(min(loads), 0) and math.isclose(max(loads), 59.5), "Zero or endpoint lost"
missing = [row for row in metric_rows if row["kind"] == "ram"]
assert missing and all(row["average"] == "" and row["samples"] == "0" for row in missing), "Missing RAM was converted to zero"
escaped = next(row for row in csv_rows if row["record_type"] == "diagnostics" and row["title"] == "'=1+1")
assert escaped["details"] == "'@SUM(1,2)\nQuoted \"fixture\"", "CSV quoting, multiline or formula protection changed new text fields"
benchmark = next(row for row in csv_rows if row["record_type"] == "benchmarks")
assert benchmark["kind"] == "cpu_single" and float(benchmark["score"]) == 1234.5 and benchmark["unit"] == "operations/s"
assert benchmark["details"].startswith("'+SYNTHETIC"), "Benchmark text formula prefix was not escaped"
with zipfile.ZipFile(paths["xlsx"]) as archive:
    assert archive.testzip() is None, "ZIP CRC failure"
    for name in archive.namelist():
        if name.endswith((".xml", ".rels")):
            ET.fromstring(archive.read(name))
    assert all(b"PRIVATE" not in archive.read(name) for name in archive.namelist()), "XLSX privacy leak"
    charts = [name for name in archive.namelist() if name.startswith("xl/charts/") and name.endswith(".xml")]
    assert charts, "Expected a native Excel chart in the synthetic CPU summary"
book = openpyxl.load_workbook(paths["xlsx"], read_only=True, data_only=False)
expected_sheets = {"Summary", "Metrics", "Diagnostics", "Tests", "CPU", "GPU", "RAM", "Storage", "Motherboard", "Network", "System"}
assert expected_sheets <= set(book.sheetnames), "Required component worksheets missing"
metrics = list(book["Metrics"].values)
assert len(metrics) == len(metric_rows) + 1
cpu_rows = list(book["CPU"].values)
load_row = next(row for row in cpu_rows[1:] if row[1] == "load")
assert load_row[3] == 0 and load_row[4] == 59.5 and math.isclose(load_row[5], 29.75), "XLSX aggregate mismatch"
ram_rows = list(book["RAM"].values)
assert all(row[3:6] == (None, None, None) for row in ram_rows[1:]), "XLSX blank/zero mismatch"
assert any("Повышенная температура" in str(row) for row in book["Diagnostics"].values), "Diagnostic explanation missing"
assert any(row[1:5] == ("cpu_single", "completed", 1234.5, "operations/s") for row in list(book["Tests"].values)[1:]), "Selected test results missing or lost numeric type"
for sheet in book:
    assert all(cell.data_type != "f" for row in sheet for cell in row), "Exported source text became a formula"
sheet_names = book.sheetnames
book.close()
pdf = PdfReader(paths["pdf"])
pdf_text = "\n".join(page.extract_text() for page in pdf.pages)
assert "NEXUS" in pdf_text and "59.50" in pdf_text
assert "PRIVATE" not in pdf_text
assert "Configured threshold exceeded" in pdf_text and "observed=95.00 degC; limit=90.00" in pdf_text, "PDF diagnostic rule or observed values missing"
assert "????" not in pdf_text, "PDF exported unreadable diagnostic text"
headings = {"summary": "SUMMARY / COMPONENT AGGREGATES", "metrics": "METRIC HISTORY", "diagnostics": "TIMELINE", "benchmarks": "BENCHMARK RESULTS"}
section_sheets = {"summary": expected_sheets - {"Metrics", "Diagnostics", "Tests"}, "metrics": {"Metrics"}, "diagnostics": {"Diagnostics"}, "benchmarks": {"Tests"}}
for section in headings:
    selected_root = root / f"selected-{section}"
    selected = {extension: max(selected_root.glob(f"*.{extension}"), key=lambda p: p.stat().st_mtime) for extension in ("csv", "xlsx", "pdf")}
    with selected["csv"].open(encoding="utf-8-sig", newline="") as handle:
        selected_rows = list(csv.DictReader(handle))
    assert selected_rows and {row["record_type"] for row in selected_rows} == {section}, f"CSV exported unchecked content in {section}"
    assert all("PRIVATE" not in str(row) for row in selected_rows)
    with zipfile.ZipFile(selected["xlsx"]) as archive:
        assert archive.testzip() is None
        assert "xl/charts/chart1.xml" not in archive.namelist(), "Chart leaked unchecked metric history"
        for name in archive.namelist():
            if name.endswith((".xml", ".rels")):
                ET.fromstring(archive.read(name))
        assert all(b"PRIVATE" not in archive.read(name) for name in archive.namelist())
    selected_book = openpyxl.load_workbook(selected["xlsx"], read_only=True, data_only=False)
    assert set(selected_book.sheetnames) == section_sheets[section], f"XLSX exported unchecked worksheets in {section}"
    assert all(cell.data_type != "f" for sheet in selected_book for row in sheet for cell in row)
    selected_book.close()
    selected_pdf = PdfReader(selected["pdf"])
    selected_text = "\n".join(page.extract_text() for page in selected_pdf.pages)
    assert "PRIVATE" not in selected_text and "????" not in selected_text
    assert all((heading in selected_text) == (candidate == section) for candidate, heading in headings.items()), f"PDF exported unchecked content in {section}"
result = {"fixture": "synthetic, not physical hardware", "csvRows": len(csv_rows), "csvMetricRows": len(metric_rows), "xlsxSheets": sheet_names,
          "zipCrc": "pass", "xml": "pass", "typedNumericAggregates": "pass", "missingVersusZero": "pass",
          "privacy": "pass", "pdfPages": len(pdf.pages), "pdfText": "pass",
          "selectedSectionsAllFormats": "pass (12 exports)", "csvTextFormulaProtection": "pass", "utcAndUnits": "pass", "pdfEnglishRuleSummary": "pass",
          "nativeExcelApplication": "not tested by this script", "pdfVisualReview": "render separately"}
(root / "verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(result, ensure_ascii=False, indent=2))
