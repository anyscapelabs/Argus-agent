use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

const GREP_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(args: &Value) -> Result<String, String> {
    let pattern = args["pattern"].as_str().ok_or("grep needs a pattern")?;
    let path = args["path"].as_str().unwrap_or(".");

    let mut c = Command::new("grep");
    c.arg("-rnH")
        .arg("--binary-files=without-match")
        .arg("--exclude-dir=.git")
        .arg("--exclude-dir=node_modules");

    if args["ignore_case"].as_bool().unwrap_or(false) {
        c.arg("-i");
    }

    c.arg("--").arg(pattern).arg(super::expand(path));

    let out = tokio::time::timeout(GREP_TIMEOUT, c.output())
        .await
        .map_err(|_| "grep timed out after 30s")?
        .map_err(|e| e.to_string())?;

    let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if txt.is_empty() {
        return Ok("(no matches)".into());
    }

    Ok(super::clip(txt))
}
