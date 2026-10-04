mod ext;
pub mod extpipe;
pub mod guard;
pub use guard::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::time::{Duration, Instant};

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use super::{page_text, ToolMeta};

const IDLE: Duration = Duration::from_secs(600);

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "browser.open",
        desc: "open a url in the user's current Chrome and read the page; omit profile unless an isolated window was asked for",
        args: "{\"url\":\"https://...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.click",
        desc: "click an element from the last browser snapshot by its ref number, including autocomplete options",
        args: "{\"ref\":3}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.type",
        desc: "type text into an element from the last browser snapshot; set submit true to press Enter",
        args: "{\"ref\":7,\"text\":\"...\",\"submit\":false}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.read",
        desc: "read the current browser page text and elements",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.scroll",
        desc: "scroll the current browser page up or down by pixels",
        args: "{\"direction\":\"down\",\"pixels\":800}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.close",
        desc: "close the browser tab",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.press",
        desc: "press a key on an element or the focused field: Escape, Enter, Tab, arrows, PageDown, PageUp, Home, End, Backspace, Delete. Escape dismisses overlays and dropdowns",
        args: "{\"ref\":3,\"key\":\"Escape\"}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.wait",
        desc: "wait until the page text contains a string, for content that loads late; fails on timeout instead of guessing",
        args: "{\"text\":\"Order confirmed\",\"timeout\":10}",
        mutating: false,
    },
    ToolMeta {
        name: "browser.upload",
        desc: "attach a local file to a file input; isolated profiles only, never real Chrome",
        args: "{\"ref\":4,\"path\":\"/home/user/report.pdf\"}",
        mutating: true,
    },
    ToolMeta {
        name: "browser.drag",
        desc: "drag an element onto another one: sortable lists, drop zones, sliders",
        args: "{\"from\":2,\"to\":5}",
        mutating: true,
    },
];

static NEXT_SNAP: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_snap() -> u64 {
    NEXT_SNAP.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, Default)]
pub struct RefEntry {
    pub path: String,
    pub label: String,
    pub kind: String,
}

#[derive(Debug, Default)]
pub struct RefTable {
    pub gen: u64,
    pub items: Vec<RefEntry>,
    pub recovery_gen: Option<u64>,
}

impl RefTable {
    pub(crate) fn refresh(&mut self, items: Vec<RefEntry>) -> u64 {
        self.gen = next_snap();
        self.items = items;
        self.gen
    }

    pub fn stale_for_tokenless(&mut self, index: usize, shown: Option<u64>) -> Option<String> {
        match shown {
            Some(g) if g != self.gen && self.items.get(index).is_some() => {
                Some(self.stale_recovery(index, g))
            }
            _ => None,
        }
    }

    pub fn resolve(&self, index: usize, presented: Option<u64>) -> Result<String, String> {
        let entry = self.items.get(index).ok_or_else(|| {
            "unknown ref — run browser.open or browser.read for a fresh list".to_string()
        })?;
        if let Some(g) = presented {
            if g != self.gen {
                return Err(format!(
                    "stale ref {index} from snapshot {g} — snapshot {} is current; run browser.read for a fresh list",
                    self.gen
                ));
            }
        }
        Ok(entry.path.clone())
    }

    pub(crate) fn label(&self, index: usize) -> Option<String> {
        self.items.get(index).map(|e| e.label.clone())
    }

    pub(crate) fn render(&self) -> String {
        render_elements(&self.items, self.gen, false)
    }

    pub(crate) fn stale_recovery(&mut self, index: usize, presented: u64) -> String {
        if self.recovery_gen == Some(self.gen) {
            self.recovery_gen = None;
            return format!(
                "stale ref {index} from snapshot {presented} — snapshot {} is current; run browser.read for a fresh list",
                self.gen
            );
        }
        self.recovery_gen = Some(self.gen);
        format!(
            "stale ref {index} from snapshot {presented} — snapshot {} is current:\n{}Choose the replacement ref from THIS snapshot and act once; if that fails, run browser.read for a fresh list instead of guessing.",
            self.gen,
            self.render()
        )
    }
}

pub(crate) fn render_elements(items: &[RefEntry], gen: u64, failed: bool) -> String {
    let mut out = format!("\nElements (snapshot {gen}):");
    if failed {
        out.push_str(" (snapshot failed)");
    }
    out.push('\n');
    for (i, el) in items.iter().enumerate() {
        if el.label.is_empty() {
            out.push_str(&format!("[{i}] {}\n", el.kind));
        } else {
            out.push_str(&format!("[{i}] {} \"{}\"\n", el.kind, el.label));
        }
    }
    out
}

pub(crate) fn snap_of(args: &Value) -> Option<u64> {
    args.get("snapshot").and_then(|v| v.as_u64())
}

struct Sess {
    _browser: Browser,
    page: Page,
    elements: RefTable,
    shown: Option<u64>,
    url: String,
    title: String,
    text_hash: u64,
    last_used: Instant,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PageState {
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) text_hash: u64,
    pub(crate) rows: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VerifyOutcome {
    Navigated,
    Changed,
    Same,
}

impl PageState {
    pub(crate) fn capture(url: &str, title: &str, text_hash: u64, items: &[RefEntry]) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            text_hash,
            rows: items
                .iter()
                .map(|e| (e.kind.clone(), e.label.clone()))
                .collect(),
        }
    }

    pub(crate) fn check_against(&self, current: &PageState) -> VerifyOutcome {
        if current.url.trim().is_empty() {
            return VerifyOutcome::Same;
        }
        if !same_url(&self.url, &current.url) {
            return VerifyOutcome::Navigated;
        }
        if self.title != current.title
            || self.text_hash != current.text_hash
            || self.rows != current.rows
        {
            return VerifyOutcome::Changed;
        }
        VerifyOutcome::Same
    }
}

pub(crate) fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

pub(crate) fn same_url(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

pub(crate) fn verify_note(outcome: &VerifyOutcome, url: &str) -> Option<String> {
    match outcome {
        VerifyOutcome::Navigated => Some(format!("note: verified: navigated to {url}")),
        VerifyOutcome::Changed => Some("note: verified: page content changed".into()),
        VerifyOutcome::Same => None,
    }
}

pub(super) struct Pool {
    root: PathBuf,
    sess: AsyncMutex<HashMap<String, Sess>>,
}

static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

pub(super) fn pool() -> &'static Pool {
    static P: OnceLock<Pool> = OnceLock::new();

    P.get_or_init(|| {
        let root = ROOT
            .lock()
            .unwrap_or_else(|err| err.into_inner())
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

pub(crate) fn session_key_for(args: &Value) -> String {
    if let Some(p) = args
        .get("profile")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return p.to_string();
    }

    match crate::sessions::ext_install::real_enabled() {
        true => "real".into(),
        false => "main".into(),
    }
}

pub(super) async fn route_profile(args: &Value) -> String {
    session_key_for(args)
}

pub fn shown_gen_in(output: &str) -> Option<u64> {
    output
        .split("(snapshot ")
        .nth(1)?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

pub(crate) async fn note_shown(args: &Value, gen: u64) {
    let key = session_key_for(args);
    if self::ext::is_real(&key) {
        self::ext::note_shown(gen).await;
        return;
    }
    if let Ok(mut map) = pool().sess.try_lock() {
        if let Some(s) = map.get_mut(&key) {
            s.shown = Some(gen);
        }
    }
}

async fn launch(root: &Path, name: &str) -> Result<Sess, String> {
    let dir = root.join(sanitize(name));
    std::fs::create_dir_all(&dir).map_err(|err| format!("profile dir failed: {err}"))?;

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

    let cfg = cfg
        .build()
        .map_err(|err| format!("browser config: {err}"))?;
    let (browser, handler) = Browser::launch(cfg)
        .await
        .map_err(|err| format!("chrome launch failed: {err}"))?;

    tokio::spawn(async move {
        let mut h = handler;
        while let Some(ev) = h.next().await {
            let _ = ev;
        }
    });

    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|err| format!("tab failed: {err}"))?;

    Ok(Sess {
        _browser: browser,
        page,
        elements: RefTable::default(),
        shown: None,
        url: String::new(),
        title: String::new(),
        text_hash: 0,
        last_used: Instant::now(),
    })
}

type SessMap = HashMap<String, Sess>;

async fn sess(name: &str) -> Result<tokio::sync::MutexGuard<'static, SessMap>, String> {
    let fresh = {
        let map = pool().sess.lock().await;
        map.get(name).is_some_and(|s| s.last_used.elapsed() < IDLE)
    };

    if !fresh {
        // Launch outside the lock: Chrome takes seconds, and the guard across it
        // queues every other profile behind one launch.
        let s = launch(&pool().root, name).await?;
        let mut map = pool().sess.lock().await;
        // Another task may have launched; keep the live one and drop the spare.
        if map.get(name).is_none_or(|e| e.last_used.elapsed() >= IDLE) {
            map.remove(name);
            map.insert(name.into(), s);
        }
    }

    Ok(pool().sess.lock().await)
}

const SNAP_JS: &str = r#"
(() => {
  const sel = 'a, button, input, textarea, select, summary, [role="button"], [role="tab"], [role="search"], [role="combobox"], [role="switch"], [role="option"], [draggable="true"], [onclick], [aria-expanded], [contenteditable="true"]';
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

  return JSON.stringify(out.slice(0, 100));
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

async fn snapshot(page: &Page, table: &mut RefTable) -> String {
    let raw = eval_str(page, SNAP_JS).await;
    let els: Vec<ElRef> = serde_json::from_str(&raw).unwrap_or_default();

    let items: Vec<RefEntry> = els
        .iter()
        .map(|el| RefEntry {
            path: el.path.clone(),
            label: redact(&el.label),
            kind: el.kind.clone(),
        })
        .collect();
    table.refresh(items);

    table.render()
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
    let out = observe(s).await?;
    if s.url.trim().is_empty() {
        return observe(s).await;
    }
    Ok(out)
}

async fn observe(s: &mut Sess) -> Result<String, String> {
    let url = eval_str(&s.page, LOC_JS).await;
    let title = s
        .page
        .get_title()
        .await
        .unwrap_or_default()
        .unwrap_or_default();
    let text = eval_str(&s.page, TEXT_JS).await;
    let text = redact(&text);
    let list = snapshot(&s.page, &mut s.elements).await;

    s.url = url.clone();
    s.title = title.clone();
    s.text_hash = text_hash(&text);

    Ok(format!(
        "url {url}\ntitle {title}\n\n---\n{}\n---{list}",
        page_text(&text)
    ))
}

pub async fn open(args: &Value) -> Result<String, String> {
    let url = url_of(args)?;
    url_guard(&url)?;
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::open(args).await;
    }

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    s.page
        .goto(url.as_str())
        .await
        .map_err(|err| format!("navigation failed: {err}"))?;

    let _ = s.page.wait_for_navigation().await;

    let out = page_out(s).await?;
    let out = landed_note(&url, &s.url, out);

    Ok(sensitive_note(&s.url, out))
}

pub(super) fn ref_of(args: &Value) -> Result<usize, String> {
    args.get("ref")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| "missing ref".into())
}

async fn target(s: &mut Sess, r: usize, snap: Option<u64>) -> Result<String, String> {
    if snap.is_none() {
        if let Some(err) = s.elements.stale_for_tokenless(r, s.shown) {
            return Err(err);
        }
    }
    match s.elements.resolve(r, snap) {
        Ok(p) => Ok(p),
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

pub(crate) fn landed_note(requested: &str, landed: &str, out: String) -> String {
    if same_url(requested, landed) {
        out
    } else {
        format!("{out}\nnote: landed on {landed} (requested {requested})")
    }
}

pub(crate) fn verify_outcome(
    before: &PageState,
    url: &str,
    title: &str,
    text_hash: u64,
    items: &[RefEntry],
    out: String,
) -> String {
    let after = PageState::capture(url, title, text_hash, items);
    match verify_note(&before.check_against(&after), url) {
        Some(note) => format!("{out}\n{note}"),
        None => out,
    }
}

pub async fn click(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::click(args).await;
    }

    let r = ref_of(args)?;
    let snap = snap_of(args);

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    let path = target(s, r, snap).await?;
    let before = PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items);

    let el = s
        .page
        .find_element(&path)
        .await
        .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;

    el.click()
        .await
        .map_err(|err| format!("click failed: {err}"))?;
    let _ = s.page.wait_for_navigation().await;

    let out = page_out(s).await?;
    Ok(verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    ))
}

pub async fn type_text(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::type_text(args).await;
    }

    let r = ref_of(args)?;
    let snap = snap_of(args);
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

    let path = target(s, r, snap).await?;
    let before = PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items);

    // Primary: trusted CDP key events. Masks, autocompletes and key handlers
    // only react to real keystrokes, so this stays first.
    let el = s
        .page
        .find_element(&path)
        .await
        .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;

    el.click()
        .await
        .map_err(|err| format!("focus failed: {err}"))?;
    el.type_str(text)
        .await
        .map_err(|err| format!("typing failed: {err}"))?;
    drop(el);

    // Verify BEFORE any submit: sending wipes the evidence a failed write
    // would leave behind, and a submit-time navigation would re-read a fresh
    // page and mistake it for landed text.
    if !landed(&eval_str(&s.page, &verify_js(&path)).await, text) {
        // Fallback: the keystrokes never reached the model — React-controlled
        // inputs swallow synthetic writes without the right InputEvent, and
        // rich-text editors only accept the native beforeinput pipeline. Drive
        // the same robust fill the extension bridge uses, then check again.
        // Fill only — the submit below stays the single send gesture.
        let res = eval_str(&s.page, &fill_js(&path, text)).await;
        let status: FillStatus = serde_json::from_str(&res).unwrap_or(FillStatus {
            status: "ok".into(),
            actual: String::new(),
        });

        if status.status == "missing" {
            return Err("stale ref — run browser.read for a fresh element list".into());
        }
        if status.status == "unsupported" {
            return Err("that element takes no text — click it or pick a field".into());
        }
        if status.status == "no-option" {
            return Err("no dropdown option matches that text".into());
        }

        let got = eval_str(&s.page, &verify_js(&path)).await;
        if !landed(&got, text) {
            let show = if got.len() > 120 {
                format!("{}…", &got[..120])
            } else {
                got
            };
            return Err(format!(
                "typing did not land (field shows {}) — the site may need one choice picked first, or the field is read-only",
                if show.is_empty() {
                    "empty".to_string()
                } else {
                    format!("\"{show}\"")
                }
            ));
        }
    }

    if submit {
        // Trusted CDP Enter first: chat composers and key-driven forms send
        // on keypress, while a premature native form submit can reload the
        // page and wipe the message.
        let el = s
            .page
            .find_element(&path)
            .await
            .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;
        let _ = el.press_key("Enter").await;
        drop(el);
        let _ = s.page.wait_for_navigation().await;

        // Post-submit proof: an app that sent the message clears the field.
        // The exact text still sitting there means the send never triggered —
        // say so with the recovery instead of reporting silent success.
        if !text.is_empty() {
            let after = eval_str(&s.page, &verify_js(&path)).await;
            if after.contains(text) && !form_submitted(s, &path).await {
                return Err("the text is in the field but sending didn't trigger — click the Send button element instead (find its ref with browser.read)".into());
            }
        }
    }

    let out = page_out(s).await?;
    Ok(verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    ))
}

/// Whether the typed text reached the field. `contains` covers the primary
/// append path; the `starts_with` arm covers maxlength truncation on replace.
fn landed(readback: &str, want: &str) -> bool {
    readback.contains(want) || (!readback.is_empty() && want.starts_with(readback))
}

/// Last-resort send for a field that swallowed Enter: fire the element's own
/// form submit and report whether one actually ran. A real form submission is
/// trusted to deliver (or navigate) — only a missing form counts as failure.
async fn form_submitted(s: &Sess, path: &str) -> bool {
    let res = eval_str(&s.page, &submit_form_js(path)).await;

    if res.contains("submitted") {
        let _ = s.page.wait_for_navigation().await;
        return true;
    }

    false
}

#[derive(serde::Deserialize)]
struct FillStatus {
    status: String,
    #[allow(dead_code)]
    actual: String,
}

fn verify_js(path: &str) -> String {
    let sel = serde_json::to_string(path).unwrap_or_default();
    format!(
        r#"((sel) => {{
  let el = null;
  try {{ el = document.querySelector(sel); }} catch {{ return ''; }}
  if (!el) return '';
  if (el.isContentEditable) return (el.innerText ?? el.textContent ?? '');
  if ('value' in el) return (el.value ?? '');
  return (el.innerText ?? el.textContent ?? '');
}})({sel})"#
    )
}

/// Mirrors `fillEl` in `extension/background.js`: native setter + InputEvent
/// with data/inputType for framework inputs, execCommand for
/// contenteditable editors. Keep the two in sync when either changes.
#[allow(clippy::too_many_lines)]
fn fill_js(path: &str, text: &str) -> String {
    let sel = serde_json::to_string(path).unwrap_or_default();
    let val = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"((sel, val) => {{
  const el = (() => {{
    try {{
      const direct = document.querySelector(sel);
      if (direct) return direct;
    }} catch {{ return null; }}
    const seen = new Set();
    const stack = [document];
    while (stack.length > 0) {{
      const root = stack.pop();
      if (!root || seen.has(root)) continue;
      seen.add(root);
      let hit = null;
      try {{ hit = root.querySelector(sel); }} catch {{ hit = null; }}
      if (hit) return hit;
      let els = [];
      try {{ els = [...root.querySelectorAll('*')]; }} catch {{ els = []; }}
      for (const n of els) {{
        if (n.shadowRoot) stack.push(n.shadowRoot);
        if (n.tagName === 'IFRAME') {{
          try {{ if (n.contentDocument) stack.push(n.contentDocument); }} catch {{}}
        }}
      }}
    }}
    return null;
  }})();
  if (!el) return JSON.stringify({{ status: 'missing', actual: '' }});
  try {{ el.scrollIntoView({{ block: 'center' }}); }} catch {{}}
  const tag = (el.tagName || '').toUpperCase();
  const type = (el.type || '').toLowerCase();
  const editable = el.isContentEditable || el.getAttribute('contenteditable') === 'true';
  if (tag === 'INPUT' && (type === 'checkbox' || type === 'radio')) {{
    el.click();
    return JSON.stringify({{ status: 'ok', actual: val }});
  }}
  if (tag === 'SELECT') {{
    const match = [...el.options].find((o) => o.value === val || (o.text || '').trim() === (val || '').trim());
    if (!match) return JSON.stringify({{ status: 'no-option', actual: '' }});
    el.focus();
    el.value = match.value;
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return JSON.stringify({{ status: 'ok', actual: val }});
  }}
  if (editable) {{
    el.focus();
    try {{
      const range = document.createRange();
      range.selectNodeContents(el);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }} catch {{}}
    let done = false;
    try {{ done = document.execCommand('insertText', false, val); }} catch {{ done = false; }}
    if (!done) {{
      try {{
        el.textContent = val;
        el.dispatchEvent(new InputEvent('input', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: val }}));
      }} catch {{ el.textContent = val; }}
    }}
    const actual = (el.innerText ?? el.textContent ?? '').trim();
    return JSON.stringify({{ status: 'ok', actual }});
  }}
  if (tag !== 'INPUT' && tag !== 'TEXTAREA' && !(el instanceof HTMLInputElement) && !(el instanceof HTMLTextAreaElement)) {{
    return JSON.stringify({{ status: 'unsupported', actual: '' }});
  }}
  el.focus();
  try {{ el.click(); }} catch {{}}
  try {{ if (typeof el.select === 'function') el.select(); }} catch {{}}
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  try {{
    const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
    if (setter) setter.call(el, val);
    else el.value = val;
  }} catch {{ el.value = val; }}
  try {{
    const tracker = el._valueTracker;
    if (tracker && typeof tracker.setValue === 'function') tracker.setValue('');
  }} catch {{}}
  try {{
    el.dispatchEvent(new InputEvent('input', {{ bubbles: true, cancelable: true, inputType: 'insertText', data: val }}));
  }} catch {{
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
  }}
  el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  return JSON.stringify({{ status: 'ok', actual: el.value ?? '' }});
}})({sel}, {val})"#
    )
}

/// Runs the element's own form submit. Only the fallback after an Enter that
/// changed nothing — Enter-first ordering is what chat composers need, and a
/// premature native submit can reload the page and wipe the message.
fn submit_form_js(path: &str) -> String {
    let sel = serde_json::to_string(path).unwrap_or_default();
    format!(
        r#"((sel) => {{
  let el = null;
  try {{ el = document.querySelector(sel); }} catch {{ return JSON.stringify({{ status: 'missing' }}); }}
  if (!el) return JSON.stringify({{ status: 'missing' }});
  const form = el.form;
  if (form && typeof form.requestSubmit === 'function') {{
    form.requestSubmit();
    return JSON.stringify({{ status: 'submitted' }});
  }}
  return JSON.stringify({{ status: 'no-form' }});
}})({sel})"#
    )
}

pub async fn read(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::read(args).await;
    }

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    page_out(s).await
}

pub async fn scroll(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::scroll(args).await;
    }

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

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    eval_str(&s.page, &format!("(() => window.scrollBy(0, {dy}))()")).await;

    page_out(s).await
}

pub(crate) fn press_key_spec(key: &str) -> Option<(&'static str, u32)> {
    Some(match key {
        "Escape" => ("Escape", 27),
        "Enter" => ("Enter", 13),
        "Tab" => ("Tab", 9),
        "ArrowDown" => ("ArrowDown", 40),
        "ArrowUp" => ("ArrowUp", 38),
        "ArrowLeft" => ("ArrowLeft", 37),
        "ArrowRight" => ("ArrowRight", 39),
        "PageDown" => ("PageDown", 34),
        "PageUp" => ("PageUp", 33),
        "Home" => ("Home", 36),
        "End" => ("End", 35),
        "Backspace" => ("Backspace", 8),
        "Delete" => ("Delete", 46),
        _ => return None,
    })
}

pub async fn press(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::press(args).await;
    }

    let key = args
        .get("key")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing key")?;
    let (code, key_code) = press_key_spec(key)
        .ok_or_else(|| format!("unsupported key \"{key}\" — use Escape, Enter, Tab, arrows, PageDown, PageUp, Home, End, Backspace or Delete"))?;

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();
    let before = PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items);

    match args.get("ref").and_then(|v| v.as_u64()) {
        Some(r) => {
            let path = target(s, r as usize, snap_of(args)).await?;
            let el =
                s.page.find_element(&path).await.map_err(|_| {
                    "stale ref — run browser.read for a fresh element list".to_string()
                })?;
            el.press_key(key)
                .await
                .map_err(|err| format!("press failed: {err}"))?;
        }
        None => {
            let js = format!(
                r#"((k, code, kc) => {{
  const t = document.activeElement;
  if (!t || t === document.body) return 'missing';
  for (const type of ['keydown', 'keypress', 'keyup']) {{
    const ev = new KeyboardEvent(type, {{ key: k, code: code, bubbles: true, cancelable: true }});
    try {{
      Object.defineProperty(ev, 'keyCode', {{ value: kc }});
      Object.defineProperty(ev, 'which', {{ value: kc }});
    }} catch {{}}
    t.dispatchEvent(ev);
  }}
  return 'ok';
}})({}, {}, {})"#,
                serde_json::to_string(key).unwrap_or_default(),
                serde_json::to_string(code).unwrap_or_default(),
                key_code,
            );
            if eval_str(&s.page, &js).await != "ok" {
                return Err("nothing focused to press on — click a field first".into());
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(300)).await;
    let out = page_out(s).await?;
    Ok(verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    ))
}

pub async fn wait(args: &Value) -> Result<String, String> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing text")?;
    let timeout = args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .clamp(1, 60);
    let deadline = Instant::now() + Duration::from_secs(timeout);

    loop {
        let out = read(args).await?;
        if out.contains(text) {
            return Ok(out);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "still not showing \"{text}\" after {timeout}s — the update may need a trigger first"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

const MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;

pub async fn upload(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::upload(args).await;
    }

    let r = ref_of(args)?;
    let snap = snap_of(args);
    let raw = args
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing path")?;
    let path = std::fs::canonicalize(crate::tools::expand(raw))
        .map_err(|_| format!("no such file: {raw}"))?;
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()));
    }
    if path.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_UPLOAD_BYTES {
        return Err("file is larger than 100MB".into());
    }

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    let kind = s
        .elements
        .items
        .get(r)
        .map(|e| e.kind.clone())
        .unwrap_or_default();
    if kind != "input file" {
        return Err("that element takes no file — pick a file input".into());
    }
    let cdp_path = target(s, r, snap).await?;
    let before = PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items);

    let el = s
        .page
        .find_element(&cdp_path)
        .await
        .map_err(|_| "stale ref — run browser.read for a fresh element list".to_string())?;
    let mut cmd =
        chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams::new(vec![path
            .to_string_lossy()
            .into_owned()]);
    cmd.backend_node_id = Some(el.backend_node_id);
    drop(el);
    s.page
        .execute(cmd)
        .await
        .map_err(|err| format!("upload failed: {err}"))?;

    let got = eval_str(&s.page, &verify_js(&cdp_path)).await;
    if got.is_empty() {
        return Err("upload did not land — the input may reset on empty selection".into());
    }

    let out = page_out(s).await?;
    Ok(verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    ))
}

fn drag_js(from: &str, to: &str) -> String {
    let a = serde_json::to_string(from).unwrap_or_default();
    let b = serde_json::to_string(to).unwrap_or_default();
    format!(
        r#"((fromSel, toSel) => {{
  const find = window.__argusFind || ((s) => document.querySelector(s));
  const src = find(fromSel);
  const dst = find(toSel);
  if (!src || !dst) return 'missing';
  try {{ src.scrollIntoView({{ block: 'center' }}); }} catch {{}}
  try {{ dst.scrollIntoView({{ block: 'center' }}); }} catch {{}}
  const dt = new DataTransfer();
  const mid = (r) => ({{ x: r.x + r.width / 2, y: r.y + r.height / 2 }});
  const p = mid(src.getBoundingClientRect());
  const q = mid(dst.getBoundingClientRect());
  const ev = (type, at) => new DragEvent(type, {{
    bubbles: true,
    cancelable: true,
    clientX: at.x,
    clientY: at.y,
    dataTransfer: dt,
  }});
  src.dispatchEvent(ev('dragstart', p));
  dst.dispatchEvent(ev('dragenter', q));
  dst.dispatchEvent(ev('dragover', q));
  dst.dispatchEvent(ev('drop', q));
  src.dispatchEvent(ev('dragend', q));
  return 'ok';
}})({a}, {b})"#
    )
}

pub async fn drag(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::drag(args).await;
    }

    let snap = snap_of(args);
    let from = args
        .get("from")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or("missing from")?;
    let to = args
        .get("to")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or("missing to")?;

    let mut map = sess(&name).await?;
    let s = map.get_mut(&name).ok_or("browser session missing")?;
    s.last_used = Instant::now();

    let a = target(s, from, snap).await?;
    let b = target(s, to, snap).await?;
    let before = PageState::capture(&s.url, &s.title, s.text_hash, &s.elements.items);

    if eval_str(&s.page, &drag_js(&a, &b)).await != "ok" {
        return Err("stale ref — run browser.read for a fresh element list".into());
    }

    tokio::time::sleep(Duration::from_millis(400)).await;
    let out = page_out(s).await?;
    Ok(verify_outcome(
        &before,
        &s.url,
        &s.title,
        s.text_hash,
        &s.elements.items,
        out,
    ))
}

pub async fn close(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::close(args).await;
    }

    let sess = {
        let mut map = pool().sess.lock().await;
        map.remove(&name)
    };

    // Close outside the lock: teardown awaits the browser.
    match sess {
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
