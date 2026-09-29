// Reading a DOCX back: locate the document part inside the zip, decode its
// entities, and turn each paragraph into markdown.
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

pub fn xml_decode(s: &str) -> String {
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

pub fn para_markdown(inner: &str) -> Option<String> {
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
