//! Presented-watermark (Design A) integration tests, ext backend + profiles.
//!
//! Mock LLM drives the real `sessions/chat.rs::send()` loop; the browser is
//! the deterministic fake-extension fixture. The mock always emits legacy
//! text `<action>` blocks with `ref` and no `snapshot`, emulating the
//! observed GLM-5.3-Fast behavior. Tool ordering is asserted through
//! structured `tool="..."` result attributes, never substring matching.

use argus_lib::gateway::schema::{Avail, ModelEntry, Provider};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: OnceLock<StdMutex<()>> = OnceLock::new();
static SANDBOX: OnceLock<PathBuf> = OnceLock::new();

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.get_or_init(|| StdMutex::new(())).lock().unwrap()
}

fn sandbox() -> PathBuf {
    SANDBOX
        .get_or_init(|| {
            let tmp = std::env::temp_dir().join(format!("argus-bshown-{}", std::process::id()));
            let data_dir = tmp.join("com.anyscapelabs.argus");
            std::fs::create_dir_all(&data_dir).unwrap();
            std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
            std::env::set_var("XDG_DATA_HOME", &tmp);
            data_dir
        })
        .clone()
}

struct LlmTurn {
    text: String,
}

fn turn(text: &str) -> LlmTurn {
    LlmTurn { text: text.into() }
}

struct MockLlm {
    base: String,
    requests: Arc<StdMutex<Vec<serde_json::Value>>>,
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

fn sse_frame(v: &serde_json::Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(v).unwrap())
}

fn sse_body(t: &LlmTurn) -> String {
    let mut out = String::new();
    if !t.text.is_empty() {
        out.push_str(&sse_frame(&serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": t.text}}]
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
                    let Some((line, body)) = read_http_request(&mut sock).await else {
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
                        let body = sse_body(&t);
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

struct Harness {
    gw: argus_lib::gateway::Gateway,
    tmp: PathBuf,
    session_id: String,
    llm_requests: Arc<StdMutex<Vec<serde_json::Value>>>,
    handle: tauri::AppHandle<tauri::test::MockRuntime>,
    chan: tauri::ipc::Channel<argus_lib::gateway::schema::StreamEvent>,
    _app: tauri::App<tauri::test::MockRuntime>,
}

async fn setup(
    title: &str,
    model: &str,
    respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync>,
) -> Harness {
    let tmp = std::env::temp_dir().join(format!(
        "argus-bshown-gw-{}-{}",
        std::process::id(),
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

    let llm = start_mock_llm(respond).await;
    let llm_requests = llm.requests.clone();
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
        skills_dir: tmp.join("skills"),
        library_dir: tmp.join("library"),
        logos_dir: tmp.join("logos"),
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
                web_search: false,
            },
        )
        .unwrap()
        .id
    };
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let chan = tauri::ipc::Channel::new(move |_| Ok(()));
    Harness {
        gw,
        tmp,
        session_id,
        llm_requests,
        handle,
        chan,
        _app: app,
    }
}

impl Harness {
    async fn send(&self, user_text: &str) -> Result<(), String> {
        argus_lib::sessions::chat::send(
            &self.gw,
            &self.handle,
            &self.session_id,
            user_text,
            &self.chan,
            "user",
        )
        .await
    }

    fn msgs(&self) -> Vec<argus_lib::sessions::schema::Msg> {
        let conn = self.gw.conn.lock().unwrap();
        argus_lib::sessions::store::list_msgs(&conn, &self.session_id).unwrap()
    }

    fn results(&self) -> Vec<argus_lib::sessions::schema::Msg> {
        self.msgs()
            .into_iter()
            .filter(|m| m.content.contains("<tool-result") || m.tool_call_id.is_some())
            .collect()
    }

    fn llm_turns(&self) -> usize {
        self.llm_requests.lock().unwrap().len()
    }
}

fn result_tool_name(content: &str) -> Option<String> {
    let re = regex::Regex::new(r#"tool="([^"]+)""#).unwrap();
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn result_tools(results: &[argus_lib::sessions::schema::Msg]) -> Vec<String> {
    results
        .iter()
        .filter_map(|m| result_tool_name(&m.content))
        .collect()
}

fn snapshot_gens(text: &str) -> Vec<u64> {
    let re = regex::Regex::new(r"\(snapshot (\d+)\)").unwrap();
    re.captures_iter(text)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

#[derive(Clone)]
struct El {
    kind: &'static str,
    label: &'static str,
    path: &'static str,
}

struct FakeState {
    methods: Vec<String>,
    clicks: u32,
    fills: u32,
    click_paths: Vec<String>,
    snapshots: u32,
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

fn initial_snapshot() -> Vec<El> {
    vec![
        El {
            kind: "button",
            label: "Probe",
            path: "p1",
        },
        El {
            kind: "input",
            label: "Name",
            path: "i1",
        },
    ]
}

fn rerendered_snapshot() -> Vec<El> {
    vec![
        El {
            kind: "button",
            label: "Overview",
            path: "ov",
        },
        El {
            kind: "button",
            label: "Probe",
            path: "p2",
        },
        El {
            kind: "input",
            label: "Name",
            path: "i2",
        },
    ]
}

async fn ext_harness() -> Arc<StdMutex<FakeState>> {
    let data_dir = sandbox();
    let sock = data_dir.join("native.sock");
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(argus_lib::tools::browser::extpipe::serve_conn(stream));
        }
    });

    let state = Arc::new(StdMutex::new(FakeState {
        methods: vec![],
        clicks: 0,
        fills: 0,
        click_paths: vec![],
        snapshots: 0,
    }));
    let served = state.clone();
    let client = tokio::net::UnixStream::connect(&sock).await.unwrap();
    tokio::spawn(async move {
        let (mut rd, mut wr) = client.into_split();
        while let Some(req) = read_frame(&mut rd).await {
            if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
                continue;
            }
            let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
            let resp_data = {
                let mut s = served.lock().unwrap();
                s.methods.push(method.to_string());
                match method {
                    "navigate" => serde_json::json!({
                        "tabId": 71,
                        "url": data.get("url").and_then(|u| u.as_str()).unwrap_or("https://shown.test/"),
                        "title": "Probe",
                        "text": "probe console",
                    }),
                    "snapshot" => {
                        s.snapshots += 1;
                        let els = if s.snapshots <= 1 {
                            initial_snapshot()
                        } else {
                            rerendered_snapshot()
                        };
                        serde_json::Value::Array(
                            els.iter()
                                .map(|e| {
                                    serde_json::json!({"kind": e.kind, "label": e.label, "path": e.path})
                                })
                                .collect(),
                        )
                    }
                    "read" => serde_json::json!({
                        "url": "https://shown.test/",
                        "title": "Probe",
                        "text": "probe console",
                    }),
                    "click" => {
                        s.clicks += 1;
                        s.click_paths.push(
                            data.get("path")
                                .and_then(|p| p.as_str())
                                .unwrap_or("")
                                .to_string(),
                        );
                        serde_json::json!({})
                    }
                    "fill" => {
                        s.fills += 1;
                        serde_json::json!({})
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
    });

    for _ in 0..100 {
        if argus_lib::tools::browser::extpipe::connected() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(argus_lib::tools::browser::extpipe::connected());
    let _ = argus_lib::tools::browser::close(&serde_json::json!({})).await;
    state
}

fn open_action(url: &str) -> String {
    format!(r#"<action tool="browser.open">{{"url":"{url}"}}</action>"#)
}

fn click_action(r: u64) -> String {
    format!(r#"<action tool="browser.click">{{"ref":{r}}}</action>"#)
}

#[tokio::test]
async fn tokenless_steady_flow_succeeds() {
    let _guard = serial();
    let state = ext_harness().await;
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|t, _| match t {
            0 => turn(&open_action("https://shown.test/steady")),
            1 => turn(&click_action(0)),
            _ => turn("done"),
        });
    let h = setup("steady", "mock/shown-steady", respond).await;
    h.send("open the probe and press it").await.expect("send");
    assert!(h.llm_turns() >= 3, "loop must have run");
    let results = h.results();
    let tools = result_tools(&results);
    assert_eq!(
        tools,
        vec!["browser.open", "browser.click"],
        "tools: {tools:?}"
    );
    assert!(results[0].content.contains("[0] button \"Probe\""));
    assert!(
        !results[1].content.contains("stale ref"),
        "got: {}",
        results[1].content
    );
    let open_gen = snapshot_gens(&results[0].content);
    assert_eq!(open_gen.len(), 1);
    let s = state.lock().unwrap();
    assert_eq!(s.clicks, 1);
    assert_eq!(s.click_paths, vec!["p1".to_string()]);
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn tokenless_drift_rejected_with_bounded_recovery() {
    let _guard = serial();
    let state = ext_harness().await;
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|t, _| match t {
            0 => turn(&open_action("https://shown.test/drift")),
            1 => turn("observing"),
            2 => turn(&click_action(0)),
            3 => turn("noting recovery"),
            4 => turn(&click_action(0)),
            _ => turn("done"),
        });
    let h = setup("drift", "mock/shown-drift", respond).await;
    h.send("open the probe page").await.expect("first send");
    // Advance the table without presenting anything new to the model.
    argus_lib::tools::browser::read(&serde_json::json!({}))
        .await
        .expect("direct read");
    h.send("now press probe").await.expect("second send");
    h.send("try the same press again")
        .await
        .expect("third send");

    let results = h.results();
    let tools = result_tools(&results);
    assert_eq!(
        tools,
        vec!["browser.open", "browser.click", "browser.click"],
        "tools: {tools:?}"
    );
    let open_gen = snapshot_gens(&results[0].content)
        .into_iter()
        .next()
        .unwrap();
    assert!(results[0].content.contains("[0] button \"Probe\""));

    let first = &results[1].content;
    assert!(first.contains("stale ref 0"), "got: {first}");
    assert!(
        first.contains(&format!("snapshot {open_gen}")),
        "got: {first}"
    );
    assert!(first.contains("Choose the replacement ref"), "got: {first}");
    assert!(first.contains("[1] button \"Probe\""), "got: {first}");
    assert!(first.contains("[0] button \"Overview\""), "got: {first}");

    let second = &results[2].content;
    assert!(second.contains("stale ref"), "got: {second}");
    assert!(
        !second.contains("Elements (snapshot"),
        "repeat must not smuggle another snapshot: {second}"
    );

    let s = state.lock().unwrap();
    assert_eq!(
        (s.clicks, s.fills),
        (0, 0),
        "stale attempts must not execute"
    );
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn same_turn_read_then_tokenless_click_is_stale() {
    let _guard = serial();
    let _state = ext_harness().await;
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|t, _| match t {
            0 => turn(&open_action("https://shown.test/sameturn")),
            1 => turn("watching"),
            2 => turn(&format!(
                r#"<action tool="browser.read">{{}}</action>{}"#,
                click_action(0)
            )),
            _ => turn("done"),
        });
    let h = setup("sameturn", "mock/shown-sameturn", respond).await;
    h.send("open the probe page").await.expect("first send");
    h.send("read then press without re-reading")
        .await
        .expect("second send");
    let results = h.results();
    let tools = result_tools(&results);
    assert_eq!(
        tools,
        vec!["browser.open", "browser.read", "browser.click"],
        "tools: {tools:?}"
    );
    let click = &results[2].content;
    assert!(
        click.contains("stale ref 0"),
        "same-turn click must be stale: {click}"
    );
    assert!(click.contains("Choose the replacement ref"), "got: {click}");
    let _ = std::fs::remove_dir_all(&h.tmp);
}

#[tokio::test]
async fn tokenless_type_drift_is_stale() {
    let _guard = serial();
    let state = ext_harness().await;
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(|t, _| match t {
            0 => turn(&open_action("https://shown.test/type")),
            1 => turn("watching"),
            2 => turn(r#"<action tool="browser.type">{"ref":1,"text":"hi"}</action>"#),
            _ => turn("done"),
        });
    let h = setup("typedrift", "mock/shown-typedrift", respond).await;
    h.send("open the probe page").await.expect("first send");
    argus_lib::tools::browser::read(&serde_json::json!({}))
        .await
        .expect("direct read");
    h.send("type into the name field")
        .await
        .expect("second send");
    let results = h.results();
    let tools = result_tools(&results);
    assert_eq!(
        tools,
        vec!["browser.open", "browser.type"],
        "tools: {tools:?}"
    );
    assert!(
        results[1].content.contains("stale ref 1"),
        "got: {}",
        results[1].content
    );
    assert_eq!(state.lock().unwrap().fills, 0);
    let _ = std::fs::remove_dir_all(&h.tmp);
}
#[tokio::test]
async fn explicit_token_path_unchanged() {
    let _guard = serial();
    let state = ext_harness().await;
    let open =
        argus_lib::tools::browser::open(&serde_json::json!({"url": "https://shown.test/exp"}))
            .await
            .unwrap();
    let g1 = snapshot_gens(&open).into_iter().next().unwrap();
    let read = argus_lib::tools::browser::read(&serde_json::json!({}))
        .await
        .unwrap();
    let g2 = snapshot_gens(&read).into_iter().next().unwrap();
    assert_ne!(g1, g2);

    let err = argus_lib::tools::browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("old explicit snapshot must be stale");
    assert!(err.contains("stale ref 0"));
    assert!(err.contains("Choose the replacement ref"));

    argus_lib::tools::browser::click(&serde_json::json!({"ref": 1, "snapshot": g2}))
        .await
        .expect("fresh explicit ref must succeed");
    assert_eq!(state.lock().unwrap().clicks, 1);
}

async fn iso_http() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let body = if path.starts_with("/second") {
                    "<html><body><h1>Second page</h1><button>Back</button></body></html>"
                } else {
                    "<html><body><a href=\"/second\">Go second</a></body></html>"
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    (base, handle)
}

#[tokio::test]
async fn profile_isolation_for_shown_watermark() {
    let _guard = serial();
    let _ = sandbox();
    let (base, _srv) = iso_http().await;
    let tag = uuid::Uuid::new_v4().as_simple().to_string()[..8].to_string();
    let prof_a = format!("wmark-a-{tag}");
    let prof_b = format!("wmark-b-{tag}");
    let profiles = std::env::temp_dir().join(format!("argus-bshown-prof-{tag}"));
    argus_lib::tools::browser::init(profiles);

    let open_a =
        format!(r#"<action tool="browser.open">{{"url":"{base}/","profile":"{prof_a}"}}</action>"#);
    let open_b =
        format!(r#"<action tool="browser.open">{{"url":"{base}/","profile":"{prof_b}"}}</action>"#);
    let click_a =
        format!(r#"<action tool="browser.click">{{"ref":0,"profile":"{prof_a}"}}</action>"#);
    let click_b =
        format!(r#"<action tool="browser.click">{{"ref":0,"profile":"{prof_b}"}}</action>"#);
    let respond: Arc<dyn Fn(usize, &serde_json::Value) -> LlmTurn + Send + Sync> =
        Arc::new(move |t, _| match t {
            0 => turn(&format!("{open_a}{open_b}")),
            1 => turn("watching both"),
            2 => turn(&format!("{click_a}{click_b}")),
            _ => turn("done"),
        });
    let h = setup("profiles", "mock/shown-profiles", respond).await;
    h.send("open the probe in two windows")
        .await
        .expect("first send");
    argus_lib::tools::browser::open(
        &serde_json::json!({"url": format!("{base}/second"), "profile": prof_a}),
    )
    .await
    .expect("drift profile A only");
    h.send("press ref zero in both windows")
        .await
        .expect("second send");

    let results = h.results();
    let tools = result_tools(&results);
    assert_eq!(
        tools,
        vec![
            "browser.open",
            "browser.open",
            "browser.click",
            "browser.click"
        ],
        "tools: {tools:?}"
    );
    let click_a_res = &results[2].content;
    let click_b_res = &results[3].content;
    assert!(
        click_a_res.contains("stale ref 0"),
        "drifted profile A must be stale: {click_a_res}"
    );
    assert!(
        click_a_res.contains("Choose the replacement ref"),
        "got: {click_a_res}"
    );
    assert!(
        !click_b_res.contains("stale ref"),
        "steady profile B must succeed: {click_b_res}"
    );
    let _ = std::fs::remove_dir_all(&h.tmp);
}
