use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::sessions::ext_install::data_dir;

struct Shot {
    w: i64,
    h: i64,
    screen_w: i64,
    screen_h: i64,
}

static SHOT: AsyncMutex<Option<Shot>> = AsyncMutex::const_new(None);

const BIN_TOOLS: &[&str] = &["xdotool", "import", "convert", "identify", "wmctrl"];

fn have(bin: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| std::path::Path::new(d).join(bin).exists())
}

fn check_tools() -> Result<(), String> {
    let missing: Vec<&str> = BIN_TOOLS.iter().filter(|b| !have(b)).copied().collect();

    if missing.is_empty() {
        return Ok(());
    }

    Err(format!(
        "computer use needs {} — install them (packages: xdotool, imagemagick, wmctrl). \
         Computer use also needs an X11 desktop.",
        missing.join(", ")
    ))
}

fn run(bin: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("{bin}: {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "{bin} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn dims(s: &str) -> Result<(i64, i64), String> {
    let mut it = s.split_whitespace().filter_map(|v| v.parse::<i64>().ok());

    match (it.next(), it.next()) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(format!("could not read dimensions from '{s}'")),
    }
}

async fn set_shot(shot_w: i64, shot_h: i64) -> Result<(), String> {
    let geom = run("xdotool", &["getdisplaygeometry"])?;
    let (screen_w, screen_h) = dims(&geom)?;

    *SHOT.lock().await = Some(Shot {
        w: shot_w,
        h: shot_h,
        screen_w,
        screen_h,
    });

    Ok(())
}

pub async fn observe() -> Result<String, String> {
    check_tools()?;

    let dir = data_dir().join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    let ts = stamp();
    let full = dir.join(format!("shot-full-{ts}.png"));
    let shot = dir.join(format!("shot-{ts}.png"));

    run("import", &["-window", "root", full.to_str().unwrap()])?;
    run(
        "convert",
        &[
            full.to_str().unwrap(),
            "-resize",
            "1280x>",
            shot.to_str().unwrap(),
        ],
    )?;
    let _ = std::fs::remove_file(&full);

    let ident = run("identify", &["-format", "%w %h", shot.to_str().unwrap()])?;
    let (w, h) = dims(&ident)?;
    set_shot(w, h).await?;

    let wins = run("wmctrl", &["l"]).unwrap_or_default();
    let wins: String = wins.lines().take(40).collect::<Vec<_>>().join("\n");

    Ok(format!(
        "screenshot: {p}\nimage {w}x{h} of screen — give x,y in image coordinates\n\nWindows:\n{wins}",
        p = shot.display()
    ))
}

/// Screenshot-space coords validated against the last capture, mapped to the
/// real screen.
async fn coords(args: &Value) -> Result<(String, String), String> {
    let g = SHOT.lock().await;
    let s = g
        .as_ref()
        .ok_or("no screenshot yet — run computer.observe first")?;

    let x = args.get("x").and_then(|v| v.as_f64()).ok_or("missing x")?;
    let y = args.get("y").and_then(|v| v.as_f64()).ok_or("missing y")?;

    if x < 0.0 || y < 0.0 || x >= s.w as f64 || y >= s.h as f64 {
        return Err(format!(
            "x,y outside the screenshot ({}x{}) — run computer.observe for a fresh one",
            s.w, s.h
        ));
    }

    let rx = (x * s.screen_w as f64 / s.w as f64).round() as i64;
    let ry = (y * s.screen_h as f64 / s.h as f64).round() as i64;

    Ok((rx.to_string(), ry.to_string()))
}

pub async fn click(args: &Value) -> Result<String, String> {
    check_tools()?;
    let (x, y) = coords(args).await?;

    let button = args
        .get("button")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, 3)
        .to_string();

    let mut a = vec!["mousemove", &x, &y, "click"];

    if args
        .get("double")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        a.extend(["--repeat", "2"]);
    }

    a.push(&button);
    run("xdotool", &a)?;

    observe().await
}

pub async fn type_text(args: &Value) -> Result<String, String> {
    check_tools()?;

    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("missing text")?;

    run("xdotool", &["type", "--delay", "20", "--", text])?;
    observe().await
}

pub async fn key(args: &Value) -> Result<String, String> {
    check_tools()?;

    let k = args
        .get("key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("missing key")?;

    if !k
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '_'))
    {
        return Err("key must be a combo like Return, ctrl+c, alt+Tab".into());
    }

    run("xdotool", &["key", "--", k])?;
    observe().await
}

pub async fn scroll(args: &Value) -> Result<String, String> {
    check_tools()?;

    let up = args
        .get("dir")
        .and_then(|v| v.as_str())
        .map(|d| d == "up")
        .unwrap_or(false);
    let amount = args
        .get("amount")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(1, 10)
        .to_string();
    let btn = if up { "4" } else { "5" };

    let mut a: Vec<String> = vec![];

    if args.get("x").is_some() && args.get("y").is_some() {
        let (x, y) = coords(args).await?;
        a.extend(["mousemove".into(), x, y]);
    }

    a.extend(["click".into(), "--repeat".into(), amount, btn.into()]);
    let refs: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
    run("xdotool", &refs)?;

    observe().await
}

pub async fn window(args: &Value) -> Result<String, String> {
    check_tools()?;

    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("activate");
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("missing id")?;

    if !id.chars().all(|c| c.is_ascii_hexdigit() || c == 'x') {
        return Err("id must be a hex window id from the observe list".into());
    }

    match action {
        "activate" => {
            run("wmctrl", &["-i", "-a", id])?;
        }
        "close" => {
            run("wmctrl", &["-i", "-c", id])?;
        }
        _ => return Err("action must be activate or close".into()),
    }

    observe().await
}
