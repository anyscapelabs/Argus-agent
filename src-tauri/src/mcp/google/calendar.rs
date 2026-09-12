use super::oauth::url_encode;
use super::{authed_delete, authed_get, authed_post};

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";

pub async fn list_events(
    calendar_id: &str,
    time_min: &str,
    max: u64,
) -> Result<serde_json::Value, String> {
    let cal = if calendar_id.trim().is_empty() {
        "primary"
    } else {
        calendar_id
    };

    let v = authed_get(&format!(
        "{BASE}/{}/events?timeMin={}&maxResults={}&singleEvents=true&orderBy=startTime",
        url_encode(cal),
        url_encode(time_min),
        max.clamp(1, 50)
    ))
    .await?;

    let items: Vec<serde_json::Value> = v
        .get("items")
        .and_then(|i| i.as_array())
        .map(|is| {
            is.iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                        "summary": e.get("summary").and_then(|s| s.as_str()).unwrap_or(""),
                        "start": e.get("start").and_then(|s| s.get("dateTime").or_else(|| s.get("date"))).unwrap_or(&serde_json::Value::Null),
                        "end": e.get("end").and_then(|s| s.get("dateTime").or_else(|| s.get("date"))).unwrap_or(&serde_json::Value::Null),
                        "link": e.get("htmlLink").and_then(|l| l.as_str()).unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(serde_json::Value::Array(items))
}

pub async fn create_event(
    calendar_id: &str,
    summary: &str,
    start: &str,
    end: &str,
    description: &str,
) -> Result<serde_json::Value, String> {
    if summary.trim().is_empty() {
        return Err("missing summary".into());
    }

    if start.trim().is_empty() || end.trim().is_empty() {
        return Err("missing start or end".into());
    }

    let cal = if calendar_id.trim().is_empty() {
        "primary"
    } else {
        calendar_id
    };

    authed_post(
        &format!("{BASE}/{}/events", url_encode(cal)),
        &serde_json::json!({
            "summary": summary,
            "description": description,
            "start": {"dateTime": start},
            "end": {"dateTime": end},
        }),
    )
    .await
}

pub async fn delete_event(calendar_id: &str, event_id: &str) -> Result<(), String> {
    if event_id.trim().is_empty() {
        return Err("missing event id".into());
    }

    let cal = if calendar_id.trim().is_empty() {
        "primary"
    } else {
        calendar_id
    };

    authed_delete(&format!(
        "{BASE}/{}/events/{}",
        url_encode(cal),
        url_encode(event_id)
    ))
    .await
}
