use rusqlite::Connection;

pub const MIGRATE: &str = "CREATE TABLE IF NOT EXISTS connector_logs (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  service    TEXT NOT NULL,
  event      TEXT NOT NULL,
  detail     TEXT NOT NULL DEFAULT '',
  status     TEXT NOT NULL DEFAULT 'ok'
);
CREATE INDEX IF NOT EXISTS idx_connector_logs_service ON connector_logs(service);";

const KEEP: i64 = 1000;

fn redact(s: &str) -> String {
    let mut out = s.to_string();

    for mark in ["?key=", "&key=", "?token=", "&token=", "client_secret="] {
        let mut from = 0;

        while let Some(i) = out[from..].find(mark) {
            let start = from + i + mark.len();
            let end = out[start..]
                .find(['&', ' ', '"', '\''])
                .map(|e| start + e)
                .unwrap_or(out.len());

            out.replace_range(start..end, "..redacted..");
            from = start + 12;
        }
    }

    out
}

fn db() -> Option<Connection> {
    let path = crate::sessions::ext_install::data_dir().join("argus.db");
    let conn = Connection::open(path).ok()?;
    let _ = conn.execute_batch("PRAGMA busy_timeout = 5000;");

    Some(conn)
}

pub fn event(service: &str, event: &str, detail: &str, status: &str) {
    let Some(conn) = db() else {
        return;
    };

    let _ = conn.execute_batch(MIGRATE);
    let _ = conn.execute(
        "INSERT INTO connector_logs (service, event, detail, status) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![service, event, redact(detail), status],
    );
    let _ = conn.execute(
        "DELETE FROM connector_logs WHERE id NOT IN (SELECT id FROM connector_logs ORDER BY id DESC LIMIT ?1)",
        rusqlite::params![KEEP],
    );
}

pub fn api<T>(service: &str, label: &str, res: &Result<T, String>) {
    match res {
        Ok(_) => event(service, "api", label, "ok"),
        Err(err) => event(service, "api", &format!("{label}: {err}"), "err"),
    }
}
