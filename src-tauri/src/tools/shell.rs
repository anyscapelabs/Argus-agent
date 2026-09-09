use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

const SHELL_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(args: &Value) -> Result<String, String> {
    let cmd = args["command"]
        .as_str()
        .ok_or("shell.run needs a command")?;

    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);

    if let Some(cwd) = args["cwd"].as_str() {
        c.current_dir(super::expand(cwd));
    }

    let out = tokio::time::timeout(SHELL_TIMEOUT, c.output())
        .await
        .map_err(|_| "shell.run timed out after 30s")?
        .map_err(|e| e.to_string())?;

    let mut txt = String::from_utf8_lossy(&out.stdout).into_owned();
    let err_txt = String::from_utf8_lossy(&out.stderr);

    if !err_txt.trim().is_empty() {
        txt.push_str(&err_txt);
    }

    if !out.status.success() {
        txt = format!("exit {}: {txt}", out.status.code().unwrap_or(-1));
    }

    Ok(super::clip(txt))
}
