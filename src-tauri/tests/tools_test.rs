use argus_lib::gateway::schema::ToolCall;
use argus_lib::tools::browser;
use argus_lib::tools::{
    build_executions, clip_ends, close_dangling_actions, normalize_actions, page_text,
    parse_actions, protocol_section, render_actions, section, web, PROTOCOL_MARKER,
};
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
}

// The tag goes, the answer stays. Dropping the tail with the tag is how a whole
// reply disappeared; keeping the tag is raw protocol syntax in the transcript.
#[test]
fn keeps_the_body_of_an_unclosed_tool_call() {
    assert_eq!(
        normalize_actions(
            "a <tool_call>\n{\"name\": \"agent.list\"\n\nHere is your summary anyway."
        ),
        "a \n{\"name\": \"agent.list\"\n\nHere is your summary anyway."
    );
}

// No `>` anywhere, so nothing marks where the tag stops and the text after it
// cannot be told from part of the tag. All that is left is the raw fragment.
#[test]
fn drops_a_tool_call_with_no_tag_end() {
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

#[test]
fn bot_wall_short_challenge_is_err() {
    use argus_lib::tools::web::bot_wall;

    let challenge = "--> DuckDuckGo Unfortunately, bots use DuckDuckGo too. Please complete the following challenge to confirm this search was made by a human.";
    assert!(bot_wall(challenge).is_some());

    let stub = "Google Search Please click here if you are not redirected within a few seconds.";
    assert!(stub.len() < 2000);
    assert!(bot_wall(stub).is_some());
}

#[test]
fn bot_wall_leaves_real_content_alone() {
    use argus_lib::tools::web::bot_wall;

    assert!(
        bot_wall("Big Data Analytics previous year papers: unit 1, unit 2, downloads.").is_none()
    );
    let long_captcha_essay = format!("captcha{}", " analysis of automated checks.".repeat(200));
    assert!(bot_wall(&long_captcha_essay).is_none());
}

#[test]
fn closes_action_block_truncated_by_stream_end() {
    let t = r#"Opening it now. <action tool="web.search">{"query":"big data papers"}"#;

    let out = close_dangling_actions(t);

    assert!(out.ends_with("</action>"), "got: {out}");
    let acts = parse_actions(&out);
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].tool, "web.search");
}

#[test]
fn leaves_completed_actions_and_plain_text_alone() {
    let done = r#"done <action tool="terminal">{"cmd":"ls"}</action>"#;
    assert_eq!(close_dangling_actions(done), done);

    let no_action = "plain text";
    assert_eq!(close_dangling_actions(no_action), no_action);
}

// A tag that opens but never carries a body it can run is not a tool call. It
// was reaching the transcript as raw syntax, and the tokenizer cannot guess
// where a tag with no `>` ends, so it printed as prose.
#[test]
fn strips_an_action_tag_with_nothing_runnable_in_it() {
    let garbage = r#"hmm <action tool="terminal">{"cmd":"#;

    assert_eq!(close_dangling_actions(garbage), "hmm");
}

// A tag cut off before its own `>` has no tool name and no body to salvage.
// It is still not a thing the user should read, and the tokenizer cannot guess
// where it ends, so it was reaching the transcript as literal text.
#[test]
fn strips_an_action_tag_that_never_closed() {
    let t = "All three reviewers are running.\n\n<action tool=\"agent_list";

    let out = close_dangling_actions(t);

    assert_eq!(out, "All three reviewers are running.");
}

// Nothing after an abandoned `<tool_call` is a tool call, and dropping it lost
// the rest of the answer with it.
#[test]
fn keeps_text_after_an_unclosed_tool_call() {
    let t = "<tool_call>\n{\"name\": \"agent.list\"\n\nHere is your summary anyway.";

    let out = normalize_actions(t);

    assert!(out.contains("Here is your summary anyway."), "got: {out}");
}

#[test]
fn salvages_invocation_style_browser_action_tags() {
    let t = "<browser-action id=\"a1\" ref=\"5\" text=\"hello\" submit=\"false\" action=\"browser.type\">browser.type 5 hello</browser-action>";
    let acts = parse_actions(&normalize_actions(t));
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].tool, "browser.type");
    let v: Value = serde_json::from_str(&acts[0].args).unwrap();
    assert_eq!(v["ref"], 5);
    assert_eq!(v["text"], "hello");
    assert_eq!(v["submit"], false);
}

#[test]
fn leaves_plain_history_browser_action_blocks_alone() {
    let t = "done <browser-action id=\"a0\" url=\"https://x.com\" action=\"browser.open\">browser.open https://x.com</browser-action>";
    let acts = parse_actions(&normalize_actions(t));
    assert!(acts.is_empty());
}

// The transcript is a rendering of what ran. A tool tag that did not become an
// execution — a native call with no tag, a text call the extractor took, an
// unparseable fragment the model abandoned mid-stream — must not reach the
// stored prose, because that prose is what the user reads.
#[test]
fn a_text_action_survives_only_as_a_canonical_tag() {
    let raw =
        "<step>read the file</step>\n<action tool=\"fs.read\">{\"path\":\"a.rs\"}</action>\ndone";
    let execs = build_executions(raw, &[], 0);
    let out = render_actions(raw, &execs);

    assert_eq!(execs.len(), 1);
    assert_eq!(
        out,
        "<step>read the file</step>\n<action tool=\"fs.read\">{\"path\":\"a.rs\"}</action>\ndone"
    );
}

#[test]
fn an_unparseable_action_tag_is_removed_from_the_prose() {
    // Half-written, no closing tag, body is not json. Nothing to execute, so
    // nothing the user should have to read.
    let raw = "here goes\n<action tool=\"fs.read\">{\"path\": ";
    let execs = build_executions(raw, &[], 0);
    let out = render_actions(raw, &execs);

    assert!(execs.is_empty());
    assert_eq!(out, "here goes\n");
}

#[test]
fn a_bare_action_tag_does_not_eat_the_answer_after_it() {
    let raw = "<action\nthe answer is 42";
    let out = render_actions(raw, &build_executions(raw, &[], 0));

    assert_eq!(out, "\nthe answer is 42");
}

#[test]
fn a_native_call_with_no_tag_is_written_into_the_prose() {
    let calls = [ToolCall {
        id: "c1".into(),
        name: "fs.read".into(),
        args: r#"{"path":"a.rs"}"#.into(),
    }];
    let execs = build_executions("reading it now", &calls, 0);
    let out = render_actions("reading it now", &execs);

    assert_eq!(execs.len(), 1);
    assert_eq!(
        out,
        "reading it now<action tool=\"fs.read\">{\"path\":\"a.rs\"}</action>"
    );
}

#[test]
fn a_native_call_and_the_same_text_call_render_once() {
    let calls = [ToolCall {
        id: "c1".into(),
        name: "fs.read".into(),
        args: r#"{"path":"a.rs"}"#.into(),
    }];
    let raw = r#"<action tool="fs.read">{"path":"a.rs"}</action>"#;
    let execs = build_executions(raw, &calls, 0);
    let out = render_actions(raw, &execs);

    assert_eq!(
        execs.len(),
        1,
        "the text copy is a duplicate of the native call"
    );
    assert_eq!(out.matches("<action").count(), 1, "got: {out}");
}

// Telling the model two ways to run a tool is why it used both, and an
// in-band tag is one malformed string away from the reader's screen. The API
// channel is the default; the in-band syntax survives only for a provider
// that has turned the payload down.
#[test]
fn the_default_prompt_teaches_no_in_band_tool_syntax() {
    let s = section(true);

    assert!(
        !s.contains(PROTOCOL_MARKER),
        "the default prompt still teaches an in-band action tag"
    );
    assert!(
        !s.contains("<tool_call>"),
        "the default prompt still teaches the tool-call tag"
    );
    assert!(
        !s.contains("Available tools:"),
        "the schemas are on the request; listing them again in prose is dead weight"
    );
    // Nothing in the API expresses how a turn ends, so this one has to stay.
    assert!(
        s.contains("<final/>"),
        "the turn terminator must still be taught"
    );
}

#[test]
fn the_degraded_prompt_is_marked_so_it_cannot_be_applied_twice() {
    let s = protocol_section();

    assert!(s.contains(PROTOCOL_MARKER));
    assert!(!section(true).contains(PROTOCOL_MARKER));
}

const ZWSP: &str = "\u{200b}";

// GLM writes its wrapper tag with a zero-width space in it, so a chat UI will
// not execute what it finds. Every match in the salvage path was on a literal
// `<tool_call`, so the whole call was neither run nor removed — it just landed
// in the transcript. Both shapes below are what that model actually emits: the
// wrapper hidden, and the whole call on one line.
#[test]
fn a_zero_width_wrapped_call_is_read_and_removed() {
    let t = format!(
        "Checking.\n<{ZWSP}tool_call>terminal<arg_key>command<arg_value>ls<arg_key>timeout<arg_value>30</{ZWSP}tool_call>"
    );
    let out = normalize_actions(&t);
    let acts = parse_actions(&out);

    assert_eq!(acts.len(), 1, "the call never ran: {out}");
    assert_eq!(acts[0].tool, "terminal");

    let v: Value = serde_json::from_str(&acts[0].args).unwrap();
    assert_eq!(v["command"], "ls");
    assert_eq!(v["timeout"], "30");

    assert!(!out.contains("arg_key"), "{out}");
    assert!(!out.contains("tool_call"), "{out}");
    assert!(out.contains("Checking."), "{out}");
}

#[test]
fn a_single_line_call_names_its_tool_before_the_first_arg() {
    let t = "<tool_call>web.search<arg_key>query<arg_value>rust 1.98</arg_key><arg_value>x</arg_value></tool_call>";
    let acts = parse_actions(&normalize_actions(t));

    assert_eq!(
        acts.len(),
        1,
        "the name and the args were read as one token"
    );
    assert_eq!(acts[0].tool, "web.search");
}

// The pairs do not always close. A stream cut mid-call leaves
// `<arg_key>k<arg_value>v<arg_key>k2<arg_value>v2`, where the next key is the
// only thing that ends a value.
#[test]
fn an_unclosed_arg_pair_is_still_a_pair() {
    let t = "<tool_call>terminal<arg_key>command<arg_value>sed -n '55,130p' f.ts<arg_key>timeout<arg_value>30</tool_call>";
    let acts = parse_actions(&normalize_actions(t));

    assert_eq!(
        acts.len(),
        1,
        "{}",
        acts.iter()
            .map(|a| a.tool.clone())
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(acts[0].tool, "terminal");

    let v: Value = serde_json::from_str(&acts[0].args).unwrap();
    assert_eq!(v["command"], "sed -n '55,130p' f.ts");
    assert_eq!(v["timeout"], "30");
}

// Salvage is not a filter. Whatever it could not read still must not be
// something a person reads, so the pair goes with its value rather than
// leaving `backgroundfalsecommandsed` welded into the prose.
#[test]
fn an_unclaimed_arg_soup_leaves_nothing_behind() {
    let t = "Resuming the review.\n<arg_key>background<arg_value>false<arg_key>command<arg_value>sed -n '55,130p' f.ts<arg_key>timeout<arg_value>30";
    let out = normalize_actions(t);

    assert_eq!(out, "Resuming the review.\n", "{out}");
}

#[test]
fn a_comparison_in_prose_is_not_a_tool_call() {
    let t = "if a < b and c > d, the <arg_key> is untouched";
    let out = normalize_actions(t);

    assert!(out.contains("if a < b and c > d"), "{out}");
    assert!(out.contains("the <arg_key> is untouched"), "{out}");
}
