use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_lib::tools::browser::{self, extpipe};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

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

async fn fake_extension(stream: UnixStream, seen_tabs: Arc<StdMutex<Vec<i64>>>) {
    let (mut rd, mut wr) = stream.into_split();
    while let Some(req) = read_frame(&mut rd).await {
        if req.get("kind").and_then(|k| k.as_str()) != Some("req") {
            continue;
        }
        let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let data = req.get("data").cloned().unwrap_or(serde_json::Value::Null);
        if method == "navigate" {
            if let Some(t) = data.get("tabId").and_then(|t| t.as_i64()) {
                seen_tabs.lock().unwrap().push(t);
            }
        }
        let resp_data = match method {
            "navigate" => serde_json::json!({
                "tabId": 7,
                "url": "https://example.com/",
                "title": "Example",
                "text": "hello from fake page",
            }),
            "snapshot" => serde_json::json!([
                {"kind": "button", "label": "Go", "path": "button"},
            ]),
            "read" => serde_json::json!({
                "url": "https://example.com/",
                "title": "Example",
                "text": "hello from fake page",
            }),
            _ => serde_json::json!({}),
        };
        write_frame(
            &mut wr,
            &serde_json::json!({"id": id, "ok": true, "data": resp_data}),
        )
        .await;
    }
}

fn sock_path(dir: &PathBuf) -> PathBuf {
    dir.join("native.sock")
}

#[tokio::test]
async fn extension_session_survives_across_tool_calls_on_one_tab() {
    let prev_xdg = std::env::var("XDG_DATA_HOME").ok();
    let tmp = std::env::temp_dir().join(format!("argus-bext-{}", std::process::id()));
    let data_dir = tmp.join("com.anyscapelabs.argus");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("extension.enabled"), b"on").unwrap();
    std::env::set_var("XDG_DATA_HOME", &tmp);

    let _ = std::fs::remove_file(sock_path(&data_dir));
    let listener = UnixListener::bind(sock_path(&data_dir)).unwrap();
    let seen_tabs: Arc<StdMutex<Vec<i64>>> = Arc::new(StdMutex::new(vec![]));

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(extpipe::serve_conn(stream));
        }
    });

    let seen = seen_tabs.clone();
    let client = UnixStream::connect(sock_path(&data_dir)).await.unwrap();
    tokio::spawn(fake_extension(client, seen));

    let mut ok = false;
    for _ in 0..100 {
        if extpipe::connected() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ok, "fake extension never connected");

    let out = browser::open(&serde_json::json!({"url": "https://example.com/"}))
        .await
        .unwrap();
    assert!(out.contains("https://example.com/"));

    let body = browser::read(&serde_json::json!({})).await.unwrap();
    assert!(body.contains("hello from fake page"));

    browser::click(&serde_json::json!({"ref": 0})).await.unwrap();

    browser::type_text(&serde_json::json!({"ref": 0, "text": "hi"}))
        .await
        .unwrap();

    browser::scroll(&serde_json::json!({"direction": "down", "pixels": 400}))
        .await
        .unwrap();

    browser::open(&serde_json::json!({"url": "https://example.org/"}))
        .await
        .unwrap();
    let tabs = seen_tabs.lock().unwrap().clone();
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[1], 7);

    browser::close(&serde_json::json!({})).await.unwrap();
    assert!(browser::read(&serde_json::json!({})).await.is_err());

    match prev_xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
