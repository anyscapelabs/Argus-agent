// The canonical action syntax: `<action tool="...">...</action>`. This is
// what the rest of the crate means by an action; `normalize` and `salvage` both
// exist to turn sloppy text into it.
use serde_json::Value;

use super::normalize::coerce_args;
use crate::tools::Action;

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

pub(super) fn tag_end(blk: &str) -> Option<usize> {
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
