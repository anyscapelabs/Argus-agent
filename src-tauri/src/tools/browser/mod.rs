mod ext;
pub mod extpipe;

use std::collections::HashMap;
use std::path::PathBuf;
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
        desc: "click an element from the last browser snapshot by its ref number",
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
];

static NEXT_SNAP: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_snap() -> u64 {
    NEXT_SNAP.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RefEntry {
    pub(crate) path: String,
    pub(crate) label: String,
    pub(crate) kind: String,
}

#[derive(Debug, Default)]
pub(crate) struct RefTable {
    pub(crate) gen: u64,
    pub(crate) items: Vec<RefEntry>,
    pub(crate) recovery_gen: Option<u64>,
}

impl RefTable {
    pub(crate) fn refresh(&mut self, items: Vec<RefEntry>) -> u64 {
        self.gen = next_snap();
        self.items = items;
        self.gen
    }

    pub(crate) fn stale_for_tokenless(
        &mut self,
        index: usize,
        shown: Option<u64>,
    ) -> Option<String> {
        match shown {
            Some(g) if g != self.gen && self.items.get(index).is_some() => {
                Some(self.stale_recovery(index, g))
            }
            _ => None,
        }
    }

    pub(crate) fn resolve(&self, index: usize, presented: Option<u64>) -> Result<String, String> {
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

fn profile_of(args: &Value) -> String {
    args.get("profile")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("main")
        .to_string()
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

async fn route_profile(args: &Value) -> String {
    session_key_for(args)
}

pub(crate) fn shown_gen_in(output: &str) -> Option<u64> {
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

async fn launch(root: &PathBuf, name: &str) -> Result<Sess, String> {
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
  const sel = 'a, button, input, textarea, select, summary, [role="button"], [role="tab"], [role="search"], [role="combobox"], [role="switch"], [onclick], [aria-expanded], [contenteditable="true"]';
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

fn ref_of(args: &Value) -> Result<usize, String> {
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

    if submit {
        let _ = el.press_key("Enter").await;
        let _ = s.page.wait_for_navigation().await;
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

pub async fn close(args: &Value) -> Result<String, String> {
    let name = route_profile(args).await;

    if self::ext::is_real(&name) {
        return self::ext::close(args).await;
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

const SECRET_PAT: &str = r"sk-[A-Za-z0-9_-]{16,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{12,}|xox[bap]-[A-Za-z0-9-]{10,}|Bearer\s+[A-Za-z0-9._-]{16,}|[a-f0-9]{32,}";

fn secret_re() -> Option<regex::Regex> {
    regex::Regex::new(SECRET_PAT).ok()
}

pub fn url_guard(url: &str) -> Result<(), String> {
    let Some(re) = secret_re() else { return Ok(()) };
    let decoded = percent_encoding::percent_decode_str(url).decode_utf8_lossy();

    if re.is_match(url) || re.is_match(&decoded) {
        return Err(
            "url looks like it carries a credential — remove the token from the url".into(),
        );
    }

    Ok(())
}

pub fn redact(s: &str) -> String {
    match secret_re() {
        Some(re) => re.replace_all(s, "[redacted]").into_owned(),
        None => s.into(),
    }
}

pub fn sensitive_note(url: &str, out: String) -> String {
    match sensitive_pats() {
        Some((url_re, _)) if url_re.is_match(url) => format!(
            "{out}\nnote: this page looks like login/checkout — further actions here will need user approval"
        ),
        _ => out,
    }
}

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

    let name = route_profile(args).await;

    if self::ext::is_real(&name) && self::ext::sensitive(args).await {
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
        if let Ok(g) = pool().sess.try_lock() {
            if let Some(s) = g.get(&name) {
                if url_re.is_match(&s.url) {
                    return true;
                }

                if let Some(l) = s.elements.label(r) {
                    if label_re.is_match(&l) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod shown_tests {
    use super::{shown_gen_in, RefEntry, RefTable};

    fn table(gen: u64, rows: &[(&str, &str, &str)]) -> RefTable {
        RefTable {
            gen,
            items: rows
                .iter()
                .map(|(kind, label, path)| RefEntry {
                    kind: kind.to_string(),
                    label: label.to_string(),
                    path: path.to_string(),
                })
                .collect(),
            recovery_gen: None,
        }
    }

    #[test]
    fn tokenless_steady_state_proceeds() {
        let mut t = table(5, &[("button", "Go", "b1"), ("input", "Name", "i1")]);
        assert!(t.stale_for_tokenless(0, Some(5)).is_none());
        assert!(t.stale_for_tokenless(1, Some(5)).is_none());
        assert!(t.recovery_gen.is_none());
    }

    #[test]
    fn tokenless_drift_returns_bounded_recovery() {
        let mut t = table(5, &[("button", "Overview", "ov"), ("button", "Go", "b2")]);
        let first = t
            .stale_for_tokenless(0, Some(4))
            .expect("drifted token-less ref must be rejected");
        assert!(first.contains("stale ref 0 from snapshot 4"));
        assert!(first.contains("snapshot 5 is current"));
        assert!(first.contains("Elements (snapshot 5)"));
        assert!(first.contains("Choose the replacement ref"));
        assert_eq!(t.recovery_gen, Some(5));

        let second = t
            .stale_for_tokenless(0, Some(4))
            .expect("repeat must still be rejected");
        assert!(second.contains("stale ref"));
        assert!(
            !second.contains("Elements (snapshot"),
            "repeat must not smuggle another snapshot: {second}"
        );
        assert!(second.contains("run browser.read"));
    }

    #[test]
    fn tokenless_without_presentation_proceeds() {
        let mut t = table(5, &[("button", "Go", "b1")]);
        assert!(t.stale_for_tokenless(0, None).is_none());
        assert!(t.recovery_gen.is_none());
    }

    #[test]
    fn tokenless_unknown_ref_under_drift_falls_through() {
        let mut t = table(5, &[("button", "Go", "b1")]);
        assert!(t.stale_for_tokenless(9, Some(4)).is_none());
        assert!(t.recovery_gen.is_none());
    }

    #[test]
    fn explicit_resolution_keeps_classic_shape() {
        let t = table(5, &[("button", "Go", "b1")]);
        let err = t
            .resolve(0, Some(4))
            .expect_err("old explicit snapshot must be stale");
        assert!(err.contains("stale ref 0 from snapshot 4"));
        assert!(!err.contains("Choose the replacement"));
    }

    #[test]
    fn shown_gen_extraction() {
        assert_eq!(
            shown_gen_in(
                "url u\ntitle t\n---\nbody\n---\nElements (snapshot 12):\n[0] button \"Go\"\n"
            ),
            Some(12)
        );
        assert_eq!(
            shown_gen_in("\nElements (snapshot 7): (snapshot failed)\n"),
            Some(7)
        );
        assert_eq!(shown_gen_in("browser closed"), None);
        assert_eq!(shown_gen_in(""), None);
    }
}
