use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::Value;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::gateway::schema::StreamEvent;

const TERM_TIMEOUT: Duration = Duration::from_secs(120);
const DRAIN: Duration = Duration::from_secs(2);

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

pub async fn run_stream(
    args: &Value,
    idx: u32,
    chan: Option<&Channel<StreamEvent>>,
) -> Result<(String, i64), String> {
    let cmd = args["command"].as_str().ok_or("terminal needs a command")?;

    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd).kill_on_drop(true);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(cwd) = args["cwd"].as_str() {
        c.current_dir(super::expand(cwd));
    }

    let mut child = c.spawn().map_err(|err| err.to_string())?;

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

    let res = tokio::time::timeout(TERM_TIMEOUT, child.wait()).await;

    match res {
        Ok(Ok(st)) => {
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
        Ok(Err(err)) => {
            t1.abort();
            t2.abort();
            Err(err.to_string())
        }
        Err(_) => {
            let _ = child.start_kill();

            let partial = take(&out_buf);
            t1.abort();
            t2.abort();

            Ok((format!("{partial}\ncommand timed out after 120s"), -1))
        }
    }
}
