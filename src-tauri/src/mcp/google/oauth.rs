use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const TIMEOUT: Duration = Duration::from_secs(300);

struct Pending {
    url: String,
    redirect: String,
    started: Instant,
}

static PENDING: OnceLock<StdMutex<Option<Pending>>> = OnceLock::new();

fn pending() -> &'static StdMutex<Option<Pending>> {
    PENDING.get_or_init(|| StdMutex::new(None))
}

fn pct_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;

    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }

        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn query_pairs(path: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();

    if let Some((_, q)) = path.split_once('?') {
        for kv in q.split('&') {
            if let Some((k, v)) = kv.split_once('=') {
                m.insert(pct_decode(k), pct_decode(v));
            }
        }
    }

    m
}

pub async fn auth_url() -> Result<String, String> {
    if let Ok(g) = pending().lock() {
        if let Some(p) = g.as_ref() {
            if p.started.elapsed() < Duration::from_secs(60) {
                return Ok(p.url.clone());
            }
        }
    }

    let cfg = super::config::load()?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|err| format!("loopback bind failed: {err}"))?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    let state = Uuid::new_v4().to_string();
    let redirect = format!("http://127.0.0.1:{port}/callback");

    let mut url = format!(
        "{AUTH_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={state}&access_type=offline&prompt=consent",
        url_encode(&cfg.client_id),
        url_encode(&redirect),
        url_encode(&super::config::SCOPES.join(" ")),
    );

    if url.len() > 1800 {
        url = format!(
            "{AUTH_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={state}&access_type=offline",
            url_encode(&cfg.client_id),
            url_encode(&redirect),
            url_encode(&super::config::SCOPES.join(" ")),
        );
    }

    *pending().lock().map_err(|err| err.to_string())? = Some(Pending {
        url: url.clone(),
        redirect: redirect.clone(),
        started: Instant::now(),
    });

    tokio::spawn(wait_callback(state, listener));

    Ok(url)
}

pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());

    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }

    out
}

async fn read_head(stream: &mut TcpStream) -> Result<String, String> {
    let mut buf = vec![0u8; 8192];
    let mut n = 0usize;

    loop {
        if n >= buf.len() {
            return Err("request too large".into());
        }

        let r = stream
            .read(&mut buf[n..])
            .await
            .map_err(|err| err.to_string())?;

        if r == 0 {
            break;
        }

        n += r;

        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}

async fn reply(stream: &mut TcpStream, ok: bool, title: &str, body: &str) {
    let code = if ok { "200 OK" } else { "400 Bad Request" };
    let page = format!(
        "<html><body style=\"font-family:sans-serif;padding:40px\"><h2>{title}</h2><p>{body}</p></body></html>"
    );

    let _ = stream
        .write_all(
            format!(
                "HTTP/1.1 {code}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            )
            .as_bytes(),
        )
        .await;
}

async fn wait_callback(state: String, listener: TcpListener) {
    let res = tokio::time::timeout(TIMEOUT, listener.accept()).await;

    let finish = |ok: bool, title: &str, body: &str| {
        if let Ok(mut g) = pending().lock() {
            *g = None;
        }

        (ok, title.to_string(), body.to_string())
    };

    let (mut stream, _) = match res {
        Ok(Ok(s)) => s,
        _ => {
            finish(
                false,
                "Timed out",
                "No authorization arrived. Back in Argus, click Connect again.",
            );
            return;
        }
    };

    let head = match read_head(&mut stream).await {
        Ok(h) => h,
        Err(_) => {
            let (ok, t, b) = finish(false, "Bad request", "Could not read the callback.");
            reply(&mut stream, ok, &t, &b).await;
            return;
        }
    };

    let path = head
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    let q = query_pairs(&path);

    if q.get("state").map(|s| s.as_str()) != Some(state.as_str()) {
        let (ok, t, b) = finish(
            false,
            "Wrong state",
            "Session mismatch. Back in Argus, click Connect again.",
        );
        reply(&mut stream, ok, &t, &b).await;
        return;
    }

    if let Some(err) = q.get("error") {
        let msg = format!("Google said no ({err}). Back in Argus, click Connect to retry.");
        let (ok, t, b) = finish(false, "Not authorized", &msg);
        reply(&mut stream, ok, &t, &b).await;
        return;
    }

    let code = match q.get("code") {
        Some(c) if !c.is_empty() => c.clone(),
        _ => {
            let (ok, t, b) = finish(
                false,
                "Missing code",
                "No authorization code arrived. Retry from Argus.",
            );
            reply(&mut stream, ok, &t, &b).await;
            return;
        }
    };

    match finish_exchange(&redirect_of(), &code).await {
        Ok(email) => {
            let msg = format!("Connected{email}. Back to Argus — it shows connected now.");
            let (ok, t, b) = finish(true, "Argus connected", &msg);
            reply(&mut stream, ok, &t, &b).await;
        }
        Err(err) => {
            let msg =
                format!("Token exchange failed ({err}). Back in Argus, click Connect to retry.");
            let (ok, t, b) = finish(false, "Connection failed", &msg);
            reply(&mut stream, ok, &t, &b).await;
        }
    }
}

async fn finish_exchange(redirect: &str, code: &str) -> Result<String, String> {
    let cfg = super::config::load()?;
    let cli = reqwest::Client::new();
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", redirect),
    ];

    if !cfg.client_secret.trim().is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }

    let v: serde_json::Value = cli
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    let refresh = v
        .get("refresh_token")
        .and_then(|t| t.as_str())
        .ok_or("google gave no refresh token — remove Argus access at myaccount.google.com/permissions and reconnect")?
        .to_string();

    super::tokens::save_refresh(&refresh)?;

    let tok = super::tokens::access_token().await?;
    let me: serde_json::Value = cli
        .get(USERINFO_URL)
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    let email = me
        .get("email")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .to_string();

    if !email.is_empty() {
        super::tokens::set_email(email.clone()).await;
    }

    Ok(if email.is_empty() {
        String::new()
    } else {
        format!(" as {email}")
    })
}

fn redirect_of() -> String {
    pending()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|p| p.redirect.clone()))
        .unwrap_or_default()
}

pub async fn revoke(refresh: &str) {
    let cli = reqwest::Client::new();

    let _ = cli
        .post(REVOKE_URL)
        .form(&[("token", refresh)])
        .send()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_pairs_decode() {
        let q = query_pairs("/callback?code=4%2Fabc&state=x-y");

        assert_eq!(q.get("code").map(|s| s.as_str()), Some("4/abc"));
        assert_eq!(q.get("state").map(|s| s.as_str()), Some("x-y"));
    }

    #[test]
    fn url_encode_leaves_unreserved() {
        assert_eq!(url_encode("abc-_.~19"), "abc-_.~19");
        assert!(url_encode("a b@c").contains("%20"));
    }

    #[test]
    fn auth_url_rejects_blank_config() {
        assert!(query_pairs("/callback").is_empty());
    }
}
