//! Real-model browser stale-reference recovery evaluation.
//!
//! Real `sessions/chat.rs::send()` against Baseten GLM-5.3-Fast (rows copied
//! from the production database; key stays in the OS keyring). Deterministic
//! fake-extension fixture over extpipe using default routing (no isolated
//! profile, no public internet).
//!
//! Flow: browser.open serves an initial page with a single Reveal control
//! (snapshot N, path reveal-v1). After the open completes, the harness flips
//! the fixture to a re-rendered layout (decoy Overview plus Reveal at a new
//! path reveal-v2) and performs one harness browser.read to advance Argus to
//! snapshot N+1. The model's first click with the old snapshot must hit
//! Argus's existing stale-reference recovery (fresh snapshot supplied once),
//! then the model must click the new ref and reveal the status.
//!
//! The status value is assembled at runtime and never appears adjacently in
//! any file, prompt, env, URL, or pre-action content.
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
const USER_TASK: &str =
    "Open the Project Status page, activate Reveal Project Status, and tell me the resulting project status.";
const REVEAL_LABEL: &str = "Reveal Project Status";

fn status_value() -> String {
    ["ORBIT", "READY", "92"].join("-")
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

fn click_calls(text: &str) -> Vec<(u64, u64)> {
    let re =
        regex::Regex::new(r#"browser\.click[^}]*"ref"\s*:\s*(\d+)[^}]*"snapshot"\s*:\s*(\d+)"#)
            .unwrap();
    let re2 =
        regex::Regex::new(r#"browser\.click[^}]*"snapshot"\s*:\s*(\d+)[^}]*"ref"\s*:\s*(\d+)"#)
            .unwrap();
    let mut out = vec![];
    for c in re.captures_iter(text) {
        if let (Some(a), Some(b)) = (
            c.get(1).and_then(|m| m.as_str().parse().ok()),
            c.get(2).and_then(|m| m.as_str().parse().ok()),
        ) {
            out.push((a, b));
        }
    }
    for c in re2.captures_iter(text) {
        if let (Some(s), Some(r)) = (
            c.get(1).and_then(|m| m.as_str().parse().ok()),
            c.get(2).and_then(|m| m.as_str().parse().ok()),
        ) {
            out.push((r, s));
        }
    }
    out
}

#[tokio::test]
#[ignore]
async fn eval_real_browser_stale_task() {
    let _guard = serial();
    let status = status_value();
    assert!(
        !USER_TASK.contains(&status),
        "harness must not leak the status into the prompt"
    );
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-rbs-{}",
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
                            "tabId": 41,
                            "url": data.get("url").and_then(|u| u.as_str()).unwrap_or("http://eval.local/project-status"),
                            "title": "Project Status",
                            "text": "Project status console. Use the control below to reveal status.",
                        }),
                        "snapshot" => {
                            s.snapshots += 1;
                            if !s.rerendered {
                                serde_json::json!([
                                    {"kind": "button", "label": REVEAL_LABEL, "path": "reveal-v1"},
                                ])
                            } else {
                                serde_json::json!([
                                    {"kind": "button", "label": "Overview", "path": "overview"},
                                    {"kind": "button", "label": REVEAL_LABEL, "path": "reveal-v2"},
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
                            if s.rerendered && path == "reveal-v2" {
                                s.revealed = true;
                            }
                            serde_json::json!({})
                        }
                        "read" => {
                            let text = if s.revealed {
                                format!("Project status console. Project status: {status}.")
                            } else {
                                "Project status console. Use the control below to reveal status."
                                    .to_string()
                            };
                            serde_json::json!({
                                "url": data.get("url").and_then(|u| u.as_str()).unwrap_or("http://eval.local/project-status"),
                                "title": "Project Status",
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
        "argus-eval-rbsw-{}",
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
                title: "Eval real browser stale".into(),
                model_id: Some(WANT_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    // Harness forced-staleness watcher: after the model's open lands, flip the
    // fixture layout and advance Argus with one harness read (N -> N+1).
    let fixture_events: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
    let watcher = tokio::spawn({
        let shared = shared.clone();
        let fixture_events = fixture_events.clone();
        async move {
            for _ in 0..200 {
                let seen_navigate = shared
                    .lock()
                    .unwrap()
                    .methods
                    .iter()
                    .any(|m| m == "navigate");
                if seen_navigate {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            {
                let mut s = shared.lock().unwrap();
                s.rerendered = true;
            }
            fixture_events
                .lock()
                .unwrap()
                .push("harness: fixture re-rendered (reveal-v1 -> overview+reveal-v2)".into());
            let out = argus_lib::tools::browser::read(&serde_json::json!({})).await;
            match out {
                Ok(text) => {
                    let gens = snapshot_gens(&text);
                    fixture_events.lock().unwrap().push(format!(
                        "harness: forced refresh read ok gens={gens:?} len={}",
                        text.len()
                    ));
                }
                Err(e) => {
                    fixture_events
                        .lock()
                        .unwrap()
                        .push(format!("harness: forced refresh read err={e}"));
                }
            }
        }
    });

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
    let _ = watcher.await;

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

    let open_idx = results
        .iter()
        .position(|m| m.content.contains("tool=\"browser.open\""));
    let open_gen = open_idx.and_then(|i| snapshot_gens(&results[i].content).into_iter().next());
    let open_has_reveal = open_idx
        .map(|i| results[i].content.contains(REVEAL_LABEL))
        .unwrap_or(false);
    let open_clean = open_idx
        .map(|i| !results[i].content.contains(&status))
        .unwrap_or(false);

    let stale_idx = results.iter().position(|m| {
        m.content.contains("tool=\"browser.click\"") && m.content.contains("stale ref")
    });
    let stale_text = stale_idx
        .map(|i| results[i].content.clone())
        .unwrap_or_default();
    let stale_gens = snapshot_gens(&stale_text);
    let stale_has_recovery = stale_text.contains("Choose the replacement ref")
        && stale_text.contains("Elements (snapshot");
    let stale_current_gen = stale_text
        .split("snapshot ")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<u64>().ok());

    let all_calls_text: String = {
        let mut t = String::new();
        for m in msgs.iter() {
            t.push_str(&m.content);
            t.push('\n');
            if let Some(c) = m.tool_calls.as_deref() {
                t.push_str(c);
                t.push('\n');
            }
        }
        t.replace('\\', "")
    };
    let clicks_arg = click_calls(&all_calls_text);
    let first_click = clicks_arg.first().copied();
    let second_click = clicks_arg.get(1).copied();

    let click_result_idxs: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, m)| m.content.contains("tool=\"browser.click\""))
        .map(|(i, _)| i)
        .collect();
    let post_click_has_status = click_result_idxs.iter().any(|&i| {
        // the successful click is the one after the stale one
        i > stale_idx.unwrap_or(usize::MAX) && results[i].content.contains(&status)
    });
    let pre_click_clean = match stale_idx {
        Some(s) => results[..s].iter().all(|m| !m.content.contains(&status)),
        None => results.iter().all(|m| !m.content.contains(&status)),
    };

    let mining = tools_used.iter().any(|t| {
        matches!(
            t.as_str(),
            "terminal"
                | "bash.run"
                | "grep"
                | "fs.write"
                | "fs.read"
                | "skill.read"
                | "skill.search"
                | "skill.create"
                | "memory.save"
                | "memory.search"
                | "memory.read"
                | "doc.create"
        ) || t.starts_with("computer.")
            || t.starts_with("web.")
    });

    let final_text = assistants
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let answer_ok = final_text.contains(&status);

    let initial_ok = open_idx.is_some() && open_has_reveal && open_clean && open_gen.is_some();
    let forced_ok = was_rerendered
        && stale_idx.is_some()
        && stale_current_gen.is_some_and(|g| Some(g) != open_gen)
        && stale_gens.contains(&stale_current_gen.unwrap_or(0));
    let new_ref_ok = match (first_click, second_click, open_gen, stale_current_gen) {
        (Some((r1, s1)), Some((r2, s2)), Some(n), Some(n1)) => {
            s1 == n && s1 != n1 && s2 == n1 && r2 != r1
        }
        _ => false,
    };
    let success_ok = fixture_clicks == 1
        && click_paths == vec!["reveal-v2".to_string()]
        && post_click_has_status
        && pre_click_clean
        && answer_ok
        && was_revealed;
    let isolation_ok = !mining && !USER_TASK.contains(&status);
    let stale_genuinely = stale_idx.is_some()
        && stale_has_recovery
        && fixture_clicks == 1
        && first_click.is_some_and(|(_, s)| Some(s) == open_gen);

    eprintln!("=== REAL BROWSER-STALE EVAL TRACE ===");
    eprintln!("provider=baseten remote={remote} elapsed={elapsed:?}");
    eprintln!("status: {status_run:?}");
    eprintln!(
        "turns: {} results: {} attempts: {attempts:?}",
        assistants.len(),
        results.len()
    );
    eprintln!("event kinds: {kinds:?}");
    eprintln!("fixture methods: {methods:?} clicks={fixture_clicks} paths={click_paths:?} snapshots={fixture_snapshots} rerendered={was_rerendered} revealed={was_revealed}");
    eprintln!("fixture events: {fixture_events:?}");
    eprintln!("tools used: {tools_used:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }
    eprintln!("open_idx={open_idx:?} open_gen={open_gen:?} open_reveal={open_has_reveal} open_clean={open_clean}");
    eprintln!("stale_idx={stale_idx:?} stale_current={stale_current_gen:?} stale_gens={stale_gens:?} recovery={stale_has_recovery}");
    eprintln!("click_args={clicks_arg:?} first={first_click:?} second={second_click:?} new_ref_ok={new_ref_ok}");
    eprintln!("pre_clean={pre_click_clean} post_status={post_click_has_status} mining={mining} answer_ok={answer_ok}");
    eprintln!("initial_ok={initial_ok} forced_ok={forced_ok} success_ok={success_ok} isolation_ok={isolation_ok} stale_genuinely={stale_genuinely}");

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
    } else if !(success_ok) {
        "PARTIAL"
    } else {
        "PASS"
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&gtmp);
    assert_eq!(
        verdict, "PASS",
        "real-model browser stale task did not pass"
    );
}
