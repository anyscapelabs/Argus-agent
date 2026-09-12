use url::Url;

use super::{authed_delete, authed_get};

const BASE: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart";

fn file_url(file_id: &str) -> Result<Url, String> {
    let mut url = Url::parse(BASE).map_err(|err| err.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "bad drive base".to_string())?
        .push(file_id);

    Ok(url)
}

pub async fn list_files(query: &str, max: u64) -> Result<serde_json::Value, String> {
    let mut url = Url::parse(BASE).map_err(|err| err.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("pageSize", &max.clamp(1, 50).to_string())
        .append_pair("fields", "files(id,name,mimeType,modifiedTime,size)");
    let v = authed_get(url.as_str()).await?;

    Ok(v.get("files").cloned().unwrap_or(serde_json::Value::Null))
}

pub async fn get_file(file_id: &str) -> Result<serde_json::Value, String> {
    if file_id.trim().is_empty() {
        return Err("missing file id".into());
    }

    let mut url = file_url(file_id)?;
    url.query_pairs_mut()
        .append_pair("fields", "id,name,mimeType,modifiedTime,size,parents");

    authed_get(url.as_str()).await
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

    authed_delete(file_url(file_id)?.as_str()).await
}

pub async fn export_text(file_id: &str) -> Result<String, String> {
    if file_id.trim().is_empty() {
        return Err("missing file id".into());
    }

    let tok = super::tokens::access_token().await?;
    let cli = reqwest::Client::new();

    let mut meta_url = file_url(file_id)?;
    meta_url.query_pairs_mut().append_pair("fields", "mimeType");
    let meta = authed_get(meta_url.as_str()).await?;
    let mt = meta.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");

    let mut url = file_url(file_id)?;

    if mt == "application/vnd.google-apps.document" {
        url.path_segments_mut()
            .map_err(|_| "bad drive base".to_string())?
            .push("export");
        url.query_pairs_mut().append_pair("mimeType", "text/plain");
    } else if mt == "application/vnd.google-apps.spreadsheet" {
        url.path_segments_mut()
            .map_err(|_| "bad drive base".to_string())?
            .push("export");
        url.query_pairs_mut().append_pair("mimeType", "text/csv");
    } else {
        url.query_pairs_mut().append_pair("alt", "media");
    }

    cli.get(url.as_str())
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
