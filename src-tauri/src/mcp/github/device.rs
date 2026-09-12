use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

const DEVICE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";
const POLL_CAP: Duration = Duration::from_secs(600);

static PENDING_SINCE: OnceLock<StdMutex<Option<Instant>>> = OnceLock::new();

fn pending() -> &'static StdMutex<Option<Instant>> {
    PENDING_SINCE.get_or_init(|| StdMutex::new(None))
}

pub async fn begin() -> Result<super::GithubDevice, String> {
    if let Ok(g) = pending().lock() {
        if let Some(t) = g.as_ref() {
            if t.elapsed() < Duration::from_secs(60) {
                return Err(
                    "a github connect is already waiting — enter the code shown before".into(),
                );
            }
        }
    }

    let cfg = super::config::load()?;
    let cli = reqwest::Client::new();
    let v: serde_json::Value = cli
        .post(DEVICE_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("scope", super::config::SCOPES),
        ])
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;

    let device_code = v
        .get("device_code")
        .and_then(|c| c.as_str())
        .ok_or("github refused device flow — check the client id")?
        .to_string();
    let user_code = v
        .get("user_code")
        .and_then(|c| c.as_str())
        .ok_or("github gave no user code")?
        .to_string();
    let verification_uri = v
        .get("verification_uri")
        .and_then(|c| c.as_str())
        .unwrap_or("https://github.com/login/device")
        .to_string();
    let interval = v.get("interval").and_then(|i| i.as_u64()).unwrap_or(5);

    *pending().lock().map_err(|err| err.to_string())? = Some(Instant::now());
    tokio::spawn(poll(device_code, interval));

    Ok(super::GithubDevice {
        verification_uri,
        user_code,
    })
}

async fn poll(device_code: String, interval: u64) {
    let cfg = super::config::load().unwrap_or(super::config::GithubCfg {
        client_id: String::new(),
        client_secret: String::new(),
    });
    let cli = reqwest::Client::new();
    let wait = Duration::from_secs(interval.max(5));
    let started = Instant::now();

    loop {
        if started.elapsed() > POLL_CAP {
            break;
        }

        tokio::time::sleep(wait).await;

        let form = [
            ("client_id", cfg.client_id.as_str()),
            ("device_code", device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ];
        let Ok(resp) = cli
            .post(TOKEN_URL)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
        else {
            continue;
        };
        let Ok(resp) = resp.error_for_status() else {
            continue;
        };
        let Ok(v): Result<serde_json::Value, _> = resp.json().await else {
            continue;
        };

        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            if err == "authorization_pending" {
                continue;
            }

            break;
        }

        if let Some(tok) = v.get("access_token").and_then(|t| t.as_str()) {
            let tok = tok.to_string();

            if super::tokens::save_token(&tok).await.is_ok() {
                if let Ok(me) = super::authed_get(USER_URL).await {
                    if let Some(login) = me.get("login").and_then(|l| l.as_str()) {
                        super::tokens::set_login(login.to_string()).await;
                    }
                }
            }

            break;
        }
    }

    if let Ok(mut g) = pending().lock() {
        *g = None;
    }
}
