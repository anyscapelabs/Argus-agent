use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::schema::{LibItem, NewLibItem, NAME_MAX};

const COLS: &str = "id, name, kind, ext, path, session_id, sz, created_at";

pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(super::schema::MIGRATE)
        .map_err(|e| e.to_string())
}

fn row_item(r: &rusqlite::Row) -> rusqlite::Result<LibItem> {
    Ok(LibItem {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        ext: r.get(3)?,
        path: r.get(4)?,
        session_id: r.get(5)?,
        sz: r.get(6)?,
        created_at: r.get(7)?,
    })
}

fn abs_path(dir: &Path, rel: &str) -> PathBuf {
    dir.join(rel)
}

fn kind_of(ext: &str) -> &'static str {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => "image",
        "mp4" | "webm" | "mov" | "avi" | "mkv" => "video",
        "ppt" | "pptx" | "odp" | "key" => "presentation",
        "xls" | "xlsx" | "ods" | "csv" => "sheet",
        "doc" | "docx" | "pdf" | "txt" | "md" | "rtf" => "doc",
        _ => "file",
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();

    let s = s.trim().to_string();
    if s.is_empty() {
        return "file".into();
    }

    s
}

fn chk_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > NAME_MAX {
        return Err(format!("name must be 1-{NAME_MAX} chars"));
    }

    Ok(())
}

pub fn add(conn: &Connection, dir: &Path, item: &NewLibItem) -> Result<LibItem, String> {
    chk_name(&item.name)?;

    let src = Path::new(&item.source_path);
    if !src.is_file() {
        return Err(format!("source file '{}' not found", item.source_path));
    }

    let ext = src
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| x.to_lowercase())
        .unwrap_or_else(|| "bin".into());

    let ym = chrono_ym();
    fs::create_dir_all(dir.join(&ym)).map_err(|e| e.to_string())?;

    let file_name = sanitize(&item.name);
    let dot_ext = if ext == "bin" {
        String::new()
    } else {
        format!(".{ext}")
    };

    let mut rel = format!("{ym}/{file_name}{dot_ext}");
    let mut n = 1;
    while dir.join(&rel).exists() {
        n += 1;
        rel = format!("{ym}/{file_name}-{n}{dot_ext}");
    }

    fs::copy(src, abs_path(dir, &rel)).map_err(|e| e.to_string())?;
    let sz = fs::metadata(abs_path(dir, &rel))
        .map_err(|e| e.to_string())?
        .len() as i64;

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO library (id, name, kind, ext, path, session_id, sz) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, item.name, kind_of(&ext), ext, rel, item.session_id, sz],
    )
    .map_err(|e| e.to_string())?;

    get(conn, dir, &id)
}

fn chrono_ym() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (y, m) = civil_from_days((now / 86_400) as i64);
    format!("{y:04}-{m:02}")
}

fn civil_from_days(z: i64) -> (i64, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m)
}

pub fn list(conn: &Connection, kind: Option<&str>) -> Result<Vec<LibItem>, String> {
    let sql = match kind {
        Some(_) => format!("SELECT {COLS} FROM library WHERE kind = ?1 ORDER BY created_at DESC"),
        None => format!("SELECT {COLS} FROM library ORDER BY created_at DESC"),
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = match kind {
        Some(k) => stmt.query_map(params![k], row_item),
        None => stmt.query_map([], row_item),
    }
    .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn get(conn: &Connection, dir: &Path, id: &str) -> Result<LibItem, String> {
    let item = conn
        .query_row(
            &format!("SELECT {COLS} FROM library WHERE id = ?1"),
            params![id],
            row_item,
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| String::from("item not found"))?;

    if !abs_path(dir, &item.path).exists() {
        conn.execute("DELETE FROM library WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        return Err("item file missing, index row dropped".into());
    }

    Ok(item)
}

pub fn delete(conn: &Connection, dir: &Path, id: &str) -> Result<(), String> {
    let row: Option<String> = conn
        .query_row("SELECT path FROM library WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;

    let rel = match row {
        Some(r) => r,
        None => return Ok(()),
    };

    match fs::remove_file(abs_path(dir, &rel)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }

    conn.execute("DELETE FROM library WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn search(conn: &Connection, query: &str, limit: i64) -> Result<Vec<LibItem>, String> {
    let fts_q: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();

    if fts_q.is_empty() {
        return Ok(vec![]);
    }

    let fts_q = fts_q.join(" ");
    let sql = format!(
        "SELECT {COLS} FROM library WHERE rowid IN
         (SELECT rowid FROM library_fts WHERE library_fts MATCH ?1 ORDER BY bm25(library_fts) LIMIT ?2)"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![fts_q, limit], row_item)
        .map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn sync(conn: &Connection, dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }

    let known: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT path FROM library")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    let known: std::collections::HashSet<String> = known.into_iter().collect();

    let mut on_disk: Vec<String> = vec![];

    for month in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let month = match month {
            Ok(m) => m.path(),
            Err(_) => continue,
        };

        if !month.is_dir() {
            continue;
        }

        for entry in fs::read_dir(&month).map_err(|e| e.to_string())? {
            let path = match entry {
                Ok(e) => e.path(),
                Err(_) => continue,
            };

            if !path.is_file() {
                continue;
            }

            let rel = match path.strip_prefix(dir) {
                Ok(r) => r.to_string_lossy().into_owned(),
                Err(_) => continue,
            };

            on_disk.push(rel.clone());

            if known.contains(&rel) {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or("file")
                .to_string();
            let ext = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("bin")
                .to_lowercase();
            let sz = fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0);

            conn.execute(
                "INSERT OR IGNORE INTO library (id, name, kind, ext, path, sz) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![Uuid::new_v4().to_string(), name, kind_of(&ext), ext, rel, sz],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    for rel in known {
        if !on_disk.contains(&rel) {
            conn.execute("DELETE FROM library WHERE path = ?1", params![rel])
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
