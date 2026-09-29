// Dispatch: validate the arguments for a kind, then hand off to its writer.
// This is the only place that knows both the argument shapes and the writers.
use super::args::{parse_rows, parse_slides};
use super::docx::docx_bytes;
use super::pdf::pdf_bytes;
use super::pptx::pptx_bytes;
use super::sheet::{csv_bytes, xlsx_bytes};
use super::zip::wrap_lines;

pub struct BuiltDoc {
    pub ext: String,
    pub bytes: Vec<u8>,
    pub pages: i64,
}

pub fn build(kind: &str, title: &str, args: &serde_json::Value) -> Result<BuiltDoc, String> {
    match kind {
        "md" | "txt" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let pages = (content.chars().count() as i64 / 1800 + 1).max(1);
            Ok(BuiltDoc {
                ext: kind.into(),
                bytes: content.into_bytes(),
                pages,
            })
        }
        "csv" => {
            let rows = parse_rows(args.get("rows").unwrap_or(&serde_json::Value::Null));
            if rows.is_empty() {
                return Err("doc rows are empty".into());
            }
            let pages = 1;
            Ok(BuiltDoc {
                ext: "csv".into(),
                bytes: csv_bytes(&rows),
                pages,
            })
        }
        "pdf" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() && title.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let lines = wrap_lines(&content, 88).len() as i64;
            let pages = (lines / 44 + 1).max(1);
            Ok(BuiltDoc {
                ext: "pdf".into(),
                bytes: pdf_bytes(title, &content),
                pages,
            })
        }
        "docx" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if content.trim().is_empty() && title.trim().is_empty() {
                return Err("doc content is empty".into());
            }
            let pages = (content.chars().count() as i64 / 1800 + 1).max(1);
            Ok(BuiltDoc {
                ext: "docx".into(),
                bytes: docx_bytes(title, &content),
                pages,
            })
        }
        "xlsx" => {
            let rows = parse_rows(args.get("rows").unwrap_or(&serde_json::Value::Null));
            if rows.is_empty() {
                return Err("doc rows are empty".into());
            }
            Ok(BuiltDoc {
                ext: "xlsx".into(),
                bytes: xlsx_bytes(&rows),
                pages: 1,
            })
        }
        "pptx" => {
            let slides = parse_slides(args.get("slides").unwrap_or(&serde_json::Value::Null));
            let slides = if slides.is_empty() {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if content.trim().is_empty() && title.trim().is_empty() {
                    return Err("doc slides are empty".into());
                }
                vec![(
                    title.to_string(),
                    content.lines().map(|l| l.to_string()).collect(),
                )]
            } else {
                slides
            };
            let pages = slides.len() as i64;
            Ok(BuiltDoc {
                ext: "pptx".into(),
                bytes: pptx_bytes(title, &slides),
                pages,
            })
        }
        _ => Err("unsupported document kind".into()),
    }
}
