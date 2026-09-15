use argus_lib::tools::web::{read_with, search_with, WebConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Routes = Arc<Mutex<HashMap<String, (u16, String, String)>>>;

struct Mock {
    base: String,
    routes: Routes,
}

async fn start_mock() -> Mock {
    let routes: Routes = Arc::new(Mutex::new(HashMap::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let table = routes.clone();

    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let table = table.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let hit: Option<(u16, String, String)> = {
                    let routes = table.lock().unwrap();
                    let mut hit = None;
                    let mut best = 0usize;
                    for (prefix, resp) in routes.iter() {
                        if path.starts_with(prefix.as_str()) && prefix.len() > best {
                            best = prefix.len();
                            hit = Some(resp.clone());
                        }
                    }
                    hit
                };
                let (status, ct, body) =
                    hit.unwrap_or((404, "text/plain".into(), "mock: no route".into()));
                let reason = match status {
                    200 => "OK",
                    403 => "Forbidden",
                    404 => "Not Found",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    _ => "Error",
                };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    Mock { base, routes }
}

fn route(mock: &Mock, prefix: &str, status: u16, ct: &str, body: &str) {
    mock.routes.lock().unwrap().insert(
        prefix.to_string(),
        (status, ct.to_string(), body.to_string()),
    );
}

fn cfg(mock: &Mock) -> WebConfig {
    WebConfig {
        searxng_pool: vec![format!("{}/search", mock.base)],
        ddg_url: format!("{}/html", mock.base),
        jina_base: mock.base.clone(),
    }
}

fn search_args(q: &str) -> serde_json::Value {
    serde_json::json!({ "query": q })
}

fn read_args(url: &str) -> serde_json::Value {
    serde_json::json!({ "url": url })
}

const SEARXNG_HITS: &str = r#"{"results":[
{"title":"Rust 1.98 released","url":"https://blog.rust-lang.org/2026/01/01/Rust-1.98.html","content":"Announces Rust 1.98 with new lints"},
{"title":"Docs","url":"https://doc.rust-lang.org/","content":""}
]}"#;

const DDG_HITS: &str = r##"
<div class="result">
<a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&rut=abc" class="result__a">Ex<b>ample</b> One</a>
<a class="result__snippet" href="#">The first &amp; best result</a>
</div>"##;

#[tokio::test]
async fn successful_search_returns_structured_results() {
    let mock = start_mock().await;
    route(&mock, "/search", 200, "application/json", SEARXNG_HITS);

    let out = search_with(&search_args("rust release"), &cfg(&mock))
        .await
        .expect("search");

    assert!(
        out.contains("1. Rust 1.98 released — blog.rust-lang.org"),
        "got: {out}"
    );
    assert!(
        out.contains("https://blog.rust-lang.org/2026/01/01/Rust-1.98.html"),
        "got: {out}"
    );
    assert!(
        out.contains("Announces Rust 1.98 with new lints"),
        "got: {out}"
    );
    assert!(out.contains("2. Docs — doc.rust-lang.org"), "got: {out}");
    assert!(out.contains("(no snippet)"), "got: {out}");
    assert!(out.contains("Provider: searxng"), "got: {out}");
}

#[tokio::test]
async fn provider_failure_falls_back_deterministically() {
    let mock = start_mock().await;
    route(&mock, "/search", 500, "text/plain", "down");
    route(&mock, "/html", 200, "text/html", DDG_HITS);

    let out = search_with(&search_args("rust"), &cfg(&mock))
        .await
        .expect("fallback search");

    assert!(out.contains("Example One"), "got: {out}");
    assert!(out.contains("https://example.com/a"), "got: {out}");
    assert!(out.contains("Provider: duckduckgo"), "got: {out}");
}

#[tokio::test]
async fn all_providers_fail_returns_structured_error() {
    let mock = start_mock().await;
    route(&mock, "/search", 500, "text/plain", "down");
    route(&mock, "/html", 500, "text/plain", "down");

    let err = search_with(&search_args("rust"), &cfg(&mock))
        .await
        .expect_err("must fail");
    assert!(err.contains("All search providers failed"), "got: {err}");
    assert!(err.contains("searxng"), "got: {err}");
    assert!(err.contains("duckduckgo"), "got: {err}");
    assert!(err.contains("rust"), "got: {err}");
}

#[tokio::test]
async fn zero_legitimate_results_is_success_not_error() {
    let mock = start_mock().await;
    route(
        &mock,
        "/search",
        200,
        "application/json",
        r#"{"results":[]}"#,
    );
    route(
        &mock,
        "/html",
        200,
        "text/html",
        "<html><body>nothing</body></html>",
    );

    let out = search_with(&search_args("zzqqxx nonsense"), &cfg(&mock))
        .await
        .expect("empty is success");
    assert!(out.contains("No results found"), "got: {out}");
    assert!(out.contains("zzqqxx nonsense"), "got: {out}");
}

#[tokio::test]
async fn successful_page_read_preserves_source() {
    let mock = start_mock().await;
    route(
        &mock,
        "/http",
        200,
        "text/plain",
        "Jina reader text about Rust.",
    );
    let page = format!("{}/page", mock.base);

    let out = read_with(&read_args(&page), &cfg(&mock))
        .await
        .expect("read");
    assert!(out.contains(&format!("Source: {page}")), "got: {out}");
    assert!(out.contains("Jina reader text about Rust."), "got: {out}");
}

#[tokio::test]
async fn reader_failure_falls_back_to_direct() {
    let mock = start_mock().await;
    route(&mock, "/http", 500, "text/plain", "jina down");
    route(
        &mock,
        "/page",
        200,
        "text/html",
        "<html><head><title>T</title></head><body><p>Direct page body here.</p></body></html>",
    );
    let page = format!("{}/page", mock.base);

    let out = read_with(&read_args(&page), &cfg(&mock))
        .await
        .expect("direct fallback");
    assert!(out.contains(&format!("Source: {page}")), "got: {out}");
    assert!(out.contains("Direct page body here."), "got: {out}");
}

#[tokio::test]
async fn direct_read_failure_is_structured_error() {
    let mock = start_mock().await;
    route(&mock, "/http", 500, "text/plain", "jina down");
    route(&mock, "/gone", 404, "text/plain", "missing");
    let page = format!("{}/gone", mock.base);

    let err = read_with(&read_args(&page), &cfg(&mock))
        .await
        .expect_err("must fail");
    assert!(err.contains("page not found (404)"), "got: {err}");
    assert!(err.contains(&page), "got: {err}");
    assert_eq!(
        argus_lib::tools::recover::classify(&err),
        argus_lib::tools::recover::RecoveryKind::NotFound
    );
}

#[tokio::test]
async fn invalid_urls_are_rejected() {
    let mock = start_mock().await;
    for bad in ["ftp://example.com/x", "notaurl", "http://", ""] {
        let err = read_with(&read_args(bad), &cfg(&mock))
            .await
            .expect_err("bad url");
        assert!(
            err.contains("must start with")
                || err.contains("invalid URL")
                || err.contains("missing url"),
            "{bad} got: {err}"
        );
    }
    let err = read_with(
        &read_args("https://x.com/?key=sk-abcdefghijklmnopqrst"),
        &cfg(&mock),
    )
    .await
    .expect_err("credential url");
    assert!(err.contains("credential"), "got: {err}");
}

#[tokio::test]
async fn blocked_and_auth_pages_are_distinguished() {
    let mock = start_mock().await;
    route(&mock, "/http", 500, "text/plain", "jina down");
    route(
        &mock,
        "/denied",
        403,
        "text/html",
        "<html><body>no</body></html>",
    );
    route(
        &mock,
        "/bot",
        200,
        "text/html",
        "<html><body>Please complete the following challenge to confirm</body></html>",
    );

    let denied = format!("{}/denied", mock.base);
    let err = read_with(&read_args(&denied), &cfg(&mock))
        .await
        .expect_err("403");
    assert!(err.contains("forbids automated access (403)"), "got: {err}");
    assert_eq!(
        argus_lib::tools::recover::classify(&err),
        argus_lib::tools::recover::RecoveryKind::AuthenticationRequired
    );

    let bot = format!("{}/bot", mock.base);
    let err = read_with(&read_args(&bot), &cfg(&mock))
        .await
        .expect_err("bot");
    assert!(err.contains("bot check"), "got: {err}");
    assert_eq!(
        argus_lib::tools::recover::classify(&err),
        argus_lib::tools::recover::RecoveryKind::RateLimited
    );
}

#[tokio::test]
async fn empty_page_is_error_not_empty_success() {
    let mock = start_mock().await;
    route(&mock, "/http", 200, "text/plain", "   ");
    route(
        &mock,
        "/empty",
        200,
        "text/html",
        "<html><body>   </body></html>",
    );
    let page = format!("{}/empty", mock.base);

    let err = read_with(&read_args(&page), &cfg(&mock))
        .await
        .expect_err("empty");
    assert!(err.contains("no readable text"), "got: {err}");
}

#[tokio::test]
async fn oversized_page_is_clipped() {
    let mock = start_mock().await;
    let big = "row of content data\n".repeat(800);
    route(&mock, "/http", 200, "text/plain", &big);

    let out = read_with(&read_args(&format!("{}/big", mock.base)), &cfg(&mock))
        .await
        .expect("big read");
    assert!(out.contains("...[truncated]..."), "got len {}", out.len());
    assert!(out.contains("Source: "), "got: {out}");
}

#[tokio::test]
async fn search_result_url_flows_into_read() {
    let mock = start_mock().await;
    route(&mock, "/search", 200, "application/json", SEARXNG_HITS);
    route(&mock, "/http", 500, "text/plain", "jina down");
    route(
        &mock,
        "/2026/01/01/Rust-1.98.html",
        200,
        "text/html",
        "<html><body><p>Rust 1.98 notes body.</p></body></html>",
    );

    let hits = search_with(&search_args("rust 1.98"), &cfg(&mock))
        .await
        .expect("search");
    let url_line = hits
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("http"))
        .expect("result url line");
    assert!(url_line.contains("blog.rust-lang.org"), "got: {hits}");

    let mock_page = url_line.replace("https://blog.rust-lang.org", &mock.base);
    let out = read_with(&read_args(&mock_page), &cfg(&mock))
        .await
        .expect("read");
    assert!(out.contains("Rust 1.98 notes body."), "got: {out}");
    assert!(out.contains(&format!("Source: {mock_page}")), "got: {out}");
}
