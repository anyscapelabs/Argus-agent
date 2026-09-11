use std::time::Instant;

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::sessions::ext_install;

use super::browser::sensitive_pats;
use super::extpipe;
use super::{browser, page_text};

struct Sess {
    tab_id: i32,
    refs: Vec<String>,
    labels: Vec<String>,
    url: String,
    last_used: Instant,
}

static SESS: AsyncMutex<Option<Sess>> = AsyncMutex::const_new(None);

pub fn is_real(name: &str) -> bool {
    matches!(browser::sanitize(name).as_str(), "real" | "chrome")
}

pub async fn open(args: &Value) -> Result<String, String> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or("missing url")?;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("url must start with http:// or https://".into());
    }

    browser::url_guard(url)?;
    ext_install::ensure_real().await?;

    let prev_tab = SESS.lock().await.as_ref().map(|s| s.tab_id).unwrap_or(0);

    let data = extpipe::request(
        "navigate",
        serde_json::json!({"url": url, "tabId": prev_tab}),
    )
    .await?;

    let tab_id = data.get("tabId").and_then(|t| t.as_i64()).unwrap_or(0) as i32;
    let page_url: String = data
        .get("url")
        .and_then(|u| u.as_str())
        .unwrap_or(url)
        .into();
    let title: String = data
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .into();
    let text: String = data
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .into();

    let mut s = Sess {
        tab_id,
        refs: vec![],
        labels: vec![],
        url: page_url.clone(),
        last_used: Instant::now(),
    };

    let out = page_out(&mut s, text, page_url.clone(), title).await;

    Ok(browser::sensitive_note(&page_url, out))
}

fn ref_of(args: &Value) -> Result<usize, String> {
    args.get("ref")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| "missing ref".into())
}

async fn path_of(args: &Value) -> Result<(i32, String), String> {
    let r = ref_of(args)?;
    let mut g = SESS.lock().await;
    let s = g.as_mut().ok_or("no page open — run browser.open first")?;
    s.last_used = Instant::now();

    match s.refs.get(r) {
        Some(p) => Ok((s.tab_id, p.clone())),
        None => Err("unknown ref — run browser.open or browser.read for a fresh list".into()),
    }
}

#[derive(serde::Deserialize)]
struct ElRef {
    kind: String,
    label: String,
    path: String,
}

async fn snapshot_list(tab_id: i32, s: &mut Sess) -> String {
    let refs: Vec<String>;
    let labels: Vec<String>;
    let list: String;

    match extpipe::request("snapshot", serde_json::json!({"tabId": tab_id})).await {
        Ok(data) => {
            let els: Vec<ElRef> = serde_json::from_value(data).unwrap_or_default();
            let mut r = vec![];
            let mut l = vec![];
            let mut out = String::from("\nElements:\n");

            for (i, el) in els.iter().enumerate() {
                let label = browser::redact(&el.label);
                r.push(el.path.clone());
                l.push(label.clone());

                if label.is_empty() {
                    out.push_str(&format!("[{}] {}\n", i, el.kind));
                } else {
                    out.push_str(&format!("[{}] {} \"{}\"\n", i, el.kind, label));
                }
            }

            refs = r;
            labels = l;
            list = out;
        }
        Err(_) => {
            refs = vec![];
            labels = vec![];
            list = String::from("\nElements: (snapshot failed)\n");
        }
    }

    s.refs = refs;
    s.labels = labels;

    list
}

async fn page_out(s: &mut Sess, text: String, url: String, title: String) -> String {
    let list = snapshot_list(s.tab_id, s).await;
    s.url = url.clone();
    s.last_used = Instant::now();

    format!(
        "url {url}\ntitle {title}\n\n---\n{}\n---{list}",
        page_text(&browser::redact(&text))
    )
}

async fn read_page(tab_id: i32) -> Result<String, String> {
    let data = extpipe::request("read", serde_json::json!({"tabId": tab_id})).await?;
    let url: String = data
        .get("url")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .into();
    let title: String = data
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .into();
    let text: String = data
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .into();

    let mut g = SESS.lock().await;

    match g.as_mut() {
        Some(s) => Ok(page_out(s, text, url, title).await),
        None => Err("no page open — run browser.open first".into()),
    }
}

pub async fn read(_args: &Value) -> Result<String, String> {
    ext_install::ensure_real().await?;

    let tab_id = SESS
        .lock()
        .await
        .as_ref()
        .map(|s| s.tab_id)
        .ok_or("no page open — run browser.open first")?;

    read_page(tab_id).await
}

pub async fn click(args: &Value) -> Result<String, String> {
    ext_install::ensure_real().await?;

    let (tab_id, path) = path_of(args).await?;

    extpipe::request("click", serde_json::json!({"tabId": tab_id, "path": path})).await?;

    read_page(tab_id).await
}

pub async fn type_text(args: &Value) -> Result<String, String> {
    ext_install::ensure_real().await?;

    let (tab_id, path) = path_of(args).await?;

    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("missing text")?;
    let submit = args
        .get("submit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    extpipe::request(
        "fill",
        serde_json::json!({"tabId": tab_id, "path": path, "text": text, "submit": submit}),
    )
    .await?;

    read_page(tab_id).await
}

pub async fn close(_args: &Value) -> Result<String, String> {
    let tab_id = SESS.lock().await.take().map(|s| s.tab_id).unwrap_or(0);

    if tab_id > 0 {
        let _ = extpipe::request("closeTab", serde_json::json!({"tabId": tab_id})).await;
    }

    Ok("browser tab closed".into())
}

/// Sensitive check against the live extension session — same policy as the
/// CDP pool: current URL or target label matching login/checkout patterns.
pub async fn sensitive(args: &Value) -> bool {
    let named = args
        .get("profile")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let name = match named {
        Some(p) => p.to_string(),
        None => match ext_install::real_enabled() {
            true => "real".into(),
            false => "main".into(),
        },
    };

    if !is_real(&name) {
        return false;
    }

    let Some((url_re, label_re)) = sensitive_pats() else {
        return false;
    };

    let g = SESS.lock().await;
    let Some(s) = g.as_ref() else { return false };

    if url_re.is_match(&s.url) {
        return true;
    }

    if let Ok(r) = ref_of(args) {
        if let Some(l) = s.labels.get(r) {
            if label_re.is_match(l) {
                return true;
            }
        }
    }

    false
}
