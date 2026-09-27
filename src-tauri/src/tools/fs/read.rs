use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::tools::ToolMeta;

pub const META: &[ToolMeta] = &[ToolMeta {
    name: "fs.read",
    desc: "Read a file the user attached to this conversation. Not a general file browser: the only paths it will open are the ones that came in with a message, so if the user wants a folder read they have to say so. Documents, spreadsheets, presentations, PDFs, source code and plain text all come back as text; an image is already in the message and does not need reading. The whole file comes back by default. Only a file too large for one reply is split, and the reply says the exact offset to continue from, so asking again moves you forward instead of returning the same page.",
    args: "{\"path\":\"...\",\"offset\":0}",
    mutating: false,
}];

/// What one reply can carry. Past this the file is split, never trimmed.
const MAX_CHARS: usize = 60_000;

pub fn read(conn: &Connection, library_dir: &Path, args: &Value) -> Result<String, String> {
    let sid = crate::tools::notepad::current_session()
        .ok_or("permission denied: fs.read only reads files attached to a conversation")?;

    let Some(path) = args.get("path").and_then(Value::as_str) else {
        return Err("fs.read needs a path".into());
    };

    let want = resolve(path);

    if !attached_paths(conn, &sid, library_dir)?.contains(&want) {
        return Err(format!(
            "permission denied: {path} was not attached to this conversation. Only a file \
             the user sent can be read this way. If they want a different one, ask them \
             to attach it."
        ));
    }

    let name = want
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());

    let bytes = std::fs::read(&want).map_err(|err| format!("{name} could not be opened: {err}"))?;

    let ext = want
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let Some(text) = crate::library::extract::text(&bytes, &ext) else {
        return Err(format!(
            "{name} ({ext}) has no text I can read. It may be a binary format or a \
             password-protected document. An image does not need this — it is already in \
             the message."
        ));
    };

    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize;

    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();

    if offset > 0 && offset >= total {
        return Ok(format!(
            "{name} is {total} characters — you are past the end."
        ));
    }

    let body: String = chars.iter().skip(offset).take(MAX_CHARS).collect();
    let next = offset + body.chars().count();

    let head = if offset == 0 {
        format!("{name} ({ext}, {total} characters)\n\n")
    } else {
        format!("{name} ({ext}) — characters {offset} to {next} of {total}\n\n")
    };

    // The point of the whole thing: the next call is told exactly where to
    // pick up, so "read the rest" cannot land on the same page twice.
    let tail = if next < total {
        format!(
            "\n\n[{name} is {total} characters. You have {offset} to {next}. \
             Continue with fs.read offset={next}.]"
        )
    } else {
        String::new()
    };

    Ok(format!("{head}{body}{tail}"))
}

/// Every file handed to this conversation, canonicalised. A path is only
/// openable if it is on this list, which is why attaching a file is the whole
/// of the permission.
fn attached_paths(
    conn: &Connection,
    sid: &str,
    library_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT attachments FROM messages
             WHERE session_id = ?1 AND attachments IS NOT NULL",
        )
        .map_err(|err| err.to_string())?;

    let raws = stmt
        .query_map([sid], |r| r.get::<_, String>(0))
        .map_err(|err| err.to_string())?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    let mut out: Vec<PathBuf> = vec![];

    for raw in raws {
        let Ok(items) = serde_json::from_str::<Vec<crate::library::schema::Attachment>>(&raw)
        else {
            continue;
        };

        for a in items {
            let Some(p) = a.path.map(PathBuf::from).or_else(|| {
                a.id.as_deref()
                    .and_then(|id| crate::library::store::abs_of(conn, library_dir, id))
            }) else {
                continue;
            };

            out.push(canonical(&p));
        }
    }

    Ok(out)
}

fn resolve(path: &str) -> PathBuf {
    canonical(Path::new(&crate::tools::expand(path)))
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}
