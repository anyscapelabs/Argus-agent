use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::Mutex as AsyncMutex;

const DEVICE_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const SERVICE: &str = "outlook";

static ACCESS: AsyncMutex<Option<(String, Instant)>> = AsyncMutex::const_new(None);
static PENDING_SINCE: OnceLock<StdMutex<Option<Instant>>> = OnceLock::new();

fn pending() -> &'static StdMutex<Option<Instant>> {
    PENDING_SINCE.get_or_init(|| StdMutex::new(None))
}

pub async fn access_token() -> Result<String, String> {
    if let Some((tok, exp)) = ACCESS.lock().await.clone() {
        if Instant::now() + Duration::from_secs(60) < exp {
            return Ok(tok);
        }
    }

    let refresh = super::super::vault::get(SERVICE)?.ok_or("outlook not connected")?;

    if refresh.is_empty() {
        return Err("outlook session expired — reconnect".into());
    }

    let cfg = super::config::load()?;
    let (tok, secs) = super::super::vault::refresh_oauth(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", cfg.client_id.as_str()),
            ("refresh_token", refresh.as_str()),
            ("scope", super::config::SCOPES),
        ],
    )
    .await?;

    *ACCESS.lock().await = Some((tok.clone(), Instant::now() + Duration::from_secs(secs)));

    Ok(tok)
}

pub async fn save_refresh(refresh: &str) -> Result<(), String> {
    super::super::vault::save(SERVICE, refresh)
}

pub async fn begin() -> Result<super::OutlookDevice, String> {
    if let Ok(g) = pending().lock() {
        if let Some(t) = g.as_ref() {
            if t.elapsed() < Duration::from_secs(60) {
                return Err(
                    "an outlook connect is already waiting — enter the code shown before".into(),
                );
            }
        }
    }

    let cfg = super::config::load()?;
    let v: serde_json::Value = reqwest::Client::new()
        .post(DEVICE_URL)
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
        .ok_or("microsoft refused device flow — check the client id")?
        .to_string();
    let user_code = v
        .get("user_code")
        .and_then(|c| c.as_str())
        .ok_or("microsoft gave no user code")?
        .to_string();
    let verification_uri = v
        .get("verification_uri")
        .and_then(|c| c.as_str())
        .unwrap_or("https://microsoft.com/devicelogin")
        .to_string();
    let interval = v.get("interval").and_then(|i| i.as_u64()).unwrap_or(5);

    *pending().lock().map_err(|err| err.to_string())? = Some(Instant::now());
    tokio::spawn(poll(device_code, interval));

    Ok(super::OutlookDevice {
        verification_uri,
        user_code,
    })
}

async fn poll(device_code: String, interval: u64) {
    let cfg = super::config::load().unwrap_or(super::config::OutlookCfg {
        client_id: String::new(),
    });
    let wait = Duration::from_secs(interval.max(5));
    let started = Instant::now();
    let mut outcome = "ended without token".to_string();

    loop {
        if started.elapsed() > Duration::from_secs(600) {
            break;
        }

        tokio::time::sleep(wait).await;

        let Ok(resp) = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", cfg.client_id.as_str()),
                ("device_code", device_code.as_str()),
            ])
            .send()
            .await
        else {
            continue;
        };
        let Ok(v): Result<serde_json::Value, _> = resp.json().await else {
            continue;
        };

        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            if err == "authorization_pending" {
                continue;
            }

            outcome = err.to_string();
            break;
        }

        match (
            v.get("access_token").and_then(|t| t.as_str()),
            v.get("refresh_token").and_then(|t| t.as_str()),
            v.get("expires_in").and_then(|t| t.as_u64()),
        ) {
            (Some(a), Some(r), secs) => {
                let _ = save_refresh(r).await;
                *ACCESS.lock().await = Some((
                    a.to_string(),
                    Instant::now() + Duration::from_secs(secs.unwrap_or(3600)),
                ));
                outcome = "connected".to_string();
                break;
            }
            _ => break,
        }
    }

    crate::connectors::log::event(
        "outlook",
        "oauth_finished",
        &outcome,
        if outcome == "connected" { "ok" } else { "err" },
    );

    if let Ok(mut g) = pending().lock() {
        *g = None;
    }
}
