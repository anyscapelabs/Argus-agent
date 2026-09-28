use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::schema::MIGRATE;

pub const MAX_EVENTS_PER_KIND_SCOPE: i64 = 200;
pub const MAX_LESSONS_PER_SCOPE: i64 = 8;
pub const MIN_EVIDENCE_TO_TEACH: i64 = 2;

/// What the harness saw. Every variant is an observation it can prove, and
/// the scope says who the fact is about — the model, or this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The model sent a call whose arguments did not parse to anything.
    EmptyArgs,
    /// The model retried a failing approach without changing it.
    Thrashing,
    /// A confined profile refused a path.
    SandboxDenied,
    /// A confined profile was asked for without the directory it needs.
    SandboxNoCwd,
    /// The turn ran out of its token budget.
    BudgetStop,
    /// The model was upgraded to the XML dialect by its own output.
    StyleXml,
    /// The provider refused the tool schemas, so the turn ran degraded.
    Degraded,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::EmptyArgs => "empty_args",
            Kind::Thrashing => "thrashing",
            Kind::SandboxDenied => "sandbox_denied",
            Kind::SandboxNoCwd => "sandbox_no_cwd",
            Kind::BudgetStop => "budget_stop",
            Kind::StyleXml => "style_xml",
            Kind::Degraded => "degraded",
        }
    }

    /// Parse back from storage. Unknown values are `None` rather than a
    /// default: a signal this build does not understand must not become a
    /// lesson phrased as one it does.
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "empty_args" => Kind::EmptyArgs,
            "thrashing" => Kind::Thrashing,
            "sandbox_denied" => Kind::SandboxDenied,
            "sandbox_no_cwd" => Kind::SandboxNoCwd,
            "budget_stop" => Kind::BudgetStop,
            "style_xml" => Kind::StyleXml,
            "degraded" => Kind::Degraded,
            _ => return None,
        })
    }

    /// Who the fact is about. Three of these are about the machine Argus runs
    /// on, and calling them model behaviour would blame the model for a
    /// sandbox it never controlled.
    pub fn scope(self) -> Scope {
        match self {
            Kind::EmptyArgs | Kind::Thrashing | Kind::StyleXml => Scope::Model,
            Kind::SandboxDenied | Kind::SandboxNoCwd | Kind::BudgetStop | Kind::Degraded => {
                Scope::Host
            }
        }
    }

    pub fn all() -> [Kind; 7] {
        [
            Kind::EmptyArgs,
            Kind::Thrashing,
            Kind::SandboxDenied,
            Kind::SandboxNoCwd,
            Kind::BudgetStop,
            Kind::StyleXml,
            Kind::Degraded,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Model,
    Host,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Model => "model",
            Scope::Host => "host",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Some(match s {
            "model" => Scope::Model,
            "host" => Scope::Host,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub kind: String,
    pub scope: String,
    pub scope_id: String,
    pub session_id: Option<String>,
    pub detail: String,
    pub seen: i64,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaybookItem {
    pub id: String,
    pub scope: String,
    pub scope_id: String,
    pub lesson_key: String,
    pub text: String,
    pub evidence: i64,
    pub last_seen: String,
    pub created_at: String,
}

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(MIGRATE).map_err(|err| err.to_string())
}

/// Record one observation. Repeats of the same kind on the same scope bump a
/// counter rather than appending, so a model that fails the same way a
/// hundred times is one row, not a hundred.
///
/// A fact is one line of harness output. Truncated on a character boundary:
/// slicing at a byte offset is a panic waiting for the first multi-byte
/// character in a command.
const DETAIL_CAP: usize = 200;

pub fn clip_detail(s: &str) -> String {
    let one_line = s.replace(['\n', '\r'], " ");
    let trimmed = one_line.trim();

    if trimmed.chars().count() <= DETAIL_CAP {
        return trimmed.to_string();
    }

    let cut: String = trimmed.chars().take(DETAIL_CAP).collect();
    format!("{cut}…")
}

pub fn record(
    conn: &Connection,
    kind: Kind,
    scope_id: &str,
    session_id: Option<&str>,
    detail: &str,
) -> Result<(), String> {
    if scope_id.trim().is_empty() {
        return Err("empty scope".into());
    }

    let detail = clip_detail(detail);
    let scope = kind.scope().as_str();

    // Two statements, not one clever UPDATE-with-subquery: `query_row` on an
    // UPDATE reports "no rows" in a way that reads like a failed match, and
    // the failure is silent — every signal appends and the dedup quietly stops
    // working. Select, then update by id.
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM protocol_events
             WHERE kind = ?1 AND scope = ?2 AND scope_id = ?3
             ORDER BY created_at DESC, id DESC LIMIT 1",
            params![kind.as_str(), scope, scope_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;

    if let Some(id) = existing {
        conn.execute(
            "UPDATE protocol_events SET seen = seen + 1, detail = ?2 WHERE id = ?1",
            params![id, detail],
        )
        .map_err(|err| err.to_string())?;

        return Ok(());
    }

    conn.execute(
        "INSERT INTO protocol_events (id, kind, scope, scope_id, session_id, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            kind.as_str(),
            scope,
            scope_id,
            session_id,
            detail,
        ],
    )
    .map_err(|err| err.to_string())?;

    // Trim by age, keeping the newest N for this kind+scope. A model that
    // churns must not grow this table without bound.
    let _ = conn.execute(
        "DELETE FROM protocol_events WHERE id NOT IN (
           SELECT id FROM protocol_events ORDER BY created_at DESC, id DESC LIMIT ?1)",
        params![MAX_EVENTS_PER_KIND_SCOPE],
    );

    Ok(())
}

const EV_COLS: &str = "id, kind, scope, scope_id, session_id, detail, seen, created_at";

fn row_signal(r: &rusqlite::Row) -> rusqlite::Result<Signal> {
    Ok(Signal {
        id: r.get(0)?,
        kind: r.get(1)?,
        scope: r.get(2)?,
        scope_id: r.get(3)?,
        session_id: r.get(4)?,
        detail: r.get(5)?,
        seen: r.get(6)?,
        created_at: r.get(7)?,
    })
}

pub fn signals_for(conn: &Connection, kind: Kind, scope_id: &str) -> Result<Vec<Signal>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {EV_COLS} FROM protocol_events
             WHERE kind = ?1 AND scope = ?2 AND scope_id = ?3
             ORDER BY created_at"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(
            params![kind.as_str(), kind.scope().as_str(), scope_id],
            row_signal,
        )
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// Upsert a lesson and report its evidence count, so the caller can see
/// whether it cleared the teaching bar. Repeated signals accumulate on the
/// same sentence instead of minting near-duplicates.
/// Store a lesson with the evidence count observed so far, and report it.
///
/// The count is set, not incremented: signals already keep their own `seen`
/// total, and incrementing here would race the two counters — a lesson would
/// teach itself on the second curation of a single occurrence. One occurrence
/// means one unit of evidence, every time.
pub fn remember(
    conn: &Connection,
    scope: Scope,
    scope_id: &str,
    lesson_key: &str,
    text: &str,
    evidence: i64,
) -> Result<i64, String> {
    if scope_id.trim().is_empty() || lesson_key.trim().is_empty() {
        return Err("empty scope or key".into());
    }

    let evidence = evidence.max(1);

    conn.execute(
        "INSERT INTO playbook_items (id, scope, scope_id, lesson_key, text, evidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(scope, scope_id, lesson_key) DO UPDATE
         SET evidence = MAX(playbook_items.evidence, excluded.evidence),
             last_seen = datetime('now'),
             text = excluded.text",
        params![
            uuid::Uuid::new_v4().to_string(),
            scope.as_str(),
            scope_id,
            lesson_key,
            text,
            evidence,
        ],
    )
    .map_err(|err| err.to_string())?;

    conn.query_row(
        "SELECT evidence FROM playbook_items
         WHERE scope = ?1 AND scope_id = ?2 AND lesson_key = ?3",
        params![scope.as_str(), scope_id, lesson_key],
        |r| r.get(0),
    )
    .map_err(|err| err.to_string())
}

const PB_COLS: &str = "id, scope, scope_id, lesson_key, text, evidence, last_seen, created_at";

fn row_item(r: &rusqlite::Row) -> rusqlite::Result<PlaybookItem> {
    Ok(PlaybookItem {
        id: r.get(0)?,
        scope: r.get(1)?,
        scope_id: r.get(2)?,
        lesson_key: r.get(3)?,
        text: r.get(4)?,
        evidence: r.get(5)?,
        last_seen: r.get(6)?,
        created_at: r.get(7)?,
    })
}

pub fn lessons(
    conn: &Connection,
    scope: Scope,
    scope_id: &str,
) -> Result<Vec<PlaybookItem>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {PB_COLS} FROM playbook_items
             WHERE scope = ?1 AND scope_id = ?2 AND evidence >= ?3
             ORDER BY evidence DESC, last_seen DESC
             LIMIT ?4"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(
            params![
                scope.as_str(),
                scope_id,
                MIN_EVIDENCE_TO_TEACH,
                MAX_LESSONS_PER_SCOPE
            ],
            row_item,
        )
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

/// Every scope this machine has lessons for, keyed for assembly.
pub fn all_scopes(
    conn: &Connection,
) -> Result<BTreeMap<(String, String), Vec<PlaybookItem>>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {PB_COLS} FROM playbook_items WHERE evidence >= ?1
             ORDER BY scope, scope_id, evidence DESC, last_seen DESC"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![MIN_EVIDENCE_TO_TEACH], row_item)
        .map_err(|err| err.to_string())?;

    let mut out: BTreeMap<(String, String), Vec<PlaybookItem>> = BTreeMap::new();
    for item in rows {
        let item = item.map_err(|err| err.to_string())?;
        let key = (item.scope.clone(), item.scope_id.clone());
        let bucket = out.entry(key).or_default();

        if (bucket.len() as i64) < MAX_LESSONS_PER_SCOPE {
            bucket.push(item);
        }
    }

    Ok(out)
}

pub fn forget(conn: &Connection, scope: Scope, scope_id: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM playbook_items WHERE scope = ?1 AND scope_id = ?2",
        params![scope.as_str(), scope_id],
    )
    .map_err(|err| err.to_string())
}

pub fn forget_all(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM playbook_items", [])
        .map_err(|err| err.to_string())
}
