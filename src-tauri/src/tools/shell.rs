use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::gateway::schema::StreamEvent;

const TERM_TIMEOUT: Duration = Duration::from_secs(120);

async fn pump<R>(
    rd: R,
    idx: u32,
    chan: Option<Channel<StreamEvent>>,
) -> String
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut all = String::new();
    let mut r = BufReader::new(rd);
    let mut chunk = Vec::new();

    loop {
        chunk.clear();

        match r.read_until(b'\n', &mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let s = String::from_utf8_lossy(&chunk);
        all.push_str(&s);

        if let Some(c) = &chan {
            let _ = c.send(StreamEvent::Term {
                idx,
                chunk: s.into_owned(),
            });
        }
    }

    all
}

pub async fn run_stream(
    args: &Value,
    idx: u32,
    chan: Option<&Channel<StreamEvent>>,
) -> Result<(String, i64), String> {
    let cmd = args["command"]
        .as_str()
        .ok_or("terminal needs a command")?;

    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd).kill_on_drop(true);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(cwd) = args["cwd"].as_str() {
        c.current_dir(super::expand(cwd));
    }

    let mut child = c.spawn().map_err(|e| e.to_string())?;

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;

    let t1 = tokio::spawn(pump(out, idx, chan.cloned()));
    let t2 = tokio::spawn(pump(err, idx, chan.cloned()));

    let run = async move {
        let (o, e) = tokio::join!(t1, t2);
        let o = o.unwrap_or_default();
        let e = e.unwrap_or_default();
        let st = child.wait().await.map_err(|err| err.to_string())?;
        Ok::<(String, String, std::process::ExitStatus), String>((o, e, st))
    };

    let res = tokio::time::timeout(TERM_TIMEOUT, run).await;

    let (out_txt, err_txt, code) = match res {
        Ok(Ok((o, e, st))) => {
            let code = st.code().map(|c| c as i64).unwrap_or(-1);
            (o, e, code)
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => return Ok(("command timed out after 120s".into(), -1)),
    };

    let mut all = out_txt;

    if !err_txt.trim().is_empty() {
        all.push_str(&err_txt);
    }

    Ok((super::clip_ends(all), code))
}
