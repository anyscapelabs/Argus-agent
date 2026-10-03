use super::docx::docx_bytes;
use super::edit::editable;
use super::pdf::pdf_bytes;
use super::pptx::pptx_bytes;
use super::sheet::xlsx_bytes;
use super::zip::wrap_lines;
use crate::gateway::Gateway;
use crate::library::schema::LibItem;
use crate::library::store;

pub fn apply_edits(text: &str, edits: &[(String, String)]) -> Result<String, String> {
    if edits.is_empty() {
        return Err("doc.patch needs at least one edit".into());
    }

    let mut cur = text.to_string();
    for (i, (old, new)) in edits.iter().enumerate() {
        if old.is_empty() {
            return Err(format!("edit {} has an empty old string", i + 1));
        }

        let hits = cur.match_indices(old).count();
        if hits == 0 {
            return Err(format!(
                "edit {} matched nothing (doc is {} chars) — library.read it first and copy the old text exactly",
                i + 1,
                cur.chars().count()
            ));
        }
        if hits > 1 {
            return Err(format!(
                "edit {} matched {hits} times — include more surrounding context so it matches once",
                i + 1
            ));
        }

        cur = cur.replacen(old, new, 1);
    }

    if cur.trim().is_empty() {
        return Err("patch emptied the document — refusing".into());
    }

    Ok(cur)
}

fn edits_of(args: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let arr = args
        .get("edits")
        .and_then(|v| v.as_array())
        .ok_or("doc.patch needs edits:[{old, new}]")?;

    let mut out = vec![];
    for e in arr {
        let old = e
            .get("old")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let new = e
            .get("new")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push((old, new));
    }

    Ok(out)
}

fn current_text(item: &LibItem, bytes: &[u8]) -> Result<String, String> {
    match item.ext.as_str() {
        "md" | "txt" | "csv" => String::from_utf8(bytes.to_vec())
            .map_err(|_| format!("{} is not text — use doc.edit instead", item.ext)),
        _ => crate::library::extract::text(bytes, &item.ext)
            .ok_or_else(|| format!("{} has no text to patch — use doc.edit instead", item.ext)),
    }
}

fn split_title(patched: &str, fallback: &str) -> (String, String) {
    let mut lines = patched.lines();
    let first = lines.next().unwrap_or("").trim();

    if let Some(t) = first.strip_prefix("# ") {
        if !t.trim().is_empty() {
            let rest: Vec<&str> = lines.collect();
            return (t.trim().to_string(), rest.join("\n"));
        }
    }

    (fallback.to_string(), patched.to_string())
}

fn table_rows(patched: &str) -> Result<Vec<Vec<String>>, String> {
    let all: Vec<&str> = patched.lines().collect();
    let start = all
        .iter()
        .position(|l| l.trim_start().starts_with('|'))
        .ok_or("no table found in the patched text — use doc.edit with rows")?;

    let mut raw = vec![];
    for line in &all[start..] {
        let t = line.trim();
        if t.is_empty() || t.starts_with("## ") {
            break;
        }
        if t.starts_with('|') || t.contains('|') {
            raw.push(*line);
        } else {
            break;
        }
    }

    let mut rows = vec![];
    for line in raw {
        let cells: Vec<String> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().replace("\\|", "|"))
            .collect();

        let is_rule = !cells.is_empty()
            && cells
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'));
        if is_rule {
            continue;
        }
        rows.push(cells);
    }

    if rows.is_empty() {
        return Err("no table found in the patched text — use doc.edit with rows".into());
    }

    Ok(rows)
}

fn slide_sections(patched: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut slides: Vec<(String, Vec<String>)> = vec![];
    let mut title = String::new();
    let mut bullets: Vec<String> = vec![];
    let mut open = false;

    for line in patched.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("## ") {
            if open || !title.is_empty() || !bullets.is_empty() {
                slides.push((std::mem::take(&mut title), std::mem::take(&mut bullets)));
            }
            let h = h.trim().strip_prefix("Slide ").unwrap_or(h).trim();
            let h = h.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
            title = if h.is_empty() {
                "Slide".into()
            } else {
                h.into()
            };
            open = true;
            continue;
        }

        if !open {
            continue;
        }
        if t.is_empty() {
            continue;
        }
        bullets.push(t.trim_start_matches(['•', '-', '*']).trim().to_string());
    }

    if open || !title.is_empty() || !bullets.is_empty() {
        slides.push((title, bullets));
    }

    if slides.is_empty() {
        return Err("no slides found in the patched text — use doc.edit with slides".into());
    }

    Ok(slides)
}

fn rebuild(
    ext: &str,
    name: &str,
    title_arg: Option<&str>,
    patched: &str,
) -> Result<(Vec<u8>, i64), String> {
    match ext {
        "md" | "txt" | "csv" => {
            let pages = (patched.chars().count() as i64 / 1800 + 1).max(1);
            Ok((patched.as_bytes().to_vec(), pages))
        }
        "docx" => {
            let (title, body) = match title_arg {
                Some(t) => (t.to_string(), patched.to_string()),
                None => split_title(patched, name),
            };
            let pages = (body.chars().count() as i64 / 1800 + 1).max(1);
            Ok((docx_bytes(&title, &body), pages))
        }
        "pdf" => {
            let (title, body) = match title_arg {
                Some(t) => (t.to_string(), patched.to_string()),
                None => split_title(patched, name),
            };
            let lines = wrap_lines(&body, 88).len() as i64;
            Ok((pdf_bytes(&title, &body), (lines / 44 + 1).max(1)))
        }
        "xlsx" => {
            let rows = table_rows(patched)?;
            Ok((xlsx_bytes(&rows), 1))
        }
        "pptx" => {
            let slides = slide_sections(patched)?;
            let pages = slides.len() as i64;
            let head = title_arg.unwrap_or(name).to_string();
            Ok((pptx_bytes(&head, &slides), pages))
        }
        _ => Err("unsupported document kind".into()),
    }
}

pub fn patch(gw: &Gateway, args: &serde_json::Value) -> Result<(LibItem, i64), String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("doc.patch needs an id — library.read shows it")?;

    let edits = edits_of(args)?;
    if edits.len() > 20 {
        return Err("at most 20 edits per call — split it up".into());
    }

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let item = store::get(&conn, &gw.library_dir, &id)?;
    drop(conn);

    if !editable(&item.ext) {
        return Err(format!(
            "only Argus documents can be edited ({} is not one)",
            item.ext
        ));
    }

    let dir = gw.library_dir.clone();
    let bytes = std::fs::read(dir.join(&item.path)).map_err(|err| err.to_string())?;
    let cur = current_text(&item, &bytes)?;
    let patched = apply_edits(&cur, &edits)?;

    if patched == cur {
        return Ok((item, 0));
    }

    let title_arg = args
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());
    let (next_bytes, pages) = rebuild(&item.ext, &item.name, title_arg, &patched)?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let next = store::update_bytes(&conn, &gw.library_dir, &id, &next_bytes)?;
    Ok((next, pages))
}
