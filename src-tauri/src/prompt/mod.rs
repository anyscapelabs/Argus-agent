// Routes only: split by change-rate (prompt voice, budgets, library).

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
