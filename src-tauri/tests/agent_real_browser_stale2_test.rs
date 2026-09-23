//! Real-model browser stale-reference recovery evaluation (8B).
//!
//! Corrected replacement for the failed Test 8. Real `sessions/chat.rs::send()`
//! against Baseten GLM-5.3-Fast (rows copied from the production database; key
//! stays in the OS keyring). Deterministic fake-extension fixture over extpipe
//! with default routing (no public internet; a dummy localhost listener only
//! reserves the dynamic port and never serves the status).
//!
//! Task (URL substituted at runtime): open the Project Finch control panel at
//! a dynamic localhost URL, activate the Finch Console control, report status.
//! Initial snapshot N has the control at R1; after the open lands, the harness
//! flips the fixture layout and performs one harness browser.read (N -> N+1,
//! control moves to R2). The model's first click with N must hit Argus's
//! existing stale-reference recovery, then click R2 from N+1 and reveal.
//!
//! The final value is assembled at runtime and never appears adjacently in any
//! file, prompt, env, URL, or pre-action content.
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
const CONTROL_LABEL: &str = "Activate Finch Console";

fn status_value() -> String {
    ["FINCH", "RECOVERED", "42"].join("-")
}

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

struct FakeShared {
    methods: Vec<String>,
    click_paths: Vec<String>,
    clicks: u32,
    snapshots: u32,
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

#[derive(Debug, Clone)]
struct NativeCall {
    turn: usize,
    name: String,
    ref_opt: Option<u64>,
    snap_opt: Option<u64>,
}

fn native_calls(msgs: &[argus_lib::sessions::schema::Msg]) -> Vec<NativeCall> {
    let mut out = vec![];
    for (turn, m) in msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "assistant")
    {
        let Some(raw) = m.tool_calls.as_deref() else {
            continue;
        };
        let calls: Vec<argus_lib::gateway::schema::ToolCall> =
            serde_json::from_str(raw).unwrap_or_default();
        for c in calls {
            let args: serde_json::Value =
                serde_json::from_str(&c.args).unwrap_or(serde_json::Value::Null);
            out.push(NativeCall {
                turn,
                name: c.name,
                ref_opt: args.get("ref").and_then(|v| v.as_u64()),
                snap_opt: args.get("snapshot").and_then(|v| v.as_u64()),
            });
        }
    }
    out
}

#[tokio::test]
#[ignore]
async fn eval_real_browser_stale2_task() {
    let _guard = serial();
    let status = status_value();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    // Dynamic localhost URL reserved at runtime; never written to any file.
    let http_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = http_listener.local_addr().unwrap().port();
    let panel_path = format!(
        "/finch-{}",
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
        "Open the Project Finch control panel at {panel_url}, activate the Finch Console control, and tell me the resulting status."
    );
    assert!(
        !user_task.contains(&status),
        "harness must not leak the status into the prompt"
    );

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rbs2-{}",
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
        snapshots: 0,
        rerendered: false,
        revealed: false,
    }));
    let client = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    tokio::spawn({
        let shared = shared.clone();
        let status = status.clone();
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
                            "tabId": 51,
                            "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                            "title": "Finch Control Panel",
                            "text": "Finch control panel. Use the control below.",
                        }),
                        "snapshot" => {
                            s.snapshots += 1;
                            if !s.rerendered {
                                serde_json::json!([
                                    {"kind": "button", "label": CONTROL_LABEL, "path": "finch-v1"},
                                ])
                            } else {
                                serde_json::json!([
                                    {"kind": "button", "label": "Overview", "path": "overview"},
                                    {"kind": "button", "label": CONTROL_LABEL, "path": "finch-v2"},
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
                            if s.rerendered && path == "finch-v2" {
                                s.revealed = true;
                            }
                            serde_json::json!({})
                        }
                        "read" => {
                            let text = if s.revealed {
                                format!("Finch control panel. Status: {status}.")
                            } else {
                                "Finch control panel. Use the control below.".to_string()
                            };
                            serde_json::json!({
                                "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                                "title": "Finch Control Panel",
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
        "argus-eval-rbs2w-{}",
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
                title: "Eval real browser stale2".into(),
                model_id: Some(WANT_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    // Deterministic invalidation: once the open's snapshot has been served,
    // flip the layout and advance Argus with exactly one harness read.
    let fixture_events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let watcher = tokio::spawn({
        let shared = shared.clone();
        let fixture_events = fixture_events.clone();
        async move {
            let mut saw_open_snapshot = false;
            for _ in 0..4800 {
                let (navigates, snapshots) = {
                    let s = shared.lock().unwrap();
                    (
                        s.methods.iter().filter(|m| *m == "navigate").count(),
                        s.methods.iter().filter(|m| *m == "snapshot").count(),
                    )
                };
                if navigates >= 1 && snapshots >= 1 {
                    saw_open_snapshot = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            if !saw_open_snapshot {
                fixture_events
                    .lock()
                    .unwrap()
                    .push("harness: no open snapshot observed; no invalidation".into());
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            {
                shared.lock().unwrap().rerendered = true;
            }
            fixture_events
                .lock()
                .unwrap()
                .push("harness: fixture re-rendered (finch-v1 -> overview+finch-v2)".into());
            match argus_lib::tools::browser::read(&serde_json::json!({})).await {
                Ok(text) => fixture_events.lock().unwrap().push(format!(
                    "harness: forced refresh read ok gens={:?} len={}",
                    snapshot_gens(&text),
                    text.len()
                )),
                Err(e) => fixture_events
                    .lock()
                    .unwrap()
                    .push(format!("harness: forced refresh read err={e}")),
            }
        }
    });

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let event_times: Arc<StdMutex<Vec<std::time::Duration>>> = Arc::new(StdMutex::new(vec![]));
    let t0 = std::time::Instant::now();
    let chan = tauri::ipc::Channel::new({
        let events = events.clone();
        let event_times = event_times.clone();
        let t0 = t0.clone();
        move |body: tauri::ipc::InvokeResponseBody| {
            let s = match body {
                tauri::ipc::InvokeResponseBody::Json(s) => s,
                tauri::ipc::InvokeResponseBody::Raw(b) => String::from_utf8_lossy(&b).into_owned(),
            };
            events.lock().unwrap().push(s);
            event_times.lock().unwrap().push(t0.elapsed());
            Ok(())
        }
    });

    let status_run =
        argus_lib::sessions::chat::send(&gw, &handle, &session_id, &user_task, &chan, "user").await;
    let elapsed = t0.elapsed();
    let _ = watcher.await;

    let msgs = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &session_id).unwrap()
    };
    let events = events.lock().unwrap().clone();
    let event_times = event_times.lock().unwrap().clone();
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
    let fixture_snapshots = snap.snapshots;
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

    // Structured ordering via tool="..." attributes (no substring ordering).
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
    let open_has_control = open_pos
        .map(|i| results[i].content.contains(CONTROL_LABEL))
        .unwrap_or(false);
    let open_clean = open_pos
        .map(|i| !results[i].content.contains(&status))
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
    let stale_current_gen: Option<u64> = stale_text.split("is current").next().and_then(|head| {
        head.rsplit("snapshot ")
            .next()?
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    });
    let recovery_gen = snapshot_gens(&stale_text).into_iter().last();

    // Structured native args (no substring matching for ordering/refs).
    let natives = native_calls(&msgs);
    let native_clicks: Vec<&NativeCall> = natives
        .iter()
        .filter(|c| c.name == "browser.click")
        .collect();
    let native_opens: Vec<&NativeCall> = natives
        .iter()
        .filter(|c| c.name == "browser.open")
        .collect();
    let first_click = native_clicks.first().copied();
    let second_click = native_clicks.get(1).copied();

    let success_pos = click_positions
        .iter()
        .copied()
        .find(|&i| results[i].content.contains(&status));
    let pre_click_clean = match stale_pos.or(success_pos) {
        Some(bound) => results[..bound]
            .iter()
            .all(|m| !m.content.contains(&status)),
        None => results.iter().all(|m| !m.content.contains(&status)),
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
    let native_names: Vec<String> = natives.iter().map(|c| c.name.clone()).collect();
    let mining = result_names.iter().any(|t| {
        forbidden.contains(&t.as_str()) || t.starts_with("computer.") || t.starts_with("web.")
    }) || native_names.iter().any(|t| {
        forbidden.contains(&t.as_str()) || t.starts_with("computer.") || t.starts_with("web.")
    });

    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(&status);

    let initial_ok = open_pos == Some(0) && open_has_control && open_clean && open_gen.is_some();
    let forced_ok = was_rerendered
        && stale_pos.is_some()
        && stale_current_gen.is_some_and(|g| Some(g) != open_gen)
        && recovery_gen == stale_current_gen;
    let new_ref_ok = match (first_click, second_click, open_gen, stale_current_gen) {
        (Some(f), Some(s), Some(n), Some(n1)) => {
            f.snap_opt == Some(n)
                && f.snap_opt != Some(n1)
                && s.snap_opt == Some(n1)
                && s.ref_opt != f.ref_opt
        }
        _ => false,
    };
    let single_activation = fixture_clicks == 1 && click_paths == vec!["finch-v2".to_string()];
    let success_ok = single_activation
        && success_pos.is_some_and(|p| Some(p) > stale_pos)
        && pre_click_clean
        && answer_ok
        && was_revealed;
    let isolation_ok = !mining && !user_task.contains(&status);
    let stale_genuinely = stale_pos.is_some()
        && stale_has_recovery
        && first_click.is_some_and(|f| f.snap_opt == open_gen);

    eprintln!("=== REAL BROWSER-STALE2 EVAL TRACE (COMPLETE HEADER) ===");
    eprintln!("provider=baseten remote={remote} model={WANT_MODEL}");
    eprintln!(
        "total_elapsed={elapsed:?} turns={} results={} attempts={attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("panel_url={panel_url}");
    eprintln!("status_run={status_run:?}");
    eprintln!(
        "event_kinds={kinds:?} stream_events={} last_event_t={:?}",
        events.len(),
        event_times.last()
    );
    eprintln!("fixture_methods={methods:?}");
    eprintln!("fixture_clicks={fixture_clicks} click_paths={click_paths:?} snapshots={fixture_snapshots} rerendered={was_rerendered} revealed={was_revealed}");
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
    eprintln!("open_pos={open_pos:?} open_gen={open_gen:?} open_control={open_has_control} open_clean={open_clean}");
    eprintln!("native_opens={native_opens:?}");
    eprintln!("native_clicks={native_clicks:?}");
    eprintln!("stale_pos={stale_pos:?} stale_current={stale_current_gen:?} recovery_gen={recovery_gen:?} has_recovery={stale_has_recovery}");
    eprintln!("success_pos={success_pos:?} pre_clean={pre_click_clean} mining={mining} answer_ok={answer_ok}");
    eprintln!("initial_ok={initial_ok} forced_ok={forced_ok} new_ref_ok={new_ref_ok} single_activation={single_activation} success_ok={success_ok} isolation_ok={isolation_ok} stale_genuinely={stale_genuinely}");
    eprintln!("initial_generation={open_gen:?} stale_generation={:?} recovery_generation={recovery_gen:?} initial_ref={:?} recovered_ref={:?}",
        first_click.and_then(|f| f.snap_opt),
        first_click.and_then(|f| f.ref_opt),
        second_click.and_then(|s| s.ref_opt));
    eprintln!("=== REAL BROWSER-STALE2 EVAL TRACE (COMPLETE TAIL) ===");

    let verdict = if !status_run.is_ok() {
        "FAIL"
    } else if !initial_ok || !pre_click_clean {
        "FAIL"
    } else if !forced_ok || !stale_has_recovery {
        "FAIL"
    } else if !new_ref_ok {
        "FAIL"
    } else if !isolation_ok {
        "FAIL"
    } else if !success_ok {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(
        verdict, "PASS",
        "real-model browser stale2 task did not pass"
    );
}
