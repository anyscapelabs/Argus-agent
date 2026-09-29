// The ingress decision: what a model's text claims it did, and what it
// actually ran. `build_executions` is the one place a text reply becomes
// executions; the two `has_*` checks are the safety net that stops a reply
// from describing an action no call backs.
use std::fmt::Write as _;

use serde_json::Value;

use super::actions::{parse_actions, tag_end};
use crate::gateway::schema::ToolCall;
use crate::tools::{takes_no_args, ToolCallStyle, ToolExecution};

fn args_equal(a: &str, b: &str) -> bool {
    let va: Result<Value, _> = serde_json::from_str(a.trim());
    let vb: Result<Value, _> = serde_json::from_str(b.trim());
    match (va, vb) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.trim() == b.trim(),
    }
}

fn args_is_empty_object(args: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(args.trim()) else {
        return false;
    };

    v.as_object().is_some_and(|m| m.is_empty())
}

pub fn build_executions(
    base_text: &str,
    native_calls: &[ToolCall],
    act_base: usize,
) -> Vec<ToolExecution> {
    build_executions_styled(base_text, native_calls, act_base, ToolCallStyle::GlmXml)
}

// Style decides what the text channel may produce. Native models called
// through the API, so their prose is never executed — not even when it looks
// like a call. Template models emit both channels, so the text is decoded and
// deduped; degraded models have no API channel left, so the text is all there is.
pub fn build_executions_styled(
    base_text: &str,
    native_calls: &[ToolCall],
    act_base: usize,
    style: ToolCallStyle,
) -> Vec<ToolExecution> {
    let mut out: Vec<ToolExecution> = Vec::new();
    let mut idx = act_base;

    for c in native_calls {
        let e = ToolExecution::from_native(c, idx);
        idx += 1;
        out.push(e);
    }

    if style == ToolCallStyle::Native {
        return out;
    }

    for a in parse_actions(base_text) {
        if a.tool.trim().is_empty() {
            continue;
        }
        // An empty object from the text channel is breakage, not a call: no
        // provider validated it, and running it only mints `arguments: "{}"`
        // history the model then imitates. Fail loudly so the model re-sends
        // with arguments instead of executing nothing. Zero-arg tools are the
        // exception: `{}` is their documented invocation.
        if args_is_empty_object(&a.args) && !takes_no_args(&a.tool) {
            let mut e = ToolExecution::from_action(&a, idx);
            idx += 1;
            e.fail(format!(
                "call for `{}` arrived with empty arguments — nothing ran; send it again with its arguments",
                a.tool
            ));
            out.push(e);
            continue;
        }
        let dup = out
            .iter()
            .any(|e| e.tool == a.tool && args_equal(&e.args, &a.args));
        if dup {
            continue;
        }
        let e = ToolExecution::from_action(&a, idx);
        idx += 1;
        out.push(e);
    }

    out
}

// True when the text channel duplicates a native call: same tool, same args.
// That pair is the signature of an XML-template model, not a failure — the
// native call already ran, so the text twin must never execute again.
pub fn has_native_text_duplicate(base_text: &str, native_calls: &[ToolCall]) -> bool {
    if native_calls.is_empty() {
        return false;
    }

    let found = parse_actions(base_text);

    if found.is_empty() {
        return false;
    }

    found.iter().any(|a| {
        native_calls
            .iter()
            .any(|c| c.name == a.tool && args_equal(&c.args, &a.args))
    })
}

pub fn has_orphaned_action_block(text: &str) -> bool {
    if !text.contains("<action") {
        return false;
    }
    parse_actions(text).is_empty()
}

// Every span that reads as a tool tag, parseable or not. A span the extractor
// rejects is still a span: leaving it behind is how a half-typed
// `<action tool="x"` ends up in the transcript as text the user has to read.
fn action_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut off = 0usize;
    let mut rest = text;

    while let Some(start) = rest.find("<action") {
        let tail = &rest[start..];
        let end = match tail.find("</action>") {
            Some(i) => i + "</action>".len(),
            // No closing tag. The tag is open, so whatever follows the `>` on
            // this line is a payload the model never finished — abandoned json,
            // not something to read.
            None => match tag_end(tail) {
                Some(i) => match tail[i + 1..].find('\n') {
                    Some(nl) => i + 1 + nl,
                    None => tail.len(),
                },
                // No `>` either, so there is no payload to account for. Take
                // the tag name and leave the rest alone; swallowing it would
                // eat the answer the tag interrupted.
                None => "<action".len(),
            },
        };

        out.push((off + start, off + start + end));
        off += start + end;
        rest = &tail[end..];
    }

    out
}

/// The tool markup is a rendering of what ran, not something the model said.
/// It goes back to the model on the `tool_calls` field and the tool result
/// that follows it, so leaving a copy in the prose hands the model a turn that
/// claims an action no call backs — and re-teaches the syntax it just retired.
pub fn strip_actions(text: &str) -> String {
    render_actions(text, &[])
}

/// The transcript is a rendering of what ran, not a copy of what the model
/// typed. A tool tag survives only if it became an execution: the tag is
/// rewritten canonical in the place the model put it, a native call with no
/// tag of its own is appended, and anything the extractor would not take is
/// gone from the prose either way.
///
/// Extraction and display stop sharing a failure mode here. If a tag cannot be
/// executed it is still not something the user should be shown.
pub fn render_actions(text: &str, execs: &[ToolExecution]) -> String {
    let spans = action_spans(text);

    if spans.is_empty() && execs.is_empty() {
        return text.to_string();
    }

    let found = parse_actions(text);
    let mut used = vec![false; execs.len()];
    let mut out = String::new();
    let mut prev = 0usize;

    for (start, end) in spans {
        out.push_str(&text[prev..start]);
        prev = end;

        let Some(a) = found.iter().find(|a| a.start == start && a.end == end) else {
            continue;
        };

        let Some(i) = (0..execs.len())
            .find(|&i| !used[i] && execs[i].tool == a.tool && args_equal(&execs[i].args, &a.args))
        else {
            continue;
        };

        used[i] = true;
        let _ = write!(out, "<action tool=\"{}\">{}</action>", a.tool, a.args);
    }

    out.push_str(&text[prev..]);

    for (i, e) in execs.iter().enumerate() {
        if used[i] || e.tool.trim().is_empty() {
            continue;
        }

        let _ = write!(out, "<action tool=\"{}\">{}</action>", e.tool, e.args);
    }

    out
}

pub const FINAL_MARKER: &str = "<final/>";

pub fn split_commit(text: &str) -> (bool, String) {
    let t = text.trim_end();

    if let Some(head) = t.strip_suffix(FINAL_MARKER) {
        return (true, head.trim_end().to_string());
    }

    if let Some(head) = t.strip_suffix("<final />") {
        return (true, head.trim_end().to_string());
    }

    (false, text.to_string())
}
