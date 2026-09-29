// DOCX writing: a WordprocessingML paragraph per block, wrapped in the
// three parts Word requires.
use super::zip::{esc_xml, ZipWriter};

pub(super) fn docx_para(text: &str, size: u32, bold: bool) -> String {
    format!(
        "<w:p><w:pPr><w:spacing w:after=\"160\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"{}\"/>{}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        size * 2,
        if bold { "<w:b/>" } else { "" },
        esc_xml(text)
    )
}

pub(super) fn docx_bytes(title: &str, body: &str) -> Vec<u8> {
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
