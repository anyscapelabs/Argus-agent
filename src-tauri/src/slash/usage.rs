use rusqlite::Connection;
use serde::Serialize;

/// A turn is one user message and the assistant messages up to the next one.
/// The rule below is the one `ChatTranscript.tsx` uses for its "Worked for"
/// labels, so the weekly total and the per-turn labels cannot disagree: it
/// starts at the first assistant message with real text (or the user's, if
/// there is not one yet) and ends at the last assistant message of the turn.
const TURNS: &str = "
    SELECT u.session_id, u.seq, u.created_at,
      COALESCE(
        (SELECT MIN(n.seq) FROM messages n
          WHERE n.session_id = u.session_id AND n.seq > u.seq
            AND n.role = 'user' AND n.content NOT LIKE '<tool-result%'),
        9223372036854775807
      ) AS end_seq,
      (SELECT MAX(a.created_at) FROM messages a
        WHERE a.session_id = u.session_id AND a.role = 'assistant'
          AND a.seq > u.seq AND a.seq <
            COALESCE(
              (SELECT MIN(n.seq) FROM messages n
                WHERE n.session_id = u.session_id AND n.seq > u.seq
                  AND n.role = 'user' AND n.content NOT LIKE '<tool-result%'),
              9223372036854775807
            )) AS end_at
    FROM messages u
    WHERE u.role = 'user' AND u.active = 1 AND u.local = 0
      AND u.content NOT LIKE '<tool-result%'
      AND u.created_at >= datetime('now', ?1)
";

/// Milliseconds of active time. A turn still running has no end yet and scores
/// zero: it is not yet time spent.
///
/// The start is the first assistant row with real text, or the user's own
/// message when there is not one yet — the rule `ChatTranscript.tsx` uses for
/// its "Worked for" labels, so the two cannot count different things.
fn worked_ms(conn: &Connection, window: &str) -> Result<f64, String> {
    let sql = format!(
        "SELECT COALESCE(SUM(
            (julianday(t.end_at) - julianday(COALESCE(
              (SELECT MIN(a.created_at) FROM messages a
                WHERE a.session_id = t.session_id AND a.role = 'assistant'
                  AND a.seq > t.seq AND a.seq < t.end_seq AND TRIM(a.content) <> ''),
              t.created_at))) * 86400000), 0)
         FROM ({TURNS}) t"
    );

    // The arithmetic stays in SQL because `created_at` is a `datetime('now')`
    // string and re-parsing it to get milliseconds back is work SQLite did.
    let ms: f64 = conn
        .query_row(&sql, rusqlite::params![window], |r| r.get(0))
        .map_err(|err| err.to_string())?;

    Ok(ms.max(0.0))
}

#[derive(Default)]
struct Totals {
    requests: i64,
    failed: i64,
    tok_in: i64,
    tok_out: i64,
    cost: f64,
}

/// `attempt` is a column and failed retries are logged, so counting
/// `status <> 'ok'` gives a real error rate rather than an impression of one.
fn totals(conn: &Connection, window: &str) -> Result<Totals, String> {
    conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN status <> 'ok' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(tok_in), 0), COALESCE(SUM(tok_out), 0),
                COALESCE(SUM(cost), 0)
         FROM request_log WHERE ts >= datetime('now', ?1)",
        rusqlite::params![window],
        |r| {
            Ok(Totals {
                requests: r.get(0)?,
                failed: r.get(1)?,
                tok_in: r.get(2)?,
                tok_out: r.get(3)?,
                cost: r.get(4)?,
            })
        },
    )
    .map_err(|err| err.to_string())
}

/// Per-model, so the expensive one is visible rather than averaged away.
fn by_model(conn: &Connection, window: &str) -> Result<Vec<ModelUsage>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(model_id, 'unknown'), COALESCE(provider_id, ''), COUNT(*),
                    COALESCE(SUM(COALESCE(tok_in, 0) + COALESCE(tok_out, 0)), 0),
                    COALESCE(SUM(cost), 0)
             FROM request_log WHERE ts >= datetime('now', ?1)
             GROUP BY model_id, provider_id
             ORDER BY SUM(COALESCE(cost, 0)) DESC, COUNT(*) DESC",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![window], |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                provider: r.get(1)?,
                requests: r.get(2)?,
                tokens: r.get(3)?,
                cost: r.get(4)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// One row per day, so the card can draw a calendar the way a month of
/// activity actually falls rather than as a single total.
fn by_day(conn: &Connection, window: &str) -> Result<Vec<DayUsage>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT substr(ts, 1, 10), COUNT(*),
                    COALESCE(SUM(COALESCE(tok_in, 0) + COALESCE(tok_out, 0)), 0),
                    COALESCE(SUM(cost), 0)
             FROM request_log WHERE ts >= datetime('now', ?1)
             GROUP BY substr(ts, 1, 10) ORDER BY 1",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![window], |r| {
            Ok(DayUsage {
                day: r.get(0)?,
                requests: r.get(1)?,
                tokens: r.get(2)?,
                cost: r.get(3)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// An argument that is silently ignored is how a number gets trusted when it
/// was computed over the wrong window, so a wrong word is refused by name.
pub fn window_of(arg: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    match arg {
        "" | "week" => Ok(("week", "This week", "-7 days")),
        "today" => Ok(("today", "Today", "0 days")),
        "month" => Ok(("month", "This month", "-30 days")),
        _ => Err(format!(
            "/usage takes today, week or month — not \"{arg}\"."
        )),
    }
}

/// Everything the card draws, for one window. The chat renders it; there is
/// no markdown in it, because a number inside a sentence is a number nobody
/// can compare with the number next to it.
#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub window: String,
    pub label: String,
    pub requests: i64,
    pub failed: i64,
    pub tok_in: i64,
    pub tok_out: i64,
    pub cost: f64,
    pub worked_ms: f64,
    pub models: Vec<ModelUsage>,
    pub days: Vec<DayUsage>,
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub provider: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost: f64,
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DayUsage {
    pub day: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost: f64,
}

pub fn report(conn: &Connection, arg: &str) -> Result<Usage, String> {
    let (key, label, window) = window_of(arg)?;
    let t = totals(conn, window)?;

    Ok(Usage {
        window: key.into(),
        label: label.into(),
        requests: t.requests,
        failed: t.failed,
        tok_in: t.tok_in,
        tok_out: t.tok_out,
        cost: t.cost,
        worked_ms: worked_ms(conn, window)?,
        models: by_model(conn, window)?,
        days: by_day(conn, window)?,
    })
}

pub fn tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}
