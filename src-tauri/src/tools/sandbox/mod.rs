pub mod backends;
pub mod plan;
pub mod policy;
pub mod record;
pub mod result;
pub mod schema;
pub mod trust;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;
use crate::gateway::store::{kv_get, kv_set};
use crate::gateway::Gateway;
use crate::tools::shell;

pub use plan::{SandboxError, SandboxResult};
pub use policy::{EnvPolicy, Policy, PolicyCtx, Profile};
pub use record::{ExecutionRecord, SandboxConfig};
pub use result::{Outcome, Termination};
pub use trust::{
    badge, classify, origin_label, origin_of_tool, record_block, trust_of, Origin, Trust,
    TrustBadge, KV_ALLOW_HOSTS,
};

const MAX_ROWS: i64 = 5000;

pub struct Request<'a> {
    pub tool: &'a str,
    pub command: &'a str,
    pub profile: Profile,
    pub cwd: Option<&'a str>,
    pub elevated: bool,
    pub permission: &'a str,
    pub timeout_secs: Option<u64>,
    pub origin: Option<&'a Origin>,
    pub background: bool,
    pub log: Option<shell::LogSink>,
}

pub fn parse_profile(args: &Value, default: Profile) -> SandboxResult<Profile> {
    match args.get("profile").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => {
            Profile::parse(s).ok_or_else(|| SandboxError::Profile(s.to_string()))
        }
        _ => Ok(default),
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn tmp_dir() -> PathBuf {
    let dir = crate::sessions::ext_install::data_dir().join("sandbox");

    if std::fs::create_dir_all(&dir).is_err() {
        return std::env::temp_dir();
    }

    dir
}

fn workdir(cwd: Option<&str>) -> PathBuf {
    match cwd.map(str::trim) {
        Some(s) if !s.is_empty() => PathBuf::from(crate::tools::expand(s)),
        _ => std::env::current_dir().unwrap_or_else(|_| home_dir()),
    }
}

fn allow_hosts(gw: &Gateway) -> Vec<String> {
    let Ok(conn) = gw.conn.lock() else {
        return Vec::new();
    };

    kv_get(&conn, KV_ALLOW_HOSTS)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn apply_env(policy: &Policy, cmd: &mut tokio::process::Command) {
    let EnvPolicy::Only(keys) = &policy.env else {
        return;
    };

    let kept: Vec<(String, String)> = keys
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.clone(), v)))
        .collect();

    cmd.env_clear();
    cmd.envs(kept);
}

fn next_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);

    format!(
        "{}-{}",
        record::now_ms(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

fn audit(gw: &Gateway, r: &ExecutionRecord) {
    let Ok(conn) = gw.conn.lock() else {
        return;
    };

    if record::insert(&conn, r).is_err() {
        return;
    }

    let _ = conn.execute(
        "DELETE FROM sandbox_runs WHERE id NOT IN (
             SELECT id FROM sandbox_runs ORDER BY started_ms DESC LIMIT ?1
         )",
        rusqlite::params![MAX_ROWS],
    );
}

pub async fn run(
    gw: &Gateway,
    req: Request<'_>,
    on_term: Option<(&Channel<StreamEvent>, u32)>,
) -> SandboxResult<Outcome> {
    let project = workdir(req.cwd);
    let tmp = tmp_dir();

    let ctx = PolicyCtx {
        project: &project,
        tmp: &tmp,
        home: &home_dir(),
    };

    let policy = policy::resolve(req.profile, &ctx);
    let (binary, args) = shell::argv(req.command, req.elevated);
    let cwd = policy.cwd.to_string_lossy().into_owned();

    let backend = if policy.profile.is_isolated() {
        backends::current_name()
    } else {
        "none"
    };

    let mut cmd = shell::command_argv(&binary, &args, Some(&cwd));
    let mut guard = None;

    if policy.profile.is_isolated() {
        let be = backends::current();
        let plan = be.plan(&policy)?;

        let (b, a) = wrap_argv(&plan, binary.clone(), &args, req.command)?;

        // Windows cannot confine a tokio Command: the AppContainer token only
        // exists if CreateProcessW is given the security capabilities, and
        // tokio offers no hook that reaches that call. It refuses rather than
        // dropping to an unconfined process.
        #[cfg(target_os = "windows")]
        {
            if let plan::Plan::Windows(jp) = &plan {
                let raw = backends::windows::spawn(
                    &b,
                    &a,
                    &cwd,
                    jp,
                    backends::windows::env_keys(&policy),
                )?;

                return finish(gw, &req, &policy, raw.into(), on_term, backend).await;
            }
        }

        cmd = shell::command_argv(&b, &a, Some(&cwd));

        apply_env(&policy, &mut cmd);
        guard = Some(be.apply(&plan, &mut cmd)?);
    } else {
        apply_env(&policy, &mut cmd);
    }

    let child = cmd
        .spawn()
        .map_err(|err| SandboxError::Spawn(err.to_string()))?;

    if let Some(g) = guard {
        let Some(pid) = child.id() else {
            return Err(SandboxError::Spawn(
                "child vanished before the guard ran".into(),
            ));
        };

        if let Err(err) = g.run(pid) {
            let _ = shell::kill_process_group(pid);
            return Err(err);
        }
    }

    finish(gw, &req, &policy, child.into(), on_term, backend).await
}

#[allow(unused_variables)]
fn wrap_argv(
    plan: &plan::Plan,
    binary: PathBuf,
    args: &[String],
    command: &str,
) -> SandboxResult<(PathBuf, Vec<String>)> {
    #[cfg(target_os = "macos")]
    if matches!(plan, plan::Plan::Macos(_)) {
        let shell_name = binary.to_string_lossy().into_owned();
        let mut argv = backends::macos::wrap(plan, &shell_name, command)?;
        let bin = PathBuf::from(argv.remove(0));

        return Ok((bin, argv));
    }

    Ok((binary, args.to_vec()))
}

async fn finish(
    gw: &Gateway,
    req: &Request<'_>,
    policy: &Policy,
    child: shell::Child,
    on_term: Option<(&Channel<StreamEvent>, u32)>,
    backend: &'static str,
) -> SandboxResult<Outcome> {
    let (idx, chan) = match on_term {
        Some((c, i)) => (i, Some(c)),
        None => (0, None),
    };

    let asked = req
        .timeout_secs
        .or(policy.limits.wall_secs)
        .unwrap_or_else(|| shell::default_timeout_for(req.command));

    let ceiling = if req.background {
        crate::jobs::DEADMAN_MAX
    } else {
        shell::TERM_TIMEOUT_MAX
    };

    let hard = Duration::from_secs(asked.clamp(1, ceiling));
    let cap = policy.limits.output_bytes.unwrap_or(shell::DEFAULT_OUT_CAP);

    let started = Instant::now();
    let ran = shell::run_child(child, idx, chan, hard, cap, req.log.clone())
        .await
        .map_err(SandboxError::Spawn)?;

    let duration_ms = started.elapsed().as_millis();

    let termination = if ran.cancelled {
        Termination::Cancelled
    } else if ran.timed_out {
        Termination::TimedOut
    } else if ran.exit == 0 {
        Termination::Completed
    } else {
        Termination::Failed
    };

    if ran.cancelled {
        return Err(SandboxError::Cancelled);
    }

    let outcome = Outcome {
        exit: ran.exit,
        stdout: ran.out,
        stderr: String::new(),
        duration_ms,
        termination,
        truncated: ran.truncated,
    };

    audit(
        gw,
        &ExecutionRecord {
            id: next_id(),
            tool: req.tool.to_string(),
            command: req.command.to_string(),
            profile: policy.profile,
            backend: backend.to_string(),
            origin: req.origin.map(|o| origin_label(Some(o), &allow_hosts(gw))),
            permission: req.permission.to_string(),
            started_ms: record::now_ms(),
            duration_ms,
            exit: outcome.exit,
            termination,
            out_bytes: outcome.stdout.len(),
            err_bytes: 0,
            truncated: outcome.truncated,
        },
    );

    Ok(outcome)
}

pub fn config(gw: &Gateway) -> Result<SandboxConfig, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    Ok(SandboxConfig {
        hosts: kv_get(&conn, KV_ALLOW_HOSTS)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default(),
        default_profile: kv_get(&conn, record::KV_DEFAULT_PROFILE)
            .and_then(|v| Profile::parse(&v))
            .unwrap_or(Profile::Restricted),
        net_allow: kv_get(&conn, record::KV_NET_ALLOW)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_else(|| vec![80, 443]),
    })
}

pub fn set_config(gw: &Gateway, cfg: &SandboxConfig) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    kv_set(
        &conn,
        KV_ALLOW_HOSTS,
        &serde_json::to_string(&cfg.hosts).unwrap_or_default(),
    )?;

    kv_set(
        &conn,
        record::KV_DEFAULT_PROFILE,
        cfg.default_profile.as_str(),
    )?;

    kv_set(
        &conn,
        record::KV_NET_ALLOW,
        &serde_json::to_string(&cfg.net_allow).unwrap_or_default(),
    )
}

#[tauri::command]
pub fn sandbox_config(gw: tauri::State<'_, Gateway>) -> Result<SandboxConfig, String> {
    config(&gw)
}

#[tauri::command]
pub fn sandbox_set_config(gw: tauri::State<'_, Gateway>, cfg: SandboxConfig) -> Result<(), String> {
    set_config(&gw, &cfg)
}

#[tauri::command]
pub fn sandbox_selftest() -> Result<Value, String> {
    let be = backends::current();

    match be.selftest() {
        Ok(probe) => Ok(serde_json::json!({
            "ok": true,
            "backend": probe.backend,
            "enforcing": probe.enforcing,
            "detail": probe.detail,
        })),
        Err(err) => Ok(serde_json::json!({
            "ok": false,
            "backend": err.backend(),
            "enforcing": false,
            "detail": err.to_string(),
        })),
    }
}

#[tauri::command]
pub fn sandbox_runs(
    gw: tauri::State<'_, Gateway>,
    limit: Option<i64>,
) -> Result<Vec<Value>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let n = limit.unwrap_or(50).clamp(1, 500);

    let mut stmt = conn
        .prepare(
            "SELECT id, tool, command, profile, backend, origin, permission, started_ms,
                    duration_ms, exit, termination, out_bytes, truncated
             FROM sandbox_runs ORDER BY started_ms DESC LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![n], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "tool": r.get::<_, String>(1)?,
                "command": r.get::<_, String>(2)?,
                "profile": r.get::<_, String>(3)?,
                "backend": r.get::<_, String>(4)?,
                "origin": r.get::<_, Option<String>>(5)?,
                "permission": r.get::<_, String>(6)?,
                "startedMs": r.get::<_, i64>(7)?,
                "durationMs": r.get::<_, i64>(8)?,
                "exit": r.get::<_, i64>(9)?,
                "termination": r.get::<_, String>(10)?,
                "outBytes": r.get::<_, i64>(11)?,
                "truncated": r.get::<_, i64>(12)? != 0,
            }))
        })
        .map_err(|err| err.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}
