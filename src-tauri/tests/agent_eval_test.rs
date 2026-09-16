//! Minimal end-to-end agent evaluation harness.
//!
//! Drives the REAL `sessions/chat.rs::send()` loop exactly as production
//! does: real prompt projection, real router, real tool parsing, real
//! `ToolExecution`, real tools, real SQLite persistence. Nothing is faked
//! except the two boundaries that must be deterministic:
//!
//! 1. The LLM itself: a local mock OpenAI-compatible SSE server. It is
//!    scripted per task but READS the request history, so follow-up turns
//!    can depend on real tool results (URLs, refs, ids parsed out of the
//!    actual conversation the loop built).
//! 2. External services: local mock HTTP servers (search index, pages) and
//!    the existing fake-extension / Xephyr patterns. No public internet,
//!    no user desktop contact.
//!
//! The single production touch enabling this harness is the generic
//! `Runtime` bound on `send()`'s otherwise-untouched `AppHandle` param,
//! which lets tests pass `tauri::test::mock_app()`'s headless handle.
//! Sessions are pre-titled so the background title task never runs.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

struct LlmTurn {
    text: String,
    calls: Vec<(String, String, String)>,
}

fn turn(text: &str) -> LlmTurn {
    LlmTurn {
        text: text.into(),
        calls: vec![],
    }
}

struct MockLlm {
    base: String,
    requests: Arc<StdMutex<Vec<serde_json::Value>>>,
}

async fn read_http_request(
    sock: &mut tokio::net::TcpStream,
) -> Option<(String, HashMap<String, String>, Vec<u8>)> {
    let mut buf = vec![0u8; 65536];
    let mut data = vec![];
    loop {
        let n = sock.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = find_subslice(&data, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..pos]).into_owned();
            let mut headers = HashMap::new();
            for line in head.lines().skip(1) {
                if let Some((k, v)) = line.split_once(':') {
                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                }
            }
            let len: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
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
            return Some((line, headers, body));
        }
        if data.len() > 1_000_000 {
            return None;
        }
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn sweep_tmp(tmp: &std::path::Path) {
    let marker = tmp.display().to_string();
    let me = std::process::id();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let pid = e.file_name().to_string_lossy().parse::<u32>().unwrap_or(0);
            if pid == 0 || pid == me {
                continue;
            }
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            let environ = std::fs::read(format!("/proc/{pid}/environ"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if !matches!(
                comm.as_str(),
                "dbus-daemon"
                    | "at-spi2-registryd"
                    | "at-spi-bus-launcher"
                    | "Xephyr"
                    | "xclock"
                    | "metacity"
            ) {
                continue;
            }
            if cmdline.contains(&marker) || environ.contains(&marker) {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .status();
            }
        }
    }
}

fn sse_frame(v: &serde_json::Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(v).unwrap())
}

fn sse_turn(t: &LlmTurn) -> String {
    let mut out = String::new();
    if !t.text.is_empty() {
        out.push_str(&sse_frame(&serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": t.text}}]
        })));
    }
    for (i, (id, name, args)) in t.calls.iter().enumerate() {
        let args: serde_json::Value = serde_json::from_str(args).unwrap_or(serde_json::Value::Null);
        out.push_str(&sse_frame(&serde_json::json!({
            "choices": [{"index": 0, "delta": {"tool_calls": [{
                "index": i,
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": args.to_string()},
            }]}}]
        })));
    }
    out.push_str(&sse_frame(&serde_json::json!({
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    })));
    out.push_str("data: [DONE]\n\n");
    out
}

async fn start_mock_llm(
    respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync>,
) -> MockLlm {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let requests: Arc<StdMutex<Vec<serde_json::Value>>> = Arc::new(StdMutex::new(vec![]));

    tokio::spawn({
        let requests = requests.clone();
        async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let respond = respond.clone();
                let requests = requests.clone();
                tokio::spawn(async move {
                    let Some((line, _, body)) = read_http_request(&mut sock).await else {
                        return;
                    };
                    let req: serde_json::Value =
                        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                    let turn_idx = req
                        .get("messages")
                        .and_then(|m| m.as_array())
                        .map(|msgs| {
                            msgs.iter()
                                .filter(|m| {
                                    m.get("role").and_then(|r| r.as_str()) == Some("assistant")
                                })
                                .count()
                        })
                        .unwrap_or(0);
                    requests.lock().unwrap().push(req.clone());

                    let stream = line.contains("/chat/completions")
                        && req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
                    let t = respond(turn_idx, &req);
                    let payload = if stream {
                        let body = sse_turn(&t);
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
                        )
                    } else {
                        let body = serde_json::json!({
                            "choices": [{"message": {"role": "assistant", "content": t.text}}],
                            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    };
                    let _ = sock.write_all(payload.as_bytes()).await;
                });
            }
        }
    });

    MockLlm { base, requests }
}

fn history_text(req: &serde_json::Value) -> String {
    req.get("messages")
        .and_then(|m| m.as_array())
        .map(|msgs| {
            msgs.iter()
                .map(|m| {
                    let mut s = m
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(calls) = m.get("tool_calls").and_then(|c| c.as_array()) {
                        for c in calls {
                            s.push_str(&c.to_string());
                        }
                    }
                    s
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn last_result_failed(req: &serde_json::Value) -> bool {
    let h = history_text(req);
    h.contains("status=\"err\"") || h.contains("error: ")
}

fn failed_turn(req: &serde_json::Value) -> LlmTurn {
    turn(&format!(
        "TASK-FAILED: {}",
        history_text(req).chars().take(300).collect::<String>()
    ))
}

fn last_tool_result_text(req: &serde_json::Value) -> Option<String> {
    req.get("messages")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .rev()
        .find_map(|m| {
            let role = m.get("role").and_then(|r| r.as_str())?;
            let content = m.get("content").and_then(|c| c.as_str())?;
            if (role == "user" && content.contains("<tool-result")) || role == "tool" {
                Some(content.to_string())
            } else {
                None
            }
        })
}

fn extract_first_url(text: &str) -> Option<String> {
    let re = regex::Regex::new(r##"https?://[^\s"<>]+"##).ok()?;
    re.find(text).map(|m| {
        m.as_str()
            .trim_end_matches(|c| c == '.' || c == ',' || c == ')' || c == ']' || c == '"')
            .to_string()
    })
}

fn extract_browser_target(text: &str) -> Option<(u64, u64)> {
    let re_ref = regex::Regex::new(r"\[(\d+)\]\s+link").ok()?;
    let re_gen = regex::Regex::new(r"\(snapshot (\d+)\)").ok()?;
    let r: u64 = re_ref.captures(text)?.get(1)?.as_str().parse().ok()?;
    let g: u64 = re_gen.captures(text)?.get(1)?.as_str().parse().ok()?;
    Some((r, g))
}

fn extract_window_id_on_line(text: &str, needle: &str) -> Option<String> {
    let re = regex::Regex::new(r"0x[0-9a-fA-F]+").ok()?;
    text.lines()
        .find(|l| l.contains(needle))
        .and_then(|l| re.find(l))
        .map(|m| m.as_str().to_string())
}

struct Harness {
    gw: argus_lib::gateway::Gateway,
    tmp: PathBuf,
    session_id: String,
    llm: MockLlm,
    events: Arc<StdMutex<Vec<String>>>,
    _app: tauri::App<tauri::test::MockRuntime>,
    handle: tauri::AppHandle<tauri::test::MockRuntime>,
    chan: tauri::ipc::Channel<argus_lib::gateway::schema::StreamEvent>,
}

struct Trace {
    status: Result<(), String>,
    msgs: Vec<argus_lib::sessions::schema::Msg>,
    requests: Vec<serde_json::Value>,
    events: Vec<String>,
    attempts: Vec<i64>,
}

async fn setup(
    title: &str,
    model: &str,
    web_search: bool,
    respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync>,
) -> Harness {
    let tmp = std::env::temp_dir().join(format!("argus-eval-{}", uuid::Uuid::new_v4().as_simple()));
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

    let llm = start_mock_llm(respond).await;

    argus_lib::gateway::store::upsert_provider(
        &conn,
        &Provider {
            id: "mock".into(),
            name: "Mock".into(),
            compatible: "openAI".into(),
            base_url: llm.base.clone(),
            api_key_ref: None,
            connected: true,
            free: true,
            priority: 0,
            logo_url: None,
            doc_url: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_connected(&conn, "mock", true).unwrap();
    argus_lib::gateway::store::add_model(
        &conn,
        &ModelEntry {
            id: model.into(),
            display_name: "Mock Test".into(),
            family: None,
            capabilities: None,
            suggested_tier: None,
        },
    )
    .unwrap();
    argus_lib::gateway::store::link_model(
        &conn,
        &Avail {
            model_id: model.into(),
            provider_id: "mock".into(),
            remote_model_id: "test".into(),
            cost_in: 0.0,
            cost_out: 0.0,
        },
    )
    .unwrap();
    argus_lib::gateway::store::set_model_enabled(&conn, model, true).unwrap();

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
                title: title.into(),
                model_id: Some(model.into()),
                permission: Some("never".into()),
                folder_id: None,
                web_search,
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

    Harness {
        gw,
        tmp,
        session_id,
        llm,
        events,
        _app: app,
        handle,
        chan,
    }
}

impl Harness {
    async fn run_task(&self, user_text: &str) -> Trace {
        let status = argus_lib::sessions::chat::send(
            &self.gw,
            &self.handle,
            &self.session_id,
            user_text,
            &self.chan,
        )
        .await;
        let msgs = {
            let conn = self.gw.conn.lock().unwrap();
            argus_lib::sessions::store::list_msgs(&conn, &self.session_id).unwrap()
        };
        let requests = self.llm.requests.lock().unwrap().clone();
        let events = self.events.lock().unwrap().clone();
        let attempts = {
            let conn = self.gw.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT attempt FROM request_log ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, i64>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        Trace {
            status,
            msgs,
            requests,
            events,
            attempts,
        }
    }

    fn assistants(&self, t: &Trace) -> Vec<String> {
        t.msgs
            .iter()
            .filter(|m| m.role == "assistant")
            .map(|m| m.content.clone())
            .collect()
    }

    fn tool_results(&self, t: &Trace) -> Vec<String> {
        t.msgs
            .iter()
            .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
            .map(|m| m.content.clone())
            .collect()
    }
}

fn assert_no_err(t: &Trace, h: &Harness, what: &str) {
    assert!(
        t.status.is_ok(),
        "{what}: send failed: {:?}\nmsgs={:#?}",
        t.status,
        h.assistants(t)
    );
    assert!(
        !h.tool_results(t)
            .iter()
            .any(|c| c.contains("status=\"err\"") || c.starts_with("error: ")),
        "{what}: unexpected err tool result: {:#?}",
        h.tool_results(t)
    );
    assert!(
        !h.assistants(t).iter().any(|c| c.contains("TASK-FAILED")),
        "{what}: model reported failure: {:#?}",
        h.assistants(t)
    );
}

#[tokio::test]
async fn eval_terminal_task() {
    let _guard = serial();
    let out_path = std::env::temp_dir().join(format!(
        "eval-terminal-{}.txt",
        uuid::Uuid::new_v4().as_simple()
    ));
    let out_arg = out_path.display().to_string();
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> = Arc::new(
        move |idx, req| {
            if last_result_failed(req) {
                return failed_turn(req);
            }
            match idx {
                0 => LlmTurn {
                    text: format!(
                        "Creating the file now.\n<action tool=\"terminal\">{{\"command\":\"printf 'EVAL-TERM-731' > '{out_arg}'\",\"cwd\":\".\"}}</action>"
                    ),
                    calls: vec![],
                },
                _ => turn("Done. The file is written."),
            }
        },
    );

    let h = setup("Eval terminal", "mock/test", false, respond).await;
    let t = h.run_task("Create a file with the given contents.").await;

    assert_no_err(&t, &h, "terminal");
    assert_eq!(h.assistants(&t).len(), 2, "expected 2 model turns");
    assert_eq!(h.tool_results(&t).len(), 1);
    assert!(h.tool_results(&t)[0].contains("status=\"ok\""));
    assert!(
        t.events.iter().any(|e| e.contains("\"type\":\"term_end\"")),
        "terminal must stream TermEnd"
    );
    assert!(
        t.attempts.iter().all(|&a| a == 1),
        "no LLM retries expected"
    );
    assert_eq!(std::fs::read_to_string(&out_path).unwrap(), "EVAL-TERM-731");
    assert_eq!(h.assistants(&t)[1], "Done. The file is written.");
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn eval_multi_tool_task() {
    let _guard = serial();
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> = Arc::new(
        |idx, req| {
            if last_result_failed(req) {
                return failed_turn(req);
            }
            match idx {
                0 => LlmTurn {
                    text: "Saving that for you.\n<action tool=\"memory.save\">{\"content\":\"eval fact BLUEBIRD-42\",\"kind\":\"fact\"}</action>".into(),
                    calls: vec![],
                },
                1 => LlmTurn {
                    text: "Looking it up.\n<action tool=\"memory.search\">{\"query\":\"BLUEBIRD\"}</action>".into(),
                    calls: vec![],
                },
                _ => turn("Found it: BLUEBIRD-42."),
            }
        },
    );

    let h = setup("Eval multi", "mock/test", false, respond).await;
    let t = h.run_task("Remember a fact, then recall it.").await;

    assert_no_err(&t, &h, "multi");
    assert_eq!(h.assistants(&t).len(), 3);
    let results = h.tool_results(&t);
    assert_eq!(results.len(), 2);
    assert!(results[0].contains("memory.save") && results[1].contains("memory.search"));
    assert_eq!(h.assistants(&t)[2], "Found it: BLUEBIRD-42.");
    assert!(t.attempts.iter().all(|&a| a == 1));
    assert!(t.requests.len() >= 2, "second request must exist");
    assert!(
        history_text(&t.requests[1]).contains("BLUEBIRD-42"),
        "save result must be in context before search"
    );
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn eval_web_task() {
    let _guard = serial();
    let prev_searx = std::env::var("ARGUS_SEARXNG_POOL").ok();
    let prev_jina = std::env::var("ARGUS_JINA_URL").ok();

    let search_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let search_base = format!("http://{}", search_listener.local_addr().unwrap());
    let fact_url = format!("{search_base}/fact");
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match search_listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let fact_url = fact_url.clone();
            tokio::spawn(async move {
                let Some((line, _, _)) = read_http_request(&mut sock).await else {
                    return;
                };
                let body;
                let (status, ct) = if line.contains("/jina") {
                    body = "jina down".to_string();
                    ("500 Internal Server Error", "text/plain")
                } else if line.contains("/search") {
                    body = serde_json::json!({"results": [
                        {"title": "Eval Fact Page", "url": fact_url, "content": "quokka snippet"}
                    ]})
                    .to_string();
                    ("200 OK", "application/json")
                } else if line.contains("/fact") {
                    body = "<html><body><p>The QUOKKA-7 fact lives here.</p></body></html>"
                        .to_string();
                    ("200 OK", "text/html")
                } else {
                    body = "no route".to_string();
                    ("404 Not Found", "text/plain")
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    std::env::set_var("ARGUS_SEARXNG_POOL", format!("{search_base}/search"));
    std::env::set_var("ARGUS_JINA_URL", format!("{search_base}/jina"));

    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|idx, req| {
            if last_result_failed(req) {
                return failed_turn(req);
            }
            match idx {
                0 => LlmTurn {
                    text: "Searching now.".into(),
                    calls: vec![(
                        "call_1".into(),
                        "web.search".into(),
                        r#"{"query":"eval query"}"#.into(),
                    )],
                },
                1 => {
                    let url = last_tool_result_text(req)
                        .as_deref()
                        .and_then(extract_first_url)
                        .unwrap_or_default();
                    LlmTurn {
                        text: "Reading the top result.".into(),
                        calls: vec![(
                            "call_2".into(),
                            "web.read".into(),
                            format!(r#"{{"url":"{url}"}}"#),
                        )],
                    }
                }
                _ => turn("The eval fact is QUOKKA-7."),
            }
        });

    let h = setup("Eval web", "mock/test", true, respond).await;
    let t = h.run_task("Search for the eval fact and report it.").await;

    match prev_searx {
        Some(v) => std::env::set_var("ARGUS_SEARXNG_POOL", v),
        None => std::env::remove_var("ARGUS_SEARXNG_POOL"),
    }
    match prev_jina {
        Some(v) => std::env::set_var("ARGUS_JINA_URL", v),
        None => std::env::remove_var("ARGUS_JINA_URL"),
    }

    assert_no_err(&t, &h, "web");
    assert_eq!(h.assistants(&t).len(), 3);
    assert_eq!(h.tool_results(&t).len(), 2);
    assert!(h.assistants(&t)[2].contains("QUOKKA-7"));
    assert!(t.attempts.iter().all(|&a| a == 1));
    assert!(
        history_text(&t.requests[1]).contains("/fact"),
        "search URL must reach turn 2"
    );
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn eval_browser_task() {
    let _guard = serial();
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-bx-{}",
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
    let clicked = Arc::new(StdMutex::new(0u32));
    let client = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    tokio::spawn({
        let clicked = clicked.clone();
        async move {
            let (mut rd, mut wr) = client.into_split();
            loop {
                let mut b = [0u8; 4];
                if rd.read_exact(&mut b).await.is_err() {
                    return;
                }
                let len = u32::from_ne_bytes(b) as usize;
                if len == 0 || len > 64 * 1_048_576 {
                    return;
                }
                let mut buf = vec![0u8; len];
                if rd.read_exact(&mut buf).await.is_err() {
                    return;
                }
                let req: serde_json::Value = match serde_json::from_slice(&buf) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
                    continue;
                }
                let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
                let resp_data = match method {
                    "navigate" => serde_json::json!({
                        "tabId": 11,
                        "url": data.get("url").and_then(|u| u.as_str()).unwrap_or("http://eval.local/"),
                        "title": "Eval",
                        "text": "Eval home page.",
                    }),
                    "snapshot" => serde_json::json!([
                        {"kind": "link", "label": "Go second", "path": "a"},
                    ]),
                    "click" => {
                        *clicked.lock().unwrap() += 1;
                        serde_json::json!({})
                    }
                    "read" => serde_json::json!({
                        "url": "http://eval.local/second",
                        "title": "Eval Second",
                        "text": "SECOND-PAGE-EVAL-MARKER reached.",
                    }),
                    _ => serde_json::json!({}),
                };
                let frame = serde_json::json!({"id": id, "ok": true, "data": resp_data});
                let b = serde_json::to_vec(&frame).unwrap();
                let _ = wr.write_all(&(b.len() as u32).to_ne_bytes()).await;
                let _ = wr.write_all(&b).await;
                let _ = wr.flush().await;
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

    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|idx, req| {
            if last_result_failed(req) {
                return failed_turn(req);
            }
            match idx {
                0 => LlmTurn {
                    text: "Opening the page.".into(),
                    calls: vec![(
                        "call_1".into(),
                        "browser.open".into(),
                        r#"{"url":"http://eval.local/"}"#.into(),
                    )],
                },
                1 => match last_tool_result_text(req)
                    .as_deref()
                    .and_then(extract_browser_target)
                {
                    Some((r, g)) => LlmTurn {
                        text: "Clicking the link.".into(),
                        calls: vec![(
                            "call_2".into(),
                            "browser.click".into(),
                            format!(r#"{{"ref":{r},"snapshot":{g}}}"#),
                        )],
                    },
                    None => turn("TASK-FAILED: no ref in snapshot"),
                },
                _ => turn("Reached the second page."),
            }
        });

    let h = setup("Eval browser", "mock/test", false, respond).await;
    let t = h
        .run_task("Open the page, follow the link, report back.")
        .await;

    match prev_xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }

    assert_no_err(&t, &h, "browser");
    assert_eq!(*clicked.lock().unwrap(), 1, "exactly one real click");
    assert_eq!(h.tool_results(&t).len(), 2);
    assert!(h.assistants(&t)[2].contains("second page"));
    assert!(t.attempts.iter().all(|&a| a == 1));
    assert!(
        history_text(&t.requests[1]).contains("Go second"),
        "snapshot must reach turn 2"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn eval_computer_task() {
    let _guard = serial();
    let prev = [
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_DATA_HOME",
        "XDG_RUNTIME_DIR",
    ]
    .iter()
    .map(|k| (k.to_string(), std::env::var(k).ok()))
    .collect::<Vec<_>>();

    let tmp = std::env::temp_dir().join(format!(
        "argus-eval-cx-{}",
        uuid::Uuid::new_v4().as_simple()
    ));
    let data_dir = tmp.join("com.anyscapelabs.argus");
    std::fs::create_dir_all(&data_dir).unwrap();
    let runtime = tmp.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(
        &runtime,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();

    let mut busd = tokio::process::Command::new("dbus-daemon")
        .args(["--session", "--print-address=1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("dbus-daemon");
    let mut line = String::new();
    {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut r = BufReader::new(busd.stdout.as_mut().unwrap());
        r.read_line(&mut line).await.expect("bus address");
    }
    let bus = line.trim().split(',').next().unwrap_or("").to_string();
    assert!(!bus.is_empty(), "no bus address");
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus);
    std::env::set_var("XDG_RUNTIME_DIR", &runtime);

    let mut launcher = tokio::process::Command::new("/usr/libexec/at-spi-bus-launcher")
        .args(["--launch-immediately", "--a11y=1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("bus launcher");
    let mut a11y_up = false;
    for _ in 0..60 {
        if let Ok(entries) = std::fs::read_dir(runtime.join("at-spi")) {
            if entries.count() > 0 {
                a11y_up = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(a11y_up, "a11y bus never appeared");
    tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
    let _ = &mut launcher;

    let mut keyringd = tokio::process::Command::new("gnome-keyring-daemon")
        .args(["--foreground", "--components=secrets"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("keyring daemon");
    let mut secrets_up = false;
    for _ in 0..60 {
        let ok = match keyring::Entry::new("argus-eval-probe", "probe") {
            Ok(e) => matches!(e.get_password(), Ok(_) | Err(keyring::Error::NoEntry)),
            Err(_) => false,
        };
        if ok {
            secrets_up = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(secrets_up, "secret service never appeared");
    let _ = &mut keyringd;

    let display = ":97";
    let mut xephyr = tokio::process::Command::new("Xephyr")
        .args([display, "-screen", "800x600", "-ac"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("xephyr");
    let sock = "/tmp/.X11-unix/X97";
    let mut up = false;
    for _ in 0..100 {
        if std::path::Path::new(sock).exists() {
            up = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(up, "xephyr never came up");
    std::env::set_var("DISPLAY", display);
    std::env::set_var("XDG_DATA_HOME", &tmp);

    let mut wm = tokio::process::Command::new("metacity")
        .args(["--display", display])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("metacity");
    let mut wm_ok = false;
    for _ in 0..60 {
        let ok = std::process::Command::new("xprop")
            .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("0x"))
            .unwrap_or(false);
        if ok {
            wm_ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(wm_ok, "no window manager");

    let mut clock = tokio::process::Command::new("xclock")
        .args(["-title", "EvalClock"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("xclock");
    let mut seen = false;
    for _ in 0..40 {
        let out = std::process::Command::new("wmctrl")
            .arg("-l")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        if out.contains("EvalClock") {
            seen = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(seen, "xclock never mapped");

    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|idx, req| {
            if last_result_failed(req) {
                return failed_turn(req);
            }
            match idx {
                0 => LlmTurn {
                    text: "Observing the desktop.".into(),
                    calls: vec![("call_1".into(), "computer.observe".into(), "{}".into())],
                },
                1 => match last_tool_result_text(req)
                    .as_deref()
                    .and_then(|t| extract_window_id_on_line(t, "EvalClock"))
                {
                    Some(id) => LlmTurn {
                        text: "Activating the clock.".into(),
                        calls: vec![(
                            "call_2".into(),
                            "computer.window".into(),
                            format!(r#"{{"action":"activate","id":"{id}"}}"#),
                        )],
                    },
                    None => turn("TASK-FAILED: no EvalClock window id"),
                },
                _ => turn("The window is active."),
            }
        });

    // xclock is Xaw (no AT-SPI tree): observe succeeds with an empty tree
    // plus the real wmctrl window list — exactly what this task needs.
    let h = setup("Eval computer", "mock/test", false, respond).await;
    let t = h
        .run_task("Observe the desktop and activate the clock.")
        .await;

    let _ = clock.kill().await;
    let _ = wm.kill().await;
    let _ = xephyr.kill().await;
    let _ = launcher.kill().await;
    let _ = busd.kill().await;
    sweep_tmp(&tmp);
    for (k, v) in prev {
        match v {
            Some(v) => std::env::set_var(k, v),
            None => std::env::remove_var(k),
        }
    }

    assert_no_err(&t, &h, "computer");
    assert_eq!(h.assistants(&t).len(), 3);
    assert_eq!(h.tool_results(&t).len(), 2);
    assert_eq!(h.assistants(&t)[2], "The window is active.");
    assert!(t.attempts.iter().all(|&a| a == 1));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&h.tmp);
}
