use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;
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

pub fn callback_query(path: &str) -> Vec<(String, String)> {
    let full = format!("http://127.0.0.1{path}");

    Url::parse(&full)
        .map(|u| {
            u.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default()
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

    let mut url = Url::parse(AUTH_URL).map_err(|err| err.to_string())?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("scope", &super::config::SCOPES.join(" "))
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    let url = url.to_string();

    *pending().lock().map_err(|err| err.to_string())? = Some(Pending {
        url: url.clone(),
        redirect: redirect.clone(),
        started: Instant::now(),
    });

    tokio::spawn(wait_callback(state, listener));

    Ok(url)
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
    let q = callback_query(&path);
    let param = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());

    if param("state").as_deref() != Some(state.as_str()) {
        let (ok, t, b) = finish(
            false,
            "Wrong state",
            "Session mismatch. Back in Argus, click Connect again.",
        );
        reply(&mut stream, ok, &t, &b).await;
        return;
    }

    if let Some(err) = param("error") {
        let msg = format!("Google said no ({err}). Back in Argus, click Connect to retry.");
        let (ok, t, b) = finish(false, "Not authorized", &msg);
        reply(&mut stream, ok, &t, &b).await;
        return;
    }

    let code = match param("code") {
        Some(c) if !c.is_empty() => c,
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
