use rusqlite::Connection;

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
    WHERE u.role = 'user' AND u.active = 1
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
fn by_model(conn: &Connection, window: &str) -> Result<Vec<(String, i64, i64, f64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(model_id, 'unknown'), COUNT(*),
                    COALESCE(SUM(tok_in + tok_out), 0), COALESCE(SUM(cost), 0)
             FROM request_log WHERE ts >= datetime('now', ?1)
             GROUP BY model_id ORDER BY SUM(cost) DESC, COUNT(*) DESC",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![window], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// An argument that is silently ignored is how a number gets trusted when it
/// was computed over the wrong window, so a wrong word is refused by name.
fn window_of(arg: &str) -> Result<(&'static str, &'static str), String> {
    match arg {
        "" | "week" => Ok(("This week", "-7 days")),
        "today" => Ok(("Today", "0 days")),
        "month" => Ok(("This month", "-30 days")),
        _ => Err(format!(
            "/usage takes today, week or month — not \"{arg}\"."
        )),
    }
}

pub fn report(conn: &Connection, arg: &str) -> Result<String, String> {
    let (label, window) = window_of(arg)?;
    let t = totals(conn, window)?;
    let models = by_model(conn, window)?;
    let worked = worked_ms(conn, window)?;

    if t.requests == 0 && worked == 0.0 {
        return Ok(format!("**{label}** · nothing yet."));
    }

    let mut out = format!("**{label}** · {} requests", t.requests);

    if t.failed > 0 {
        out.push_str(&format!(" · {} failed", t.failed));
    }

    out.push_str(&format!(" · {} tokens\n\n", tokens(t.tok_in + t.tok_out)));

    if !models.is_empty() {
        out.push_str("| Model | Requests | Tokens | Cost |\n| --- | --- | --- | --- |\n");

        for (model, reqs, tks, cost) in &models {
            out.push_str(&format!(
                "| {model} | {reqs} | {} | {} |\n",
                tokens(*tks),
                money(*cost)
            ));
        }

        out.push('\n');
    }

    out.push_str(&format!(
        "**Worked for {}** · {} total",
        duration(worked),
        money(t.cost)
    ));

    Ok(out)
}

/// The same shape the transcript's "Worked for" label uses, so the two read
/// the same: seconds under a minute, then minutes, then hours.
fn duration(ms: f64) -> String {
    let sec = (ms / 1000.0).round().max(0.0) as i64;

    if sec < 60 {
        return format!("{sec} sec");
    }

    let mins = sec / 60;
    let rem = sec % 60;

    if mins < 60 {
        return if rem == 0 {
            format!("{mins} min")
        } else {
            format!("{mins} min {rem} sec")
        };
    }

    let hours = mins / 60;
    let rem_h = mins % 60;

    if rem_h == 0 {
        format!("{hours} hr")
    } else {
        format!("{hours} hr {rem_h} min")
    }
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

pub fn money(cost: f64) -> String {
    if cost > 0.0 && cost < 0.01 {
        return "under a cent".into();
    }

    format!("${cost:.2}")
}
