// The split is by format because that is how these change: a new spreadsheet
// producer is a new writer, not an edit to the PDF table.

mod args;
pub mod build;
mod create;
mod docx;
pub mod edit;
pub mod patch;
mod pdf;
mod pptx;
pub mod read;
mod sheet;
mod zip;

pub use build::{build, BuiltDoc};
pub use create::create;
pub use edit::edit;
pub use patch::patch;
pub use read::{para_markdown, preview_docx, xml_decode};

/// Or `None` for a file we can only store, never generate.
pub fn kind_of(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next().unwrap_or("").trim().to_lowercase();
    match ext.as_str() {
        "docx" => Some("docx"),
        "pdf" => Some("pdf"),
        "pptx" => Some("pptx"),
        "xlsx" => Some("xlsx"),
        "csv" => Some("csv"),
        "md" => Some("md"),
        "txt" => Some("txt"),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => None,
        _ => None,
    }
}
