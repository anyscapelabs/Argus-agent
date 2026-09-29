// The entry point the document tool calls: build the bytes, then store them in
// the library and return the item plus its page count.

use super::build::build;
use super::kind_of;
use crate::gateway::Gateway;
use crate::library::schema::LibItem;
use crate::library::store;

pub fn create(
    gw: &Gateway,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> Result<(LibItem, i64), String> {
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
