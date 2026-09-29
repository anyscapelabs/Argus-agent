// Recovering an action from text that was cut off, mistyped, or written in a
// dialect we do not officially support. Every function here returns what it
// could recover and nothing more: a reply we cannot read becomes no action
// rather than a wrong one.
use std::fmt::Write as _;

use serde_json::Value;

use super::actions::tag_end;
use super::argtext::{arg_pairs, ARG_KEY};
use super::normalize::attrs_to_args;

fn salvage_browser_action(tag: &str) -> Option<(String, String)> {
    let tool = tag
        .split("action=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .or_else(|| {
            tag.split("action='")
                .nth(1)
                .and_then(|s| s.split('\'').next())
        })
        .unwrap_or("")
        .to_string();

    if !tool.starts_with("browser.") {
        return None;
    }

    let all = attrs_to_args(tag)?;
    let obj = serde_json::from_str::<serde_json::Map<String, Value>>(&all).ok()?;

    let mut kept = serde_json::Map::new();
    for k in ["ref", "text", "submit"] {
        if let Some(v) = obj.get(k) {
            kept.insert(k.into(), v.clone());
        }
    }

    if kept.is_empty() {
        return None;
    }

    Some((tool, Value::Object(kept).to_string()))
}

pub(super) fn salvage_browser_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;

    while let Some(start) = rest.find("<browser-action") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];

        let Some(te) = tag_end(tail) else {
            out.push_str(tail);
            break;
        };

        let tag = &tail[..te];
        let self_closed = tag.trim_end().ends_with('/');
        let close_tag = "</browser-action>";

        let consumed = if self_closed {
            te + 1
        } else if let Some(close) = tail.find(close_tag) {
            close + close_tag.len()
        } else {
            tail.len()
        };

        if let Some((tool, args)) = salvage_browser_action(tag) {
            let _ = write!(out, "<action tool=\"{tool}\">{args}</action>");
        } else {
            out.push_str(&tail[..consumed]);
        }

        rest = &tail[consumed..];
    }

    out.push_str(rest);
    out
}

pub(super) fn salvage_call(inner: &str) -> Option<(String, String)> {
    let body = inner.strip_prefix("<tool_call")?.trim_start();
    let body = body.strip_prefix('>').unwrap_or(body).trim();

    if body.starts_with('{') {
        let v: Value = serde_json::from_str(body).ok()?;
        let tool = v.get("name")?.as_str()?.to_string();
        let args = v
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        args.as_object()?;

        return Some((tool, args.to_string()));
    }

    let tool = head_name(body)?;
    let (args, _) = arg_pairs(body);

    (!args.is_empty()).then(|| (tool, Value::Object(args).to_string()))
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// The name is whatever runs before the first arg tag, on its own line or
/// not: a model that is not formatting puts the whole call on one line, and
/// reading only the first line swallowed the name and the args together.
pub(super) fn head_name(body: &str) -> Option<String> {
    body.split(ARG_KEY)
        .next()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .filter(|t| t.chars().all(name_char))
        .map(str::to_string)
}

/// Where the next call starts, in either shape. The wrapper is the common
/// one. The bare form is a model that wrote the name straight into the args
/// with no wrapper around it at all, which cost the reader the name and left
/// the arguments to be eaten as debris.
pub(super) fn next_call_start(text: &str) -> Option<usize> {
    match (text.find("<tool_call"), bare_call_start(text)) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (found, None) => found,
        (None, found) => found,
    }
}

/// The start of a wrapperless call: a tool name at the head of a line,
/// followed by the arguments. Anchoring on the name is what keeps ordinary
/// prose out of it, the same way vLLM limits its own recovery to a requested
/// tool name. A wrapper that never arrived leaves the name behind, and that
/// name is the only thing left that says which tool to run.
fn bare_call_start(text: &str) -> Option<usize> {
    let at = text.find(ARG_KEY)?;

    // The name is the run of name characters sitting against the tag, not
    // whatever prose happens to precede it: a model puts the call on its own
    // line, and the line before it is the user's sentence. The gap between
    // the two is whitespace, and it can be a newline.
    let head = text[..at].trim_end();
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| name_char(*c))
        .last()
        .map(|(i, _)| i)?;

    if start == head.len() {
        return None;
    }

    // Shape alone cannot tell a wrapperless call from a sentence that happens
    // to end in a tag — `runtime<arg_key>` is both. Position is what settles
    // it: a dropped wrapper leaves the name at the start of its line, and
    // guessing anywhere else runs a tool on the user's prose.
    if start > 0 && !head[..start].ends_with('\n') {
        return None;
    }

    Some(start)
}
