// Turning sloppy markup into the canonical action syntax. Models drop quotes,
// write bare tags, wrap calls in invisible characters, and return arguments in
// a dozen shapes. None of that is an error here — it is just text to rewrite.
use std::fmt::Write as _;

use serde_json::Value;

use super::actions::{close_dangling_actions, tag_end};
use super::argtext::{arg_pairs, strip_protocol};
use super::salvage::{head_name, next_call_start, salvage_browser_blocks, salvage_call};

fn coerce_val(v: &str) -> Value {
    match v {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => v
            .parse::<i64>()
            .ok()
            .map(Value::from)
            .or_else(|| {
                v.parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::from)
            })
            .unwrap_or_else(|| Value::String(v.into())),
    }
}

fn attr_val(rest: &str) -> Option<(&str, &str, usize)> {
    let eq = rest.find('=')?;
    let key = rest[..eq]
        .trim_end()
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .filter(|k| !k.is_empty())?;
    let after = rest[eq + 1..].trim_start();
    let lead = rest.len() - eq - 1 - after.len();

    if let Some(mark) = after.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let inner = &after[mark.len_utf8()..];
        let (val, end) = match inner.find(mark) {
            Some(e) => (&inner[..e], e + mark.len_utf8()),
            None => (inner, inner.len()),
        };

        return Some((key, val, eq + 1 + lead + mark.len_utf8() + end));
    }

    let val = after.split([' ', '/']).next().filter(|s| !s.is_empty())?;

    Some((key, val, eq + 1 + lead + val.len()))
}

pub(super) fn attrs_to_args(t: &str) -> Option<String> {
    let mut args = serde_json::Map::new();
    let mut rest = t;

    while let Some((key, val, used)) = attr_val(rest) {
        rest = &rest[used..];

        if !val.is_empty() && key != "tool" && key != "action" {
            args.insert(key.to_string(), coerce_val(val));
        }
    }

    (!args.is_empty()).then(|| Value::Object(args).to_string())
}

pub(super) fn coerce_args(tag: &str, body: &str) -> String {
    let t = body.trim();

    if serde_json::from_str::<Value>(t).is_ok() {
        return t.into();
    }

    attrs_to_args(tag).unwrap_or_else(|| if t.is_empty() { "{}".into() } else { t.into() })
}

pub(super) const TOOL_CALL_CLOSE: &str = "</tool_call>";

/// GLM and the Hermes line hide the wrapper tag behind a zero-width space, so a
/// chat UI will not auto-execute what it finds. Every match below is on a
/// literal `<tool_call`, so that one invisible byte defeated the entire salvage
/// path: the block was not recognised, not removed, and not run — it just
/// landed in the transcript, wrapper and all.
///
/// Nothing a model writes legitimately contains a zero-width character, and
/// leaving one in place only makes the transcript harder to read and search.
fn strip_invisibles(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' | '\u{00ad}'
            )
        })
        .collect()
}

pub fn normalize_actions(raw: &str) -> String {
    let text = strip_invisibles(raw);
    let mut out = String::new();
    let mut rest = text.as_str();

    while let Some(start) = next_call_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];

        // A wrapper that never arrived. The name is the only thing marking
        // where the call begins and the first arg is the only thing marking
        // where it ends, so the call runs exactly as far as its arguments do.
        if !tail.starts_with("<tool_call") {
            let (args, used) = arg_pairs(tail);

            if args.is_empty() {
                out.push_str(tail);
                break;
            }

            let span = &tail[..used];

            if let Some(tool) = head_name(span) {
                let _ = write!(
                    out,
                    "<action tool=\"{tool}\">{}</action>",
                    Value::Object(args)
                );
            }

            rest = &tail[used..];
            continue;
        }

        let Some(close) = tail.find(TOOL_CALL_CLOSE) else {
            // Cut the opening tag and keep what the model wrote inside it —
            // that body is the answer, and dropping the tail with the tag is
            // how a whole reply used to disappear. With no `>` there is nothing
            // to tell the tag from the text after it, so nothing is kept.
            if let Some(i) = tag_end(tail) {
                out.push_str(&tail[i + 1..]);
            }

            return strip_protocol(&out);
        };

        if let Some((tool, args)) = salvage_call(&tail[..close]) {
            let _ = write!(out, "<action tool=\"{tool}\">{args}</action>");
        }

        rest = &tail[close + TOOL_CALL_CLOSE.len()..];
    }

    out.push_str(rest);
    strip_protocol(&salvage_browser_blocks(&close_dangling_actions(&out)))
}
