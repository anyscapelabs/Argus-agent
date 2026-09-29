// Tabular output: CSV is written directly, XLSX wraps the same rows in a
// workbook. Numeric-looking cells are stored as numbers so Excel does not
// left-align them.
use super::zip::{esc_xml, ZipWriter};

pub(super) fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub(super) fn csv_bytes(rows: &[Vec<String>]) -> Vec<u8> {
    rows.iter()
        .map(|r| r.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

pub(super) fn is_number_cell(s: &str) -> bool {
    !s.is_empty() && s.parse::<f64>().is_ok()
}

pub(super) fn col_name(mut n: usize) -> String {
    let mut out = String::new();
    n += 1;
    while n > 0 {
        let m = (n - 1) % 26;
        out.insert(0, (b'A' + m as u8) as char);
        n = (n - 1) / 26;
    }
    out
}

pub(super) fn xlsx_bytes(rows: &[Vec<String>]) -> Vec<u8> {
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let nrows = rows.len().max(1);
    let dim = format!("A1:{}{}", col_name(ncols - 1), nrows);
    let mut sheet = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"{}\"><dimension ref=\"{}\"/><sheetData>",
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main", dim
    );
    for (i, row) in rows.iter().enumerate() {
        sheet.push_str(&format!("<row r=\"{}\">", i + 1));
        for (j, cell) in row.iter().enumerate() {
            let addr = format!("{}{}", col_name(j), i + 1);
            if is_number_cell(cell) {
                sheet.push_str(&format!("<c r=\"{}\"><v>{}</v></c>", addr, esc_xml(cell)));
            } else {
                sheet.push_str(&format!(
                    "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                    addr,
                    esc_xml(cell)
                ));
            }
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    let mut z = ZipWriter::new();
    z.file("[Content_Types].xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"{}\"><Default Extension=\"rels\" ContentType=\"{}\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>", "http://schemas.openxmlformats.org/package/2006/content-types", "application/vnd.openxmlformats-package.relationships+xml").as_bytes());
    z.file("_rels/.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"xl/workbook.xml\"/></Relationships>", "http://schemas.openxmlformats.org/package/2006/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument").as_bytes());
    z.file("xl/workbook.xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"{}\" xmlns:r=\"{}\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>", "http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://schemas.openxmlformats.org/officeDocument/2006/relationships").as_bytes());
    z.file("xl/_rels/workbook.xml.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"worksheets/sheet1.xml\"/></Relationships>", "http://schemas.openxmlformats.org/package/2006/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet").as_bytes());
    z.file("xl/worksheets/sheet1.xml", sheet.as_bytes());
    z.finish()
}
