use url::Url;

use super::{authed_delete, authed_get, authed_post};

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";

fn cal_id(calendar_id: &str) -> String {
    if calendar_id.trim().is_empty() {
        "primary".to_string()
    } else {
        calendar_id.to_string()
    }
}

fn events_url(calendar_id: &str) -> Result<Url, String> {
    let mut url = Url::parse(BASE).map_err(|err| err.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "bad calendar base".to_string())?
        .push(&cal_id(calendar_id))
        .push("events");

    Ok(url)
}

pub async fn list_events(
    calendar_id: &str,
    time_min: &str,
    max: u64,
) -> Result<serde_json::Value, String> {
    let mut url = events_url(calendar_id)?;
    url.query_pairs_mut()
        .append_pair("timeMin", time_min)
        .append_pair("maxResults", &max.clamp(1, 50).to_string())
        .append_pair("singleEvents", "true")
        .append_pair("orderBy", "startTime");
    let v = authed_get(url.as_str()).await?;

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

    let url = events_url(calendar_id)?;

    authed_post(
        url.as_str(),
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

    let mut url = events_url(calendar_id)?;
    url.path_segments_mut()
        .map_err(|_| "bad calendar base".to_string())?
        .push(event_id);

    authed_delete(url.as_str()).await
}
