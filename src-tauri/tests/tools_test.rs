use argus_lib::tools::browser;
use argus_lib::tools::{clip_ends, normalize_actions, page_text, parse_actions, web};
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

#[test]
fn blocks_urls_carrying_credentials() {
    assert!(browser::url_guard("https://x.com/?key=sk-abcdefghijklmnopqrst").is_err());
    assert!(browser::url_guard("https://x.com/?key=sk%2Dabcdefghijklmnopqrst").is_err());
    assert!(browser::url_guard("https://x.com/?t=ghp_abcdefghijklmnopqrstuvwx").is_err());
    assert!(browser::url_guard("https://x.com/?t=AKIAABCDEFGHIJKLMNOP").is_err());
    assert!(browser::url_guard("https://x.com/?h=a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4").is_err());
    assert!(browser::url_guard("https://en.wikipedia.org/wiki/Rust").is_ok());
    assert!(browser::url_guard("https://x.com/?q=skateboard").is_ok());
}

#[test]
fn redacts_token_shapes_only() {
    assert_eq!(
        browser::redact("key sk-abcdefghijklmnopqrst end"),
        "key [redacted] end"
    );
    assert_eq!(browser::redact("plain prose stays"), "plain prose stays");
    assert_eq!(
        browser::redact("visit skateboard.com"),
        "visit skateboard.com"
    );
}

#[test]
fn page_text_caches_full_text_when_truncated() {
    let short = "just a short page";
    assert_eq!(page_text(short), short);

    let long = "row of pricing data\n".repeat(800);
    let out = page_text(&long);

    assert!(out.contains("...[truncated]..."));
    assert!(
        out.contains("full text saved to "),
        "truncated output must point at the cached full text"
    );
}

#[test]
fn clip_ends_prefers_line_boundaries() {
    let mut s = String::new();

    for i in 0..100 {
        s.push_str(&format!("line {i}: {}\n", "x".repeat(60)));
    }

    let out = clip_ends(s);
    let (head, rest) = out.split_once("...[truncated]...").expect("truncated");

    assert!(out.starts_with("line 0"));
    assert!(head.ends_with('\n'), "head should end at a line boundary");
    assert!(rest.trim_start_matches('\n').starts_with("line "));

    let flat = "y".repeat(9000);
    assert!(clip_ends(flat).starts_with("yyyy"));
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

#[tokio::test]
async fn run_stream_returns_when_shell_exits_even_if_pipe_held() {
    let start = std::time::Instant::now();
    let args: Value = serde_json::json!({"command": "echo start; (sleep 30 &) ; exit 0"});

    let (out, code) = argus_lib::tools::shell::run_stream(&args, 0, None)
        .await
        .expect("run_stream");

    assert_eq!(code, 0);
    assert!(out.contains("start"), "got: {out}");
    assert!(
        start.elapsed().as_secs() < 20,
        "hung on a pipe held by a detached child"
    );
}

#[test]
fn coerces_attribute_style_actions_to_json_args() {
    let t = r#"do it <action tool="computer.click" x="96" y="740" double="false"></action> done"#;
    let acts = parse_actions(t);

    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].tool, "computer.click");
    let args: Value = serde_json::from_str(&acts[0].args).expect("coerced json");
    assert_eq!(args["x"], 96);
    assert_eq!(args["y"], 740);
    assert_eq!(args["double"], false);
}

#[test]
fn empty_and_quoted_attribute_args() {
    let acts = parse_actions(r#"<action tool="computer.observe"></action>"#);
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].args, "{}");

    let acts = parse_actions(r#"<action tool='computer.type' text="hello world">junk</action>"#);
    assert_eq!(acts[0].tool, "computer.type");
    let args: Value = serde_json::from_str(&acts[0].args).expect("coerced json");
    assert_eq!(args["text"], "hello world");

    let acts = parse_actions(r#"<action tool="computer.type" text="say > ok">junk</action>"#);
    let args: Value = serde_json::from_str(&acts[0].args).expect("coerced json");
    assert_eq!(args["text"], "say > ok");
}

#[test]
fn json_body_actions_stay_untouched() {
    let acts = parse_actions(r#"<action tool="computer.click">{"x":100,"y":200}</action>"#);
    assert_eq!(acts[0].args, r#"{"x":100,"y":200}"#);
}

#[test]
fn attribute_args_cover_quote_styles_and_coercions() {
    let acts =
        parse_actions(r#"<action tool="t" a='single' b=bare c="3.5" d="" e="a=b"></action>"#);
    assert_eq!(acts.len(), 1);
    let args: Value = serde_json::from_str(&acts[0].args).expect("coerced json");
    assert_eq!(args["a"], "single");
    assert_eq!(args["b"], "bare");
    assert_eq!(args["c"], 3.5);
    assert!(args.get("d").is_none());
    assert_eq!(args["e"], "a=b");
}
