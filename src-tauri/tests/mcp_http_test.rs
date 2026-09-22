#[test]
fn http_client_sends_user_agent() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen_tx = seen.clone();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        *seen_tx.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).into_owned();
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}");
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        argus_lib::mcp::http_client()
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap();
    });

    handle.join().unwrap();

    let req = seen.lock().unwrap();
    assert!(
        req.to_lowercase().contains("user-agent: argus-agent"),
        "github rejects UA-less requests with 403; got:\n{req}"
    );
}
