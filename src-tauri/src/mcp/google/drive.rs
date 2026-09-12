use super::oauth::url_encode;
use super::{authed_delete, authed_get};

const BASE: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart";

pub async fn list_files(query: &str, max: u64) -> Result<serde_json::Value, String> {
    let v = authed_get(&format!(
        "{BASE}?q={}&pageSize={}&fields=files(id,name,mimeType,modifiedTime,size)",
        url_encode(query),
        max.clamp(1, 50)
    ))
    .await?;

    Ok(v.get("files").cloned().unwrap_or(serde_json::Value::Null))
}

pub async fn get_file(file_id: &str) -> Result<serde_json::Value, String> {
    if file_id.trim().is_empty() {
        return Err("missing file id".into());
    }

    authed_get(&format!(
        "{BASE}/{}?fields=id,name,mimeType,modifiedTime,size,parents",
        url_encode(file_id)
    ))
    .await
}

pub async fn create_file(
    name: &str,
    mime: &str,
    content: &str,
) -> Result<serde_json::Value, String> {
    if name.trim().is_empty() {
        return Err("missing name".into());
    }

    let mt = if mime.trim().is_empty() {
        "text/plain"
    } else {
        mime
    };
    let boundary = "argusdriveboundary";
    let meta = serde_json::json!({"name": name, "mimeType": mt}).to_string();
    let body = format!(
        "--{boundary}\r\nContent-Type: application/json\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {mt}\r\n\r\n{content}\r\n--{boundary}--\r\n"
    );

    let tok = super::tokens::access_token().await?;
    let cli = reqwest::Client::new();

    cli.post(UPLOAD)
        .bearer_auth(tok)
        .header(
            "Content-Type",
            format!("multipart/related; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn delete_file(file_id: &str) -> Result<(), String> {
    if file_id.trim().is_empty() {
        return Err("missing file id".into());
    }

    authed_delete(&format!("{BASE}/{}", url_encode(file_id))).await
}

pub async fn export_text(file_id: &str) -> Result<String, String> {
    if file_id.trim().is_empty() {
        return Err("missing file id".into());
    }

    let tok = super::tokens::access_token().await?;
    let cli = reqwest::Client::new();

    let meta = authed_get(&format!("{BASE}/{}?fields=mimeType", url_encode(file_id))).await?;
    let mt = meta.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");

    let url = if mt == "application/vnd.google-apps.document" {
        format!("{BASE}/{}/export?mimeType=text/plain", url_encode(file_id))
    } else if mt == "application/vnd.google-apps.spreadsheet" {
        format!("{BASE}/{}/export?mimeType=text/csv", url_encode(file_id))
    } else {
        format!("{BASE}/{}?alt=media", url_encode(file_id))
    };

    cli.get(&url)
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .text()
        .await
        .map_err(|err| err.to_string())
}
