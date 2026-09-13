pub mod config;

use crate::mcp::vault;

const BASE: &str = "https://api.trello.com/1";
const SERVICE: &str = "trello";

async fn authed(path: &str) -> Result<String, String> {
    let key = config::load()?.api_key;
    let tok = vault::get(SERVICE)?.ok_or("trello not connected")?;

    Ok(format!("{BASE}{path}?key={key}&token={tok}"))
}

async fn get(path: &str) -> Result<serde_json::Value, String> {
    reqwest::Client::new()
        .get(authed(path).await?)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

async fn post(path: &str, params: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    let mut url = url::Url::parse(&authed(path).await?).map_err(|err| err.to_string())?;
    url.query_pairs_mut().extend_pairs(params);

    reqwest::Client::new()
        .post(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

pub async fn boards() -> Result<serde_json::Value, String> {
    let v = get("/members/me/boards?fields=name,url").await?;

    Ok(v.as_array()
        .map(|bs| {
            bs.iter()
                .map(|b| serde_json::json!({"id": b.get("id"), "name": b.get("name")}))
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn lists(board_id: &str) -> Result<serde_json::Value, String> {
    if board_id.trim().is_empty() {
        return Err("missing board".into());
    }

    let v = get(&format!("/boards/{board_id}/lists?fields=name")).await?;

    Ok(v.as_array()
        .map(|ls| {
            ls.iter()
                .map(|l| serde_json::json!({"id": l.get("id"), "name": l.get("name")}))
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn cards(list_id: &str) -> Result<serde_json::Value, String> {
    if list_id.trim().is_empty() {
        return Err("missing list".into());
    }

    let v = get(&format!("/lists/{list_id}/cards?fields=name,desc,due")).await?;

    Ok(v
        .as_array()
        .map(|cs| {
            cs.iter()
                .map(|c| {
                    serde_json::json!({"id": c.get("id"), "name": c.get("name"), "due": c.get("due")})
                })
                .collect()
        })
        .unwrap_or(serde_json::Value::Null))
}

pub async fn create_card(
    list_id: &str,
    name: &str,
    desc: &str,
) -> Result<serde_json::Value, String> {
    if list_id.trim().is_empty() || name.trim().is_empty() {
        return Err("missing list or name".into());
    }

    post(
        "/cards",
        &[("idList", list_id), ("name", name), ("desc", desc)],
    )
    .await
}

pub async fn comment_card(card_id: &str, text: &str) -> Result<serde_json::Value, String> {
    if card_id.trim().is_empty() || text.trim().is_empty() {
        return Err("missing card or text".into());
    }

    post(
        &format!("/cards/{card_id}/actions/comments"),
        &[("text", text)],
    )
    .await
}
