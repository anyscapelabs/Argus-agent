//! Real-model web evaluation with the production Baseten configuration.
//!
//! Drives the real `sessions/chat.rs::send()` loop against the same
//! provider/model the Argus application uses (Baseten +
//! `baseten/zai-org/GLM-5.3-Fast`, rows copied from the production
//! database; the API key stays in the OS keyring and is never read here).
//! The web layer hits local fixtures only (SearXNG mock + page server);
//! no public internet is used.
//!
//! Ignored by default (`cargo test -- --ignored`). Runs exactly once per
//! invocation; never retries the task automatically.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const PROD_DB: &str = "/home/brnx/.local/share/com.anyscapelabs.argus/argus.db";
const WANT_PROVIDER: &str = "baseten";
const WANT_MODEL: &str = "baseten/zai-org/GLM-5.3-Fast";
const FACT: &str = "COPPER-19";
const USER_TASK: &str = "Search the web for Project Finch, open the relevant result, find its launch code, and tell me the launch code.";

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

async fn read_http_request(sock: &mut tokio::net::TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buf = vec![0u8; 65536];
    let mut data = vec![];
    loop {
        let n = sock.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..pos]).into_owned();
            let len: usize = head
                .lines()
                .skip(1)
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim().to_lowercase() == "content-length")
                .and_then(|(_, v)| v.trim().parse().ok())
                .unwrap_or(0);
            let mut body = data[pos + 4..].to_vec();
            while body.len() < len {
                let n = sock.read(&mut buf).await.ok()?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            body.truncate(len);
            let line = head.lines().next().unwrap_or("").to_string();
            return Some((line, body));
        }
        if data.len() > 1_000_000 {
            return None;
        }
    }
}

#[tokio::test]
#[ignore]
async fn eval_real_web_task() {
    let _guard = serial();
    assert!(
        !USER_TASK.contains(FACT),
        "harness must not leak the fact into the prompt"
    );

    let prev_searx = std::env::var("ARGUS_SEARXNG_POOL").ok();
    let prev_jina = std::env::var("ARGUS_JINA_URL").ok();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fact_url = format!("{base}/finch");
    let hits: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    tokio::spawn({
        let hits = hits.clone();
        let fact_url = fact_url.clone();
        async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hits = hits.clone();
                let fact_url = fact_url.clone();
                tokio::spawn(async move {
                    let Some((line, _)) = read_http_request(&mut sock).await else {
                        return;
                    };
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    hits.lock().unwrap().push(path.clone());
                    let (status, ct, body) = if path.starts_with("/jina") {
                        (
                            "500 Internal Server Error",
                            "text/plain",
                            "jina down".to_string(),
                        )
                    } else if path.starts_with("/search") {
                        (
                            "200 OK",
                            "application/json",
                            serde_json::json!({"results": [
                                {"title": "Project Finch Program Overview", "url": fact_url, "content": "history and milestones of the program"}
                            ]})
                            .to_string(),
                        )
                    } else if path.starts_with("/finch") {
                        (
                            "200 OK",
                            "text/html",
                            "<html><body><h1>Project Finch</h1><p>Project Finch has a launch code of COPPER-19. Do not share it.</p></body></html>".to_string(),
                        )
                    } else {
                        ("404 Not Found", "text/plain", "no route".to_string())
                    };
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        }
    });
    std::env::set_var("ARGUS_SEARXNG_POOL", format!("{base}/search"));
    std::env::set_var("ARGUS_JINA_URL", format!("{base}/jina"));

    let prod = rusqlite::Connection::open(PROD_DB).expect("production database must exist");
    let prov: Provider = prod
        .query_row(
            "SELECT id, name, compatible, base_url, api_key_ref, connected, free, priority, logo_url, doc_url FROM providers WHERE id = ?1",
            rusqlite::params![WANT_PROVIDER],
            |r| {
                Ok(Provider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    compatible: r.get(2)?,
                    base_url: r.get(3)?,
                    api_key_ref: r.get(4)?,
                    connected: r.get(5)?,
                    free: r.get(6)?,
                    priority: r.get(7)?,
                    logo_url: r.get(8)?,
                    doc_url: r.get(9)?,
                })
            },
        )
        .expect("production baseten provider row must exist");
    assert_eq!(prov.base_url, "https://inference.baseten.co/v1");
    let (remote, cost_in, cost_out): (String, f64, f64) = prod
        .query_row(
            "SELECT remote_model_id, cost_in, cost_out FROM model_providers WHERE provider_id = ?1 AND model_id = ?2",
            rusqlite::params![WANT_PROVIDER, WANT_MODEL],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("production baseten model link must exist");
    let model_name: String = prod
        .query_row(
            "SELECT display_name FROM models WHERE id = ?1 AND enabled = 1",
            rusqlite::params![WANT_MODEL],
            |r| r.get(0),
        )
        .expect("production model must be enabled");

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-realweb-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(tmp.join(d)).unwrap();
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    argus_lib::gateway::store::upsert_provider(&conn, &prov).unwrap();
    argus_lib::gateway::store::set_connected(&conn, WANT_PROVIDER, true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: WANT_MODEL.into(),
            display_name: model_name,
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: WANT_MODEL.into(),
            provider_id: WANT_PROVIDER.into(),
            remote_model_id: remote.clone(),
            cost_in,
            cost_out,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, WANT_MODEL, true).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir: tmp.join("skills"),
        library_dir: tmp.join("library"),
        logos_dir: tmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: StdMutex::new(HashMap::new()),
        events: StdMutex::new(HashMap::new()),
        turns: StdMutex::new(HashSet::new()),
        watching: StdMutex::new(None),
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real web".into(),
                model_id: Some(WANT_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: true,
            },
        )
        .unwrap()
        .id
    };

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let chan = tauri::ipc::Channel::new({
        let events = events.clone();
        move |body: tauri::ipc::InvokeResponseBody| {
            let s = match body {
                tauri::ipc::InvokeResponseBody::Json(s) => s,
                tauri::ipc::InvokeResponseBody::Raw(b) => String::from_utf8_lossy(&b).into_owned(),
            };
            events.lock().unwrap().push(s);
            Ok(())
        }
    });

    let t0 = std::time::Instant::now();
    let status =
        argus_lib::sessions::chat::send(&gw, &handle, &session_id, USER_TASK, &chan, "user").await;
    let elapsed = t0.elapsed();

    let msgs = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &session_id).unwrap()
    };
    let events = events.lock().unwrap().clone();
    let attempts: Vec<i64> = {
        let conn = gw.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT attempt FROM request_log ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let hits = hits.lock().unwrap().clone();

    match prev_searx {
        Some(v) => std::env::set_var("ARGUS_SEARXNG_POOL", v),
        None => std::env::remove_var("ARGUS_SEARXNG_POOL"),
    }
    match prev_jina {
        Some(v) => std::env::set_var("ARGUS_JINA_URL", v),
        None => std::env::remove_var("ARGUS_JINA_URL"),
    }

    let assistants: Vec<&argus_lib::sessions::schema::Msg> =
        msgs.iter().filter(|m| m.role == "assistant").collect();
    let results: Vec<&argus_lib::sessions::schema::Msg> = msgs
        .iter()
        .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
        .collect();
    let kinds: HashSet<String> = events
        .iter()
        .filter_map(|e| {
            e.split("\"type\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();

    let searched = hits.iter().any(|p| p.starts_with("/search"));
    let read = hits.iter().any(|p| p.starts_with("/finch"));
    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(FACT);
    let snippet_leak = results
        .iter()
        .filter(|m| m.content.contains("web.search"))
        .any(|m| m.content.contains(FACT));
    let tools_used: Vec<String> = results
        .iter()
        .filter_map(|m| {
            m.content
                .split("tool=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();

    eprintln!("=== REAL WEB EVAL TRACE ===");
    eprintln!("provider=baseten remote={remote} elapsed={elapsed:?}");
    eprintln!("status: {status:?}");
    eprintln!(
        "turns: {} results: {} attempts: {attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("event kinds: {kinds:?}");
    eprintln!("fixture hits: {hits:?}");
    eprintln!("tools used: {tools_used:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("searched={searched} read={read} answer_ok={answer_ok} snippet_leak={snippet_leak}");

    let verdict = match (&status, searched, read, answer_ok) {
        (Ok(_), true, true, true) => "PASS",
        (Ok(_), _, _, _) => "PARTIAL",
        _ => "FAIL",
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(verdict, "PASS", "real-model web task did not pass");
}
