use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::schema::{LearningEvent, LearningExample, LearningPreference};

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|err| err.to_string())
}

pub fn record_event(
    conn: &Connection,
    session_id: Option<&str>,
    message_id: Option<&str>,
    kind: &str,
    payload: &str,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO learning_events (id, session_id, message_id, kind, payload)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, session_id, message_id, kind, payload],
    )
    .map_err(|err| err.to_string())?;

    Ok(id)
}

pub fn pending(conn: &Connection, limit: i64) -> Result<Vec<LearningEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, message_id, kind, payload, processed, created_at, processed_at
             FROM learning_events WHERE processed = 0 ORDER BY created_at LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![limit.max(1)], |row| {
            Ok(LearningEvent {
                id: row.get(0)?,
                session_id: row.get(1)?,
                message_id: row.get(2)?,
                kind: row.get(3)?,
                payload: row.get(4)?,
                processed: row.get::<_, i64>(5)? != 0,
                created_at: row.get(6)?,
                processed_at: row.get(7)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn mark_processed(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE learning_events SET processed = 1, processed_at = datetime('now') WHERE id = ?1",
        params![id],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn upsert_preference(
    conn: &Connection,
    scope: &str,
    category: &str,
    key: &str,
    value: &str,
    confidence: f64,
    explicit: bool,
) -> Result<(), String> {
    let value = value.trim();

    if value.is_empty() {
        return Ok(());
    }

    let confidence = confidence.clamp(0.0, 1.0);

    let existing: Option<(String, f64, i64, bool)> = conn
        .query_row(
            "SELECT id, confidence, evidence_count, explicit FROM learning_preferences
             WHERE scope = ?1 AND category = ?2 AND key = ?3",
            params![scope, category, key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            },
        )
        .optional()
        .map_err(|err| err.to_string())?;

    match existing {
        Some((id, old_confidence, evidence, old_explicit)) => {
            let final_explicit = old_explicit || explicit;
            let final_confidence = if final_explicit {
                confidence.max(old_confidence)
            } else {
                (old_confidence * 0.65 + confidence * 0.35).clamp(0.0, 1.0)
            };

            conn.execute(
                "UPDATE learning_preferences SET value = ?2, confidence = ?3, explicit = ?4,
                 evidence_count = ?5, last_confirmed = datetime('now'), updated_at = datetime('now')
                 WHERE id = ?1",
                params![
                    id,
                    value,
                    final_confidence,
                    final_explicit as i64,
                    evidence + 1
                ],
            )
            .map_err(|err| err.to_string())?;
        }
        None => {
            conn.execute(
                "INSERT INTO learning_preferences
                 (id, scope, category, key, value, confidence, explicit, evidence_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
                params![
                    Uuid::new_v4().to_string(),
                    scope,
                    category,
                    key,
                    value,
                    confidence,
                    explicit as i64,
                ],
            )
            .map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

pub fn add_example(
    conn: &Connection,
    scope: &str,
    kind: &str,
    content: &str,
    quality: f64,
    source: &str,
) -> Result<(), String> {
    let content: String = content.trim().chars().take(4000).collect();

    if content.is_empty() {
        return Ok(());
    }

    conn.execute(
        "INSERT INTO learning_examples (id, scope, kind, content, quality, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            Uuid::new_v4().to_string(),
            scope,
            kind,
            content,
            quality.clamp(0.0, 1.0),
            source,
        ],
    )
    .map_err(|err| err.to_string())?;

    conn.execute(
        "DELETE FROM learning_examples WHERE id IN (
           SELECT id FROM learning_examples WHERE scope = ?1 AND kind = ?2
           ORDER BY quality DESC, created_at DESC LIMIT -1 OFFSET 12
         )",
        params![scope, kind],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

const PREF_COLS: &str = "id, scope, category, key, value, confidence, explicit,
    evidence_count, last_confirmed, created_at, updated_at";

fn row_preference(row: &rusqlite::Row) -> rusqlite::Result<LearningPreference> {
    Ok(LearningPreference {
        id: row.get(0)?,
        scope: row.get(1)?,
        category: row.get(2)?,
        key: row.get(3)?,
        value: row.get(4)?,
        confidence: row.get(5)?,
        explicit: row.get::<_, i64>(6)? != 0,
        evidence_count: row.get(7)?,
        last_confirmed: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub fn list_preferences(
    conn: &Connection,
    scope: Option<&str>,
) -> Result<Vec<LearningPreference>, String> {
    let sql = match scope {
        Some(_) => format!(
            "SELECT {PREF_COLS} FROM learning_preferences WHERE scope = ?1
             ORDER BY explicit DESC, confidence DESC, updated_at DESC"
        ),
        None => format!(
            "SELECT {PREF_COLS} FROM learning_preferences
             ORDER BY explicit DESC, confidence DESC, updated_at DESC"
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
    let rows = match scope {
        Some(scope) => stmt.query_map(params![scope], row_preference),
        None => stmt.query_map([], row_preference),
    }
    .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn examples(
    conn: &Connection,
    scope: &str,
    kind: &str,
    limit: i64,
) -> Result<Vec<LearningExample>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, scope, kind, content, quality, source, created_at
             FROM learning_examples WHERE scope = ?1 AND kind = ?2
             ORDER BY quality DESC, created_at DESC LIMIT ?3",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![scope, kind, limit.max(1)], |row| {
            Ok(LearningExample {
                id: row.get(0)?,
                scope: row.get(1)?,
                kind: row.get(2)?,
                content: row.get(3)?,
                quality: row.get(4)?,
                source: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn delete_preference(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM learning_preferences WHERE id = ?1",
        params![id],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn message_context(
    conn: &Connection,
    message_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    conn.query_row(
        "SELECT session_id, role, content FROM messages WHERE id = ?1",
        params![message_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(|err| err.to_string())
}
