pub mod detect;

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::gateway::schema::StreamEvent;

pub use detect::{ShellConfig, ShellKind};

const TERM_TIMEOUT_MIN: u64 = 10;
const TERM_TIMEOUT_MAX: u64 = 1800;
const TERM_TIMEOUT_DEF: u64 = 120;
const TERM_TIMEOUT_LONG: u64 = 600;
const DRAIN: Duration = Duration::from_secs(2);

/// Default ceiling on what one command can accumulate. Display clips far below
/// this, so it never changes what the agent sees — it only stops a runaway
/// writer from eating Argus's memory.
pub const DEFAULT_OUT_CAP: usize = 8 * 1024 * 1024;

pub fn needs_elevation(cmd: &str) -> bool {
    matches!(
        cmd.trim_start().split_whitespace().next().unwrap_or(""),
        "sudo" | "su" | "doas"
    )
}

tokio::task_local! {
    pub static CANCEL: Arc<tokio::sync::Notify>;
}

type Buf = Arc<StdMutex<String>>;

struct Budget {
    left: AtomicUsize,
    dropped: AtomicUsize,
}

async fn pump<R>(
    rd: R,
    idx: u32,
    chan: Option<Channel<StreamEvent>>,
    buf: Buf,
    eof: watch::Sender<usize>,
    budget: Arc<Budget>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut r = BufReader::new(rd);
    let mut chunk = Vec::new();

    loop {
        chunk.clear();

        match r.read_until(b'\n', &mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let room = budget.left.load(Ordering::Relaxed);
        let take = chunk.len().min(room);

        if take < chunk.len() {
            budget
                .dropped
                .fetch_add(chunk.len() - take, Ordering::Relaxed);
        }

        budget.left.store(room - take, Ordering::Relaxed);

        if take == 0 {
            continue;
        }

        // Sliced as bytes: a split codepoint degrades to a replacement char
        // instead of panicking on a char boundary.
        let s = String::from_utf8_lossy(&chunk[..take]);

        if let Ok(mut g) = buf.lock() {
            g.push_str(&s);
        }

        if let Some(c) = &chan {
            let _ = c.send(StreamEvent::Term {
                idx,
                chunk: s.into_owned(),
            });
        }
    }

    eof.send_modify(|n| *n += 1);
}

fn take(buf: &Buf) -> String {
    std::mem::take(&mut buf.lock().unwrap_or_else(|p| p.into_inner()))
}

pub fn default_timeout_for(cmd: &str) -> u64 {
    const LONG: [&str; 12] = [
        "cargo build",
        "cargo test",
        "bun install",
        "npm install",
        "pip install",
        "apt ",
        "dnf ",
        "pacman ",
        "rm -rf",
        "find /",
        "du -sh",
        "rsync ",
    ];

    let lower = cmd.to_lowercase();

    if LONG.iter().any(|p| lower.contains(p)) {
        return TERM_TIMEOUT_LONG;
    }

    TERM_TIMEOUT_DEF
}

pub fn kill_process_group(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let out = unsafe { libc::killpg(pid as i32, libc::SIGKILL) };

        if out != 0 {
            let code = std::io::Error::last_os_error().raw_os_error();

            if code != Some(libc::ESRCH) {
                return Err(format!("killpg {pid} failed: {code:?}"));
            }
        }

        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        Err("process groups unsupported on this platform".into())
    }
}

/// Split from spawning so the sandbox can wrap the argv — macOS puts
/// `sandbox-exec` in front of it — without a second copy of the shell rules.
pub fn argv(cmd: &str, elevated: bool) -> (PathBuf, Vec<String>) {
    let cfg = detect::status();

    let mut args: Vec<String> = Vec::new();

    match cfg.kind {
        ShellKind::Wsl => {
            args.push("bash".into());
            args.push("-c".into());
        }
        ShellKind::PowerShell => {
            args.push("-NoProfile".into());
            args.push("-NonInteractive".into());
            args.push("-Command".into());
        }
        ShellKind::Cmd => {
            args.push("/d".into());
            args.push("/s".into());
            args.push("/c".into());
        }
        ShellKind::Bash | ShellKind::Sh => {
            args.push("-c".into());
        }
    }

    args.push(cmd.into());

    if elevated {
        // pkexec opens the OS authorization dialog itself: the password goes to
        // polkit, never to Argus, and pkexec runs with a sanitized env.
        let mut full = vec![cfg.binary.to_string_lossy().into_owned()];
        full.extend(args);
        return (PathBuf::from("pkexec"), full);
    }

    (cfg.binary, args)
}

pub fn command_argv(binary: &PathBuf, args: &[String], cwd: Option<&str>) -> Command {
    let mut c = Command::new(binary);
    c.args(args);

    #[cfg(unix)]
    c.process_group(0);

    c.kill_on_drop(true);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(dir) = cwd {
        c.current_dir(super::expand(dir));
    }

    c
}

pub fn spawn_argv(
    binary: &PathBuf,
    args: &[String],
    cwd: Option<&str>,
) -> Result<tokio::process::Child, String> {
    command_argv(binary, args, cwd)
        .spawn()
        .map_err(|err| err.to_string())
}

fn spawn_shell(
    cmd: &str,
    cwd: Option<&str>,
    elevated: bool,
) -> Result<tokio::process::Child, String> {
    let cfg = detect::status();

    if elevated && !matches!(cfg.kind, ShellKind::Bash | ShellKind::Sh) {
        return Err("admin elevation is only supported on Linux bash/sh shells".into());
    }

    let (binary, args) = argv(cmd, elevated);
    spawn_argv(&binary, &args, cwd)
}

enum WaitOut {
    Done(std::io::Result<std::process::ExitStatus>),
    Cancelled,
}

async fn wait_hard(child: &mut tokio::process::Child) -> WaitOut {
    let cancel = async {
        if let Ok(n) = CANCEL.try_get() {
            n.notified().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::pin!(cancel);

    loop {
        tokio::select! {
            res = child.wait() => return WaitOut::Done(res),
            _ = &mut cancel => return WaitOut::Cancelled,
        }
    }
}

pub struct RawRun {
    pub out: String,
    pub exit: i64,
    pub cancelled: bool,
    pub timed_out: bool,
    pub out_bytes: usize,
    pub err_bytes: usize,
    pub truncated: bool,
}

/// Shared by `terminal` and `sandbox` so there is one execution path: both
/// stream through `chan`, drain both pipes, and tear the group down.
pub async fn run_child(
    mut child: tokio::process::Child,
    idx: u32,
    chan: Option<&Channel<StreamEvent>>,
    hard: Duration,
    cap: usize,
) -> Result<RawRun, String> {
    let pid = child.id();

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;

    let out_buf: Buf = Arc::new(StdMutex::new(String::new()));
    let err_buf: Buf = Arc::new(StdMutex::new(String::new()));
    let (eof_tx, eof_rx) = watch::channel(0usize);

    let budget = Arc::new(Budget {
        left: AtomicUsize::new(cap),
        dropped: AtomicUsize::new(0),
    });

    let t1 = tokio::spawn(pump(
        out,
        idx,
        chan.cloned(),
        out_buf.clone(),
        eof_tx.clone(),
        budget.clone(),
    ));
    let t2 = tokio::spawn(pump(
        err,
        idx,
        chan.cloned(),
        err_buf.clone(),
        eof_tx,
        budget.clone(),
    ));

    let outcome = tokio::time::timeout(hard, wait_hard(&mut child)).await;

    let finish = |mut all: String, err_txt: String| -> String {
        if !err_txt.trim().is_empty() {
            all.push_str(&err_txt);
        }

        super::clip_ends(all)
    };

    match outcome {
        Ok(WaitOut::Done(Ok(st))) => {
            let code = st.code().map(|c| c as i64).unwrap_or(-1);

            let mut rx = eof_rx.clone();
            let _ = tokio::time::timeout(DRAIN, rx.wait_for(|n| *n >= 2)).await;

            t1.abort();
            t2.abort();

            Ok(RawRun {
                out: finish(take(&out_buf), take(&err_buf)),
                exit: code,
                cancelled: false,
                timed_out: false,
                out_bytes: budget.left.load(Ordering::Relaxed),
                err_bytes: 0,
                truncated: budget.dropped.load(Ordering::Relaxed) > 0,
            })
        }
        Ok(WaitOut::Done(Err(err))) => {
            t1.abort();
            t2.abort();
            Err(err.to_string())
        }
        Ok(WaitOut::Cancelled) => {
            if let Some(p) = pid {
                let _ = kill_process_group(p);
            }

            let _ = child.start_kill();
            t1.abort();
            t2.abort();

            Ok(RawRun {
                out: String::new(),
                exit: -2,
                cancelled: true,
                timed_out: false,
                out_bytes: 0,
                err_bytes: 0,
                truncated: false,
            })
        }
        Err(_) => {
            if let Some(p) = pid {
                let _ = kill_process_group(p);
            }

            let _ = child.start_kill();

            let partial = take(&out_buf);
            t1.abort();
            t2.abort();

            Ok(RawRun {
                out: partial,
                exit: -1,
                cancelled: false,
                timed_out: true,
                out_bytes: 0,
                err_bytes: 0,
                truncated: budget.dropped.load(Ordering::Relaxed) > 0,
            })
        }
    }
}

pub fn timeout_from(args: &Value, cmd: &str) -> Duration {
    let secs = args["timeout"]
        .as_u64()
        .map(|t| t.clamp(TERM_TIMEOUT_MIN, TERM_TIMEOUT_MAX))
        .unwrap_or_else(|| default_timeout_for(cmd));

    Duration::from_secs(secs)
}

pub async fn run_stream(
    args: &Value,
    idx: u32,
    chan: Option<&Channel<StreamEvent>>,
) -> Result<(String, i64), String> {
    let cmd = args["command"].as_str().ok_or("terminal needs a command")?;
    let elevated = args.get("privilege").and_then(|v| v.as_str()) == Some("admin");

    if !elevated && needs_elevation(cmd) {
        return Err(
            "that command needs elevated privileges — do not write sudo yourself; \
            call the terminal tool again with privilege \"admin\" instead, and the user \
            will approve it first in Argus and then in the OS dialog"
                .into(),
        );
    }

    let hard = timeout_from(args, cmd);
    let child = spawn_shell(cmd, args["cwd"].as_str(), elevated)?;
    let run = run_child(child, idx, chan, hard, DEFAULT_OUT_CAP).await?;

    if run.cancelled {
        return Err("stopped".into());
    }

    if run.timed_out {
        let secs = hard.as_secs();
        return Ok((format!("{}\ncommand timed out after {secs}s", run.out), -1));
    }

    Ok((run.out, run.exit))
}
