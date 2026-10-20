"""Read-only independent validation of native --self-check exports.

Use the bundled Python runtime (openpyxl and pypdf). Does not author reports.
"""
import csv
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
assert len(csv_rows) >= 360, "Expected the synthetic integration fixture, not an empty report"
assert all("PRIVATE" not in str(row) for row in csv_rows), "Anonymization leaked fixture identifiers"
loads = [float(row["average"]) for row in csv_rows if row["kind"] == "cpu" and row["metric"] == "load"]
assert math.isclose(min(loads), 0) and math.isclose(max(loads), 59.5), "Zero or endpoint lost"
missing = [row for row in csv_rows if row["kind"] == "ram"]
assert missing and all(row["average"] == "" and row["samples"] == "0" for row in missing), "Missing RAM was converted to zero"
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
assert len(metrics) == len(csv_rows) + 1
cpu_rows = list(book["CPU"].values)
load_row = next(row for row in cpu_rows[1:] if row[1] == "load")
assert load_row[3] == 0 and load_row[4] == 59.5 and math.isclose(load_row[5], 29.75), "XLSX aggregate mismatch"
ram_rows = list(book["RAM"].values)
assert all(row[3:6] == (None, None, None) for row in ram_rows[1:]), "XLSX blank/zero mismatch"
assert any("Повышенная температура" in str(row) for row in book["Diagnostics"].values), "Diagnostic explanation missing"
for sheet in book:
    assert all(cell.data_type != "f" for row in sheet for cell in row), "Exported source text became a formula"
sheet_names = book.sheetnames
book.close()
pdf = PdfReader(paths["pdf"])
pdf_text = "\n".join(page.extract_text() for page in pdf.pages)
assert "NEXUS" in pdf_text and "59.50" in pdf_text
assert "PRIVATE" not in pdf_text
result = {"fixture": "synthetic, not physical hardware", "csvRows": len(csv_rows), "xlsxSheets": sheet_names,
          "zipCrc": "pass", "xml": "pass", "typedNumericAggregates": "pass", "missingVersusZero": "pass",
          "privacy": "pass", "pdfPages": len(pdf.pages), "pdfText": "pass",
          "nativeExcelApplication": "not tested by this script", "pdfVisualReview": "render separately"}
(root / "verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(result, ensure_ascii=False, indent=2))
