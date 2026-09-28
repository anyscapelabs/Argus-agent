// Reading a tool call back out of a model's text.
//
// The formats here are not one format. A model may write the template XML, a
// bare tag, a native call, or a partial one that was cut off mid-write. Every
// parser below takes a string a model produced and returns what it could
// recover, and none of them fails: an unparseable fragment yields nothing
// rather than an error, because the loop above already treats a reply that
// describes an action without running one as something to ask about.

use serde_json::Value;

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

use super::{Action, ToolCallStyle, ToolExecution};
use crate::gateway::schema::ToolCall;

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
        // with arguments instead of executing nothing.
        if args_is_empty_object(&a.args) {
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
        out.push_str(&format!("<action tool=\"{}\">{}</action>", a.tool, a.args));
    }

    out.push_str(&text[prev..]);

    for (i, e) in execs.iter().enumerate() {
        if used[i] || e.tool.trim().is_empty() {
            continue;
        }

        out.push_str(&format!("<action tool=\"{}\">{}</action>", e.tool, e.args));
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

pub fn parse_actions(text: &str) -> Vec<Action> {
    let mut out = vec![];
    let mut rest = text;
    let mut off = 0usize;

    while let Some(start) = rest.find("<action") {
        let tail = &rest[start..];
        let Some(end) = tail.find("</action>") else {
            if let Some((tool, args)) = salvage_dangling(tail) {
                out.push(Action {
                    tool,
                    args,
                    start: off + start,
                    end: off + start + tail.len(),
                });
            }

            break;
        };

        let blk = &tail[..end];
        let tool = blk
            .split("tool=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .or_else(|| {
                blk.split("tool='")
                    .nth(1)
                    .and_then(|s| s.split('\'').next())
            })
            .unwrap_or("")
            .to_string();

        let args = match tag_end(blk) {
            Some(i) => coerce_args(&blk[..i], &blk[i + 1..]),
            None => "{}".into(),
        };

        if !tool.is_empty() {
            out.push(Action {
                tool,
                args,
                start: off + start,
                end: off + start + end + 9,
            });
        }

        off += start + end + 9;
        rest = &tail[end + 9..];
    }

    out
}

fn tag_end(blk: &str) -> Option<usize> {
    let b = blk.as_bytes();
    let mut i = 0usize;
    let mut q = 0u8;

    while i < b.len() {
        if q != 0 {
            if b[i] == q {
                q = 0;
            }
        } else if b[i] == b'"' || b[i] == b'\'' {
            q = b[i];
        } else if b[i] == b'>' {
            return Some(i);
        }

        i += 1;
    }

    None
}

fn salvage_dangling(tail: &str) -> Option<(String, String)> {
    let tag = tag_end(tail)?;
    let tool = tail
        .split("tool=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .or_else(|| {
            tail.split("tool='")
                .nth(1)
                .and_then(|s| s.split('\'').next())
        })
        .filter(|t| !t.is_empty())?
        .to_string();

    let body = tail[tag + 1..].trim();

    if serde_json::from_str::<Value>(body).is_err() {
        return None;
    }

    Some((tool, body.into()))
}

pub fn close_dangling_actions(text: &str) -> String {
    let Some(start) = text.rfind("<action") else {
        return text.into();
    };

    if text[start..].contains("</action>") {
        return text.into();
    }

    if salvage_dangling(&text[start..]).is_some() {
        return format!("{text}</action>");
    }

    text[..start].trim_end().to_string()
}

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

fn attrs_to_args(t: &str) -> Option<String> {
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

fn coerce_args(tag: &str, body: &str) -> String {
    let t = body.trim();

    if serde_json::from_str::<Value>(t).is_ok() {
        return t.into();
    }

    attrs_to_args(tag).unwrap_or_else(|| if t.is_empty() { "{}".into() } else { t.into() })
}

const TOOL_CALL_CLOSE: &str = "</tool_call>";

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
                out.push_str(&format!(
                    "<action tool=\"{tool}\">{}</action>",
                    Value::Object(args)
                ));
            }

            rest = &tail[used..];
            continue;
        }

        let Some(close) = tail.find(TOOL_CALL_CLOSE) else {
            // Cut the opening tag and keep what the model wrote inside it —
            // that body is the answer, and dropping the tail with the tag is
            // how a whole reply used to disappear. With no `>` there is nothing
            // to tell the tag from the text after it, so nothing is kept.
            match tag_end(tail) {
                Some(i) => out.push_str(&tail[i + 1..]),
                None => {}
            }

            return strip_protocol(&out);
        };

        if let Some((tool, args)) = salvage_call(&tail[..close]) {
            out.push_str(&format!("<action tool=\"{tool}\">{args}</action>"));
        }

        rest = &tail[close + TOOL_CALL_CLOSE.len()..];
    }

    out.push_str(rest);
    strip_protocol(&salvage_browser_blocks(&close_dangling_actions(&out)))
}

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

fn salvage_browser_blocks(text: &str) -> String {
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
            out.push_str(&format!("<action tool=\"{tool}\">{args}</action>"));
        } else {
            out.push_str(&tail[..consumed]);
        }

        rest = &tail[consumed..];
    }

    out.push_str(rest);
    out
}

fn salvage_call(inner: &str) -> Option<(String, String)> {
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
fn head_name(body: &str) -> Option<String> {
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
fn next_call_start(text: &str) -> Option<usize> {
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

const ARG_KEY: &str = "<arg_key>";
const ARG_KEY_CLOSE: &str = "</arg_key>";
const ARG_VALUE: &str = "<arg_value>";
const ARG_VALUE_CLOSE: &str = "</arg_value>";

/// The pairs come in three shapes. Closed, they are
/// `<arg_key>k</arg_key><arg_value>v</arg_value>`. Open — a stream cut
/// mid-call, or a model that simply omits the closes — they run
/// `<arg_key>k<arg_value>v<arg_key>k2<arg_value>v2`, where the next key is
/// the only thing that ends a value. And with the value opener missing
/// entirely, `<arg_key>k</arg_key>v`, which is what GLM emits under load.
///
/// Reading only the closed shape is how a whole tool call reached the reader
/// as raw text. Reading only the first two is worse: a dropped opener
/// discarded the arguments AND the call, so the model was told nothing, ran
/// nothing, and tried the same thing again.
fn arg_pairs(body: &str) -> (serde_json::Map<String, Value>, usize) {
    let mut out = serde_json::Map::new();
    let mut rest = body;
    let mut used = 0usize;

    while let Some(i) = rest.find(ARG_KEY) {
        let tail = &rest[i + ARG_KEY.len()..];
        let v = tail.find(ARG_VALUE);
        let key_end = tail.find(ARG_KEY_CLOSE);

        // A key ends at its own closer, or at the value tag that follows it,
        // whichever comes first. Only when the value tag never opened does the
        // closer become the whole story: `command</arg_key>cargo test`.
        let (key, after) = match v {
            Some(v) => {
                let key = match key_end {
                    Some(c) if c < v => tail[..c].trim(),
                    _ => tail[..v].trim(),
                };

                (key, &tail[v + ARG_VALUE.len()..])
            }
            None => match key_end {
                Some(c) => (tail[..c].trim(), &tail[c + ARG_KEY_CLOSE.len()..]),
                None => break,
            },
        };

        let (value, next) = match first_of(after, &[ARG_VALUE_CLOSE, ARG_KEY, TOOL_CALL_CLOSE]) {
            Some((e, ARG_VALUE_CLOSE)) => (&after[..e], &after[e + ARG_VALUE_CLOSE.len()..]),
            Some((e, _)) => (&after[..e], &after[e..]),
            None => (after, ""),
        };

        if key.is_empty() {
            break;
        }

        out.insert(key.to_string(), Value::String(value.trim().to_string()));

        used += rest.len() - next.len();
        rest = next;
    }

    (out, used)
}

fn first_of(hay: &str, needles: &[&'static str]) -> Option<(usize, &'static str)> {
    needles
        .iter()
        .filter_map(|n| hay.find(n).map(|i| (i, *n)))
        .min_by_key(|(i, _)| *i)
}

/// How far a value runs past its `<arg_key>`, so the pair can be taken whole.
/// Closed, it is the `</arg_value>` and what it wraps. Open, it is the next
/// key, the closing wrapper, or the end of the line.
fn arg_value_len(after_key: &str) -> usize {
    let Some(v) = after_key.find(ARG_VALUE) else {
        return 0;
    };

    let after = &after_key[v + ARG_VALUE.len()..];

    match first_of(after, &[ARG_VALUE_CLOSE, ARG_KEY, "<tool_call>"]) {
        Some((e, ARG_VALUE_CLOSE)) => v + ARG_VALUE.len() + e + ARG_VALUE_CLOSE.len(),
        Some((e, _)) => v + ARG_VALUE.len() + e,
        None => after_key.len(),
    }
}

/// Whatever the salvage above could not read is still not something a reader
/// should be shown, so it goes. This is the floor under the whole class: a
/// protocol tag reaches the transcript only by becoming an action, or not at
/// all.
fn strip_protocol(text: &str) -> String {
    const TAGS: &[&str] = &[
        "<tool_call>",
        TOOL_CALL_CLOSE,
        ARG_KEY,
        ARG_KEY_CLOSE,
        ARG_VALUE,
        ARG_VALUE_CLOSE,
        "<function_results>",
        "</function_results>",
    ];

    let mut spans: Vec<(usize, usize)> = vec![];
    let mut off = 0usize;
    let mut rest = text;

    'outer: while let Some(i) = rest.find('<') {
        let tail = &rest[i..];
        let hit = TAGS.iter().find(|t| tail.starts_with(**t));

        if let Some(tag) = hit {
            // Offsets here are tail-relative; `i` is added back when the span
            // is recorded against `text`.
            let end = match tail[1..].find('>') {
                Some(gt) => gt + 2,
                // A tag with no `>` is the whole rest of the line. Take that
                // and no more: the words after it are prose, not payload.
                None => match tail.find('\n') {
                    Some(nl) => nl,
                    None => tail.len(),
                },
            };

            // A pair nobody claimed takes its value with it. Stripping the tag
            // and leaving `backgroundfalsecommandsed -n` behind trades one
            // unreadable thing for another. A key with no value after it is
            // not a pair at all — it is a `<` in prose, and eating that is
            // the mistake this whole function exists to stop making.
            let end = if *tag == ARG_KEY {
                let v = arg_value_len(&tail[end..]);

                if v == 0 {
                    // Literal. Step over the `<` and keep looking.
                    off += i + 1;
                    rest = &tail[1..];
                    continue 'outer;
                }

                end + v
            } else {
                end
            };

            spans.push((off + i, off + i + end));
            off += i + end;
            rest = &tail[end..];
            continue 'outer;
        }

        // Not one of ours. A `<` that opens nothing real is a literal, and so
        // is everything after it that is not a tag we know.
        let Some(gt) = tail[1..].find('>') else {
            break;
        };

        off += 1 + gt + 1;
        rest = &tail[1 + gt + 1..];
    }

    if spans.is_empty() {
        return text.to_string();
    }

    let mut out = String::new();
    let mut prev = 0usize;

    for (start, end) in spans {
        out.push_str(&text[prev..start]);
        prev = end;
    }

    out.push_str(&text[prev..]);
    out
}
