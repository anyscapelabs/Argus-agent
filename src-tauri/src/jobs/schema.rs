pub const MIGRATE: &str = "
CREATE TABLE IF NOT EXISTS jobs (
    id          TEXT PRIMARY KEY,
    session_id  TEXT,
    label       TEXT NOT NULL DEFAULT '',
    command     TEXT NOT NULL,
    cwd         TEXT,
    profile     TEXT NOT NULL,
    privilege   TEXT NOT NULL DEFAULT 'user',
    state       TEXT NOT NULL,
    exit        INTEGER,
    wake        INTEGER NOT NULL DEFAULT 1,
    permission  TEXT NOT NULL,
    started_ms  INTEGER NOT NULL,
    ended_ms    INTEGER,
    duration_ms INTEGER,
    out_bytes   INTEGER NOT NULL DEFAULT 0,
    truncated   INTEGER NOT NULL DEFAULT 0,
    log_path    TEXT NOT NULL,
    note        TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_started
    ON jobs (started_ms DESC);

CREATE INDEX IF NOT EXISTS idx_jobs_session
    ON jobs (session_id, started_ms DESC);
";

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(MIGRATE).map_err(|err| err.to_string())
}
