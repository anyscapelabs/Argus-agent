// Serial env access on purpose: EXA_API_KEY/ARGUS_EXA_BASE are process-global.
#![allow(clippy::await_holding_lock)]

use argus_lib::tools::web::{search_with, WebConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

type Routes = Arc<Mutex<HashMap<String, (u16, String, String)>>>;

struct Mock {
    base: String,
    routes: Routes,
}

async fn start_mock() -> Mock {
    let routes: Routes = Arc::new(Mutex::new(HashMap::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let table = routes.clone();

    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let table = table.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let hit: Option<(u16, String, String)> = {
                    let routes = table.lock().unwrap();
                    let mut hit = None;
                    let mut best = 0usize;
                    for (prefix, resp) in routes.iter() {
                        if path.starts_with(prefix.as_str()) && prefix.len() > best {
                            best = prefix.len();
                            hit = Some(resp.clone());
                        }
                    }
                    hit
                };
                let (status, ct, body) =
                    hit.unwrap_or((404, "text/plain".into(), "mock: no route".into()));
                let reason = if status == 200 { "OK" } else { "Error" };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    Mock { base, routes }
}

fn route(mock: &Mock, prefix: &str, status: u16, ct: &str, body: &str) {
    mock.routes.lock().unwrap().insert(
        prefix.to_string(),
        (status, ct.to_string(), body.to_string()),
    );
}

fn exa_env(mock: &Mock) -> (Option<String>, Option<String>, Option<String>) {
    let tmp = std::env::temp_dir().join(format!("argus-wexa-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();
    let prev_key = std::env::var("EXA_API_KEY").ok();
    let prev_base = std::env::var("ARGUS_EXA_BASE").ok();
    std::env::set_var("XDG_DATA_HOME", &tmp);
    std::env::set_var("EXA_API_KEY", "test-key");
    std::env::set_var("ARGUS_EXA_BASE", &mock.base);
    (prev_xdg, prev_key, prev_base)
}

fn restore_env(prev: (Option<String>, Option<String>, Option<String>)) {
    let (xdg, key, base) = prev;
    match xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    match key {
        Some(v) => std::env::set_var("EXA_API_KEY", v),
        None => std::env::remove_var("EXA_API_KEY"),
    }
    match base {
        Some(v) => std::env::set_var("ARGUS_EXA_BASE", v),
        None => std::env::remove_var("ARGUS_EXA_BASE"),
    }
}

fn cfg(mock: &Mock) -> WebConfig {
    WebConfig {
        searxng_pool: vec![format!("{}/search", mock.base)],
        ddg_url: format!("{}/html", mock.base),
        jina_base: mock.base.clone(),
        exa: true,
    }
}

const EXA_HITS: &str = r#"{"results":[
{"title":"Exa finds rust","url":"https://exa.example/rust","text":"Exa neural snippet about rust"}
]}"#;

const SEARXNG_HITS: &str = r#"{"results":[
{"title":"Local finds rust","url":"https://local.example/rust","content":"Local snippet about rust"}
]}"#;

#[tokio::test]
async fn exa_results_win_when_key_present() {
    let _guard = serial();
    let mock = start_mock().await;
    route(&mock, "/search", 200, "application/json", EXA_HITS);
    let prev = exa_env(&mock);

    let out = search_with(&serde_json::json!({ "query": "rust" }), &cfg(&mock)).await;

    restore_env(prev);

    let out = out.expect("exa search");
    assert!(out.contains("Exa finds rust"), "got: {out}");
    assert!(out.contains("https://exa.example/rust"), "got: {out}");
    assert!(out.contains("Provider: exa"), "got: {out}");
    assert!(!out.contains("Local finds rust"), "got: {out}");
}

#[tokio::test]
async fn empty_exa_falls_back_to_local() {
    let _guard = serial();
    let mock = start_mock().await;
    route(
        &mock,
        "/search",
        200,
        "application/json",
        r#"{"results":[]}"#,
    );
    let prev = exa_env(&mock);

    let local_hits = SEARXNG_HITS;
    route(&mock, "/fallback", 200, "application/json", local_hits);
    let mut config = cfg(&mock);
    config.searxng_pool = vec![format!("{}/fallback", mock.base)];

    let out = search_with(&serde_json::json!({ "query": "rust" }), &config).await;

    restore_env(prev);

    let out = out.expect("fallback search");
    assert!(out.contains("Local finds rust"), "got: {out}");
    assert!(out.contains("Provider: searxng"), "got: {out}");
}
