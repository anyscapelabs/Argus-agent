use std::time::Instant;

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::sessions::ext_install;
use crate::tools::page_text;

use super::extpipe;
use super::sensitive_pats;

struct Sess {
    tab_id: i32,
    elements: super::RefTable,
    shown: Option<u64>,
    url: String,
    title: String,
    text_hash: u64,
    last_used: Instant,
}

async fn current_state() -> Option<super::PageState> {
    let g = SESS.lock().await;
    g.as_ref()
        .map(|s| super::PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items))
}

async fn apply_verify(before: Option<super::PageState>, out: String) -> String {
    let Some(before) = before else {
        return out;
    };
    let g = SESS.lock().await;
    let Some(s) = g.as_ref() else {
        return out;
    };
    super::verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    )
}

static SESS: AsyncMutex<Option<Sess>> = AsyncMutex::const_new(None);

pub(crate) async fn note_shown(gen: u64) {
    if let Some(s) = SESS.lock().await.as_mut() {
        s.shown = Some(gen);
    }
}

pub fn is_real(name: &str) -> bool {
    matches!(super::sanitize(name).as_str(), "real" | "chrome")
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

    super::url_guard(url)?;
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
        elements: super::RefTable::default(),
        shown: None,
        url: page_url.clone(),
        title: String::new(),
        text_hash: 0,
        last_used: Instant::now(),
    };

    let out = page_out(&mut s, text, page_url.clone(), title).await;
    let out = super::landed_note(url, &page_url, out);

    *SESS.lock().await = Some(s);

    Ok(super::sensitive_note(&page_url, out))
}

fn ref_of(args: &Value) -> Result<usize, String> {
    args.get("ref")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| "missing ref".into())
}

async fn path_of(args: &Value) -> Result<(i32, String), String> {
    let r = ref_of(args)?;
    let snap = super::snap_of(args);
    let mut g = SESS.lock().await;
    let s = g.as_mut().ok_or("no page open — run browser.open first")?;
    s.last_used = Instant::now();

    if snap.is_none() {
        if let Some(err) = s.elements.stale_for_tokenless(r, s.shown) {
            return Err(err);
        }
    }
    match s.elements.resolve(r, snap) {
        Ok(p) => Ok((s.tab_id, p)),
        Err(e) => {
            let superseded =
                snap.is_some_and(|g| g != s.elements.gen) && s.elements.items.get(r).is_some();
            if superseded {
                return Err(s.elements.stale_recovery(r, snap.unwrap_or(0)));
            }
            Err(e)
        }
    }
}

#[derive(serde::Deserialize)]
struct ElRef {
    kind: String,
    label: String,
    path: String,
}

async fn snapshot_list(tab_id: i32, s: &mut Sess) -> String {
    let failed: bool;
    let items: Vec<super::RefEntry> =
        match extpipe::request("snapshot", serde_json::json!({"tabId": tab_id})).await {
            Ok(data) => {
                failed = false;
                let els: Vec<ElRef> = serde_json::from_value(data).unwrap_or_default();
                els.into_iter()
                    .map(|el| super::RefEntry {
                        path: el.path,
                        label: super::redact(&el.label),
                        kind: el.kind,
                    })
                    .collect()
            }
            Err(_) => {
                failed = true;
                vec![]
            }
        };

    let gen = s.elements.refresh(items);

    super::render_elements(&s.elements.items, gen, failed)
}

async fn page_out(s: &mut Sess, text: String, url: String, title: String) -> String {
    let list = snapshot_list(s.tab_id, s).await;
    s.url = url.clone();
    s.title = title.clone();
    s.text_hash = super::text_hash(&super::redact(&text));
    s.last_used = Instant::now();

    format!(
        "url {url}\ntitle {title}\n\n---\n{}\n---{list}",
        page_text(&super::redact(&text))
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
    let before = current_state().await;

    extpipe::request("click", serde_json::json!({"tabId": tab_id, "path": path})).await?;

    let out = read_page(tab_id).await?;
    Ok(apply_verify(before, out).await)
}

pub async fn type_text(args: &Value) -> Result<String, String> {
    ext_install::ensure_real().await?;

    let (tab_id, path) = path_of(args).await?;
    let before = current_state().await;

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

    let out = read_page(tab_id).await?;
    Ok(apply_verify(before, out).await)
}

pub async fn scroll(args: &Value) -> Result<String, String> {
    ext_install::ensure_real().await?;

    let tab_id = {
        let g = SESS.lock().await;
        g.as_ref()
            .map(|s| s.tab_id)
            .ok_or("no page open — run browser.open first")?
    };

    let dir = args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("down");
    let px = args
        .get("pixels")
        .and_then(|v| v.as_i64())
        .unwrap_or(800)
        .clamp(100, 5000);
    let dy = if dir == "up" { -px } else { px };

    extpipe::request("scroll", serde_json::json!({"tabId": tab_id, "dy": dy})).await?;

    read_page(tab_id).await
}

pub async fn close(_args: &Value) -> Result<String, String> {
    let tab_id = SESS.lock().await.take().map(|s| s.tab_id).unwrap_or(0);

    if tab_id > 0 {
        let _ = extpipe::request("closeTab", serde_json::json!({"tabId": tab_id})).await;
    }

    Ok("browser tab closed".into())
}

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
        if let Some(l) = s.elements.label(r) {
            if label_re.is_match(&l) {
                return true;
            }
        }
    }

    false
}
