//! Real-model agent evaluation (single task).
//!
//! Unlike agent_eval_test.rs (scripted mock model), this drives the real
//! `sessions/chat.rs::send()` loop against a real model: local Ollama
//! (http://127.0.0.1:11435, model qwen2.5:3b). No production code is
//! modified for this test, no tool calls are injected, no results are
//! faked, and the task is never retried automatically.
//!
//! Ignored by default (`cargo test -- --ignored`) because it needs the
//! local model daemon plus CPU inference time.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

const REAL_BASE: &str = "http://127.0.0.1:11435/v1";
const REAL_REMOTE: &str = "qwen2.5:3b";
const REAL_MODEL: &str = "real/test";
const TARGET: &str = "/tmp/argus-eval-1.txt";
const EXPECTED: &str = "ARGUS-REAL-731";
const USER_TASK: &str = "Create a file at /tmp/argus-eval-1.txt containing exactly ARGUS-REAL-731, then verify that the file exists and contains exactly that text. Once verified, tell me the result.";

fn short(s: &str) -> String {
    s.chars().take(1500).collect()
}

#[tokio::test]
#[ignore]
async fn eval_real_terminal_task() {
    let probe = reqwest::Client::new()
        .get("http://127.0.0.1:11435/api/tags")
        .send()
        .await
        .expect("real-model daemon unreachable at 127.0.0.1:11435");
    let tags = probe.text().await.unwrap_or_default();
    assert!(
        tags.contains("qwen2.5:3b"),
        "qwen2.5:3b not served by local daemon"
    );

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-real-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let skills_dir = tmp.join("skills");
    let library_dir = tmp.join("library");
    let logos_dir = tmp.join("logos");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::create_dir_all(&logos_dir).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    argus_lib::gateway::store::upsert_provider(
        &conn,
        &Provider {
            id: "real".into(),
            name: "Real".into(),
            compatible: "openAI".into(),
            base_url: REAL_BASE.into(),
            api_key_ref: None,
            connected: true,
            free: true,
            priority: 0,
            logo_url: None,
            doc_url: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_connected(&conn, "real", true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: REAL_MODEL.into(),
            display_name: "Real Test".into(),
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: REAL_MODEL.into(),
            provider_id: "real".into(),
            remote_model_id: REAL_REMOTE.into(),
            cost_in: 0.0,
            cost_out: 0.0,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, REAL_MODEL, true).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir,
        library_dir,
        logos_dir,
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };

    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real terminal".into(),
                model_id: Some(REAL_MODEL.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search: false,
            },
        )
        .unwrap()
        .id
    };

    let _ = std::fs::remove_file(TARGET);

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

    let status = argus_lib::sessions::chat::send(&gw, &handle, &session_id, USER_TASK, &chan).await;

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

    let assistants: Vec<&argus_lib::sessions::schema::Msg> =
        msgs.iter().filter(|m| m.role == "assistant").collect();
    let results: Vec<&argus_lib::sessions::schema::Msg> = msgs
        .iter()
        .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
        .collect();
    let kinds: std::collections::HashSet<String> = events
        .iter()
        .filter_map(|e| {
            e.split("\"type\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();

    eprintln!("=== REAL EVAL TRACE ===");
    eprintln!("status: {status:?}");
    eprintln!("turns(assistant msgs): {}", assistants.len());
    eprintln!("tool results: {}", results.len());
    eprintln!("llm attempts: {attempts:?}");
    eprintln!("event kinds: {kinds:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }

    let on_disk: Option<Vec<u8>> = std::fs::read(TARGET).ok();
    eprintln!(
        "on-disk bytes: {:?}",
        on_disk
            .as_ref()
            .map(|b| (b.len(), String::from_utf8_lossy(b).into_owned()))
    );
    let content_ok = on_disk.as_deref() == Some(EXPECTED.as_bytes());

    let verified_by_model = results.len() >= 2
        || assistants
            .last()
            .map(|m| {
                let c = m.content.to_lowercase();
                c.contains("verif") || c.contains("confirm") || c.contains("exists")
            })
            .unwrap_or(false);
    let verdict = match (&status, content_ok, verified_by_model) {
        (Ok(_), true, true) => "PASS",
        (Ok(_), true, false) => "PARTIAL",
        _ => "FAIL",
    };
    eprintln!("content_ok={content_ok} verified_by_model={verified_by_model} verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(verdict, "PASS", "real-model terminal task did not pass");
}

const SRC_2: &str = "/tmp/argus-eval-2-source.txt";
const DST_2: &str = "/tmp/argus-eval-2-result.txt";
const FACT_2: &str = "BLUEBIRD-42";
const USER_TASK_2: &str = "Find the file /tmp/argus-eval-2-source.txt, read its contents, and create /tmp/argus-eval-2-result.txt containing exactly the same contents. Then verify the result file matches the source file exactly and tell me what you found.";

#[tokio::test]
#[ignore]
async fn eval_real_copy_task() {
    let probe = reqwest::Client::new()
        .get("http://127.0.0.1:11435/api/tags")
        .send()
        .await
        .expect("real-model daemon unreachable at 127.0.0.1:11435");
    let tags = probe.text().await.unwrap_or_default();
    assert!(
        tags.contains("qwen2.5:3b"),
        "qwen2.5:3b not served by local daemon"
    );

    std::fs::write(SRC_2, FACT_2).unwrap();
    let _ = std::fs::remove_file(DST_2);
    assert!(std::fs::read(SRC_2).unwrap() == FACT_2.as_bytes());

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-real2-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let skills_dir = tmp.join("skills");
    let library_dir = tmp.join("library");
    let logos_dir = tmp.join("logos");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::create_dir_all(&logos_dir).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();

    argus_lib::gateway::store::upsert_provider(
        &conn,
        &Provider {
            id: "real".into(),
            name: "Real".into(),
            compatible: "openAI".into(),
            base_url: REAL_BASE.into(),
            api_key_ref: None,
            connected: true,
            free: true,
            priority: 0,
            logo_url: None,
            doc_url: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_connected(&conn, "real", true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: REAL_MODEL.into(),
            display_name: "Real Test".into(),
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: REAL_MODEL.into(),
            provider_id: "real".into(),
            remote_model_id: REAL_REMOTE.into(),
            cost_in: 0.0,
            cost_out: 0.0,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, REAL_MODEL, true).unwrap();

    let gw = argus_lib::gateway::Gateway {
        conn: StdMutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir,
        library_dir,
        logos_dir,
        approvals: StdMutex::new(HashMap::new()),
        tasks: StdMutex::new(HashMap::new()),
    };

    let session_id = {
        let conn = gw.conn.lock().unwrap();
        argus_lib::sessions::store::create_session(
            &conn,
            &argus_lib::sessions::schema::NewSession {
                title: "Eval real copy".into(),
                model_id: Some(REAL_MODEL.into()),
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

    let status =
        argus_lib::sessions::chat::send(&gw, &handle, &session_id, USER_TASK_2, &chan).await;

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

    let assistants: Vec<&argus_lib::sessions::schema::Msg> =
        msgs.iter().filter(|m| m.role == "assistant").collect();
    let results: Vec<&argus_lib::sessions::schema::Msg> = msgs
        .iter()
        .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
        .collect();
    let kinds: std::collections::HashSet<String> = events
        .iter()
        .filter_map(|e| {
            e.split("\"type\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string)
        })
        .collect();

    eprintln!("=== REAL EVAL 2 TRACE ===");
    eprintln!("status: {status:?}");
    eprintln!("turns(assistant msgs): {}", assistants.len());
    eprintln!("tool results: {}", results.len());
    eprintln!("llm attempts: {attempts:?}");
    eprintln!("event kinds: {kinds:?}");
    for (i, m) in assistants.iter().enumerate() {
        eprintln!("--- assistant[{i}] ---\n{}", short(&m.content));
    }
    for (i, m) in results.iter().enumerate() {
        eprintln!("--- tool-result[{i}] ---\n{}", short(&m.content));
    }

    let src = std::fs::read(SRC_2).ok();
    let dst = std::fs::read(DST_2).ok();
    eprintln!(
        "src bytes: {:?}",
        src.as_ref()
            .map(|b| (b.len(), String::from_utf8_lossy(b).into_owned()))
    );
    eprintln!(
        "dst bytes: {:?}",
        dst.as_ref()
            .map(|b| (b.len(), String::from_utf8_lossy(b).into_owned()))
    );
    let files_ok =
        src.is_some() && dst.is_some() && src == dst && dst.as_deref() == Some(FACT_2.as_bytes());

    let mut seen_in_result = false;
    let mut used_in_later_call = false;
    let mut first_write_idx = None;
    let mut verify_after_write = false;
    for (i, m) in msgs.iter().enumerate() {
        let is_result = m.content.contains("<tool-result") || m.tool_call_id.is_some();
        if is_result && m.content.contains(FACT_2) {
            seen_in_result = true;
        }
        if !is_result {
            let mut calls_text = m.content.clone();
            if let Some(calls) = m.tool_calls.as_deref() {
                calls_text.push_str(calls);
            }
            if calls_text.contains(DST_2)
                && (calls_text.contains("write")
                    || calls_text.contains("fs.write")
                    || calls_text.contains('>')
                    || calls_text.contains("cp ")
                    || calls_text.contains("tee"))
            {
                if first_write_idx.is_none() {
                    first_write_idx = Some(i);
                }
            } else if calls_text.contains(DST_2) && first_write_idx.is_some() {
                verify_after_write = true;
            }
            if seen_in_result && calls_text.contains(FACT_2) {
                let has_call = m.tool_calls.as_deref().is_some_and(|c| c != "[]")
                    || m.content.contains("<action")
                    || m.content.contains("<terminal")
                    || m.content.contains("browser-action");
                if has_call {
                    used_in_later_call = true;
                }
            }
        }
    }
    eprintln!("seen_in_result={seen_in_result} used_in_later_call={used_in_later_call} verify_after_write={verify_after_write}");

    let verdict = match (
        &status,
        files_ok,
        seen_in_result && used_in_later_call,
        verify_after_write,
    ) {
        (Ok(_), true, true, true) => "PASS",
        (Ok(_), true, _, _) => "PARTIAL",
        _ => "FAIL",
    };
    eprintln!("verdict={verdict}");

    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(verdict, "PASS", "real-model copy task did not pass");
}
