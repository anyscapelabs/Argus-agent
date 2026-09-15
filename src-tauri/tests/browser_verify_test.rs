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
            let tmp = std::env::temp_dir().join(format!("argus-bver-{}", std::process::id()));
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
    clicks: u32,
    fills: u32,
    tab_seq: i32,
    last_url: String,
    last_title: String,
    snapshots: Vec<Vec<El>>,
    nav_on_click: Option<(String, String)>,
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
                    s.last_title = "Example".into();
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
                    let title = s.last_title.clone();
                    serde_json::json!({
                        "url": url,
                        "title": title,
                        "text": "hello from fake page",
                    })
                }
                "click" => {
                    s.clicks += 1;
                    if let Some((url, title)) = s.nav_on_click.clone() {
                        s.last_url = url;
                        s.last_title = title;
                    }
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

async fn ext_harness(
    snapshots: Vec<Vec<El>>,
    nav_on_click: Option<(String, String)>,
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
        last_title: "Example".into(),
        snapshots,
        nav_on_click,
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
                    "<html><body><h1>Second page</h1></body></html>"
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

fn iso_profiles() -> PathBuf {
    ISO_ONCE
        .get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("argus-bver-iso-{}", std::process::id()));
            browser::init(dir.clone());
            dir
        })
        .clone()
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
async fn click_navigation_is_verified() {
    let _guard = serial();
    let state = ext_harness(
        vec![two_el()],
        Some(("https://example.com/second".into(), "Second".into())),
        None,
    )
    .await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert!(
        out.contains("verified: navigated to https://example.com/second"),
        "got: {out}"
    );
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn click_content_change_is_verified() {
    let _guard = serial();
    ext_harness(
        vec![
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
        ],
        None,
        None,
    )
    .await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert!(out.contains("verified: page content changed"), "got: {out}");
}

#[tokio::test]
async fn click_without_change_is_inconclusive_not_failure() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()], None, None).await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert!(!out.contains("verified"), "got: {out}");
    assert!(out.contains("[0] button \"Go\""), "got: {out}");
    assert_eq!(state.lock().unwrap().clicks, 1);
}

#[tokio::test]
async fn execution_failure_is_not_verification() {
    let _guard = serial();
    ext_harness(vec![two_el()], None, Some("click".into())).await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .expect_err("failing click");
    assert!(err.contains("injected click failed"), "got: {err}");
    assert!(!err.contains("verified"), "got: {err}");
}

#[tokio::test]
async fn navigation_url_is_verified() {
    let _guard = serial();
    ext_harness(vec![two_el()], None, None).await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/redirect-me"}))
        .await
        .unwrap();
    assert!(out.contains("url https://example.com/landed"), "got: {out}");
    assert!(
        out.contains("landed on https://example.com/landed"),
        "got: {out}"
    );

    let out = browser::open(&serde_json::json!({"url": "https://example.com/plain"}))
        .await
        .unwrap();
    assert!(out.contains("url https://example.com/plain"), "got: {out}");
    assert!(!out.contains("landed on"), "got: {out}");
}

#[tokio::test]
async fn typing_without_readback_stays_safe() {
    let _guard = serial();
    let state = ext_harness(
        vec![vec![El {
            kind: "input password",
            label: "Secret",
            path: "input",
        }]],
        None,
        None,
    )
    .await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let out = browser::type_text(&serde_json::json!({"ref": 0, "text": "hunter2-secret"}))
        .await
        .unwrap();
    assert!(!out.contains("hunter2-secret"), "got: {out}");
    assert_eq!(state.lock().unwrap().fills, 1);
    let _ = g;
}

#[tokio::test]
async fn verification_never_leaks_secrets() {
    let _guard = serial();
    ext_harness(
        vec![vec![El {
            kind: "button",
            label: "show sk-abcdefghijklmnopqrst",
            path: "button",
        }]],
        None,
        None,
    )
    .await;

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    assert!(out.contains("[redacted]"), "got: {out}");
    assert!(!out.contains("sk-abcdefghijklmnopqrst"), "got: {out}");

    let g = snap_gen(&out);
    let out = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert!(!out.contains("sk-abcdefghijklmnopqrst"), "got: {out}");
}

#[tokio::test]
async fn verification_failure_is_single_tool_result() {
    let _guard = serial();
    ext_harness(vec![two_el()], None, Some("click".into())).await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let failed = browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .expect_err("click fails");

    let mut exec = argus_lib::tools::build_executions(
        &format!(r#"<action tool="browser.click">{{"ref":0,"snapshot":{g}}}</action>"#),
        &[],
        0,
    )
    .pop()
    .unwrap();
    exec.begin();
    exec.fail(failed);
    assert!(exec.status.is_terminal());
    let results = vec![exec.to_tool_result(4000)];
    assert_eq!(results.len(), 1);
    assert!(
        results[0].contains(r#"status="err""#),
        "got: {}",
        results[0]
    );
}

#[tokio::test]
async fn verification_never_repeats_the_action() {
    let _guard = serial();
    let state = ext_harness(vec![two_el()], None, None).await;

    let g = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    browser::click(&serde_json::json!({"ref": 0, "snapshot": g}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().clicks, 1);

    browser::type_text(&serde_json::json!({"ref": 1, "text": "x"}))
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().fills, 1);
}

#[tokio::test]
async fn stale_recovery_still_works_with_verification() {
    let _guard = serial();
    ext_harness(vec![two_el()], None, None).await;

    let g1 = snap_gen(
        &browser::open(&serde_json::json!({"url": "https://example.com/"}))
            .await
            .unwrap(),
    );
    let g2 = snap_gen(&browser::read(&serde_json::json!({})).await.unwrap());
    assert_ne!(g1, g2);

    let err = browser::click(&serde_json::json!({"ref": 0, "snapshot": g1}))
        .await
        .expect_err("stale");
    assert!(err.contains(&format!("(snapshot {g2})")), "got: {err}");
    assert!(err.contains("[0] button \"Go\""), "got: {err}");
    assert_eq!(
        argus_lib::tools::recover::classify(&err),
        argus_lib::tools::recover::RecoveryKind::StaleReference
    );

    browser::click(&serde_json::json!({"ref": 0, "snapshot": snap_gen(&err)}))
        .await
        .unwrap();
}

#[tokio::test]
async fn iso_click_navigation_is_verified() {
    let _guard = serial();
    let _ = iso_profiles();
    let (base, _srv) = iso_http().await;

    let out = browser::open(&serde_json::json!({"url": format!("{base}/"), "profile": "ver-a"}))
        .await
        .expect("isolated open");
    let g = snap_gen(&out);
    let r = out
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with('[') && l.contains("Go second"))
        .and_then(|l| {
            l.strip_prefix('[')
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse::<usize>().ok())
        })
        .expect("link ref");

    let out = browser::click(&serde_json::json!({"ref": r, "snapshot": g, "profile": "ver-a"}))
        .await
        .expect("isolated click");
    assert!(out.contains("Second page"), "got: {out}");
    assert!(
        out.contains(&format!("verified: navigated to {base}/second")),
        "got: {out}"
    );
}
