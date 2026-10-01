// Argument text: the `<arg_key>`/`<arg_value>` pair, the length rules for a
// value, and the protocol prefix a model leaves at the head of a call. The
// closing tag carries a zero-width joiner so a model cannot close a call with a
// lookalike string.
use serde_json::Value;

use super::normalize::TOOL_CALL_CLOSE;

pub(super) const ARG_KEY: &str = "<arg_key>";
pub(super) const ARG_KEY_CLOSE: &str = "</arg_key>";
pub(super) const ARG_VALUE: &str = "<arg_value>";
pub(super) const ARG_VALUE_CLOSE: &str = "</arg_value>";

/// The pairs come in three shapes. Closed, they are
/// `<arg_key>k</arg_key><arg_value>v</arg_value>`. Open — a stream cut mid-call,
/// or a model that omits the closes — they run
/// `<arg_key>k<arg_value>v<arg_key>k2<arg_value>v2`, where the next key is the
/// only thing that ends a value. And with the value opener missing entirely,
/// `<arg_key>k</arg_key>v`, which is what GLM emits under load.
///
/// Reading only the closed shape is how a whole tool call reached the reader as
/// raw text. Dropping the third shape discarded the arguments AND the call.
pub(super) fn arg_pairs(body: &str) -> (serde_json::Map<String, Value>, usize) {
    let mut out = serde_json::Map::new();
    let mut rest = body;
    let mut used = 0usize;

    while let Some(i) = rest.find(ARG_KEY) {
        let tail = &rest[i + ARG_KEY.len()..];
        let v = tail.find(ARG_VALUE);
        let key_end = tail.find(ARG_KEY_CLOSE);

        // A key ends at its own closer, or at the value tag that follows it.
        // Only when the value tag never opened does the closer end it:
        // `command</arg_key>cargo test`.
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

pub(super) fn first_of(hay: &str, needles: &[&'static str]) -> Option<(usize, &'static str)> {
    needles
        .iter()
        .filter_map(|n| hay.find(n).map(|i| (i, *n)))
        .min_by_key(|(i, _)| *i)
}

/// How far a value runs past its `<arg_key>`, so the pair can be taken whole:
/// the `</arg_value>` and what it wraps, or the next key, the closing wrapper,
/// or the end of the line.
pub(super) fn arg_value_len(after_key: &str) -> usize {
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

/// Whatever the salvage above could not read still must not reach a reader. A
/// protocol tag gets into the transcript by becoming an action, or not at all.
pub(super) fn strip_protocol(text: &str) -> String {
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
            // Offsets are tail-relative; `i` is added back against `text`.
            let end = match tail[1..].find('>') {
                Some(gt) => gt + 2,
                // A tag with no `>` takes the rest of the line and no more:
                // the words after it are prose, not payload.
                None => match tail.find('\n') {
                    Some(nl) => nl,
                    None => tail.len(),
                },
            };

            // A pair nobody claimed takes its value with it. Stripping the tag
            // and leaving `backgroundfalsecommandsed -n` behind trades one
            // unreadable thing for another. A key with no value is not a pair —
            // it is a `<` in prose, and eating that is the mistake this
            // function exists to stop making.
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

        // Not one of ours: a literal, and so is everything after it that is
        // not a tag we know.
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
