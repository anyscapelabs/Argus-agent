//! Real-model browser evaluation with the production Baseten configuration.
//!
//! Real `sessions/chat.rs::send()` against Baseten GLM-5.3-Fast (rows copied
//! from the production database; key stays in the OS keyring). The browser
//! is the deterministic fake-extension fixture over extpipe using default
//! routing (no isolated profile): navigate shows a page with a single
//! "Reveal launch status" control and no status value; click flips the page
//! to expose LAUNCH-READY-88. No public internet is used.
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
const STATUS_VALUE: &str = "LAUNCH-READY-88";
const USER_TASK: &str = "Open the test page, find the ‘Reveal launch status’ control, click it, and tell me the launch status.";

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

struct FakeLog {
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
    re.captures_iter(text)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

#[tokio::test]
#[ignore]
async fn eval_real_browser_task() {
    let _guard = serial();
    assert!(
        !USER_TASK.contains(STATUS_VALUE),
        "harness must not leak the status into the prompt"
    );
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rbx-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    let data_dir = tmp.join("com.anyscapelabs.argus");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
    std::env::set_var("XDG_DATA_HOME", &tmp);

    let sock_path = data_dir.join("native.sock");
    let listener = tokio::net::UnixListener::bind(&sock_path).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(argus_lib::tools::browser::extpipe::serve_conn(stream));
        }
    });

    let log = Arc::new(FakeLog {
        methods: StdMutex::new(vec![]),
        clicks: StdMutex::new(0),
        revealed: StdMutex::new(false),
    });
    let client = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    tokio::spawn({
        let log = log.clone();
        async move {
            let (mut rd, mut wr) = client.into_split();
            while let Some(req) = read_frame(&mut rd).await {
                if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
                    continue;
                }
                let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                let method = req
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
                log.methods.lock().unwrap().push(method.clone());
                let revealed = *log.revealed.lock().unwrap();
                let resp_data = match method.as_str() {
                    "navigate" => serde_json::json!({
                        "tabId": 21,
                        "url": data.get("url").and_then(|u| u.as_str()).unwrap_or("http://eval.local/launch"),
                        "title": "Launch Console",
                        "text": "Launch console. Use the control below to reveal status.",
                    }),
                    "snapshot" => serde_json::json!([
                        {"kind": "button", "label": "Reveal launch status", "path": "button"},
                    ]),
                    "click" => {
                        *log.clicks.lock().unwrap() += 1;
                        *log.revealed.lock().unwrap() = true;
                        serde_json::json!({})
                    }
                    "read" => {
                        let text = if revealed {
                            "Launch console. Status: LAUNCH-READY-88."
                        } else {
                            "Launch console. Use the control below to reveal status."
                        };
                        serde_json::json!({
                            "url": "http://eval.local/launch",
                            "title": "Launch Console",
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

    let gtmp = std::env::temp_dir().join(format!(
        "argus-eval-rbw-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&gtmp).unwrap();
    for d in ["skills", "library", "logos"] {
        std::fs::create_dir_all(gtmp.join(d)).unwrap();
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
        skills_dir: gtmp.join("skills"),
        library_dir: gtmp.join("library"),
        logos_dir: gtmp.join("logos"),
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
        jobs_dir: std::env::temp_dir().join("argus-jobs"),
        jobs: StdMutex::new(HashMap::new()),
        events: StdMutex::new(HashMap::new()),
        turns: StdMutex::new(HashSet::new()),
        watching: StdMutex::new(HashSet::new()),
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real browser".into(),
                model_id: Some(WANT_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
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
    let methods = log.methods.lock().unwrap().clone();
    let clicks = *log.clicks.lock().unwrap();

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

    let open_msg = results.iter().find(|m| m.content.contains("browser.open"));
    let click_msg = results.iter().find(|m| m.content.contains("browser.click"));
    let open_ok = open_msg.is_some();
    let snapshot_seen = open_msg
        .map(|m| m.content.contains("Reveal launch status") && m.content.contains("(snapshot "))
        .unwrap_or(false);
    let pre_absence = open_msg
        .map(|m| !m.content.contains(STATUS_VALUE))
        .unwrap_or(false);
    let open_gen = open_msg.and_then(|m| snapshot_gens(&m.content).into_iter().next());

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
    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(STATUS_VALUE);
    let changed_ok = click_msg
        .map(|m| m.content.contains(STATUS_VALUE))
        .unwrap_or(false);
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

    eprintln!("=== REAL BROWSER EVAL TRACE ===");
    eprintln!("provider=baseten remote={remote} elapsed={elapsed:?}");
    eprintln!("status: {status:?}");
    eprintln!(
        "turns: {} results: {} attempts: {attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("event kinds: {kinds:?}");
    eprintln!("fixture methods: {methods:?} clicks={clicks}");
    eprintln!("tools used: {tools_used:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("open_ok={open_ok} snapshot_seen={snapshot_seen} pre_absence={pre_absence} open_gen={open_gen:?} click_ref={click_ref:?} click_snap={click_snap:?} gen_ok={gen_ok}");
    eprintln!("changed_ok={changed_ok} answer_ok={answer_ok}");

    let verdict = if !status.is_ok() {
        "FAIL"
    } else if !pre_absence {
        "FAIL"
    } else if !(open_ok && snapshot_seen) {
        "FAIL"
    } else if clicks != 1 || !gen_ok || click_ref != Some(0) {
        "FAIL"
    } else if !(changed_ok && answer_ok) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(verdict, "PASS", "real-model browser task did not pass");
}
