// Attachments: resolve to a file, inline the ones small enough, unwrap a tool
// result's body for the transcript.
use std::path::Path;

use rusqlite::Connection;

use crate::library::schema::Attachment;

/// The file behind an attachment. A message outlives the file it named, and
/// that is not a reason to fail.
pub(super) fn attachment_path(
    conn: &Connection,
    library_dir: &Path,
    a: &Attachment,
) -> Option<String> {
    if let Some(p) = a.path.as_deref() {
        return Some(p.to_string());
    }

    let id = a.id.as_deref()?;

    crate::library::store::abs_of(conn, library_dir, id).map(|p| p.to_string_lossy().into_owned())
}

/// Images go inline; the model can already see one. A document that will not
/// fit is not trimmed: it is named with its path and read whole, in pieces. A
/// partial copy is worse than none — the model cannot tell it is partial.
pub(super) fn attachments(
    raw: Option<&str>,
    resolve: &dyn Fn(&Attachment) -> Option<String>,
) -> (Vec<String>, String) {
    let Some(raw) = raw else {
        return (vec![], String::new());
    };

    let items: Vec<Attachment> = serde_json::from_str(raw).unwrap_or_default();
    if items.is_empty() {
        return (vec![], String::new());
    }

    let mut images = vec![];
    let mut inlined: Vec<(String, String)> = vec![];
    let mut named = vec![];
    let mut room = INLINE_TOTAL_CHARS;

    for a in &items {
        if a.kind == "image" {
            if let Some(path) = resolve(a) {
                images.push(path);
            }

            continue;
        }

        let path = resolve(a);
        let text = path.as_deref().and_then(|p| inline_body(p, room));

        let Some(text) = text else {
            named.push(match path {
                Some(p) => format!("- {} ({}, {} bytes) at {p}", a.name, a.kind, a.sz),
                None => format!(
                    "- {} ({}, {} bytes) — not on disk any more",
                    a.name, a.kind, a.sz
                ),
            });
            continue;
        };

        room = room.saturating_sub(text.len());
        inlined.push((a.name.clone(), text));
    }

    if named.is_empty() && inlined.is_empty() {
        return (images, String::new());
    }

    let mut note = String::from("<attachments>\n");

    if !named.is_empty() {
        note.push_str(&format!(
            "{} file{} came with this message and {} too big to include. \
             Call fs.read with the path to read one, then answer from what it returns.\n{}\n",
            named.len(),
            if named.len() == 1 { "" } else { "s" },
            if named.len() == 1 { "is" } else { "are" },
            named.join("\n")
        ));
    }

    for (name, text) in &inlined {
        note.push_str(&format!("\n--- {name} ---\n{text}\n"));
    }

    note.push_str("</attachments>");

    (images, note)
}

/// Past this, the read costs more than the answer is worth.
const INLINE_MAX_BYTES: u64 = 8_000_000;
const INLINE_TOTAL_CHARS: usize = 400_000;

/// The whole text of a file, or nothing. A partial file that looks whole is a
/// wrong answer.
fn inline_body(path: &str, room: usize) -> Option<String> {
    if room == 0 {
        return None;
    }

    let p = std::path::Path::new(path);

    if p.metadata().ok()?.len() > INLINE_MAX_BYTES {
        return None;
    }

    let bytes = std::fs::read(p).ok()?;
    let ext = p.extension()?.to_str()?.to_ascii_lowercase();
    let text = crate::library::extract::text(&bytes, &ext)?;

    if text.len() > room {
        return None;
    }

    Some(text.trim_end().to_string())
}

pub(super) fn unwrap_result(content: &str) -> String {
    let inner = match (content.find('>'), content.rfind("</tool-result>")) {
        (Some(open), Some(end)) if open < end => &content[open + 1..end],
        _ => content,
    };

    let err = content.contains("status=\"err\"");

    if err {
        format!("error: {inner}")
    } else {
        inner.to_string()
    }
}
