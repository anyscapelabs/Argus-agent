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
            .map_err(|err| err.to_string())?;
    }

    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|err| err.to_string())
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

fn bundle_dir(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join(name)
}

fn skill_md(dir: &Path, name: &str) -> std::path::PathBuf {
    bundle_dir(dir, name).join("SKILL.md")
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

fn chk_body(body: &str) -> Result<(), String> {
    if body.chars().count() < 200 {
        return Err(
            "skill body too short — write at least a When-to-use, Steps, and Pitfalls section"
                .into(),
        );
    }

    if body.matches("## ").count() < 2 {
        return Err(
            "skill body needs at least 2 '## ' sections (e.g. When to use, Steps, Pitfalls)".into(),
        );
    }

    Ok(())
}

fn body_hash(body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn ftime(path: &Path) -> Result<i64, String> {
    let meta = fs::metadata(path).map_err(|err| err.to_string())?;
    Ok(meta
        .modified()
        .map_err(|err| err.to_string())?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0))
}

fn parse_md(content: &str) -> Result<(String, String, String), String> {
    let rest = content.strip_prefix("---\n").ok_or("missing frontmatter")?;
    let (fm, body) = rest.split_once("\n---\n").ok_or("unclosed frontmatter")?;

    let mut name = String::new();
    let mut desc = String::new();
    let mut folded = false;

    for line in fm.lines() {
        if folded {
            if line.starts_with(' ') || line.starts_with('\t') {
                desc.push(' ');
                desc.push_str(line.trim());
                continue;
            }

            folded = false;
        }

        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().into();
        } else if let Some(v) = line.strip_prefix("description:") {
            let v = v.trim();

            if v == ">" || v == "|" {
                desc.clear();
                folded = true;
            } else {
                desc = v.trim_matches('"').into();
            }
        }
    }

    if name.is_empty() || desc.is_empty() {
        return Err("frontmatter needs name + description".into());
    }

    Ok((name, desc.trim().to_string(), body.trim_start().into()))
}

fn write_md(dir: &Path, name: &str, description: &str, body: &str) -> Result<(), String> {
    let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n");
    let path = skill_md(dir, name);
    fs::create_dir_all(path.parent().ok_or("bad skill path")?).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

fn index_upsert(conn: &Connection, dir: &Path, name: &str) -> Result<(), String> {
    let path = skill_md(dir, name);
    let content = fs::read_to_string(&path).map_err(|err| err.to_string())?;
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
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn delete_row(conn: &Connection, name: &str) -> Result<(), String> {
    conn.execute("DELETE FROM skills WHERE name = ?1", params![name])
        .map_err(|err| err.to_string())?;

    Ok(())
}

pub fn sync(conn: &Connection, dir: &Path) -> Result<usize, String> {
    let mut seen: HashSet<String> = HashSet::new();

    if let Err(_) = fs::create_dir_all(dir) {
        return Ok(0);
    }

    let entries = match fs::read_dir(dir) {
        Ok(err) => err,
        Err(_) => return Ok(0),
    };

    for entry in entries {
        let path = match entry {
            Ok(err) => err.path(),
            Err(_) => continue,
        };

        if path.is_dir() {
            if !skill_md(
                &dir,
                path.file_name().and_then(|x| x.to_str()).unwrap_or(""),
            )
            .exists()
            {
                continue;
            }

            let name = match path.file_name().and_then(|x| x.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            if chk_name(&name).is_err() || index_upsert(conn, dir, &name).is_err() {
                continue;
            }

            seen.insert(name);
            continue;
        }

        // Legacy flat <name>.md — migrate into a bundle.
        if path.extension().and_then(|x| x.to_str()) == Some("md") {
            let Some(name) = path.file_stem().and_then(|x| x.to_str()) else {
                continue;
            };

            if chk_name(name).is_err() {
                continue;
            }

            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            let Ok((_, desc, body)) = parse_md(&content) else {
                continue;
            };

            if write_md(dir, name, &desc, &body).is_err() {
                continue;
            }

            let _ = fs::remove_file(&path);

            if index_upsert(conn, dir, name).is_err() {
                continue;
            }

            seen.insert(name.to_string());
        }
    }

    let all: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM skills")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map_err(|err| err.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string())?
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
    chk_body(&s.body)?;

    if skill_md(dir, &s.name).exists() || bundle_dir(dir, &s.name).exists() {
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
    .map_err(|err| err.to_string())?;

    index_upsert(conn, dir, &s.name)?;
    get_skill(conn, dir, &s.name)
}

pub fn get_skill(conn: &Connection, dir: &Path, name: &str) -> Result<Skill, String> {
    let path = skill_md(dir, name);

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
        .map_err(|err| err.to_string())?;

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
    .map_err(|err| err.to_string())?
    .ok_or_else(|| format!("skill '{name}' not found"))
}

pub fn list_skills(conn: &Connection) -> Result<Vec<Skill>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS}, {JOIN_STATS} ORDER BY COALESCE(st.last_used_at, s.created_at) DESC"
        ))
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], row_skill)
        .map_err(|err| err.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
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
    match fs::remove_dir_all(bundle_dir(dir, name)) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.to_string()),
    }

    delete_row(conn, name)
}

pub fn touch_skill(conn: &Connection, name: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO skill_stats (name, use_count, last_used_at) VALUES (?1, 1, datetime('now'))
         ON CONFLICT(name) DO UPDATE SET use_count = use_count + 1, last_used_at = datetime('now')",
        params![name],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn walk_files(root: &Path, rel: &Path, out: &mut Vec<String>) {
    let entries = match fs::read_dir(root.join(rel)) {
        Ok(err) => err,
        Err(_) => return,
    };

    for entry in entries {
        let Ok(entry) = entry else { continue };
        let child = rel.join(entry.file_name());

        if entry.path().is_dir() {
            walk_files(root, &child, out);
        } else {
            out.push(child.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub fn list_files(dir: &Path, name: &str) -> Result<Vec<String>, String> {
    chk_name(name)?;
    let root = bundle_dir(dir, name);

    if !skill_md(dir, name).exists() {
        return Err(format!("skill '{name}' not found"));
    }

    let mut out = vec!["SKILL.md".to_string()];
    let mut rest: Vec<String> = vec![];
    walk_files(&root, Path::new("reference"), &mut rest);
    walk_files(&root, Path::new("scripts"), &mut rest);
    walk_files(&root, Path::new("templates"), &mut rest);

    for base in ["README.md", "config.yaml"] {
        if root.join(base).exists() {
            rest.push(base.to_string());
        }
    }

    rest.sort();
    out.extend(rest);

    Ok(out)
}

pub fn read_file(dir: &Path, name: &str, rel: &str) -> Result<String, String> {
    chk_name(name)?;

    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == ".." || p.is_empty()) {
        return Err("bad skill file path".into());
    }

    if rel != "SKILL.md"
        && !rel.starts_with("reference/")
        && !rel.starts_with("scripts/")
        && !rel.starts_with("templates/")
        && rel != "README.md"
        && rel != "config.yaml"
    {
        return Err("file is outside the skill's allowed folders".into());
    }

    let path = bundle_dir(dir, name).join(rel);

    if !path.is_file() {
        return Err(format!("skill file '{rel}' not found"));
    }

    fs::read_to_string(&path).map_err(|err| err.to_string())
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

    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![fts_q, limit], row_skill)
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}
