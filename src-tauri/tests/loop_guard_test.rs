use argus_lib::sessions::guards::{budget_state, thrashing, turn_budget, Budget};

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

// The two moves the tool description tells the agent to make when a
// foreground call keeps failing must not read as the same attempt.
#[test]
fn backgrounding_and_escalating_are_not_churn() {
    let build = |bg: &str| {
        (
            String::from("terminal"),
            format!("{{\"command\":\"cargo build\"{bg}}}"),
        )
    };
    let net = |priv_: &str| {
        (
            String::from("terminal"),
            format!("{{\"command\":\"netstat -tlnp\"{priv_}}}"),
        )
    };

    // Repeating the same backgrounded build is still churn.
    let bg = build(",\"background\":true");
    assert!(thrashing(
        &[bg.clone(), bg.clone(), bg.clone()],
        &[true, true, true],
        &bg,
    ));

    // Moving to background after three foreground failures is the recovery.
    assert!(!thrashing(
        &[build(""), build(""), build("")],
        &[true, true, true],
        &build(",\"background\":true"),
    ));

    // Escalating after "operation not permitted" is the recovery.
    assert!(!thrashing(
        &[net(""), net(""), net("")],
        &[true, true, true],
        &net(",\"privilege\":\"admin\""),
    ));
}

// Two different files with the same body are two different files. Value
// tokens alone cannot tell them apart; the path can.
#[test]
fn different_files_with_the_same_body_do_not_trip() {
    // No quotes in the body: an unescaped one makes this invalid JSON, and
    // the point of the case is a well-formed pair of long and short args.
    let body = "fn main() { let x = 1; } // the same body in every file";
    let w = |p: &str| {
        (
            String::from("fs.write"),
            format!("{{\"path\":\"{p}\",\"content\":\"{body}\"}}"),
        )
    };

    assert!(!thrashing(
        &[w("a.rs"), w("b.rs"), w("c.rs")],
        &[true, true, true],
        &w("d.rs"),
    ));

    // ...but the same file, retried, is churn.
    let same = w("a.rs");
    assert!(thrashing(
        &[same.clone(), same.clone(), same.clone()],
        &[true, true, true],
        &same,
    ));
}

// Drift compounds: every neighbour can look alike while the ends diverge.
// Adjacent similarity alone is not enough.
#[test]
fn a_drifting_chain_does_not_trip_when_the_ends_differ() {
    let c = |s: &str| (String::from("terminal"), format!("{{\"command\":\"{s}\"}}"));

    assert!(!thrashing(
        &[
            c("a1 a2 a3 a4 a5 a6"),
            c("a2 a3 a4 a5 a6 b1"),
            c("a3 a4 a5 a6 b1 b2"),
        ],
        &[true, true, true],
        &c("a4 a5 a6 b1 b2 b3"),
    ));
}

// Boolean-only and nested args carry no comparable tokens. No evidence is not
// evidence of sameness.
#[test]
fn uninformative_args_do_not_trip() {
    let b = |v: &str| (String::from("mystery"), format!("{{\"flag\":{v}}}"));

    assert!(!thrashing(
        &[b("true"), b("true"), b("true")],
        &[true, true, true],
        &b("false")
    ));

    let n = |d: &str| {
        (
            String::from("mystery"),
            format!("{{\"a\":{{\"deep\":{d}}}}}"),
        )
    };
    assert!(!thrashing(
        &[n("1"), n("2"), n("3")],
        &[true, true, true],
        &n("4")
    ));
}

// The raw-token fallback must not fail OPEN: junk that shares nothing is
// different, even though both sides are unparseable.
#[test]
fn unparseable_junk_must_not_trip() {
    let j = |s: &str| (String::from("terminal"), s.to_string());

    assert!(!thrashing(
        &[
            j("alpha bravo charlie delta"),
            j("echo zulu yankee xray"),
            j("printf whiskey victor"),
        ],
        &[true, true, true],
        &j("completely other words entirely"),
    ));

    // Identical unparseable bodies still trip, by the exact-equality branch.
    assert!(thrashing(
        &[j("xx yy zz"), j("xx yy zz"), j("xx yy zz")],
        &[true, true, true],
        &j("xx yy zz"),
    ));
}

#[test]
fn a_success_at_any_window_position_blocks_the_trip() {
    let hist = vec![term("ls"), term("ls -la"), term("ls -la /tmp")];

    for failed in [
        vec![false, true, true],
        vec![true, false, true],
        vec![true, true, false],
    ] {
        assert!(
            !thrashing(&hist, &failed, &term("ls -la /tmp | head")),
            "a success anywhere in the window must exempt it: {failed:?}"
        );
    }
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
