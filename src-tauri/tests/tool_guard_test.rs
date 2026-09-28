use argus_lib::gateway::schema::ToolCall;
use argus_lib::tools::{build_executions, normalize_actions, strip_actions, ToolStatus};

fn native(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        args: args.into(),
    }
}

// An empty object from the text channel is breakage, not a call. It must fail
// loudly with its arguments named, never run, and never mint `arguments: "{}"`.
#[test]
fn empty_text_args_fail_instead_of_running() {
    let execs = build_executions(r#"<action tool="terminal">{}</action>"#, &[], 0);

    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].status, ToolStatus::Failed);
    assert!(execs[0]
        .error
        .as_deref()
        .unwrap_or("")
        .contains("empty arguments"));
    assert!(
        execs[0].result.is_none(),
        "nothing ran, so nothing returned"
    );
}

// Native calls are provider-validated, including legitimately arg-less ones.
// The guard is text-channel only.
#[test]
fn empty_native_args_still_run() {
    let execs = build_executions("go", &[native("c1", "skill.list", "{}")], 0);

    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].status, ToolStatus::Created);
}

// Non-empty text calls are untouched by the guard.
#[test]
fn text_call_with_args_still_runs() {
    let execs = build_executions(
        r#"<action tool="terminal">{"command":"ls"}</action>"#,
        &[],
        0,
    );

    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].status, ToolStatus::Created);
}

// The reported leak shapes leave no protocol debris behind after normalize.
#[test]
fn normalized_calls_leave_no_wrapper_debris() {
    let out = normalize_actions(
        "<tool_call>terminal<arg_key>command</arg_key><arg_value>ls</arg_value></tool_call>",
    );

    assert!(out.contains("<action tool=\"terminal\""), "{out}");
    assert!(!out.contains("arg_key"), "{out}");
    assert!(!out.contains("tool_call"), "{out}");
}

// History handed to the model carries no executable markup.
#[test]
fn history_text_carries_no_actions() {
    let stored = r#"did it <action tool="terminal">{"command":"ls"}</action> done"#;

    let hist = strip_actions(stored);

    assert!(!hist.contains("<action"), "{hist}");
    assert!(hist.contains("did it"), "{hist}");
    assert!(hist.contains("done"), "{hist}");
}
