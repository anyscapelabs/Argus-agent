//! Real-agent validation of Design A presented-watermark grounding.
//!
//! Production Baseten `zai-org/GLM-5.3-Fast` via `sessions/chat.rs::send()`.
//! The model is expected to use legacy `<browser-action>` text calls with
//! `ref` and no `snapshot`; the test verifies Argus grounds them safely.
//!
//! Fixture (fake extension, isolated per run): `browser.open` serves one
//! control at ref 0 (snapshot N). A watcher observes the session database
//! for the stored open tool-result, then deterministically flips the fixture
//! layout (ref 0 becomes a decoy, the control moves to ref 1) and performs
//! one harness `browser.read`, advancing Argus to snapshot N+1. The model's
//! token-less click on ref 0 must then be rejected by the presented
//! watermark (`shown == N != N+1`) with the existing bounded stale recovery
//! instead of executing against the decoy. The model must pick the new ref
//! from the recovery snapshot; that click reveals a runtime-generated value
//! that exists nowhere else.
//!
//! Ignored by default (`cargo test -- --ignored`). Runs exactly once per
//! invocation; never retries the task automatically.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

const PROD_DB: &str = "/home/brnx/.local/share/com.anyscapelabs.argus/argus.db";
const WANT_PROVIDER: &str = "baseten";
const WANT_MODEL: &str = "baseten/zai-org/GLM-5.3-Fast";
const CONTROL_LABEL: &str = "Tune Zephyr Relay";
const DECOY_LABEL: &str = "Zephyr Chart";

fn secret_value() -> String {
    let suffix: String = uuid::Uuid::new_v4()
        .as_simple()
        .to_string()
        .chars()
        .take(6)
        .collect::<String>()
        .to_uppercase();
    format!("{}-{suffix}", ["ZEPHYR", "TONE"].join("-"))
}

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

struct FakeShared {
    methods: Vec<String>,
    click_paths: Vec<String>,
    clicks: u32,
    rerendered: bool,
    revealed: bool,
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

fn snapshot_gens(text: &str) -> Vec<u64> {
    let re = regex::Regex::new(r"\(snapshot (\d+)\)").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

fn result_tool_name(content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"tool="([^"]+)""#).unwrap();
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

#[derive(Debug)]
struct NativeClick {
    ref_opt: Option<u64>,
    snap_opt: Option<u64>,
}

fn native_clicks(msgs: &[argus_lib::sessions::schema::Msg]) -> Vec<NativeClick> {
    let mut out = vec![];
    for m in msgs.iter().filter(|m| m.role == "assistant") {
        let Some(raw) = m.tool_calls.as_deref() else {
            continue;
        };
        let calls: Vec<argus_lib::gateway::schema::ToolCall> =
            serde_json::from_str(raw).unwrap_or_default();
        for c in calls.iter().filter(|c| c.name == "browser.click") {
            let args: serde_json::Value =
                serde_json::from_str(&c.args).unwrap_or(serde_json::Value::Null);
            out.push(NativeClick {
                ref_opt: args.get("ref").and_then(|v| v.as_u64()),
                snap_opt: args.get("snapshot").and_then(|v| v.as_u64()),
            });
        }
    }
    out
}

#[tokio::test]
#[ignore]
async fn eval_real_browser_ground_task() {
    let _guard = serial();
    let secret = secret_value();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = http_listener.local_addr().unwrap().port();
    let panel_path = format!(
        "/zephyr-{}",
        &uuid::Uuid::new_v4().as_simple().to_string()[..8]
    );
    let panel_url = format!("http://127.0.0.1:{port}{panel_path}");
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = http_listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let body = "not found";
                let resp = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    let user_task = format!(
        "Open the Zephyr Console at {panel_url}, tune the Zephyr Relay control, and tell me the resulting status."
    );
    assert!(
        !user_task.contains(&secret),
        "harness must not leak the value into the prompt"
    );

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rbg-{}",
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

    let shared = Arc::new(StdMutex::new(FakeShared {
        methods: vec![],
        click_paths: vec![],
        clicks: 0,
        rerendered: false,
        revealed: false,
    }));
    let client = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    tokio::spawn({
        let shared = shared.clone();
        let secret = secret.clone();
        let panel_url = panel_url.clone();
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
                let resp_data = {
                    let mut s = shared.lock().unwrap();
                    s.methods.push(method.clone());
                    match method.as_str() {
                        "navigate" => serde_json::json!({
                            "tabId": 61,
                            "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                            "title": "Zephyr Console",
                            "text": "Zephyr console. Use the control below.",
                        }),
                        "snapshot" => {
                            if !s.rerendered {
                                serde_json::json!([
                                    {"kind": "button", "label": CONTROL_LABEL, "path": "relay-v1"},
                                ])
                            } else {
                                serde_json::json!([
                                    {"kind": "button", "label": DECOY_LABEL, "path": "chart"},
                                    {"kind": "button", "label": CONTROL_LABEL, "path": "relay-v2"},
                                ])
                            }
                        }
                        "click" => {
                            let path = data
                                .get("path")
                                .and_then(|p| p.as_str())
                                .unwrap_or("")
                                .to_string();
                            s.clicks += 1;
                            s.click_paths.push(path.clone());
                            if s.rerendered && path == "relay-v2" {
                                s.revealed = true;
                            }
                            serde_json::json!({})
                        }
                        "read" => {
                            let text = if s.revealed {
                                format!("Zephyr console. Status: {secret}.")
                            } else {
                                "Zephyr console. Use the control below.".to_string()
                            };
                            serde_json::json!({
                                "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                                "title": "Zephyr Console",
                                "text": text,
                            })
                        }
                        _ => serde_json::json!({}),
                    }
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
        "argus-eval-rbgw-{}",
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
    };
    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real browser grounding".into(),
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

    let done = Arc::new(AtomicBool::new(false));
    let fixture_events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let t0 = std::time::Instant::now();
    let (status_run, _) = tokio::join!(
        async {
            let r =
                argus_lib::sessions::chat::send(&gw, &handle, &session_id, &user_task, &chan).await;
            done.store(true, Ordering::SeqCst);
            r
        },
        async {
            loop {
                if done.load(Ordering::SeqCst) {
                    break;
                }
                let saw_open = {
                    let conn = gw.conn.lock().unwrap();
                    argus_lib::sessions::store::list_msgs(&conn, &session_id)
                        .map(|msgs| {
                            msgs.iter().any(|m| {
                                m.content.contains("tool=\"browser.open\"")
                                    && m.content.contains(CONTROL_LABEL)
                            })
                        })
                        .unwrap_or(false)
                };
                if saw_open {
                    shared.lock().unwrap().rerendered = true;
                    fixture_events.lock().unwrap().push(format!(
                        "harness: open tool-result observed at {:?}; layout flipped",
                        t0.elapsed()
                    ));
                    match argus_lib::tools::browser::read(&serde_json::json!({})).await {
                        Ok(text) => fixture_events.lock().unwrap().push(format!(
                            "harness: refresh read ok gens={:?} at {:?}",
                            snapshot_gens(&text),
                            t0.elapsed()
                        )),
                        Err(e) => fixture_events
                            .lock()
                            .unwrap()
                            .push(format!("harness: refresh read err={e}")),
                    }
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
    );
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
    let snap = shared.lock().unwrap();
    let methods = snap.methods.clone();
    let click_paths = snap.click_paths.clone();
    let fixture_clicks = snap.clicks;
    let was_rerendered = snap.rerendered;
    let was_revealed = snap.revealed;
    drop(snap);
    let fixture_events = fixture_events.lock().unwrap().clone();

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

    let result_tools: Vec<Option<String>> = results
        .iter()
        .map(|m| result_tool_name(&m.content))
        .collect();
    let open_pos = result_tools
        .iter()
        .position(|t| t.as_deref() == Some("browser.open"));
    let click_positions: Vec<usize> = result_tools
        .iter()
        .enumerate()
        .filter(|(_, t)| t.as_deref() == Some("browser.click"))
        .map(|(i, _)| i)
        .collect();

    let open_gen = open_pos.and_then(|i| snapshot_gens(&results[i].content).into_iter().next());
    let open_single = open_pos.map(|i| {
        results[i]
            .content
            .contains("[0] button \"Tune Zephyr Relay\"")
            && !results[i].content.contains("[1]")
    });
    let open_clean = open_pos
        .map(|i| !results[i].content.contains(&secret))
        .unwrap_or(false);

    let stale_pos = click_positions
        .iter()
        .copied()
        .find(|&i| results[i].content.contains("stale ref"));
    let stale_text = stale_pos
        .map(|i| results[i].content.clone())
        .unwrap_or_default();
    let stale_has_recovery = stale_text.contains("Choose the replacement ref")
        && stale_text.contains("Elements (snapshot");
    let stale_current: Option<u64> = stale_text.split("is current").next().and_then(|head| {
        head.rsplit("snapshot ")
            .next()?
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    });
    let recovery_new_ref = stale_text
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix('[')?.split(']').next()?;
            let n: u64 = rest.parse().ok()?;
            t.contains(CONTROL_LABEL).then_some(n)
        })
        .next();

    let native = native_clicks(&msgs);
    let supplied_snapshot = native.iter().any(|c| c.snap_opt.is_some());
    let first_native_ref = native.first().and_then(|c| c.ref_opt);
    let success_pos = click_positions
        .iter()
        .copied()
        .find(|&i| results[i].content.contains(&secret));
    let pre_success_clean = match success_pos {
        Some(p) => results[..p].iter().all(|m| !m.content.contains(&secret)),
        None => results.iter().all(|m| !m.content.contains(&secret)),
    };

    let forbidden = [
        "terminal",
        "bash.run",
        "grep",
        "fs.read",
        "fs.write",
        "memory.save",
        "memory.search",
        "memory.read",
        "skill.read",
        "skill.search",
        "skill.create",
        "doc.create",
    ];
    let result_names: Vec<String> = result_tools.iter().filter_map(|t| t.clone()).collect();
    let mining = result_names.iter().any(|t| {
        forbidden.contains(&t.as_str()) || t.starts_with("computer.") || t.starts_with("web.")
    });
    let unexpected_browser = result_names.iter().any(|t| {
        !matches!(
            t.as_str(),
            "browser.open" | "browser.click" | "browser.read"
        )
    });

    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(&secret);

    let initial_ok =
        open_pos == Some(0) && open_single.unwrap_or(false) && open_clean && open_gen.is_some();
    let drift_ok =
        was_rerendered && stale_pos.is_some() && stale_current.is_some_and(|g| Some(g) != open_gen);
    let zero_exec_ok = click_paths == vec!["relay-v2".to_string()];
    let recovery_ok = stale_has_recovery && recovery_new_ref == Some(1);
    let single_activation_ok = fixture_clicks == 1 && was_revealed;
    let exclusive_ok =
        success_pos.is_some_and(|p| Some(p) > stale_pos) && pre_success_clean && answer_ok;
    let isolation_ok = !mining && !unexpected_browser && !user_task.contains(&secret);

    eprintln!("=== REAL BROWSER-GROUND EVAL TRACE (COMPLETE HEADER) ===");
    eprintln!("provider=baseten remote={remote} model={WANT_MODEL}");
    eprintln!(
        "total_elapsed={elapsed:?} turns={} results={} attempts={attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("panel_url={panel_url}");
    eprintln!("status_run={status_run:?}");
    eprintln!("event_kinds={kinds:?} stream_events={}", events.len());
    eprintln!("fixture_methods={methods:?}");
    eprintln!("fixture_clicks={fixture_clicks} click_paths={click_paths:?} rerendered={was_rerendered} revealed={was_revealed}");
    eprintln!("fixture_events={fixture_events:?}");
    eprintln!("result_tools={result_tools:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!(
            "--- assistant[{i}] native={:?}\n{}",
            m.tool_calls.as_deref().unwrap_or("none"),
            short(&m.content)
        );
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("open_pos={open_pos:?} open_gen={open_gen:?} open_single={open_single:?} open_clean={open_clean}");
    eprintln!("stale_pos={stale_pos:?} stale_current={stale_current:?} has_recovery={stale_has_recovery} recovery_new_ref={recovery_new_ref:?}");
    eprintln!("native_clicks={native:?} supplied_snapshot={supplied_snapshot} first_native_ref={first_native_ref:?}");
    eprintln!("success_pos={success_pos:?} pre_clean={pre_success_clean} answer_ok={answer_ok}");
    eprintln!("initial_ok={initial_ok} drift_ok={drift_ok} zero_exec_ok={zero_exec_ok} recovery_ok={recovery_ok}");
    eprintln!("single_activation_ok={single_activation_ok} exclusive_ok={exclusive_ok} isolation_ok={isolation_ok}");
    eprintln!("=== REAL BROWSER-GROUND EVAL TRACE (COMPLETE TAIL) ===");

    let verdict = if !status_run.is_ok() {
        "FAIL"
    } else if !initial_ok || !pre_success_clean {
        "FAIL"
    } else if !drift_ok || !stale_has_recovery {
        "FAIL"
    } else if !zero_exec_ok || !recovery_ok {
        "FAIL"
    } else if !isolation_ok {
        "FAIL"
    } else if !(single_activation_ok && exclusive_ok) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(
        verdict, "PASS",
        "real-model browser grounding task did not pass"
    );
}
