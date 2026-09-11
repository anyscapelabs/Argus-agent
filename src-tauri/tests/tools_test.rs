use argus_lib::tools::browser;
use argus_lib::tools::{normalize_actions, parse_actions, web};
use serde_json::Value;

#[test]
fn salvages_native_key_value_tool_call() {
    let t = "<tool_callweb.search\n<arg_key>query</arg_key>\n<arg_value>\"Announcing Rust 1.98.0\" blog.rust-lang.org</arg_value>\n</tool_call>";
    let out = normalize_actions(t);

    assert!(
        out.contains(r#"<action tool="web.search">{"query":"#),
        "got: {out}"
    );

    let acts = parse_actions(&out);
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].tool, "web.search");

    let args: Value = serde_json::from_str(&acts[0].args).expect("args json");
    assert_eq!(
        args["query"],
        "\"Announcing Rust 1.98.0\" blog.rust-lang.org"
    );
}

#[test]
fn salvages_json_tool_call() {
    let t = "pre <tool_call{\"name\":\"web.read\",\"arguments\":{\"url\":\"https://x.y\"}}</tool_call> post";
    let acts = parse_actions(&normalize_actions(t));

    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].tool, "web.read");

    let args: Value = serde_json::from_str(&acts[0].args).expect("args json");
    assert_eq!(args["url"], "https://x.y");
    assert!(normalize_actions(t).starts_with("pre "));
    assert!(normalize_actions(t).ends_with(" post"));
}

#[test]
fn drops_unsalvageable_tool_call() {
    assert_eq!(normalize_actions("<tool_call???' </tool_call>"), "");
    assert_eq!(normalize_actions("a <tool_callweb.search b"), "a ");
}

#[test]
fn parses_ddg_html() {
    let html = r##"
      <div class="result">
      <a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&rut=abc" class="result__a">Ex<b>ample</b> One</a>
      <a class="result__snippet" href="#">The first &amp; best result</a>
      </div>
      <a rel="nofollow" href="https://direct.example.com/b" class="result__a">Direct Two</a>
      <a class="result__snippet" href="#">second snippet</a>
    "##;

    let out = web::parse_results(html);

    assert_eq!(out.len(), 2);
    assert_eq!(out[0].0, "Example One");
    assert_eq!(out[0].1, "https://example.com/a");
    assert_eq!(out[0].2, "The first & best result");
    assert_eq!(out[1].1, "https://direct.example.com/b");
}

#[test]
fn html_to_text_drops_scripts() {
    let html =
        "<html><script>var x=1;</script><style>a{}</style><body><p>hello world</p></body></html>";

    assert_eq!(web::html_to_text(html), "hello world");
}

#[test]
fn sanitizes_profile_names() {
    assert_eq!(browser::sanitize("My Work!"), "mywork");
    assert_eq!(browser::sanitize("../etc"), "etc");
    assert_eq!(browser::sanitize(""), "main");
    assert_eq!(browser::sanitize("main"), "main");
}

#[tokio::test]
async fn flags_sensitive_urls_and_labels() {
    assert!(
        browser::sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://github.com/login"})
        )
        .await
    );
    assert!(
        browser::sensitive(
            "browser.click",
            &serde_json::json!({"text": "Place order", "ref": 1})
        )
        .await
    );
    assert!(
        !browser::sensitive(
            "browser.open",
            &serde_json::json!({"url": "https://en.wikipedia.org/wiki/Rust"})
        )
        .await
    );
    assert!(!browser::sensitive("terminal", &serde_json::json!({"command": "ls"})).await);
}
