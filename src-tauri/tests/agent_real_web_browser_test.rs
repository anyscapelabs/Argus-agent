//! Real-model web-to-browser evaluation with production Baseten config.
//!
//! Real `sessions/chat.rs::send()` against Baseten GLM-5.3-Fast (rows copied
//! from the production database; key stays in the OS keyring). Local
//! fixtures only: a SearXNG mock plus the fake-extension browser fixture.
//!
//! Isolation: the browser URL is generated dynamically per run (ephemeral
//! port plus random path) and the expected status is assembled at runtime,
//! so neither value exists in any file the model could mine. The search
//! snippet and pre-click page never contain the status.
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
const USER_TASK: &str = "Find the Project Finch control panel on the web, open it in the browser, activate the control panel, and tell me the resulting status.";

fn status_value() -> String {
    ["FINCH", "READY", "27"].join("-")
}

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

async fn read_http_request(
    sock: &mut tokio::net::TcpStream,
) -> Option<(String, Vec<u8>)> {
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

struct ExtLog {
    methods: StdMutex<Vec<String>>,
    clicks: StdMutex<u32>,
    revealed: StdMutex<bool>,
}

async fn read_frame(rd: &mut tokio::net::unix::OwnedReadHalf) -> Option<serde_json::Value> {
    let mut b = [0u8; 4];
    rd.read_exact(&mut b).await.ok()?;
    let len = u32::from_ne_bytes(b) as usize;
    if len == 0 || len > 64 * 1_048_576 {
        return None;
    }
    let mut buf = vec![0u8; len];
    rd.read_exact(&mut buf).await.ok()?;
    serde_json::from_slice(&buf).ok()
}

async fn write_frame(wr: &mut tokio::net::unix::OwnedWriteHalf, v: &serde_json::Value) {
    let b = serde_json::to_vec(v).unwrap();
    let _ = wr.write_all(&(b.len() as u32).to_ne_bytes()).await;
    let _ = wr.write_all(&b).await;
    let _ = wr.flush().await;
}

fn first_u64(text: &str, key: &str) -> Option<u64> {
    let re = regex::Regex::new(&format!(r#""{key}"\s*:\s*(\d+)"#)).ok()?;
    re.captures(text)?.get(1)?.as_str().parse().ok()
}

fn snapshot_gens(text: &str) -> Vec<u64> {
    let re = regex::Regex::new(r"\(snapshot (\d+)\)").unwrap();
    re.captures_iter(text).filter_map(|c| c.get(1)?.as_str().parse().ok()).collect()
}

fn first_url_on_line(text: &str, needle: &str) -> Option<String> {
    let re = regex::Regex::new(r##"https?://[^\s"<>]+"##).ok()?;
    text.lines()
        .find(|l| l.contains(needle))
        .and_then(|l| re.find(l))
        .map(|m| {
            m.as_str()
                .trim_end_matches(|c| c == '.' || c == ',' || c == ')' || c == ']')
                .to_string()
        })
}

#[tokio::test]
#[ignore]
async fn eval_real_web_browser_task() {
    let _guard = serial();
    let status = status_value();
    let prev_searx = std::env::var("ARGUS_SEARXNG_POOL").ok();
    let prev_jina = std::env::var("ARGUS_JINA_URL").ok();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let panel_path = format!("/panel-{}", &uuid::Uuid::new_v4().as_simple().to_string()[..8]);
    let panel_url = format!("{base}{panel_path}");
    let web_hits: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    tokio::spawn({
        let web_hits = web_hits.clone();
        let panel_url = panel_url.clone();
        let panel_path = panel_path.clone();
        async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let web_hits = web_hits.clone();
                let panel_url = panel_url.clone();
                let panel_path = panel_path.clone();
                tokio::spawn(async move {
                    let Some((line, _)) = read_http_request(&mut sock).await else {
                        return;
                    };
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    web_hits.lock().unwrap().push(path.clone());
                    let (st, ct, body) = if path.starts_with("/jina") {
                        ("500 Internal Server Error", "text/plain", "jina down".to_string())
                    } else if path.starts_with("/search") {
                        (
                            "200 OK",
                            "application/json",
                            serde_json::json!({"results": [
                                {"title": "Project Finch Control Panel", "url": panel_url, "content": "control panel for the program"}
                            ]})
                            .to_string(),
                        )
                    } else if path == panel_path {
                        (
                            "200 OK",
                            "text/html",
                            "<html><body><p>Finch control panel. Use the control below.</p></body></html>".to_string(),
                        )
                    } else {
                        ("404 Not Found", "text/plain", "no route".to_string())
                    };
                    let resp = format!(
                        "HTTP/1.1 {st}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        }
    });
    std::env::set_var("ARGUS_SEARXNG_POOL", format!("{base}/search"));
    std::env::set_var("ARGUS_JINA_URL", format!("{base}/jina"));

    let tmp = std::env::temp_dir().join(format!("argus-eval-rwb-{}", uuid::Uuid::new_v4().as_simple()));
    let data_dir = tmp.join("com.anyscapelabs.argus");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
    std::env::set_var("XDG_DATA_HOME", &tmp);

    let sock_path = data_dir.join("native.sock");
    let elistener = tokio::net::UnixListener::bind(&sock_path).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = elistener.accept().await {
            tokio::spawn(argus_lib::tools::browser::extpipe::serve_conn(stream));
        }
    });
    let elog = Arc::new(ExtLog {
        methods: StdMutex::new(vec![]),
        clicks: StdMutex::new(0),
        revealed: StdMutex::new(false),
    });
    let eclient = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    tokio::spawn({
        let elog = elog.clone();
        let status = status.clone();
        async move {
            let (mut rd, mut wr) = eclient.into_split();
            while let Some(req) = read_frame(&mut rd).await {
                if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
                    continue;
                }
                let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
                let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
                elog.methods.lock().unwrap().push(method.clone());
                let revealed = *elog.revealed.lock().unwrap();
                let resp_data = match method.as_str() {
                    "navigate" => serde_json::json!({
                        "tabId": 31,
                        "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                        "title": "Finch Panel",
                        "text": "Finch control panel. Use the control below.",
                    }),
                    "snapshot" => serde_json::json!([
                        {"kind": "button", "label": "Open Finch control panel", "path": "panel-btn"},
                    ]),
                    "click" => {
                        *elog.clicks.lock().unwrap() += 1;
                        *elog.revealed.lock().unwrap() = true;
                        serde_json::json!({})
                    }
                    "read" => {
                        let text = if revealed {
                            format!("Finch control panel. Status: {status}.")
                        } else {
                            "Finch control panel. Use the control below.".to_string()
                        };
                        serde_json::json!({
                            "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                            "title": "Finch Panel",
                            "text": text,
                        })
                    }
                    _ => serde_json::json!({}),
                };
                write_frame(
                    &mut wr,
                    &serde_json::json!({"id": id, "ok": true, "data": resp_data}),
                )
                .await;
            }
        }
    });
    for _ in 0..100 {
        if argus_lib::tools::browser::extpipe::connected() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(argus_lib::tools::browser::extpipe::connected());

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

    let gtmp = std::env::temp_dir().join(format!("argus-eval-rwbw-{}", uuid::Uuid::new_v4().as_simple()));
    std::fs::create_dir_all(&gtmp).unwrap();
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(gtmp.join(d)).unwrap();
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE).unwrap();
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
        skills_dir: gtmp.join("skills"),
        library_dir: gtmp.join("library"),
        logos_dir: gtmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real web-browser".into(),
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
    let status_run =
        argus_lib::sessions::chat::send(&gw, &handle, &session_id, USER_TASK, &chan).await;
    let elapsed = t0.elapsed();

    let msgs = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &session_id).unwrap()
    };
    let events = events.lock().unwrap().clone();
    let attempts: Vec<i64> = {
        let conn = gw.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT attempt FROM request_log ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let web_hits = web_hits.lock().unwrap().clone();
    let methods = elog.methods.lock().unwrap().clone();
    let clicks = *elog.clicks.lock().unwrap();

    match prev_searx {
        Some(v) => std::env::set_var("ARGUS_SEARXNG_POOL", v),
        None => std::env::remove_var("ARGUS_SEARXNG_POOL"),
    }
    match prev_jina {
        Some(v) => std::env::set_var("ARGUS_JINA_URL", v),
        None => std::env::remove_var("ARGUS_JINA_URL"),
    }
    match prev_xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
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

    let search_msg = results.iter().find(|m| m.content.contains("web.search"));
    let search_ok = search_msg.is_some();
    let search_url = search_msg.and_then(|m| first_url_on_line(&m.content, "127.0.0.1"));
    let url_ok = search_url.as_deref() == Some(panel_url.as_str());

    let open_msg = results.iter().find(|m| m.content.contains("browser.open"));
    let open_url = open_msg
        .and_then(|m| first_url_on_line(&m.content, "127.0.0.1"));
    let open_ok = open_url.as_deref() == Some(panel_url.as_str());
    let open_gen = open_msg.and_then(|m| snapshot_gens(&m.content).into_iter().next());
    let pre_absence = open_msg.map(|m| !m.content.contains(&status)).unwrap_or(false);

    let click_text: String = assistants
        .iter()
        .filter_map(|m| {
            let mut s = m.content.clone();
            if let Some(c) = m.tool_calls.as_deref() {
                s.push_str(c);
            }
            if s.contains("browser.click") {
                Some(s.replace('\\', ""))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let click_ref = first_u64(&click_text, "ref");
    let click_snap = first_u64(&click_text, "snapshot");
    let gen_ok = match (open_gen, click_snap) {
        (Some(g), Some(s)) => g == s,
        _ => false,
    };
    let click_msg = results.iter().find(|m| m.content.contains("browser.click"));
    let changed_ok = click_msg.map(|m| m.content.contains(&status)).unwrap_or(false);

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
    let mining = tools_used.iter().any(|t| {
        matches!(
            t.as_str(),
            "terminal" | "bash.run" | "grep" | "fs.write" | "skill.read"
                | "skill.search" | "skill.create" | "memory.save" | "memory.search"
                | "memory.read" | "doc.create"
        ) || t.starts_with("computer.")
    });

    let final_text = assistants.last().map(|m| m.content.clone()).unwrap_or_default();
    let answer_ok = final_text.contains(&status);

    eprintln!("=== REAL WEB-BROWSER EVAL TRACE ===");
    eprintln!("provider=baseten remote={remote} elapsed={elapsed:?}");
    eprintln!("status: {status_run:?}");
    eprintln!("turns: {} results: {} attempts: {attempts:?}", assistants.len(), results.len());
    eprintln!("event kinds: {kinds:?}");
    eprintln!("web hits: {web_hits:?}");
    eprintln!("browser methods: {methods:?} clicks={clicks}");
    eprintln!("tools used: {tools_used:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("search_ok={search_ok} url_ok={url_ok} open_ok={open_ok} pre_absence={pre_absence}");
    eprintln!("open_gen={open_gen:?} click_ref={click_ref:?} click_snap={click_snap:?} gen_ok={gen_ok} mining={mining}");
    eprintln!("changed_ok={changed_ok} answer_ok={answer_ok}");

    let verdict = if !status_run.is_ok() {
        "FAIL"
    } else if mining {
        "FAIL"
    } else if !(search_ok && url_ok && open_ok && pre_absence) {
        "FAIL"
    } else if clicks != 1 || !gen_ok || click_ref.is_none() {
        "FAIL"
    } else if !(changed_ok && answer_ok) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(verdict, "PASS", "real-model web-browser task did not pass");
}
