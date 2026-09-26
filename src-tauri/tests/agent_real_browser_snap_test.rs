//! Real-model browser snapshot-token compliance evaluation (TEST 9).
//!
//! Diagnostic, NOT a stale-recovery test: nothing is deliberately invalidated.
//! Real `sessions/chat.rs::send()` against Baseten GLM-5.3-Fast (rows copied
//! from the production database; key stays in the OS keyring). Deterministic
//! fake-extension fixture over extpipe with default routing (no public
//! internet; a dummy localhost listener only reserves the dynamic port and
//! never serves the value).
//!
//! Flow: open -> snapshot N with one control (Activate Finch Console) ->
//! click (must carry snapshot=N) -> snapshot N+1 with the second control
//! (Continue to Status) -> click (must carry snapshot=N+1) -> final text
//! reveals the value. The value is assembled at runtime and never appears
//! adjacently in any file, prompt, env, URL, or pre-action content.
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
const FIRST_LABEL: &str = "Activate Finch Console";
const SECOND_LABEL: &str = "Continue to Status";

fn status_value() -> String {
    ["SNAPSHOT", "OK", "57"].join("-")
}

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

struct FakeShared {
    methods: Vec<String>,
    click_paths: Vec<String>,
    clicks: u32,
    snapshots: u32,
    step: u32,
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
struct ClickAttempt {
    turn: usize,
    source: &'static str,
    ref_opt: Option<u64>,
    snap_opt: Option<u64>,
}

fn attr_in(tag: &str, key: &str) -> Option<String> {
    for q in ['"', '\''] {
        let pat = format!("{key}={q}");
        if let Some(i) = tag.find(&pat) {
            let rest = &tag[i + pat.len()..];
            if let Some(e) = rest.find(q) {
                return Some(rest[..e].to_string());
            }
        }
    }
    None
}

fn num_in_body(body: &str, key: &str) -> Option<u64> {
    for pat in [
        format!(r#""{key}"\s*:\s*(\d+)"#),
        format!(r#"{key}\s*=\s*(\d+)"#),
        format!(r#"{key}\s+(\d+)"#),
    ] {
        if let Ok(re) = regex::Regex::new(&pat) {
            if let Some(c) = re.captures(body) {
                if let Some(m) = c.get(1).and_then(|m| m.as_str().parse().ok()) {
                    return Some(m);
                }
            }
        }
    }
    None
}

/// Structured native tool_calls JSON first; fall back to parsed XML action
/// attributes/bodies in assistant turn order (no substring tool ordering).
fn click_attempts(msgs: &[argus_lib::sessions::schema::Msg]) -> Vec<ClickAttempt> {
    let mut out = vec![];
    let action_re =
        regex::Regex::new(r#"(?s)<action\b[^>]*tool="browser\.click"[^>]*>(.*?)</action>"#)
            .unwrap();
    let bact_re =
        regex::Regex::new(r#"(?s)<browser-action\b([^>]*)>(.*?)</browser-action>"#).unwrap();
    for (turn, m) in msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "assistant")
    {
        if let Some(raw) = m.tool_calls.as_deref() {
            let calls: Vec<argus_lib::gateway::schema::ToolCall> =
                serde_json::from_str(raw).unwrap_or_default();
            for c in calls.iter().filter(|c| c.name == "browser.click") {
                let args: serde_json::Value =
                    serde_json::from_str(&c.args).unwrap_or(serde_json::Value::Null);
                out.push(ClickAttempt {
                    turn,
                    source: "native",
                    ref_opt: args.get("ref").and_then(|v| v.as_u64()),
                    snap_opt: args.get("snapshot").and_then(|v| v.as_u64()),
                });
            }
        }
        for cap in action_re.captures_iter(&m.content) {
            let body = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let args: serde_json::Value = serde_json::from_str(body.trim()).unwrap_or_default();
            let ref_opt = args
                .get("ref")
                .and_then(|v| v.as_u64())
                .or_else(|| num_in_body(body, "ref"));
            let snap_opt = args
                .get("snapshot")
                .and_then(|v| v.as_u64())
                .or_else(|| num_in_body(body, "snapshot"));
            out.push(ClickAttempt {
                turn,
                source: "action-tag",
                ref_opt,
                snap_opt,
            });
        }
        for cap in bact_re.captures_iter(&m.content) {
            let tag = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let is_click = attr_in(tag, "action").is_some_and(|a| a == "browser.click")
                || body.contains("browser.click");
            if !is_click {
                continue;
            }
            let ref_opt = attr_in(tag, "ref")
                .and_then(|v| v.parse().ok())
                .or_else(|| num_in_body(body, "ref"));
            let snap_opt = attr_in(tag, "snapshot")
                .and_then(|v| v.parse().ok())
                .or_else(|| num_in_body(body, "snapshot"));
            out.push(ClickAttempt {
                turn,
                source: "browser-action",
                ref_opt,
                snap_opt,
            });
        }
    }
    out
}

#[tokio::test]
#[ignore]
async fn eval_real_browser_snap_task() {
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
        "Open the Finch Console at {panel_url}, activate it, then continue to Status and tell me the resulting status."
    );
    assert!(
        !user_task.contains(&status),
        "harness must not leak the status into the prompt"
    );

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rbsnap-{}",
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
        step: 0,
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
                            "tabId": 61,
                            "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                            "title": "Finch Console",
                            "text": "Finch console. Use the control below.",
                        }),
                        "snapshot" => {
                            s.snapshots += 1;
                            match s.step {
                                0 => serde_json::json!([
                                    {"kind": "button", "label": FIRST_LABEL, "path": "finch-v1"},
                                ]),
                                _ => serde_json::json!([
                                    {"kind": "button", "label": SECOND_LABEL, "path": "status-v2"},
                                ]),
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
                            if s.step == 0 && path == "finch-v1" {
                                s.step = 1;
                            } else if s.step == 1 && path == "status-v2" {
                                s.step = 2;
                            }
                            serde_json::json!({})
                        }
                        "read" => {
                            let text = match s.step {
                                0 => "Finch console. Use the control below.".to_string(),
                                1 => "Finch console. Continue to Status.".to_string(),
                                _ => format!("Finch console. Status: {status}."),
                            };
                            serde_json::json!({
                                "url": data.get("url").and_then(|u| u.as_str()).unwrap_or(&panel_url),
                                "title": "Finch Console",
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
        "argus-eval-rbsnapw-{}",
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
                title: "Eval real browser snapshot compliance".into(),
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
    let t0 = std::time::Instant::now();
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

    // Single evaluation run; no task retry.
    let status_run =
        argus_lib::sessions::chat::send(&gw, &handle, &session_id, &user_task, &chan, "user").await;
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
    let fixture_snapshots = snap.snapshots;
    let final_step = snap.step;
    drop(snap);

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

    // Structured ordering via tool="..." result attributes.
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
    let open_has_first = open_pos
        .map(|i| results[i].content.contains(FIRST_LABEL))
        .unwrap_or(false);
    let open_clean = open_pos
        .map(|i| !results[i].content.contains(&status))
        .unwrap_or(false);

    let first_click_pos = click_positions.first().copied();
    let second_click_pos = click_positions.get(1).copied();
    let second_gen =
        first_click_pos.and_then(|i| snapshot_gens(&results[i].content).into_iter().next());
    let second_has_control = first_click_pos
        .map(|i| results[i].content.contains(SECOND_LABEL))
        .unwrap_or(false);

    let attempts_parsed = click_attempts(&msgs);
    let first_attempt = attempts_parsed.first().cloned();
    let second_attempt = attempts_parsed.get(1).cloned();
    let used_native = attempts_parsed.iter().any(|a| a.source == "native");
    let used_xml = attempts_parsed
        .iter()
        .any(|a| a.source == "action-tag" || a.source == "browser-action");

    let no_stale = results.iter().all(|m| !m.content.contains("stale ref"));
    let success_pos = click_positions
        .iter()
        .copied()
        .find(|&i| results[i].content.contains(&status));
    let pre_second_clean = match success_pos {
        Some(p) => results[..p].iter().all(|m| !m.content.contains(&status)),
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
    let mining = result_names.iter().any(|t| {
        forbidden.contains(&t.as_str()) || t.starts_with("computer.") || t.starts_with("web.")
    });

    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(&status);

    let open_first_ok = open_pos == Some(0) && open_has_first && open_clean;
    let click1_ok = match (&first_attempt, open_gen) {
        (Some(a), Some(n)) => a.ref_opt == Some(0) && a.snap_opt == Some(n),
        _ => false,
    };
    let first_action_ok = first_click_pos.is_some_and(|p| Some(p) > open_pos)
        && second_gen.is_some_and(|g| Some(g) != open_gen)
        && second_has_control;
    let click2_ok = match (&second_attempt, &second_gen) {
        (Some(a), Some(n)) => a.ref_opt == Some(0) && a.snap_opt == Some(*n),
        _ => false,
    };
    let both_snapshots_ok = click1_ok && click2_ok;
    let one_per_control =
        fixture_clicks == 2 && click_paths == vec!["finch-v1".to_string(), "status-v2".to_string()];
    let status_exclusive =
        success_pos.is_some_and(|p| second_click_pos.is_some_and(|c| p == c)) && pre_second_clean;
    let isolation_ok = !mining && !user_task.contains(&status);
    let loop_ok = status_run.is_ok();

    eprintln!("=== REAL BROWSER-SNAPSHOT EVAL TRACE (COMPLETE HEADER) ===");
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
    eprintln!("fixture_clicks={fixture_clicks} click_paths={click_paths:?} snapshots={fixture_snapshots} final_step={final_step}");
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
    eprintln!("open_pos={open_pos:?} initial_generation={open_gen:?} open_first={open_has_first} open_clean={open_clean}");
    eprintln!("click_attempts={attempts_parsed:?}");
    eprintln!("first_click_pos={first_click_pos:?} second_generation={second_gen:?} second_control={second_has_control}");
    eprintln!("second_click_pos={second_click_pos:?} success_pos={success_pos:?}");
    eprintln!(
        "click_1_ref={:?} click_1_snapshot={:?} click_1_ok={click1_ok}",
        first_attempt.as_ref().and_then(|a| a.ref_opt),
        first_attempt.as_ref().and_then(|a| a.snap_opt)
    );
    eprintln!(
        "click_2_ref={:?} click_2_snapshot={:?} click_2_ok={click2_ok}",
        second_attempt.as_ref().and_then(|a| a.ref_opt),
        second_attempt.as_ref().and_then(|a| a.snap_opt)
    );
    eprintln!(
        "click_1_snapshot == initial_generation: {}",
        matches!((first_attempt.as_ref().and_then(|a| a.snap_opt), open_gen), (Some(a), Some(n)) if a == n)
    );
    eprintln!(
        "click_2_snapshot == second_generation: {}",
        matches!((second_attempt.as_ref().and_then(|a| a.snap_opt), second_gen), (Some(a), Some(n)) if a == n)
    );
    eprintln!("used_native={used_native} used_xml={used_xml} no_stale={no_stale} one_per_control={one_per_control}");
    eprintln!("pre_second_clean={pre_second_clean} status_exclusive={status_exclusive} mining={mining} answer_ok={answer_ok}");
    eprintln!("open_first_ok={open_first_ok} first_action_ok={first_action_ok} both_snapshots_ok={both_snapshots_ok} isolation_ok={isolation_ok} loop_ok={loop_ok}");
    eprintln!("=== REAL BROWSER-SNAPSHOT EVAL TRACE (COMPLETE TAIL) ===");

    let verdict = if !loop_ok {
        "FAIL"
    } else if !open_first_ok || !pre_second_clean {
        "FAIL"
    } else if !isolation_ok || !no_stale {
        "FAIL"
    } else if !both_snapshots_ok {
        "FAIL"
    } else if !(first_action_ok && one_per_control && status_exclusive && answer_ok) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(
        verdict, "PASS",
        "real-model browser snapshot task did not pass"
    );
}
