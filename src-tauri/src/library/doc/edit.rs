// Full-rewrite path: same id, same kind, new bytes. The patch tool shares
// the overwrite below; this one just rebuilds from complete arguments.
use super::build::build;
use crate::gateway::Gateway;
use crate::library::schema::LibItem;
use crate::library::store;

const EDITABLE: &[&str] = &["docx", "pdf", "pptx", "xlsx", "csv", "md", "txt"];

pub fn editable(ext: &str) -> bool {
    EDITABLE.contains(&ext)
}

pub fn edit(
    gw: &Gateway,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> Result<(LibItem, i64), String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("doc.edit needs an id — library.read shows it")?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let item = store::get(&conn, &gw.library_dir, &id)?;
    drop(conn);

    if !editable(&item.ext) {
        return Err(format!(
            "only Argus documents can be edited ({} is not one)",
            item.ext
        ));
    }

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| item.name.clone());

    let built = build(&item.ext, &title, args)?;
    if built.ext != item.ext {
        return Err("rebuild changed the file kind — aborting".into());
    }

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let next = store::update_bytes(&conn, &gw.library_dir, &id, &built.bytes)?;
    drop(conn);

    let _ = session_id;
    Ok((next, built.pages))
}
