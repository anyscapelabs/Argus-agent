use argus_lib::tools::browser::{self, extpipe};
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
            let tmp = std::env::temp_dir().join(format!("argus-brec-{}", std::process::id()));
            let data_dir = tmp.join("com.anyscapelabs.argus");
            std::fs::create_dir_all(&data_dir).unwrap();
            std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
            std::env::set_var("XDG_DATA_HOME", &tmp);
            data_dir
        })
        .clone()
}

fn test_gw(tag: &str) -> (argus_lib::gateway::Gateway, PathBuf) {
    use std::collections::HashMap;
    use std::sync::Mutex;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(argus_lib::gateway::schema::MIGRATE)
        .unwrap();
    argus_lib::sessions::store::migrate(&conn).unwrap();
    argus_lib::skills::store::migrate(&conn).unwrap();
    argus_lib::library::store::migrate(&conn).unwrap();
    argus_lib::memory::store::migrate(&conn).unwrap();
    let base = std::env::temp_dir().join(format!("argus-brec-gw-{}-{tag}", std::process::id()));
    let skills_dir = base.join("skills");
    let library_dir = base.join("library");
    let logos_dir = base.join("logos");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&library_dir).unwrap();
    std::fs::create_dir_all(&logos_dir).unwrap();
    let gw = argus_lib::gateway::Gateway {
        conn: Mutex::new(conn),
        http: reqwest::Client::new(),
        skills_dir,
        library_dir,
        logos_dir,
        approvals: Mutex::new(HashMap::new()),
        tasks: Mutex::new(HashMap::new()),
    };
    (gw, base)
}

#[derive(Clone)]
struct El {
    kind: &'static str,
    label: &'static str,
    path: &'static str,
}

struct FakeState {
    clicks: u32,
    fills: u32,
    tab_seq: i32,
    last_url: String,
    snapshots: Vec<Vec<El>>,
    fail_on: Option<String>,
}

impl FakeState {
    fn current_snapshot(&self) -> Vec<El> {
        let idx = ((self.clicks + self.fills) as usize).min(self.snapshots.len().saturating_sub(1));
        self.snapshots[idx].clone()
    }
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

async fn fake_extension(stream: tokio::net::UnixStream, state: Arc<StdMutex<FakeState>>) {
    let (mut rd, mut wr) = stream.into_split();
    while let Some(req) = read_frame(&mut rd).await {
        if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
            continue;
        }
        let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
        let fail = state.lock().unwrap().fail_on.clone();
        if fail.as_deref() == Some(method) {
            write_frame(
                &mut wr,
                &serde_json::json!({"id": id, "ok": false, "err": "injected click failed"}),
            )
            .await;
            continue;
        }
        let resp_data = {
            let mut s = state.lock().unwrap();
            match method {
                "navigate" => {
                    s.tab_seq += 1;
                    let asked = data
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("https://example.com/");
                    s.last_url = if asked.contains("redirect-me") {
                        "https://example.com/landed".into()
                    } else {
                        asked.to_string()
                    };
                    let tab = s.tab_seq;
                    let url = s.last_url.clone();
                    serde_json::json!({
                        "tabId": tab,
                        "url": url,
                        "title": "Example",
                        "text": "hello from fake page",
                    })
                }
                "snapshot" => {
                    let els: Vec<serde_json::Value> = s
                        .current_snapshot()
                        .iter()
                        .map(|e| serde_json::json!({"kind": e.kind, "label": e.label, "path": e.path}))
                        .collect();
                    serde_json::Value::Array(els)
                }
                "read" => {
                    let url = s.last_url.clone();
                    serde_json::json!({
                        "url": url,
                        "title": "Example",
                        "text": "hello from fake page",
                    })
                }
                "click" => {
                    s.clicks += 1;
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
}

async fn ext_harness(snapshots: Vec<Vec<El>>) -> Arc<StdMutex<FakeState>> {
    ext_harness_full(snapshots, None).await
}

async fn ext_harness_full(
    snapshots: Vec<Vec<El>>,
    fail_on: Option<String>,
) -> Arc<StdMutex<FakeState>> {
    let data_dir = sandbox();
    let sock = data_dir.join("native.sock");
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(extpipe::serve_conn(stream));
        }
    });

    let state = Arc::new(StdMutex::new(FakeState {
        clicks: 0,
        fills: 0,
        tab_seq: 7,
        last_url: "https://example.com/".into(),
        snapshots,
        fail_on,
    }));
    let served = state.clone();
    let client = tokio::net::UnixStream::connect(&sock).await.unwrap();
    tokio::spawn(fake_extension(client, served));

    for _ in 0..100 {
        if extpipe::connected() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(extpipe::connected(), "fake extension never connected");

    let _ = browser::close(&serde_json::json!({})).await;
    state
}

fn snap_gen(out: &str) -> u64 {
    out.split("(snapshot ")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .expect("snapshot header with generation")
}

fn two_el() -> Vec<El> {
    vec![
        El {
            kind: "button",
            label: "Go",
            path: "button",
        },
        El {
            kind: "input",
            label: "Name",
            path: "input",
        },
    ]
}

#[tokio::test]
async fn stale_then_fresh_snapshot_new_ref_succeeds() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let g2 = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());
    assert_ne!(g1, g2);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("old ref must be stale");
    assert!(err.contains("stale ref 0"), "got: {err}");
    assert!(err.contains(&format!("(snapshot {g2})")), "got: {err}");
    assert!(err.contains("[0] button \"Go\""), "got: {err}");
    assert!(err.contains("act once"), "got: {err}");

    let g3 = snap_gen(&err);
    assert_eq!(g3, g2);
    browser::click(&serde_json::json!({"ref": 0, "snapshot": g3}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn stale_then_recovered_action_failure_returns_normally() {
    let _guard = serial();
    let state = ext_harness_full(vec![two_el()], Some("click".into())).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let g2 = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("stale");
    assert!(err.contains(&format!("(snapshot {g2})")), "got: {err}");

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g2}))
        .await
        .expect_err("recovered click fails");
    assert!(err.contains("injected click failed"), "got: {err}");
    assert!(
        !err.contains("(snapshot "),
        "recovered failure must not smuggle a snapshot: {err}"
    );
    assert_eq!(state.lock().unwrap().clicks, 0);
}

#[tokio::test]
async fn stale_ref_is_never_executed() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let _ = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());

    assert!(
        browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
            .await
            .is_err()
    );
    assert!(
        browser::type_text(&serde_json::json!({"ref": 1, "text": "x", "snapshot": g1}))
            .await
            .is_err()
    );
    let s = state.lock().unwrap();
    assert_eq!((s.clicks, s.fills), (0, 0));
}

#[tokio::test]
async fn assisted_recovery_is_bounded_to_one_attempt() {
    let _guard = serial();
    ext_harness(vec![two_el()]).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let _ = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());

    let first = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("stale");
    assert!(first.contains("Elements (snapshot"), "got: {first}");

    let second = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("same stale token again");
    assert!(!second.contains("Elements (snapshot"), "got: {second}");
    assert!(second.contains("run browser.read"), "got: {second}");

    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g3 = snap_gen(&out);
    browser::click(&serde_json::json!({"ref": 0, "snapshot": g3}))
        .await
        .unwrap();
}

#[tokio::test]
async fn fresh_refs_need_no_recovery() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let g = snap_gen(&out);
    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert!(!out.contains("stale ref"), "got: {out}");
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn approval_and_sensitive_checks_survive_recovery() {
    let _guard = serial();
    let state = ext_harness(vec![vec![El {
        kind: "button",
        label: "Place order",
        path: "button",
    }]])
    .await;
    let (gw, base) = test_gw("approval");

    assert!(
        !browser::sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://example.com/"})
        )
        .await
    );

    browser::open(&serde_json::json!({"url": "https://shop.example.com/checkout"}))
        .await
        .unwrap();

    assert!(
        browser::sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://shop.example.com/checkout"})
        )
        .await
    );

    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g = snap_gen(&out);
    assert!(out.contains("Place order"), "got: {out}");
    assert!(
        browser::sensitive(
            "browser.click",
            &serde_json::json!({"ref": 0, "snapshot": g})
        )
        .await
    );

    let err = argus_lib::tools::exec(
        &gw,
        "browser.click",
        &serde_json::json!({"ref": 0, "snapshot": g}).to_string(),
        "ask",
        false,
        false,
        None,
    )
    .await
    .expect_err("mutating without approval must stay blocked");
    assert!(err.contains("asks before acting"), "got: {err}");

    argus_lib::tools::exec(
        &gw,
        "browser.click",
        &serde_json::json!({"ref": 0, "snapshot": g}).to_string(),
        "ask",
        false,
        true,
        None,
    )
    .await
    .expect("approved click runs");
    assert_eq!(state.lock().unwrap().clicks, 1);

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn sensitive_recovered_actions_stay_gated() {
    let _guard = serial();
    ext_harness(vec![two_el()]).await;

    browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    assert!(!browser::sensitive("browser.click", &serde_json::json!({"ref": 0})).await);
    assert!(
        browser::sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://github.com/login"})
        )
        .await
    );
}

#[tokio::test]
async fn navigation_stale_recovers_with_landed_check() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/a"}))
        .await
        .unwrap();
    assert!(!out.contains("landed on"), "got: {out}");
    let g1 = snap_gen(&out);

    let out = browser::open(&serde_json::json!({"url": "https://example.com/redirect-me"}))
        .await
        .unwrap();
    assert!(
        out.contains("landed on https://example.com/landed"),
        "got: {out}"
    );
    let g2 = snap_gen(&out);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("stale after navigation");
    assert!(err.contains(&format!("(snapshot {g2})")), "got: {err}");

    browser::click(&serde_json::json!({"ref": 1, "snapshot": snap_gen(&err)}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn rerender_stale_recovers_with_new_ref() {
    let _guard = serial();
    let state = ext_harness(vec![
        vec![El {
            kind: "button",
            label: "Go",
            path: "b1",
        }],
        vec![El {
            kind: "button",
            label: "Stop",
            path: "b2",
        }],
    ])
    .await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .unwrap();

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("re-rendered");
    assert!(err.contains("\"Stop\""), "got: {err}");

    browser::click(&serde_json::json!({"ref": 0, "snapshot": snap_gen(&err)}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().clicks, 2);
}

#[tokio::test]
async fn recovery_failure_is_one_structured_tool_result() {
    let _guard = serial();
    let state = ext_harness_full(vec![two_el()], Some("click".into())).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let g2 = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());

    let stale = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("stale");
    assert!(stale.contains(&format!("(snapshot {g2})")));

    let failed = browser::click(&serde_json::json!({"ref": 0, "snapshot": g2}))
        .await
        .expect_err("recovered click fails");
    assert_eq!(state.lock().unwrap().clicks, 0);

    let mut exec = argus_lib::tools::build_executions(
        &format!(r#"<action tool="browser.click">{{"ref":0,"snapshot":{g2}}}</action>"#),
        &[],
        0,
    )
    .pop()
    .unwrap();
    exec.begin();
    exec.fail(failed.clone());
    assert!(exec.status.is_terminal());
    let msg = exec.to_tool_result(4000);
    assert!(
        msg.contains(r#"tool="browser.click" status="err""#),
        "got: {msg}"
    );
    assert!(msg.contains("injected click failed"), "got: {msg}");
    assert_eq!(
        argus_lib::tools::recover::classify(&failed),
        argus_lib::tools::recover::RecoveryKind::Recoverable
    );
}
