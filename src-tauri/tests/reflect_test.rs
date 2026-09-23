use argus_lib::sessions::chat::{check_block, parse_reflection_verdict, should_reflect};

#[test]
fn reflection_gates_on_toggle_work_and_budget() {
    assert!(should_reflect(true, 3, 0));
    assert!(!should_reflect(false, 3, 0));
    assert!(!should_reflect(true, 0, 0));
    assert!(!should_reflect(true, 5, 1));
}

#[test]
fn verdict_accepts_pass_forms_and_nothing_else() {
    assert_eq!(parse_reflection_verdict("PASS"), None);
    assert_eq!(parse_reflection_verdict("pass."), None);
    assert_eq!(parse_reflection_verdict("PASS: all good"), None);
    assert_eq!(parse_reflection_verdict("Pass\nExtra"), None);

    assert_eq!(
        parse_reflection_verdict("The reply misses the calendar step; ask for the date."),
        Some("The reply misses the calendar step; ask for the date.".into())
    );
    assert_eq!(
        parse_reflection_verdict("PASSABLE effort, but incomplete"),
        Some("PASSABLE effort, but incomplete".into())
    );
    assert_eq!(parse_reflection_verdict(""), Some("".into()));
}

#[test]
fn verdict_instruction_is_capped() {
    let long = "x".repeat(5000);
    let out = parse_reflection_verdict(&long).unwrap();
    assert_eq!(out.chars().count(), 2000);
}

#[test]
fn check_block_marks_pass_and_retry() {
    assert_eq!(check_block(true), "<check status=\"pass\"/>");
    assert_eq!(check_block(false), "<check status=\"retry\"/>");
}
