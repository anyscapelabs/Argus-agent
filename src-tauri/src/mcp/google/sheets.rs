use super::oauth::url_encode;
use super::{authed_get, authed_post, authed_put};

const BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

pub async fn get_values(id: &str, range: &str) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing spreadsheet id".into());
    }

    if range.trim().is_empty() {
        return Err("missing range".into());
    }

    authed_get(&format!(
        "{BASE}/{}/values/{}",
        url_encode(id),
        url_encode(range)
    ))
    .await
}

pub async fn update_values(
    id: &str,
    range: &str,
    values: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing spreadsheet id".into());
    }

    if range.trim().is_empty() {
        return Err("missing range".into());
    }

    authed_put(
        &format!(
            "{BASE}/{}/values/{}?valueInputOption=USER_ENTERED",
            url_encode(id),
            url_encode(range)
        ),
        &serde_json::json!({"values": values}),
    )
    .await
}

pub async fn append_values(
    id: &str,
    range: &str,
    values: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing spreadsheet id".into());
    }

    if range.trim().is_empty() {
        return Err("missing range".into());
    }

    authed_post(
        &format!(
            "{BASE}/{}/values/{}:append?valueInputOption=USER_ENTERED",
            url_encode(id),
            url_encode(range)
        ),
        &serde_json::json!({"values": values}),
    )
    .await
}

pub async fn create_sheet(title: &str) -> Result<serde_json::Value, String> {
    if title.trim().is_empty() {
        return Err("missing title".into());
    }

    authed_post(BASE, &serde_json::json!({"properties": {"title": title}})).await
}
