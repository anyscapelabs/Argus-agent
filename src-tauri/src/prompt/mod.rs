// Prompt assembly in four files: the layered system prompt, attachment
// inlining, budget accounting, and the result types. This file only routes, so
// every caller still reaches `prompt::project` and `prompt::Projection` exactly
// as before.
//
// The split is by change-rate, not by size: the system prompt changes when the
// product's voice changes, budgets when models change, attachments when the
// library does. One file made all three impossible to review separately.

use tauri::State;

use crate::gateway::Gateway;

pub mod attachments;
pub mod budget;
pub mod compressor;
pub mod config;
pub mod project;
mod types;

pub use budget::{budget, full_budget, tools_budget, PromptBudget};
pub use project::{project, strip_display_tags, BASE};
pub use types::{Projection, PromptPreview};

#[tauri::command]
pub fn prompt_preview(gw: State<'_, Gateway>, session_id: String) -> Result<PromptPreview, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let p = project(&conn, &session_id, &gw.library_dir)?;

    Ok(PromptPreview {
        session_id: p.session_id,
        model_id: p.model_id,
        system: p.system,
        summary: p.summary,
        msgs: p.msgs,
        prefix_hash: p.prefix_hash,
        ctx_tokens: p.ctx_tokens,
        compact_seq: p.compact_seq,
    })
}
