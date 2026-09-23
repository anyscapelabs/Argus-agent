use super::store;
use crate::gateway::Gateway;

pub fn kind_of(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next().unwrap_or("").trim().to_lowercase();
    match ext.as_str() {
        "docx" => Some("docx"),
        "pdf" => Some("pdf"),
        "pptx" => Some("pptx"),
        "xlsx" => Some("xlsx"),
        "csv" => Some("csv"),
        "md" => Some("md"),
        "txt" => Some("txt"),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => None,
        _ => None,
    }
}

fn esc_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

struct ZipEntry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

struct ZipWriter {
    buf: Vec<u8>,
    entries: Vec<ZipEntry>,
}

impl ZipWriter {
    fn new() -> Self {
        ZipWriter {
            buf: vec![],
            entries: vec![],
        }
    }

    fn file(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_bytes = name.as_bytes();
        self.buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf
            .extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name_bytes);
        self.buf.extend_from_slice(data);
        self.entries.push(ZipEntry {
            name: name.into(),
            crc,
            size,
            offset,
        });
    }

    fn finish(mut self) -> Vec<u8> {
        let dir_off = self.buf.len() as u32;
        let mut count: u32 = 0;
        for e in &self.entries {
            let name_bytes = e.name.as_bytes();
            self.buf.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            self.buf.extend_from_slice(&20u16.to_le_bytes());
            self.buf.extend_from_slice(&20u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&e.crc.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf.extend_from_slice(&e.size.to_le_bytes());
            self.buf
                .extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u32.to_le_bytes());
            self.buf.extend_from_slice(&e.offset.to_le_bytes());
            self.buf.extend_from_slice(name_bytes);
            count += 1;
        }
        let dir_size = self.buf.len() as u32 - dir_off;
        self.buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&count.to_le_bytes());
        self.buf.extend_from_slice(&count.to_le_bytes());
        self.buf.extend_from_slice(&dir_size.to_le_bytes());
        self.buf.extend_from_slice(&dir_off.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf
    }
}

fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            if cur.is_empty() {
                cur.push_str(word);
                continue;
            }
            if cur.len() + 1 + word.len() > width {
                lines.push(cur);
                cur = word.to_string();
                continue;
            }
            cur.push(' ');
            cur.push_str(word);
        }
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn esc_pdf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            _ if c.is_control() => {}
            _ => out.push(c),
        }
    }
    out
}

fn pdf_bytes(title: &str, body: &str) -> Vec<u8> {
    let mut text_lines: Vec<String> = vec![];
    if !title.trim().is_empty() {
        text_lines.push(title.trim().to_string());
        text_lines.push(String::new());
    }
    text_lines.extend(wrap_lines(body, 88));
    let per_page: usize = 44;
    let pages: Vec<Vec<String>> = text_lines.chunks(per_page).map(|c| c.to_vec()).collect();
    let npages = pages.len().max(1);
    let mut objects: Vec<Vec<u8>> = vec![];
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..npages)
                .map(|i| format!("{} 0 R", 4 + i * 2))
                .collect::<Vec<_>>()
                .join(" "),
            npages
        )
        .into_bytes(),
    );
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec());
    for (i, lines) in pages.iter().enumerate() {
        let content = lines
            .iter()
            .enumerate()
            .map(|(j, l)| format!("BT /F1 11 Tf 56 {} Td ({}) Tj ET", 760 - j * 16, esc_pdf(l)))
            .collect::<Vec<_>>()
            .join("\n");
        let page_obj = (4 + i * 2) as u32;
        let stream_obj = page_obj + 1;
        objects.push(
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", stream_obj).into_bytes(),
        );
        let mut stream = vec![];
        stream.extend_from_slice(format!("<< /Length {} >>\nstream\n", content.len()).as_bytes());
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = vec![];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            objects.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_bytes(rows: &[Vec<String>]) -> Vec<u8> {
    rows.iter()
        .map(|r| r.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn is_number_cell(s: &str) -> bool {
    !s.is_empty() && s.parse::<f64>().is_ok()
}

fn col_name(mut n: usize) -> String {
    let mut out = String::new();
    n += 1;
    while n > 0 {
        let m = (n - 1) % 26;
        out.insert(0, (b'A' + m as u8) as char);
        n = (n - 1) / 26;
    }
    out
}

fn xlsx_bytes(rows: &[Vec<String>]) -> Vec<u8> {
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

fn docx_para(text: &str, size: u32, bold: bool) -> String {
    format!(
        "<w:p><w:pPr><w:spacing w:after=\"160\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"{}\"/>{}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        size * 2,
        if bold { "<w:b/>" } else { "" },
        esc_xml(text)
    )
}

fn docx_bytes(title: &str, body: &str) -> Vec<u8> {
    let mut doc = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");
    if !title.trim().is_empty() {
        doc.push_str(&docx_para(title.trim(), 28, true));
    }
    for line in body.split('\n') {
        doc.push_str(&docx_para(line, 11, false));
    }
    doc.push_str("</w:body></w:document>");
    let mut z = ZipWriter::new();
    z.file("[Content_Types].xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"{}\"><Default Extension=\"rels\" ContentType=\"{}\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>", "http://schemas.openxmlformats.org/package/2006/content-types", "application/vnd.openxmlformats-package.relationships+xml").as_bytes());
    z.file("_rels/.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"word/document.xml\"/></Relationships>", "http://schemas.openxmlformats.org/package/2006/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument").as_bytes());
    z.file("word/document.xml", doc.as_bytes());
    z.finish()
}

fn pptx_shape(
    id: u32,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    texts: &[String],
    size: u32,
    bold: bool,
) -> String {
    let ps = texts
        .iter()
        .map(|t| {
            format!(
                "<a:p><a:r><a:rPr sz=\"{}\"{} /><a:t>{}</a:t></a:r></a:p>",
                size * 100,
                if bold { " b=\"1\"" } else { "" },
                esc_xml(t)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"box{}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>", id, id, x, y, w, h, ps)
}

fn pptx_slide(title: &str, bullets: &[String]) -> Vec<u8> {
    let mut texts = vec![title.to_string()];
    texts.extend(bullets.iter().map(|b| format!("{} {}", "\u{2022}", b)));
    let shape = pptx_shape(2, 685800, 365760, 7772400, 4000000, &texts, 20, false);
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld></p:sld>", shape).into_bytes()
}

fn pptx_bytes(title: &str, slides: &[(String, Vec<String>)]) -> Vec<u8> {
    let all: Vec<(String, Vec<String>)> = if slides.is_empty() {
        vec![(title.to_string(), vec![])]
    } else {
        slides.to_vec()
    };
    let n = all.len();
    let mut z = ZipWriter::new();
    let mut overrides = String::new();
    for i in 1..=n {
        overrides.push_str(&format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", i));
    }
    z.file("[Content_Types].xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"{}\"><Default Extension=\"rels\" ContentType=\"{}\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>{}</Types>", "http://schemas.openxmlformats.org/package/2006/content-types", "application/vnd.openxmlformats-package.relationships+xml", overrides).as_bytes());
    z.file("_rels/.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"ppt/presentation.xml\"/></Relationships>", "http://schemas.openxmlformats.org/package/2006/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument").as_bytes());
    let sld_ids: String = (1..=n)
        .map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId{}\" />", 255 + i, i))
        .collect::<Vec<_>>()
        .join("");
    let pres_rels: String = (1..=n)
        .map(|i| {
            format!(
                "<Relationship Id=\"rId{}\" Type=\"{}\" Target=\"slides/slide{}.xml\"/>",
                i, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide", i
            )
        })
        .collect::<Vec<_>>()
        .join("");
    z.file("ppt/presentation.xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><p:sldMasterIdLst><p:sldMasterId r:id=\"rIdMaster\"/></p:sldMasterIdLst><p:sldIdLst>{}</p:sldIdLst><p:sldSzCx cx=\"9144000\" cy=\"5143500\"/></p:presentation>", sld_ids).as_bytes());
    z.file("ppt/_rels/presentation.xml.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdMaster\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>{}</Relationships>", pres_rels).as_bytes());
    z.file("ppt/slideMasters/slideMaster1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"1A1A1A\"/></a:solidFill></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\"/></a:lvl1pPr></p:titleStyle></p:txStyles></p:sldMaster>");
    z.file("ppt/slideLayouts/slideLayout1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" type=\"titleAndContent\" preserve=\"1\"><p:cSld name=\"Title and Content\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld></p:sldLayout>");
    z.file("ppt/theme/theme1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Argus\"><a:themeElements><a:clrScheme name=\"x\"><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk1><a:srgbClr val=\"1A1A1A\"/></a:dk1></a:clrScheme></a:themeElements></a:theme>");
    z.file("docProps/core.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"><cp:title>Argus</cp:title></cp:coreProperties>");
    z.file("docProps/app.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Argus</Application></Properties>");
    for (i, (t, bullets)) in all.iter().enumerate() {
        let n = i + 1;
        z.file(
            &format!("ppt/slides/slide{}.xml", n),
            &pptx_slide(t, bullets),
        );
        z.file(
            &format!("ppt/slides/_rels/slide{}.xml.rels", n),
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../../slideLayouts/slideLayout1.xml\"/></Relationships>").as_bytes(),
        );
    }
    z.finish()
}

fn parse_rows(v: &serde_json::Value) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![];
    let arr = match v.as_array() {
        Some(a) => a,
        None => return out,
    };
    for row in arr {
        match row {
            serde_json::Value::Array(cells) => {
                out.push(
                    cells
                        .iter()
                        .map(|c| match c {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => String::new(),
                        })
                        .collect(),
                );
            }
            serde_json::Value::String(s) => out.push(vec![s.clone()]),
            _ => {}
        }
    }
    out
}

fn parse_slides(v: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    let arr = match v.as_array() {
        Some(a) => a,
        None => return out,
    };
    for s in arr {
        let title = s
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let bullets: Vec<String> = s
            .get("bullets")
            .and_then(|b| b.as_array())
            .map(|a| {
                a.iter()
                    .map(|c| match c {
                        serde_json::Value::String(x) => x.clone(),
                        _ => String::new(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push((title, bullets));
    }
    out
}

fn le_u16(b: &[u8], at: usize) -> usize {
    (b[at] as usize) | ((b[at + 1] as usize) << 8)
}

fn le_u32(b: &[u8], at: usize) -> usize {
    (b[at] as usize)
        | ((b[at + 1] as usize) << 8)
        | ((b[at + 2] as usize) << 16)
        | ((b[at + 3] as usize) << 24)
}

fn find_document_xml(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut at = 0;
    while at + 30 <= bytes.len() {
        if &bytes[at..at + 4] != b"PK\x03\x04" {
            at += 1;
            continue;
        }
        let method = le_u16(bytes, at + 8);
        let comp_sz = le_u32(bytes, at + 18);
        let name_len = le_u16(bytes, at + 26);
        let ext_len = le_u16(bytes, at + 28);
        let head = at + 30;
        if head + name_len > bytes.len() {
            break;
        }
        let name = &bytes[head..head + name_len];
        let data_at = head + name_len + ext_len;
        if data_at + comp_sz > bytes.len() {
            break;
        }
        if name == b"word/document.xml" {
            if method != 0 {
                return None;
            }
            return Some(bytes[data_at..data_at + comp_sz].to_vec());
        }
        at = data_at + comp_sz;
    }
    None
}

fn xml_decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn para_text(inner: &str) -> String {
    let mut out = String::new();
    let mut rest = inner;
    while let Some(s) = rest.find("<w:t") {
        let gt = match rest[s..].find('>') {
            Some(g) => s + g + 1,
            None => break,
        };
        let end = match rest[gt..].find("</w:t>") {
            Some(e) => gt + e,
            None => break,
        };
        out.push_str(&xml_decode(&rest[gt..end]));
        rest = &rest[end + 6..];
    }
    out
}

fn para_markdown(inner: &str) -> Option<String> {
    let txt = para_text(inner).trim().to_string();
    if txt.is_empty() {
        return None;
    }
    if inner.contains("w:val=\"Heading1\"") || inner.contains("w:val=\"Title\"") {
        return Some(format!("# {txt}"));
    }
    if inner.contains("w:val=\"Heading2\"") {
        return Some(format!("## {txt}"));
    }
    if inner.contains("w:val=\"Heading3\"") {
        return Some(format!("### {txt}"));
    }
    if inner.contains("<w:numPr") {
        return Some(format!("- {txt}"));
    }
    if inner.contains("<w:b/>") && inner.contains("w:val=\"56\"") {
        return Some(format!("# {txt}"));
    }
    Some(txt)
}

pub fn preview_docx(bytes: &[u8]) -> Option<String> {
    let xml = find_document_xml(bytes)?;
    let text = String::from_utf8_lossy(&xml).into_owned();
    let mut out: Vec<String> = vec![];
    let mut rest = text.as_str();
    while let Some(s) = rest.find("<w:p") {
        let after = rest[s + 4..].chars().next();
        match after {
            Some('>') | Some(' ') => {}
            _ => {
                rest = &rest[s + 4..];
                continue;
            }
        }
        let gt = match rest[s..].find('>') {
            Some(g) => s + g + 1,
            None => break,
        };
        let end = match rest[gt..].find("</w:p>") {
            Some(e) => gt + e,
            None => break,
        };
        if let Some(md) = para_markdown(&rest[s..end]) {
            out.push(md);
        }
        rest = &rest[end + 6..];
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("\n\n"))
}

pub struct BuiltDoc {
    pub ext: String,
    pub bytes: Vec<u8>,
    pub pages: i64,
}

pub fn build(kind: &str, title: &str, args: &serde_json::Value) -> Result<BuiltDoc, String> {
    match kind {
        "md" | "txt" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let pages = (content.chars().count() as i64 / 1800 + 1).max(1);
            Ok(BuiltDoc {
                ext: kind.into(),
                bytes: content.into_bytes(),
                pages,
            })
        }
        "csv" => {
            let rows = parse_rows(args.get("rows").unwrap_or(&serde_json::Value::Null));
            if rows.is_empty() {
                return Err("doc rows are empty".into());
            }
            let pages = 1;
            Ok(BuiltDoc {
                ext: "csv".into(),
                bytes: csv_bytes(&rows),
                pages,
            })
        }
        "pdf" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() && title.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let lines = wrap_lines(&content, 88).len() as i64;
            let pages = (lines / 44 + 1).max(1);
            Ok(BuiltDoc {
                ext: "pdf".into(),
                bytes: pdf_bytes(title, &content),
                pages,
            })
        }
        "docx" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() && title.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let pages = (content.chars().count() as i64 / 1800 + 1).max(1);
            Ok(BuiltDoc {
                ext: "docx".into(),
                bytes: docx_bytes(title, &content),
                pages,
            })
        }
        "xlsx" => {
            let rows = parse_rows(args.get("rows").unwrap_or(&serde_json::Value::Null));
            if rows.is_empty() {
                return Err("doc rows are empty".into());
            }
            Ok(BuiltDoc {
                ext: "xlsx".into(),
                bytes: xlsx_bytes(&rows),
                pages: 1,
            })
        }
        "pptx" => {
            let slides = parse_slides(args.get("slides").unwrap_or(&serde_json::Value::Null));
            let slides = if slides.is_empty() {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if content.trim().is_empty() && title.trim().is_empty() {
                    return Err("doc slides are empty".into());
                }
                vec![(
                    title.to_string(),
                    content.lines().map(|l| l.to_string()).collect(),
                )]
            } else {
                slides
            };
            let pages = slides.len() as i64;
            Ok(BuiltDoc {
                ext: "pptx".into(),
                bytes: pptx_bytes(title, &slides),
                pages,
            })
        }
        _ => Err("unsupported document kind".into()),
    }
}

pub fn create(
    gw: &Gateway,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> Result<(super::schema::LibItem, i64), String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("missing name")?;
    let kind = args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or("missing kind")?;
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&name)
        .to_string();
    if kind_of(&format!("x.{kind}")).is_none() {
        return Err("unsupported kind: use docx, pdf, pptx, xlsx, csv, md or txt".into());
    }
    let built = build(&kind, &title, args)?;
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let item = store::create_bytes(
        &conn,
        &gw.library_dir,
        &name,
        &built.ext,
        &built.bytes,
        session_id,
    )?;
    Ok((item, built.pages))
}
