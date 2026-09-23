pub const MIGRATE: &str = "
CREATE TABLE IF NOT EXISTS sandbox_runs (
    id           TEXT PRIMARY KEY,
    tool         TEXT NOT NULL,
    command      TEXT NOT NULL,
    profile      TEXT NOT NULL,
    backend      TEXT NOT NULL,
    origin       TEXT,
    permission   TEXT NOT NULL,
    started_ms   INTEGER NOT NULL,
    duration_ms  INTEGER NOT NULL,
    exit         INTEGER NOT NULL,
    termination  TEXT NOT NULL,
    out_bytes    INTEGER NOT NULL,
    err_bytes    INTEGER NOT NULL,
    truncated    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sandbox_runs_started
    ON sandbox_runs (started_ms DESC);
";

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(MIGRATE).map_err(|err| err.to_string())
}
