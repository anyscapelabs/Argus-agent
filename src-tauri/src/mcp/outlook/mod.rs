pub mod config;
pub mod device;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookStatus {
    pub connected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookDevice {
    pub verification_uri: String,
    pub user_code: String,
}

#[tauri::command]
pub async fn outlook_status() -> Result<OutlookStatus, String> {
    Ok(OutlookStatus {
        connected: super::vault::get("outlook")?.is_some(),
    })
}

#[tauri::command]
pub async fn outlook_connect() -> Result<OutlookDevice, String> {
    device::begin().await
}

#[tauri::command]
pub async fn outlook_disconnect() -> Result<(), String> {
    super::vault::clear("outlook")
}

async fn authed(
    method: reqwest::Method,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let tok = device::access_token().await?;
    let cli = reqwest::Client::new();
    let mut req = cli
        .request(method, format!("https://graph.microsoft.com/v1.0{path}"))
        .bearer_auth(tok);

    if let Some(b) = body {
        req = req.json(b);
    }

    req.send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

fn slim_msg(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": v.get("id"),
        "subject": v.get("subject"),
        "from": v.get("from").and_then(|f| f.get("emailAddress")).and_then(|e| e.get("address")),
        "received": v.get("receivedDateTime"),
        "preview": v.get("bodyPreview"),
    })
}

pub async fn messages() -> Result<serde_json::Value, String> {
    let v = authed(
        reqwest::Method::GET,
        "/me/messages?$top=20&$select=id,subject,from,receivedDateTime,bodyPreview&$orderby=receivedDateTime desc",
        None,
    )
    .await?;

    Ok(v.get("value")
        .and_then(|m| m.as_array())
        .map(|ms| ms.iter().map(slim_msg).collect())
        .unwrap_or(serde_json::Value::Null))
}

pub async fn send(to: &str, subject: &str, body: &str) -> Result<(), String> {
    if to.trim().is_empty() || subject.trim().is_empty() {
        return Err("missing to or subject".into());
    }

    authed(
        reqwest::Method::POST,
        "/me/sendMail",
        Some(&serde_json::json!({
            "message": {
                "subject": subject,
                "body": {"contentType": "Text", "content": body},
                "toRecipients": [{"emailAddress": {"address": to}}],
            },
        })),
    )
    .await?;

    Ok(())
}

pub async fn events() -> Result<serde_json::Value, String> {
    let v = authed(
        reqwest::Method::GET,
        "/me/events?$top=20&$select=id,subject,start,end,location&$orderby=start/dateTime",
        None,
    )
    .await?;

    Ok(v.get("value").cloned().unwrap_or(serde_json::Value::Null))
}

pub async fn create_event(
    subject: &str,
    start: &str,
    end: &str,
) -> Result<serde_json::Value, String> {
    if subject.trim().is_empty() || start.trim().is_empty() || end.trim().is_empty() {
        return Err("missing subject, start, or end".into());
    }

    authed(
        reqwest::Method::POST,
        "/me/events",
        Some(&serde_json::json!({
            "subject": subject,
            "start": {"dateTime": start, "timeZone": "UTC"},
            "end": {"dateTime": end, "timeZone": "UTC"},
        })),
    )
    .await
}
