use serde::{Deserialize, Serialize};

use super::policy::Profile;
use super::result::Termination;

pub const KV_DEFAULT_PROFILE: &str = "sandbox.default_profile";
pub const KV_NET_ALLOW: &str = "sandbox.net_allow";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    pub hosts: Vec<String>,
    pub default_profile: Profile,
    pub net_allow: Vec<u16>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            hosts: Vec::new(),
            default_profile: Profile::Restricted,
            net_allow: vec![80, 443],
        }
    }
}

/// Metadata only: output is never persisted, so the audit trail does not become
/// a second place secrets live.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub id: String,
    pub tool: String,
    pub command: String,
    pub profile: Profile,
    pub backend: String,
    pub origin: Option<String>,
    pub permission: String,
    pub started_ms: i64,
    pub duration_ms: u128,
    pub exit: i64,
    pub termination: Termination,
    pub out_bytes: usize,
    pub err_bytes: usize,
    pub truncated: bool,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn insert(conn: &rusqlite::Connection, r: &ExecutionRecord) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sandbox_runs (id, tool, command, profile, backend, origin, permission,
                                   started_ms, duration_ms, exit, termination,
                                   out_bytes, err_bytes, truncated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            r.id,
            r.tool,
            r.command,
            r.profile.as_str(),
            r.backend,
            r.origin,
            r.permission,
            r.started_ms,
            r.duration_ms as i64,
            r.exit,
            r.termination.as_str(),
            r.out_bytes as i64,
            r.err_bytes as i64,
            r.truncated as i64,
        ],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}
