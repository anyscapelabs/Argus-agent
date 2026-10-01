// What the harness has learned about a model, and about this machine.
//
// Two rules: a lesson is a fact with evidence, never a scolding, and nothing
// here is generated — each signal maps to a fixed sentence in code, so a model
// cannot rewrite the rules it is judged by.
pub mod schema;
pub mod store;

use rusqlite::Connection;

pub use store::{Kind, PlaybookItem, Scope, Signal};

pub fn migrate(conn: &Connection) -> Result<(), String> {
    schema::migrate(conn)
}

/// The sentence for a signal and the stable key it accumulates evidence under.
fn lesson_for(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::EmptyArgs => (
            "empty_args",
            "tool calls sometimes arrive with no arguments; nothing runs, so send the call again \
             with its arguments filled in",
        ),
        Kind::Thrashing => (
            "thrashing",
            "a failing call retried with the same approach is refused; change approach or ask the \
             user rather than trying the same thing again",
        ),
        Kind::SandboxDenied => (
            "sandbox_denied",
            "a confined profile only reaches inside its root; a refusal there means the path is \
             outside it — pass cwd inside it, or run with profile host",
        ),
        Kind::SandboxNoCwd => (
            "sandbox_no_cwd",
            "profile project needs a cwd; without one it would confine to the wrong directory, so \
             pass the project directory or use profile host",
        ),
        Kind::BudgetStop => (
            "budget_stop",
            "this turn ran out of its token budget; when work is large, scope it and report what \
             is left instead of continuing in one turn",
        ),
        Kind::StyleXml => (
            "style_xml",
            "this model writes tool calls as XML text as well as calling the API; that is expected \
             for it and both copies run only once",
        ),
        Kind::Degraded => (
            "degraded",
            "this provider did not accept the tool schemas, so the turn fell back to the in-band \
             action format; the result is still accurate",
        ),
    }
}

/// Turn recorded signals into lessons. Called after a turn, cheap and
/// synchronous: it is a map lookup and one UPSERT per kind, because the
/// sentences are fixed. A lesson enters the prompt only once its evidence
/// clears the bar, which lives in `store::MIN_EVIDENCE_TO_TEACH`.
pub fn curate(
    conn: &Connection,
    scope_id: &str,
    session_id: Option<&str>,
) -> Result<usize, String> {
    let mut taught = 0usize;

    for kind in Kind::all() {
        let signals = store::signals_for(conn, kind, scope_id)?;
        let evidence: i64 = signals.iter().map(|s| s.seen).sum();

        if evidence < store::MIN_EVIDENCE_TO_TEACH {
            continue;
        }

        let (key, text) = lesson_for(kind);
        let count = store::remember(conn, kind.scope(), scope_id, key, text, evidence)?;

        if count == evidence {
            taught += 1;
        }
    }

    let _ = session_id;
    Ok(taught)
}

/// The prompt blocks. Both are opt-in by evidence, and a scope with no
/// lessons contributes nothing — which is what keeps a model with no history
/// byte-identical to one that has never run this code.
pub fn prompt_context(conn: &Connection, model_id: Option<&str>) -> Result<String, String> {
    let mut out = String::new();

    if let Some(model) = model_id.filter(|m| !m.trim().is_empty()) {
        let items = store::lessons(conn, Scope::Model, model)?;

        if !items.is_empty() {
            out.push_str(
                "<model-playbook>\n\
                 Observed on this model across earlier sessions. Facts, not instructions.\n",
            );

            for item in &items {
                out.push_str(&format!("- {} (seen {})\n", item.text, item.evidence));
            }

            out.push_str("</model-playbook>");
        }
    }

    let host_id = crate::sessions::ext_install::host_id();
    let host = store::lessons(conn, Scope::Host, &host_id)?;

    if !host.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }

        out.push_str(
            "<environment-notes>\n\
             Observed on this machine. Facts about the environment, not about you.\n",
        );

        for item in &host {
            out.push_str(&format!("- {} (seen {})\n", item.text, item.evidence));
        }

        out.push_str("</environment-notes>");
    }

    Ok(out)
}
