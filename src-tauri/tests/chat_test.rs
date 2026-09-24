use argus_lib::sessions::chat::{clean_title, fakes_output};
use argus_lib::tools::{protocol_section, split_commit, FINAL_MARKER};

#[test]
fn closes_only_on_the_marker() {
    let (closed, text) = split_commit("The file says hello.\n<final/>");

    assert!(closed);
    assert_eq!(text, "The file says hello.");
}

#[test]
fn tolerates_spacing_and_trailing_whitespace() {
    let (closed, text) = split_commit("Answer here.  \n<final />\n\n  ");

    assert!(closed);
    assert_eq!(text, "Answer here.");
}

#[test]
fn an_empty_reply_closes() {
    let (closed, text) = split_commit("<final/>");

    assert!(closed);
    assert_eq!(text, "");
}

#[test]
fn leaves_unclosed_replies_untouched() {
    let (closed, text) = split_commit("Here is what I found.");

    assert!(!closed);
    assert_eq!(text, "Here is what I found.");
}

// The two turns that actually stalled in this repo's own transcript. Both use
// U+2019, which the old ASCII-only apostrophe class could never match.
#[test]
fn catches_the_real_stalls_that_beat_the_old_regex() {
    let a = "I\u{2019}ve located the repository. Next I\u{2019}ll read the documentation, \
then trace the call path.";
    let b = "I\u{2019}ll continue the audit, focusing on the agent loop.";

    assert!(
        !split_commit(a).0,
        "curly-apostrophe promise must not close"
    );
    assert!(
        !split_commit(b).0,
        "curly-apostrophe promise must not close"
    );
}

// A marker anywhere but the tail is not a close. This is what keeps a model
// from parking the token early and then describing work it never did.
#[test]
fn a_mid_reply_marker_does_not_close() {
    let (closed, text) = split_commit("<final/>\nNow let me open the browser and click.");

    assert!(!closed);
    assert!(text.contains("let me open the browser"));
}

#[test]
fn rejects_junk_titles() {
    assert_eq!(clean_title("<tool_call>web_search"), None);
    assert_eq!(clean_title("  "), None);
    assert_eq!(
        clean_title("\"Rust release notes\""),
        Some("Rust release notes".into())
    );
}

#[test]
fn strips_think_tags_from_replies() {
    use argus_lib::sessions::chat::sanitize_tags;

    assert_eq!(
        sanitize_tags("done.</think> Folder created."),
        "done. Folder created."
    );
    assert_eq!(
        sanitize_tags("<think>Checking sources.</think> Here you go."),
        "Checking sources. Here you go."
    );
}

#[test]
fn catches_faked_tool_output_blocks() {
    assert!(fakes_output(
        "<browser-action id=\"a1\" action=\"browser.click\">browser.click 15</browser-action>"
    ));
    assert!(fakes_output("done <terminal id=\"a0\">output</terminal>"));
    assert!(!fakes_output("Here is what I found."));
    assert!(!fakes_output(""));
}

// The marker is only useful if the model is told it exists.
#[test]
fn protocol_declares_the_close_marker() {
    let p = protocol_section();

    assert!(p.contains(FINAL_MARKER), "protocol must name the marker");
    assert!(
        p.contains("ends one of exactly two ways"),
        "protocol must state the two-way turn contract"
    );
}
