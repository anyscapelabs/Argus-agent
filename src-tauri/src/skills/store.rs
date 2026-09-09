use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension};

use super::schema::{NewSkill, Skill, UpdSkill, BODY_MAX, DESC_MAX, NAME_MAX};

const COLS: &str = "s.name, s.description, s.body, s.source, s.origin, s.created_at, s.file_mtime";
const JOIN_STATS: &str =
    "COALESCE(st.use_count, 0), st.last_used_at FROM skills s LEFT JOIN skill_stats st ON st.name = s.name";

pub fn migrate(conn: &Connection) -> Result<(), String> {
    let legacy: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('skills') WHERE name = 'id'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if legacy > 0 {
        conn.execute_batch("DROP TABLE IF EXISTS skills; DROP TABLE IF EXISTS skills_fts;")
            .map_err(|e| e.to_string())?;
    }

    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|e| e.to_string())
}

fn row_skill(r: &rusqlite::Row) -> rusqlite::Result<Skill> {
    Ok(Skill {
        name: r.get(0)?,
        description: r.get(1)?,
        body: r.get(2)?,
        source: r.get(3)?,
        origin: r.get(4)?,
        created_at: r.get(5)?,
        file_mtime: r.get(6)?,
        use_count: r.get(7)?,
        last_used_at: r.get(8)?,
    })
}

fn md_path(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join(format!("{name}.md"))
}

fn chk_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > NAME_MAX {
        return Err(format!("skill name must be 1-{NAME_MAX} chars"));
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
    if description.len() > DESC_MAX || description.contains('\n') {
        return Err(format!(
            "description must be one line, at most {DESC_MAX} bytes"
        ));
    }

    if body.is_empty() || body.len() > BODY_MAX {
        return Err(format!("body must be 1-{BODY_MAX} bytes"));
    }

    Ok(())
}

fn body_hash(body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn ftime(path: &Path) -> Result<i64, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0))
}

fn parse_md(content: &str) -> Result<(String, String, String), String> {
    let rest = content.strip_prefix("---\n").ok_or("missing frontmatter")?;
    let (fm, body) = rest.split_once("\n---\n").ok_or("unclosed frontmatter")?;

    let mut name = String::new();
    let mut desc = String::new();

    for line in fm.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().into();
        }
        if let Some(v) = line.strip_prefix("description:") {
            desc = v.trim().into();
        }
    }

    if name.is_empty() || desc.is_empty() {
        return Err("frontmatter needs name + description".into());
    }

    Ok((name, desc, body.trim_start().into()))
}

fn write_md(dir: &Path, name: &str, description: &str, body: &str) -> Result<(), String> {
    let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n");
    fs::write(md_path(dir, name), content).map_err(|e| e.to_string())
}

fn index_upsert(conn: &Connection, dir: &Path, name: &str) -> Result<(), String> {
    let path = md_path(dir, name);
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let (fname, desc, body) = parse_md(&content)?;

    if fname != name {
        return Err(format!(
            "frontmatter name '{fname}' must match filename '{name}'"
        ));
    }

    conn.execute(
        "INSERT INTO skills (name, description, body, file_mtime, body_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET description=?2, body=?3, file_mtime=?4, body_hash=?5",
        params![name, desc, body, ftime(&path)?, body_hash(&body)],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn delete_row(conn: &Connection, name: &str) -> Result<(), String> {
    conn.execute("DELETE FROM skills WHERE name = ?1", params![name])
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn sync(conn: &Connection, dir: &Path) -> Result<usize, String> {
    let mut seen: HashSet<String> = HashSet::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    for entry in entries {
        let path = match entry {
            Ok(e) => e.path(),
            Err(_) => continue,
        };

        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }

        let name = match path.file_stem().and_then(|x| x.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if chk_name(&name).is_err() || index_upsert(conn, dir, &name).is_err() {
            continue;
        }

        seen.insert(name);
    }

    let all: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM skills")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    for name in all {
        if !seen.contains(&name) {
            delete_row(conn, &name)?;
        }
    }

    Ok(seen.len())
}

pub fn create_skill(conn: &Connection, dir: &Path, s: &NewSkill) -> Result<Skill, String> {
    chk_name(&s.name)?;
    chk_sizes(&s.description, &s.body)?;

    if md_path(dir, &s.name).exists() {
        return Err(format!(
            "skill '{}' already exists, update it instead",
            s.name
        ));
    }

    write_md(dir, &s.name, &s.description, &s.body)?;

    let source = match s.source.as_deref() {
        Some("agent") => "agent",
        _ => "user",
    };

    conn.execute(
        "INSERT INTO skills (name, source, origin) VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO NOTHING",
        params![s.name, source, s.origin],
    )
    .map_err(|e| e.to_string())?;

    index_upsert(conn, dir, &s.name)?;
    get_skill(conn, dir, &s.name)
}

pub fn get_skill(conn: &Connection, dir: &Path, name: &str) -> Result<Skill, String> {
    let path = md_path(dir, name);

    if !path.exists() {
        delete_row(conn, name)?;
        return Err(format!("skill '{name}' not found"));
    }

    let on_disk = ftime(&path)?;
    let stored: Option<i64> = conn
        .query_row(
            "SELECT file_mtime FROM skills WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if stored != Some(on_disk) {
        index_upsert(conn, dir, name)?;
    }

    fetch_one(conn, name)
}

fn fetch_one(conn: &Connection, name: &str) -> Result<Skill, String> {
    conn.query_row(
        &format!("SELECT {COLS}, {JOIN_STATS} WHERE s.name = ?1"),
        params![name],
        row_skill,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("skill '{name}' not found"))
}

pub fn list_skills(conn: &Connection) -> Result<Vec<Skill>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS}, {JOIN_STATS} ORDER BY COALESCE(st.last_used_at, s.created_at) DESC"
        ))
        .map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], row_skill).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn update_skill(
    conn: &Connection,
    dir: &Path,
    name: &str,
    u: &UpdSkill,
) -> Result<Skill, String> {
    let cur = get_skill(conn, dir, name)?;
    let desc = u.description.clone().unwrap_or(cur.description);
    let body = u.body.clone().unwrap_or(cur.body);

    chk_sizes(&desc, &body)?;
    write_md(dir, name, &desc, &body)?;
    index_upsert(conn, dir, name)?;

    get_skill(conn, dir, name)
}

pub fn delete_skill(conn: &Connection, dir: &Path, name: &str) -> Result<(), String> {
    match fs::remove_file(md_path(dir, name)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }

    delete_row(conn, name)
}

pub fn touch_skill(conn: &Connection, name: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO skill_stats (name, use_count, last_used_at) VALUES (?1, 1, datetime('now'))
         ON CONFLICT(name) DO UPDATE SET use_count = use_count + 1, last_used_at = datetime('now')",
        params![name],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn search_skills(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Skill>, String> {
    let fts_q: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();

    if fts_q.is_empty() {
        return Ok(vec![]);
    }

    let fts_q = fts_q.join(" ");
    let sql = format!(
        "SELECT {COLS}, {JOIN_STATS}
         WHERE s.rowid IN (SELECT rowid FROM skills_fts WHERE skills_fts MATCH ?1 ORDER BY bm25(skills_fts) LIMIT ?2)"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![fts_q, limit], row_skill)
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
