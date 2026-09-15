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
            let tmp = std::env::temp_dir().join(format!("argus-brefs-{}", std::process::id()));
            let data_dir = tmp.join("com.anyscapelabs.argus");
            std::fs::create_dir_all(&data_dir).unwrap();
            std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
            std::env::set_var("XDG_DATA_HOME", &tmp);
            data_dir
        })
        .clone()
}

#[derive(Clone)]
struct El {
    kind: &'static str,
    label: &'static str,
    path: &'static str,
}

struct FakeState {
    navigates: Vec<i64>,
    clicks: u32,
    fills: u32,
    tab_seq: i32,
    last_url: String,
    snapshots: Vec<Vec<El>>,
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
        let resp_data = {
            let mut s = state.lock().unwrap();
            match method {
                "navigate" => {
                    if let Some(t) = data.get("tabId").and_then(|t| t.as_i64()) {
                        s.navigates.push(t);
                    }
                    s.tab_seq += 1;
                    s.last_url = data
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("https://example.com/")
                        .to_string();
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
        navigates: vec![],
        clicks: 0,
        fills: 0,
        tab_seq: 7,
        last_url: "https://example.com/".into(),
        snapshots,
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
async fn ext_open_snapshot_then_valid_ref_click_succeeds() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    assert!(out.contains("[0] button \"Go\""), "got: {out}");
    let g1 = snap_gen(&out);

    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .unwrap();
    let g2 = snap_gen(&out);
    assert_ne!(g1, g2);
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn ext_navigate_rejects_old_ref_as_stale() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/a"}))
        .await
        .unwrap();
    let g1 = snap_gen(&out);
    let out = browser::open(&serde_json::json!({"url": "https://example.com/b"}))
        .await
        .unwrap();
    let g2 = snap_gen(&out);
    assert_ne!(g1, g2);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("old snapshot ref must be stale");
    assert!(err.contains("stale ref 0"), "got: {err}");
    assert!(
        err.contains(&format!("snapshot {g2} is current")),
        "got: {err}"
    );
    assert_eq!(state.lock().unwrap().clicks, 0);
}

#[tokio::test]
async fn ext_changed_page_rejects_old_ref_without_action() {
    let _guard = serial();
    let state = ext_harness(vec![
        vec![El {
            kind: "button",
            label: "Go",
            path: "button",
        }],
        vec![El {
            kind: "button",
            label: "Stop",
            path: "button",
        }],
    ])
    .await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let g1 = snap_gen(&out);

    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .unwrap();
    assert!(out.contains("\"Stop\""), "got: {out}");
    assert_eq!(state.lock().unwrap().clicks, 1);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("superseded ref must be stale");
    assert!(err.contains("stale ref"), "got: {err}");
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn ext_fresh_snapshot_new_ref_succeeds() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g = snap_gen(&out);

    browser::click(&serde_json::json!({"ref": 1, "snapshot": g}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().clicks, 1);

    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g = snap_gen(&out);
    browser::type_text(&serde_json::json!({"ref": 1, "text": "hi", "snapshot": g}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().fills, 1);
}

#[tokio::test]
async fn ext_unknown_ref_is_structured_error() {
    let _guard = serial();
    ext_harness(vec![two_el()]).await;

    browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g = snap_gen(&out);

    for args in [
        serde_json::json!({"ref": 99, "snapshot": g}),
        serde_json::json!({"ref": 99}),
    ] {
        let err = browser::click(&args).await.expect_err("out of range");
        assert!(err.contains("unknown ref"), "got: {err}");
        assert_eq!(
            argus_lib::tools::recover::classify(&err),
            argus_lib::tools::recover::RecoveryKind::StaleReference
        );
    }
}

#[tokio::test]
async fn ext_snapshots_never_share_generations() {
    let _guard = serial();
    ext_harness(vec![two_el()]).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let g2 = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());
    let g3 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.org/"}))
            .await
            .unwrap(),
    );
    assert!(g1 != g2 && g2 != g3 && g1 != g3, "{g1} {g2} {g3}");
}

#[tokio::test]
async fn ext_restart_invalidates_old_refs() {
    let _guard = serial();
    ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let g_old = snap_gen(&out);
    browser::close(&serde_json::json!({})).await.unwrap();

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let g_new = snap_gen(&out);
    assert_ne!(g_old, g_new);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g_old}))
        .await
        .expect_err("pre-restart ref must be stale");
    assert!(err.contains("stale ref"), "got: {err}");
}

#[tokio::test]
async fn ext_multiple_refs_resolve_without_snapshot_token() {
    let _guard = serial();
    let state = ext_harness(vec![vec![
        El {
            kind: "button",
            label: "One",
            path: "b1",
        },
        El {
            kind: "button",
            label: "Two",
            path: "b2",
        },
        El {
            kind: "input",
            label: "Three",
            path: "i3",
        },
    ]])
    .await;

    browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    browser::click(&serde_json::json!({"ref": 0}))
        .await
        .unwrap();
    browser::click(&serde_json::json!({"ref": 1}))
        .await
        .unwrap();
    browser::type_text(&serde_json::json!({"ref": 2, "text": "x"}))
        .await
        .unwrap();

    let s = state.lock().unwrap();
    assert_eq!(s.clicks, 2);
    assert_eq!(s.fills, 1);
}

#[test]
fn stale_errors_reach_recovery_as_stale_reference() {
    for msg in [
        "stale ref 0 from snapshot 3 — snapshot 5 is current; run browser.read for a fresh list",
        "stale ref — run browser.read for a fresh element list",
        "unknown ref — run browser.open or browser.read for a fresh list",
    ] {
        assert_eq!(
            argus_lib::tools::recover::classify(msg),
            argus_lib::tools::recover::RecoveryKind::StaleReference,
            "{msg}"
        );
    }
    assert!(!argus_lib::tools::recover::RecoveryKind::StaleReference.may_retry());
}

#[tokio::test]
async fn ext_stale_rejection_sends_no_browser_request() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()]).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    let g1 = snap_gen(&out);
    let out = browser::read(&serde_json::json!({})).await.unwrap();
    let g2 = snap_gen(&out);
    assert_ne!(g1, g2);

    let before = {
        let s = state.lock().unwrap();
        (s.clicks, s.fills)
    };
    assert!(
        browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
            .await
            .is_err()
    );
    assert!(
        browser::type_text(&serde_json::json!({"ref": 0, "text": "x", "snapshot": g1}))
            .await
            .is_err()
    );
    let after = {
        let s = state.lock().unwrap();
        (s.clicks, s.fills)
    };
    assert_eq!(before, after);
}

static ISO_ONCE: OnceLock<PathBuf> = OnceLock::new();

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
                    "<html><body><a href=\"/second\">Go second</a><input placeholder=\"Name\"></body></html>"
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

fn iso_profiles() -> PathBuf {
    ISO_ONCE
        .get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("argus-brefs-iso-{}", std::process::id()));
            browser::init(dir.clone());
            dir
        })
        .clone()
}

fn link_ref(out: &str) -> usize {
    out.lines()
        .map(str::trim)
        .find(|l| l.starts_with('[') && l.contains("Go second"))
        .and_then(|l| {
            l.strip_prefix('[')
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse().ok())
        })
        .expect("link ref for Go second")
}

#[tokio::test]
async fn iso_open_snapshot_click_current_gen_succeeds() {
    let _guard = serial();
    let _ = iso_profiles();
    let (base, _srv) = iso_http().await;

    let out = browser::open(&serde_json::json!({"url": format!("{base}/"), "profile": "iso-a"}))
        .await
        .expect("isolated open");
    assert!(out.contains("(snapshot "), "got: {out}");
    let g1 = snap_gen(&out);
    let r = link_ref(&out);

    let out = browser::click(&serde_json::json!({"ref": r, "snapshot": g1, "profile": "iso-a"}))
        .await
        .expect("isolated click");
    assert!(out.contains("Second page"), "got: {out}");
    assert_ne!(snap_gen(&out), g1);
}

#[tokio::test]
async fn iso_old_gen_rejected_after_navigate() {
    let _guard = serial();
    let _ = iso_profiles();
    let (base, _srv) = iso_http().await;

    let out = browser::open(&serde_json::json!({"url": format!("{base}/"), "profile": "iso-b"}))
        .await
        .expect("isolated open");
    let g1 = snap_gen(&out);
    let out =
        browser::open(&serde_json::json!({"url": format!("{base}/second"), "profile": "iso-b"}))
            .await
            .expect("isolated navigate");
    assert_ne!(snap_gen(&out), g1);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1, "profile": "iso-b"}))
        .await
        .expect_err("old generation must be stale");
    assert!(err.contains("stale ref"), "got: {err}");
    assert_eq!(
        argus_lib::tools::recover::classify(&err),
        argus_lib::tools::recover::RecoveryKind::StaleReference
    );
}
