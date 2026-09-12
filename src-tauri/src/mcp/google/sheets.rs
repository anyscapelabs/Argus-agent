use url::Url;

use super::{authed_get, authed_post, authed_put};

const BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

fn values_url(id: &str, range: &str) -> Result<Url, String> {
    let mut url = Url::parse(BASE).map_err(|err| err.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "bad sheets base".to_string())?
        .push(id)
        .push("values")
        .push(range);

    Ok(url)
}

pub async fn get_values(id: &str, range: &str) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing spreadsheet id".into());
    }

    if range.trim().is_empty() {
        return Err("missing range".into());
    }

    let url = values_url(id, range)?;

    authed_get(url.as_str()).await
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

    let mut url = values_url(id, range)?;
    url.query_pairs_mut()
        .append_pair("valueInputOption", "USER_ENTERED");

    authed_put(url.as_str(), &serde_json::json!({"values": values})).await
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

    let mut url = values_url(id, range)?;
    url.path_segments_mut()
        .map_err(|_| "bad sheets base".to_string())?
        .pop()
        .push(&format!("{range}:append"));
    url.query_pairs_mut()
        .append_pair("valueInputOption", "USER_ENTERED");

    authed_post(url.as_str(), &serde_json::json!({"values": values})).await
}

pub async fn create_sheet(title: &str) -> Result<serde_json::Value, String> {
    if title.trim().is_empty() {
        return Err("missing title".into());
    }

    authed_post(BASE, &serde_json::json!({"properties": {"title": title}})).await
}
