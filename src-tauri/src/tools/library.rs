use rusqlite::Connection;
use serde_json::Value;

use super::ToolMeta;

pub const META: &[ToolMeta] = &[ToolMeta {
    name: "library.read",
    desc: "Read a file from the user's library as text. Most attached documents arrive with their contents in the message already, so reach for this when the attachment listing named a file instead — one too large to include, or one you need more of than the message carried. Also use it when the user asks about a file they have added earlier. Pass the id exactly as it was given. Works on documents, spreadsheets, presentations, PDFs, source code and plain text; an image is already attached to the message and does not need reading. Never returns an empty string: a file with no extractable text says so, so an empty result means the call failed rather than the file being blank.",
    args: "{\"id\":\"...\",\"max_chars\":12000}",
    mutating: false,
}];

const DEFAULT_CHARS: usize = 12_000;
const MAX_CHARS: usize = 60_000;

pub fn read(conn: &Connection, dir: &std::path::Path, args: &Value) -> Result<String, String> {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return Err("library.read needs an id — the one the attachment was listed with".into());
    };

    let max = args
        .get("max_chars")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).min(MAX_CHARS))
        .unwrap_or(DEFAULT_CHARS);

    let item = crate::library::store::get(conn, dir, id)
        .map_err(|err| format!("no library file with id {id}: {err}"))?;

    let preview = crate::library::store::preview(conn, dir, id, max)
        .map_err(|err| format!("could not read {id}: {err}"))?;

    let Some(text) = preview.text else {
        return Err(format!(
            "{name} ({ext}, {sz} bytes) has no text I can read. It may be an image, a binary format, or a password-protected document. Try a different file or ask the user for the text.",
            name = item.name,
            ext = item.ext,
            sz = item.sz
        ));
    };

    let head = format!(
        "{name} ({ext}, {sz} bytes)\n\n",
        name = item.name,
        ext = item.ext,
        sz = item.sz
    );

    let body = if preview.truncated {
        format!(
            "{text}\n\n[truncated at {max} characters — ask for a later part if you need it]",
            text = text.trim_end()
        )
    } else {
        text
    };

    Ok(format!("{head}{body}"))
}
