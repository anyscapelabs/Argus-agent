pub mod detect;

use std::process::Stdio;
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

tokio::task_local! {
    pub static CANCEL: Arc<tokio::sync::Notify>;
}

type Buf = Arc<StdMutex<String>>;

async fn pump<R>(
    rd: R,
    idx: u32,
    chan: Option<Channel<StreamEvent>>,
    buf: Buf,
    eof: watch::Sender<usize>,
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

        let s = String::from_utf8_lossy(&chunk);

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

fn spawn_shell(cmd: &str, cwd: Option<&str>) -> Result<tokio::process::Child, String> {
    let cfg = detect::status();
    let mut c = Command::new(&cfg.binary);

    match cfg.kind {
        ShellKind::Wsl => {
            c.arg("bash").arg("-c").arg(cmd);
        }
        ShellKind::PowerShell => {
            c.arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg(cmd);
        }
        ShellKind::Cmd => {
            c.arg("/d").arg("/s").arg("/c").arg(cmd);
        }
        ShellKind::Bash | ShellKind::Sh => {
            c.arg("-c").arg(cmd);
        }
    }

    #[cfg(unix)]
    c.process_group(0);

    c.kill_on_drop(true);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(dir) = cwd {
        c.current_dir(super::expand(dir));
    }

    c.spawn().map_err(|err| err.to_string())
}

pub async fn run_stream(
    args: &Value,
    idx: u32,
    chan: Option<&Channel<StreamEvent>>,
) -> Result<(String, i64), String> {
    let cmd = args["command"].as_str().ok_or("terminal needs a command")?;
    let hard = args["timeout"]
        .as_u64()
        .map(|t| t.clamp(TERM_TIMEOUT_MIN, TERM_TIMEOUT_MAX))
        .unwrap_or_else(|| default_timeout_for(cmd));
    let hard_dur = Duration::from_secs(hard);

    let mut child = spawn_shell(cmd, args["cwd"].as_str())?;
    let pid = child.id();

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;

    let out_buf: Buf = Arc::new(StdMutex::new(String::new()));
    let err_buf: Buf = Arc::new(StdMutex::new(String::new()));
    let (eof_tx, eof_rx) = watch::channel(0usize);

    let t1 = tokio::spawn(pump(
        out,
        idx,
        chan.cloned(),
        out_buf.clone(),
        eof_tx.clone(),
    ));
    let t2 = tokio::spawn(pump(err, idx, chan.cloned(), err_buf.clone(), eof_tx));

    let outcome = tokio::time::timeout(hard_dur, wait_hard(&mut child)).await;

    match outcome {
        Ok(WaitOut::Done(Ok(st))) => {
            let code = st.code().map(|c| c as i64).unwrap_or(-1);

            let mut rx = eof_rx.clone();
            let _ = tokio::time::timeout(DRAIN, rx.wait_for(|n| *n >= 2)).await;

            t1.abort();
            t2.abort();

            let mut all = take(&out_buf);

            let err_txt = take(&err_buf);
            if !err_txt.trim().is_empty() {
                all.push_str(&err_txt);
            }

            Ok((super::clip_ends(all), code))
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

            Err("stopped".into())
        }
        Err(_) => {
            if let Some(p) = pid {
                let _ = kill_process_group(p);
            }

            let _ = child.start_kill();

            let partial = take(&out_buf);
            t1.abort();
            t2.abort();

            Ok((format!("{partial}\ncommand timed out after {hard}s"), -1))
        }
    }
}
