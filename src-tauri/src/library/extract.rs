use std::io::Read;

use super::doc::para_markdown;
use super::doc::xml_decode;

/// Past this, extraction costs more than the answer is worth.
pub const MAX_BYTES: usize = 25_000_000;

/// A binary file announces itself: decoding it produces replacement
/// characters, and a file that is mostly those is not text.
const BINARY_RATIO: f64 = 0.10;

/// Every kind of file a model can be asked to read. `None` means this file
/// has no text we can get at — never an empty string, which reads as an
/// empty file rather than an unreadable one.
pub fn text(bytes: &[u8], ext: &str) -> Option<String> {
    if bytes.len() > MAX_BYTES {
        return None;
    }

    match ext {
        "docx" => docx(bytes),
        "xlsx" | "xlsm" => xlsx(bytes),
        "pptx" => pptx(bytes),
        "odt" | "ods" | "odp" => odf(bytes),
        "pdf" => pdf(bytes),
        _ => plain(bytes),
    }
}

// The zip family, all of it. Word deflates, which is why the old reader that
// only understood stored entries never saw a real .docx.
fn entry(bytes: &[u8], name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut file = zip.by_name(name).ok()?;
    let mut out = String::new();

    file.read_to_string(&mut out).ok()?;
    Some(out)
}

fn entries(bytes: &[u8], prefix: &str, suffix: &str) -> Vec<String> {
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return vec![];
    };

    // Ascending by number, so `slide2.xml` lands between 1 and 10 rather than
    // wherever its name sorts.
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .collect();

    names.sort_by_key(|n| {
        n.trim_start_matches(prefix)
            .trim_end_matches(suffix)
            .parse::<u32>()
            .unwrap_or(0)
    });

    names
}

// docx. The paragraph-to-markdown logic is the old reader's and is good; only
// the entry lookup changed.
fn docx(bytes: &[u8]) -> Option<String> {
    let xml = entry(bytes, "word/document.xml")?;
    let mut out: Vec<String> = vec![];
    let mut rest = xml.as_str();

    while let Some(s) = rest.find("<w:p") {
        let after = rest[s + 4..].chars().next();
        match after {
            Some('>') | Some(' ') => {}
            _ => {
                rest = &rest[s + 4..];
                continue;
            }
        }

        let Some(gt) = rest[s..].find('>') else {
            break;
        };

        let gt = s + gt + 1;
        let Some(end) = rest[gt..].find("</w:p>") else {
            break;
        };

        if let Some(md) = para_markdown(&rest[s..gt + end]) {
            out.push(md);
        }

        rest = &rest[gt + end + 6..];
    }

    (!out.is_empty()).then(|| out.join("\n\n"))
}

// xlsx. Cells reference the shared string table by index and each other by
// `r="B7"`, so the grid has to be built from the ref, not from the order.
fn xlsx(bytes: &[u8]) -> Option<String> {
    let shared = entry(bytes, "xl/sharedStrings.xml")
        .map(|s| strings(&s))
        .unwrap_or_default();

    let names = entries(bytes, "xl/worksheets/sheet", ".xml");
    let mut out = vec![];

    for name in names {
        let Some(xml) = entry(bytes, &name) else {
            continue;
        };

        let rows = sheet(&xml, &shared);
        if rows.is_empty() {
            continue;
        }

        out.push(format!("## {}\n\n{}", title(&name), table(&rows)));
    }

    (!out.is_empty()).then(|| out.join("\n\n"))
}

fn title(name: &str) -> String {
    name.trim_start_matches("xl/worksheets/")
        .trim_end_matches(".xml")
        .to_string()
}

/// Every `<t>` in the string table, in order. Rows then index into this.
fn strings(xml: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = xml;

    while let Some(s) = rest.find("<si") {
        let Some(close) = rest[s..].find("</si>") else {
            break;
        };

        let inner = &rest[s..s + close];
        out.push(tags(inner, "t"));
        rest = &rest[s + close + 5..];
    }

    out
}

/// The concatenated text of every `<tag>` run in a fragment.
fn tags(frag: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut rest = frag;

    while let Some(s) = rest.find(&open) {
        // `<t` also opens `<text`; the ref has to end at the name.
        let after = rest[s + open.len()..].chars().next();
        if !matches!(after, Some('>') | Some(' ') | Some('/')) {
            rest = &rest[s + open.len()..];
            continue;
        }

        let Some(gt) = rest[s..].find('>') else {
            break;
        };

        let gt = s + gt + 1;
        let Some(end) = rest[gt..].find(&close) else {
            break;
        };

        out.push_str(&rest[gt..gt + end]);
        rest = &rest[gt + end + close.len()..];
    }

    xml_decode(&out)
}

/// `"BC12"` is column 55. Only the letters matter.
fn col_of(refattr: &str) -> usize {
    let mut n = 0usize;

    for c in refattr.chars().take_while(|c| c.is_ascii_alphabetic()) {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }

    n
}

fn sheet(xml: &str, shared: &[String]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = vec![];
    let mut rest = xml;

    while let Some(s) = rest.find("<row") {
        let Some(rend) = rest[s..].find("</row>") else {
            break;
        };

        let inner = &rest[s..s + rend];
        let mut cells: Vec<String> = vec![];
        let mut cur = inner;

        while let Some(c) = cur.find("<c ") {
            let Some(cend) = cur[c..].find("</c>") else {
                break;
            };

            let cell = &cur[c..c + cend];
            let at = col_of(&attr(cell, "r").unwrap_or_default());
            let v = cell_value(cell, shared);

            // A row can skip cells, so the ref is the only column number.
            if at > 0 {
                while cells.len() < at - 1 {
                    cells.push(String::new());
                }

                cells.push(v);
            }

            cur = &cur[c + cend + 4..];
        }

        if !cells.is_empty() {
            rows.push(cells);
        }

        rest = &rest[s + rend + 6..];
    }

    rows
}

/// `t="s"` means the cell holds an index into the shared string table.
/// `t="inlineStr"` means the text is in the cell, under `<is>`. A number has no
/// `t` and is in `<v>`.
fn cell_value(cell: &str, shared: &[String]) -> String {
    match attr(cell, "t").as_deref() {
        Some("s") => tags(cell, "v")
            .parse::<usize>()
            .ok()
            .and_then(|i| shared.get(i).cloned())
            .unwrap_or_default(),
        Some("inlineStr") => tags(cell, "t"),
        _ => tags(cell, "v"),
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let at = tag.find(&key)? + key.len();
    let end = tag[at..].find('"')? + at;
    Some(tag[at..end].to_string())
}

fn table(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let pad = |r: &Vec<String>| -> String {
        (0..width)
            .map(|i| r.get(i).map(|c| c.replace('|', "\\|")).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let head = pad(&rows[0]);
    let rule = (0..width).map(|_| "---").collect::<Vec<_>>().join(" | ");
    let body = rows[1..].iter().map(pad).collect::<Vec<_>>().join("\n");

    format!("| {head} |\n| {rule} |\n{body}")
}

// pptx. Slide order is the number in the name, and each slide is a list of
// text runs.
fn pptx(bytes: &[u8]) -> Option<String> {
    let names = entries(bytes, "ppt/slides/slide", ".xml");
    let mut out = vec![];

    for (i, name) in names.iter().enumerate() {
        let Some(xml) = entry(bytes, name) else {
            continue;
        };

        let text = tags(&xml, "a:t");
        if text.trim().is_empty() {
            continue;
        }

        out.push(format!("## Slide {}\n\n{}", i + 1, text.trim()));
    }

    (!out.is_empty()).then(|| out.join("\n\n"))
}

// OpenDocument, which is a zip with one big XML in it.
fn odf(bytes: &[u8]) -> Option<String> {
    let xml = entry(bytes, "content.xml")?;
    let text = tags(&xml, "text:p");

    (!text.trim().is_empty()).then_some(text)
}

fn pdf(bytes: &[u8]) -> Option<String> {
    let out = pdf_extract::extract_text_from_mem(bytes).ok()?;
    (!out.trim().is_empty()).then_some(out)
}

// Everything else. A code file is text, and so is a `.env` nobody listed, so
// there is no extension table to keep in step with the disk.
fn plain(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let bad = text.chars().filter(|c| *c == '\u{fffd}').count();

    if !text.is_empty() && (bad as f64) / (text.chars().count() as f64) > BINARY_RATIO {
        return None;
    }

    (!text.trim().is_empty()).then_some(text.into_owned())
}
