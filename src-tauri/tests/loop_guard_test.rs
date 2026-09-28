use argus_lib::sessions::chat::{budget_state, thrashing, turn_budget, Budget};

fn k(tool: &str, args: &str) -> (String, String) {
    (tool.into(), args.into())
}

fn term(cmd: &str) -> (String, String) {
    (
        String::from("terminal"),
        format!("{{\"command\":\"{cmd}\"}}"),
    )
}

// The sandbox burn pattern: stat works, listing is denied, the model retries
// the same listing with small variations until the turn dies of old age.
#[test]
fn variant_chain_trips_on_the_fourth_attempt() {
    let hist = vec![term("ls"), term("ls -la"), term("ls -la /tmp")];
    let failed = vec![true, true, true];

    assert!(
        thrashing(&hist, &failed, &term("ls -la /tmp | head")),
        "three failed variants plus a fourth must trip"
    );
}

// Retries differing only in volatile keys are the same attempt.
#[test]
fn volatile_only_differences_trip() {
    let hist = vec![
        k("terminal", r#"{"command":"cargo test","timeout":10}"#),
        k("terminal", r#"{"command":"cargo test","timeout":20}"#),
        k("terminal", r#"{"command":"cargo test","timeout":30}"#),
    ];
    let failed = vec![true, true, true];

    assert!(thrashing(
        &hist,
        &failed,
        &k("terminal", r#"{"command":"cargo test","timeout":60}"#),
    ));
}

#[test]
fn different_tools_never_trip() {
    let hist = vec![term("ls"), term("ls -la"), k("grep", r#"{"pattern":"x"}"#)];
    let failed = vec![true, true, true];

    assert!(!thrashing(&hist, &failed, &term("ls -la /tmp")));
}

// Sequential similar calls that succeed are multi-step work, not churn.
#[test]
fn successes_never_trip() {
    let hist = vec![term("ls"), term("ls -la"), term("ls -la /tmp")];
    let failed = vec![true, true, false];

    assert!(!thrashing(&hist, &failed, &term("pwd")));
    assert!(!thrashing(&hist, &[true, false, true], &term("pwd")));
}

#[test]
fn short_history_never_trips() {
    let hist = vec![term("ls"), term("ls -la")];

    assert!(!thrashing(&hist, &[true, true], &term("ls -la /tmp")));
    assert!(!thrashing(&[], &[], &term("ls")));
}

// Distinct tasks with the same tool are not a loop.
#[test]
fn dissimilar_args_never_trip() {
    let hist = vec![
        term("cat ~/notes.txt"),
        term("git status"),
        term("du -sh /tmp"),
    ];
    let failed = vec![true, true, true];

    assert!(!thrashing(&hist, &failed, &term("ps aux")));
}

// Unparseable bodies fall back to raw tokens rather than failing open.
#[test]
fn non_json_args_use_raw_tokens() {
    let hist = vec![
        k("terminal", "not-json-1 ls"),
        k("terminal", "not-json-1 ls -la"),
        k("terminal", "not-json-1 ls -la /tmp"),
    ];
    let failed = vec![true, true, true];

    assert!(thrashing(
        &hist,
        &failed,
        &k("terminal", "not-json-1 ls -la /tmp | head")
    ));
}

// Budget: a quarter of the window, so a 1M model is not throttled like a 32k
// one, and never so small that ordinary work cannot finish.
#[test]
fn budget_scales_with_the_window() {
    assert_eq!(turn_budget(0), 32_000, "unknown window gets the floor");
    assert_eq!(turn_budget(128_000), 32_000);
    assert_eq!(turn_budget(400_000), 100_000);
    assert_eq!(turn_budget(8_000_000), 200_000, "capped");
    assert!(turn_budget(200_000) >= 32_000);
}

#[test]
fn budget_warns_then_stops() {
    let b = 1000;

    assert_eq!(budget_state(0, b), Budget::Ok);
    assert_eq!(budget_state(699, b), Budget::Ok);
    assert_eq!(budget_state(700, b), Budget::Warn);
    assert_eq!(budget_state(999, b), Budget::Warn);
    assert_eq!(budget_state(1000, b), Budget::Stop);
    assert_eq!(budget_state(5_000_000, b), Budget::Stop);
}

// A non-positive budget must not stop every turn.
#[test]
fn a_broken_budget_never_stops_a_turn() {
    assert_eq!(budget_state(1_000_000, 0), Budget::Ok);
    assert_eq!(budget_state(1_000_000, -5), Budget::Ok);
}
