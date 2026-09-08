use uuid::Uuid;

use rusqlite::{params, Connection, OptionalExtension};

use super::schema::{NAME_MAX, NewSkill, Skill, UpdSkill, BODY_MAX, DESC_MAX};

const COLS: &str =
  "id, name, description, body, source, origin, use_count, last_used_at, created_at, updated_at";

pub fn migrate(conn: &Connection) -> Result<(), String> {
  conn.execute_batch(super::schema::MIGRATE).map_err(|e| e.to_string())
}

fn row_skill(r: &rusqlite::Row) -> rusqlite::Result<Skill> {
  Ok(Skill {
    id: r.get(0)?,
    name: r.get(1)?,
    description: r.get(2)?,
    body: r.get(3)?,
    source: r.get(4)?,
    origin: r.get(5)?,
    use_count: r.get(6)?,
    last_used_at: r.get(7)?,
    created_at: r.get(8)?,
    updated_at: r.get(9)?,
  })
}

fn chk_name(name: &str) -> Result<(), String> {
  if name.is_empty() || name.len() > NAME_MAX {
    return Err(format!("skill name must be 1-{NAME_MAX} chars")); // Drop it
  }
  let ok = name.chars().enumerate().all(|(i, c)| {
    c.is_ascii_lowercase() || c.is_ascii_digit() || (c == '-' && i > 0 && i + 1 < name.len())
  });
  if !ok {
    return Err("skill name must be kebab-case (lowercase, digits, dashes)".into());
  }
  Ok(())
}

fn chk_sizes(description: &str, body: &str) -> Result<(), String> {
  if description.len() > DESC_MAX {
    return Err(format!("description must be at most {DESC_MAX} bytes"));
  }
  if body.is_empty() || body.len() > BODY_MAX {
    return Err(format!("body must be 1-{BODY_MAX} bytes"));
  }
  Ok(())
}

pub fn create_skill(conn: &Connection, s: &NewSkill) -> Result<Skill, String> {
  chk_name(&s.name)?;
  chk_sizes(&s.description, &s.body)?;
  let dup = get_skill(conn, &s.name).is_ok();
  if dup {
    return Err(format!("skill '{}' already exists, update it instead", s.name)); // Drop it
  }
  let source = match s.source.as_deref() {
    Some("agent") => "agent",
    _ => "user",
  };
  let id = Uuid::new_v4().to_string();
  conn
    .execute(
      "INSERT INTO skills (id, name, description, body, source, origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
      params![id, s.name, s.description, s.body, source, s.origin],
    )
    .map_err(|e| e.to_string())?;
  get_skill(conn, &s.name)
}

pub fn get_skill(conn: &Connection, name: &str) -> Result<Skill, String> {
  conn
    .query_row(&format!("SELECT {COLS} FROM skills WHERE name = ?1"), params![name], row_skill)
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("skill '{name}' not found"))
}

pub fn list_skills(conn: &Connection) -> Result<Vec<Skill>, String> {
  let mut stmt = conn
    .prepare(&format!("SELECT {COLS} FROM skills ORDER BY updated_at DESC"))
    .map_err(|e| e.to_string())?;
  let rows = stmt.query_map([], row_skill).map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

// UI edit path: full row keyed by id.
pub fn save_skill(conn: &Connection, s: &Skill) -> Result<(), String> {
  chk_sizes(&s.description, &s.body)?;
  conn
    .execute(
      "UPDATE skills SET description=?2, body=?3, updated_at=datetime('now') WHERE id=?1",
      params![s.id, s.description, s.body],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

// Agent refinement path: partial update keyed by name.
pub fn update_skill(conn: &Connection, name: &str, u: &UpdSkill) -> Result<Skill, String> {
  let cur = get_skill(conn, name)?;
  let desc = u.description.clone().unwrap_or(cur.description);
  let body = u.body.clone().unwrap_or(cur.body);
  chk_sizes(&desc, &body)?;
  conn
    .execute(
      "UPDATE skills SET description=?2, body=?3, updated_at=datetime('now') WHERE name=?1",
      params![name, desc, body],
    )
    .map_err(|e| e.to_string())?;
  get_skill(conn, name)
}

// key = name or id, so agent and UI share one delete path.
pub fn delete_skill(conn: &Connection, key: &str) -> Result<(), String> {
  conn
    .execute("DELETE FROM skills WHERE name = ?1 OR id = ?1", params![key])
    .map_err(|e| e.to_string())?;
  Ok(())
}

// Agent pulled this skill to work with it.
pub fn touch_skill(conn: &Connection, name: &str) -> Result<(), String> {
  conn
    .execute(
      "UPDATE skills SET use_count = use_count + 1, last_used_at = datetime('now') WHERE name = ?1",
      params![name],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

// FTS5 search, BM25 ranked. Query is quoted per-token to dodge syntax errors.
pub fn search_skills(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Skill>, String> {
  let fts_q: Vec<String> = query
    .split_whitespace()
    .map(|t| format!("\"{}\"", t.replace('"', "")))
    .collect();
  if fts_q.is_empty() {
    return Ok(vec![]); // Drop it, nothing to match
  }
  let fts_q = fts_q.join(" ");
  let sql = format!(
    "SELECT {COLS} FROM skills WHERE rowid IN
     (SELECT rowid FROM skills_fts WHERE skills_fts MATCH ?1 ORDER BY bm25(skills_fts) LIMIT ?2)"
  );
  let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
  let rows = stmt
    .query_map(params![fts_q, limit], row_skill)
    .map_err(|e| e.to_string())?;
  rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
