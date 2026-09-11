use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use super::{clip_ends, ToolMeta};

const IDLE: Duration = Duration::from_secs(600);

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "browser.open",
        desc: "open a url in the Argus browser (headed Chrome window) and read the page",
        args: "{\"url\":\"https://...\",\"profile\":\"main\"}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.click",
        desc: "click an element from the last browser snapshot by its ref number",
        args: "{\"ref\":3,\"profile\":\"main\"}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.type",
        desc: "type text into an element from the last browser snapshot; set submit true to press Enter",
        args: "{\"ref\":7,\"text\":\"...\",\"submit\":false,\"profile\":\"main\"}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.read",
        desc: "read the current browser page text and elements",
        args: "{\"profile\":\"main\"}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.close",
        desc: "close the Argus browser for a profile",
        args: "{\"profile\":\"main\"}",
        mutating: false,
    },
];

struct Sess {
    _browser: Browser,
    page: Page,
    refs: Vec<String>,
    labels: Vec<String>,
    url: String,
    last_used: Instant,
}

struct Pool {
    root: PathBuf,
    sess: AsyncMutex<HashMap<String, Sess>>,
}

static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

fn pool() -> &'static Pool {
    static P: OnceLock<Pool> = OnceLock::new();

    P.get_or_init(|| {
        let root = ROOT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(default_root);

        Pool {
            root,
            sess: AsyncMutex::new(HashMap::new()),
        }
    })
}

fn default_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".local/share/Argus/browser-profiles")
}

pub fn init(dir: PathBuf) {
    if let Ok(mut g) = ROOT.lock() {
        *g = Some(dir);
    }
}

pub fn profile_dir(name: &str) -> PathBuf {
    pool().root.join(sanitize(name))
}

pub fn sanitize(name: &str) -> String {
    let t: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();

    if t.is_empty() {
        "main".into()
    } else {
        t
    }
}

fn profile_of(args: &Value) -> String {
    args.get("profile")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("main")
        .to_string()
}

async fn launch(root: &PathBuf, name: &str) -> Result<Sess, String> {
    let dir = root.join(sanitize(name));
    std::fs::create_dir_all(&dir).map_err(|e| format!("profile dir failed: {e}"))?;

    let mut cfg = BrowserConfig::builder()
        .with_head()
        .user_data_dir(dir)
        .window_size(1280, 900)
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-blink-features=AutomationControlled");

    if let Ok(bin) = std::env::var("ARGUS_CHROME") {
        if !bin.trim().is_empty() {
            cfg = cfg.chrome_executable(bin.trim());
        }
    }

    let cfg = cfg.build().map_err(|e| format!("browser config: {e}"))?;
    let (browser, handler) = Browser::launch(cfg)
        .await
        .map_err(|e| format!("chrome launch failed: {e}"))?;

    tokio::spawn(async move {
        let mut h = handler;
        while let Some(ev) = h.next().await {
            let _ = ev;
        }
    });

    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| format!("tab failed: {e}"))?;

    Ok(Sess {
        _browser: browser,
        page,
        refs: vec![],
        labels: vec![],
        url: String::new(),
        last_used: Instant::now(),
    })
}

type SessMap = HashMap<String, Sess>;

async fn sess(name: &str) -> Result<tokio::sync::MutexGuard<'static, SessMap>, String> {
    let mut map = pool().sess.lock().await;

    if let Some(s) = map.get(name) {
        if s.last_used.elapsed() < IDLE {
            return Ok(map);
        }
    }

    map.remove(name);
    let s = launch(&pool().root, name).await?;
    map.insert(name.into(), s);

    Ok(map)
}

const SNAP_JS: &str = r#"
(() => {
  const sel = 'a, button, input, textarea, select, [role="button"], [onclick]';
  const els = [...document.querySelectorAll(sel)];
  const out = [];

  for (const el of els) {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    if (el.disabled) continue;

    const label = (el.innerText || el.value || el.placeholder ||
      el.getAttribute('aria-label') || el.getAttribute('title') || '')
      .trim().replace(/\s+/g, ' ').slice(0, 60);

    let path = '';
    for (let n = el; n && n !== document.body; n = n.parentElement) {
      if (n.id) { path = `#${CSS.escape(n.id)}${path ? ' > ' + path : ''}`; break; }
      const p = n.parentElement;
      if (!p) break;
      let s = n.tagName.toLowerCase();
      const sib = [...p.children].filter(c => c.tagName === n.tagName);
      if (sib.length > 1) s += `:nth-of-type(${sib.indexOf(n) + 1})`;
      path = path ? `${s} > ${path}` : s;
    }

    out.push({
      kind: el.tagName.toLowerCase() === 'input' && el.type ? `input ${el.type}` : el.tagName.toLowerCase(),
      label,
      path,
    });
  }

  return JSON.stringify(out.slice(0, 60));
})()
"#;

const TEXT_JS: &str = "(() => document.body ? document.body.innerText : '')()";
const LOC_JS: &str = "(() => location.href)()";

async fn eval_str(page: &Page, js: &str) -> String {
    page.evaluate(js)
        .await
        .ok()
        .and_then(|r| r.into_value::<String>().ok())
        .unwrap_or_default()
}

#[derive(serde::Deserialize)]
struct ElRef {
    kind: String,
    label: String,
    path: String,
}

async fn snapshot(page: &Page, refs: &mut Vec<String>, labels: &mut Vec<String>) -> String {
    refs.clear();
    labels.clear();

    let raw = eval_str(page, SNAP_JS).await;
    let els: Vec<ElRef> = serde_json::from_str(&raw).unwrap_or_default();

    let mut list = String::from("\nElements:\n");
    for (i, el) in els.iter().enumerate() {
        refs.push(el.path.clone());
        labels.push(el.label.clone());

        if el.label.is_empty() {
            list.push_str(&format!("[{}] {}\n", i, el.kind));
        } else {
            list.push_str(&format!("[{}] {} \"{}\"\n", i, el.kind, el.label));
        }
    }

    list
}

fn url_of(args: &Value) -> Result<String, String> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or("missing url")?;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("url must start with http:// or https://".into());
    }

    Ok(url.into())
}

async fn page_out(s: &mut Sess) -> Result<String, String> {
    let url = eval_str(&s.page, LOC_JS).await;
    let title = s
        .page
        .get_title()
        .await
        .unwrap_or_default()
        .unwrap_or_default();
    let text = eval_str(&s.page, TEXT_JS).await;
    let list = snapshot(&s.page, &mut s.refs, &mut s.labels).await;

    s.url = url.clone();

    Ok(format!(
        "url {url}\ntitle {title}\n\n---\n{}\n---{list}",
        clip_ends(text)
    ))
}

pub async fn open(args: &Value) -> Result<String, String> {
    let url = url_of(args)?;
    let name = profile_of(args);

    if super::browser_ext::is_real(&name) {
        return super::browser_ext::open(args).await;
    }

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    s.page
        .goto(url.as_str())
        .await
        .map_err(|e| format!("navigation failed: {e}"))?;

    let _ = s.page.wait_for_navigation().await;
    page_out(s).await
}

fn ref_of(args: &Value) -> Result<usize, String> {
    args.get("ref")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| "missing ref".into())
}

async fn target(s: &Sess, r: usize) -> Result<String, String> {
    match s.refs.get(r) {
        Some(p) => Ok(p.clone()),
        None => Err("unknown ref — run browser.open or browser.read for a fresh list".into()),
    }
}

pub async fn click(args: &Value) -> Result<String, String> {
    let name = profile_of(args);

    if super::browser_ext::is_real(&name) {
        return super::browser_ext::click(args).await;
    }

    let r = ref_of(args)?;

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    let path = target(s, r).await?;

    let el = s
        .page
        .find_element(&path)
        .await
        .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;

    el.click().await.map_err(|e| format!("click failed: {e}"))?;
    let _ = s.page.wait_for_navigation().await;

    page_out(s).await
}

pub async fn type_text(args: &Value) -> Result<String, String> {
    let name = profile_of(args);

    if super::browser_ext::is_real(&name) {
        return super::browser_ext::type_text(args).await;
    }

    let r = ref_of(args)?;
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("missing text")?;
    let submit = args
        .get("submit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    let path = target(s, r).await?;

    let el = s
        .page
        .find_element(&path)
        .await
        .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;

    el.click().await.map_err(|e| format!("focus failed: {e}"))?;
    el.type_str(text)
        .await
        .map_err(|e| format!("typing failed: {e}"))?;

    if submit {
        let _ = el.press_key("Enter").await;
        let _ = s.page.wait_for_navigation().await;
    }

    page_out(s).await
}

pub async fn read(args: &Value) -> Result<String, String> {
    let name = profile_of(args);

    if super::browser_ext::is_real(&name) {
        return super::browser_ext::read(args).await;
    }

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    page_out(s).await
}

pub async fn close(args: &Value) -> Result<String, String> {
    let name = profile_of(args);

    if super::browser_ext::is_real(&name) {
        return super::browser_ext::close(args).await;
    }

    let mut map = pool().sess.lock().await;

    match map.remove(&name) {
        Some(mut s) => {
            let _ = s.page.close().await;
            let _ = s._browser.close().await;
            Ok("browser closed".into())
        }
        None => Ok("no browser running for this profile".into()),
    }
}

pub fn close_profile(name: &str) {
    let name = sanitize(name);

    if let Ok(mut g) = pool().sess.try_lock() {
        if let Some(mut s) = g.remove(&name) {
            tokio::spawn(async move {
                let _ = s.page.close().await;
                let _ = s._browser.close().await;
            });
        }
    }
}

const URL_PAT: &str =
    "login|signin|sign-in|sign_up|signup|/auth|checkout|cart|/pay|billing|order|password";
const LABEL_PAT: &str = "sign in|sign-in|signin|log in|log-in|login|checkout|pay now|payment|place order|buy now|add to cart|password";

pub fn sensitive_pats() -> Option<(regex::Regex, regex::Regex)> {
    Some((
        regex::Regex::new(&format!("(?i)({URL_PAT})")).ok()?,
        regex::Regex::new(&format!("(?i)({LABEL_PAT})")).ok()?,
    ))
}

pub async fn sensitive(tool: &str, args: &Value) -> bool {
    if !tool.starts_with("browser.") {
        return false;
    }

    let (url_re, label_re) = match sensitive_pats() {
        Some(p) => p,
        None => return false,
    };

    if super::browser_ext::is_real(&profile_of(args))
        && super::browser_ext::sensitive(args).await
    {
        return true;
    }

    if let Some(u) = args.get("url").and_then(|v| v.as_str()) {
        if url_re.is_match(u) {
            return true;
        }
    }

    if let Some(t) = args.get("text").and_then(|v| v.as_str()) {
        if label_re.is_match(t) {
            return true;
        }
    }

    if let Ok(r) = ref_of(args) {
        let name = profile_of(args);

        if let Ok(g) = pool().sess.try_lock() {
            if let Some(s) = g.get(&name) {
                if url_re.is_match(&s.url) {
                    return true;
                }

                if let Some(l) = s.labels.get(r) {
                    if label_re.is_match(l) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_profile_names() {
        assert_eq!(sanitize("My Work!"), "mywork");
        assert_eq!(sanitize("../etc"), "etc");
        assert_eq!(sanitize(""), "main");
        assert_eq!(sanitize("main"), "main");
    }

    #[tokio::test]
    async fn flags_sensitive_urls_and_labels() {
        assert!(sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://github.com/login"})
        )
        .await);
        assert!(!sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://en.wikipedia.org/wiki/Rust"})
        )
        .await);
        assert!(!sensitive(
            "terminal",
            &serde_json::json!({"command": "ls"})
        )
        .await);
    }
}
