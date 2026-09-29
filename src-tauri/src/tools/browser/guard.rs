// What the browser will not do on its own.
//
// A credentialed URL, a payment page, and a field that looks like a secret are
// all refused here rather than downstream, so no caller can forget the check by
// reaching for the page directly. The patterns are the whole policy.

use std::sync::OnceLock;

use serde_json::Value;

use super::{ext, pool, ref_of, route_profile};

const URL_PAT: &str =
    "login|signin|sign-in|sign_up|signup|/auth|checkout|cart|/pay|billing|order|password";
const LABEL_PAT: &str = "sign in|sign-in|signin|log in|log-in|login|checkout|pay now|payment|place order|buy now|add to cart|password";

const SECRET_PAT: &str = r"sk-[A-Za-z0-9_-]{16,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{12,}|xox[bap]-[A-Za-z0-9-]{10,}|Bearer\s+[A-Za-z0-9._-]{16,}|[a-f0-9]{32,}";

fn secret_re() -> Option<regex::Regex> {
    static RE: OnceLock<Option<regex::Regex>> = OnceLock::new();
    // Literal pattern: a compile failure here is a code bug, cached once.
    RE.get_or_init(|| regex::Regex::new(SECRET_PAT).ok())
        .clone()
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
    static PATS: OnceLock<Option<(regex::Regex, regex::Regex)>> = OnceLock::new();
    // Literal patterns: compile once, not once per tool call.
    PATS.get_or_init(|| {
        Some((
            regex::Regex::new(&format!("(?i)({URL_PAT})")).ok()?,
            regex::Regex::new(&format!("(?i)({LABEL_PAT})")).ok()?,
        ))
    })
    .clone()
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
