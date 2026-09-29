// Which dialect this model speaks, and what to remember about the step.

use super::Turn;
use crate::gateway::router;
use crate::gateway::Gateway;
use crate::tools::{self, ToolCallStyle};

/// Which dialect this model speaks for this turn.
///
/// Degraded replies have no API channel, so their text must execute
/// regardless of style. Otherwise the stored classification decides, and an
/// unknown model showing the duplication signature is classified once here
/// so the next turn resolves to the template style directly.
impl Turn {
    pub(super) fn resolve_style(
        gw: &Gateway,
        stats: &router::StreamStats,
        base_text: &str,
    ) -> ToolCallStyle {
        if stats.degraded {
            return ToolCallStyle::GlmXml;
        }

        let style = match gw.conn.lock() {
            Ok(conn) => crate::prompt::config::tool_style(&conn, Some(stats.model_id.as_str())),
            Err(_) => ToolCallStyle::Native,
        };

        if style == ToolCallStyle::Native
            && tools::has_native_text_duplicate(base_text, &stats.tool_calls)
        {
            if let Ok(conn) = gw.conn.lock() {
                crate::prompt::config::upgrade_tool_style(&conn, &stats.model_id);
            }
        }

        style
    }

    /// Note the two things worth remembering about a step. Both are recorded
    /// where the classification happens, so a lesson exists for the same turn
    /// that caused it.
    ///
    /// A turn that wrote its tool call as XML text has told us which dialect it
    /// speaks. A degraded turn means the provider refused the tool schemas — the
    /// turn still worked via the in-band format, so that is infrastructure, not
    /// a model failure, and is recorded against the host.
    pub(super) fn record_observations(
        &self,
        gw: &Gateway,
        session_id: &str,
        style: ToolCallStyle,
        stats: &router::StreamStats,
    ) {
        if style == ToolCallStyle::GlmXml && !stats.degraded {
            if let Ok(conn) = gw.conn.lock() {
                let _ = crate::playbook::store::record(
                    &conn,
                    crate::playbook::Kind::StyleXml,
                    &stats.model_id,
                    Some(session_id),
                    "wrote a tool call as XML text",
                );
            }
        }

        if stats.degraded {
            if let Ok(conn) = gw.conn.lock() {
                let _ = crate::playbook::store::record(
                    &conn,
                    crate::playbook::Kind::Degraded,
                    &crate::sessions::ext_install::host_id(),
                    Some(session_id),
                    &stats.provider_id,
                );
            }
        }
    }
}
