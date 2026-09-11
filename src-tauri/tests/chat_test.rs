use argus_lib::sessions::chat::{claims_action, clean_title};

#[test]
fn catches_claim_without_block() {
    assert!(claims_action(
        "I'm opening MrBeast's YouTube channel in your Chrome."
    ));
    assert!(claims_action("I'll run the search now."));
    assert!(claims_action("Let me check that for you."));
}

#[test]
fn ignores_plain_answers() {
    assert!(!claims_action(
        "The latest video is on his channel, uploaded yesterday."
    ));
    assert!(!claims_action("Here is what I found."));
    assert!(!claims_action(""));
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
