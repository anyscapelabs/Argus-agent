//! Deterministic recovery classification for tool failures.
//!
//! Additive and minimal: a pure function over the existing `Err(String)`
//! messages produced by `tools::exec` and the tool implementations. No tool
//! was given a new error hierarchy; browser/computer/web/MCP behavior is
//! untouched.
//!
//! The goal is not a smarter LLM — it is Rust understanding what kind of
//! failure occurred and what the agent may do next. Every failure still
//! becomes exactly one structured `<tool-result>` for the model via the
//! existing `ToolExecution` lifecycle; retries below are same-call,
//! same-args sub-attempts inside one execution, never extra results.
//!
//! What is NOT implemented here (by design): browser snapshot re-observe and
//! computer observation recovery. `StaleReference` and `NotFound` are
//! classified so that future recovery can hook in, but an identical blind
//! retry would fail identically today, so no automatic retry is allowed for
//! them yet.

use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;

/// What kind of tool failure occurred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryKind {
    InvalidArguments,
    ToolNotFound,
    PermissionDenied,
    ApprovalRequired,
    AuthenticationRequired,
    NotFound,
    Timeout,
    StaleReference,
    RateLimited,
    Recoverable,
    Fatal,
}

impl RecoveryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryKind::InvalidArguments => "invalid_arguments",
            RecoveryKind::ToolNotFound => "tool_not_found",
            RecoveryKind::PermissionDenied => "permission_denied",
            RecoveryKind::ApprovalRequired => "approval_required",
            RecoveryKind::AuthenticationRequired => "authentication_required",
            RecoveryKind::NotFound => "not_found",
            RecoveryKind::Timeout => "timeout",
            RecoveryKind::StaleReference => "stale_reference",
            RecoveryKind::RateLimited => "rate_limited",
            RecoveryKind::Recoverable => "recoverable",
            RecoveryKind::Fatal => "fatal",
        }
    }

    /// Total attempts allowed for one accepted call, including the first try.
    /// `1` means no automatic retry: the model corrects arguments, the user
    /// acts, or the structured error simply goes back to the model.
    pub fn max_attempts(&self) -> u32 {
        match self {
            // Transient under existing infrastructure: one blind retry is
            // safe because the call is side-effect-identical and bounded.
            RecoveryKind::Timeout | RecoveryKind::RateLimited | RecoveryKind::Recoverable => 2,
            // Everything else needs new information (corrected args, fresh
            // refs, credentials, user approval) that an identical retry
            // cannot supply.
            _ => 1,
        }
    }

    pub fn may_retry(&self) -> bool {
        self.max_attempts() > 1
    }
}

fn has_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Classify an existing tool error string. Matching is deliberately
/// substring-based over messages the codebase already emits, ordered from
/// most specific to most general; anything unrecognized is `Fatal`
/// (conservative: no retry) rather than guessed recoverable.
pub fn classify(err: &str) -> RecoveryKind {
    let e = err.to_lowercase();

    if e.contains("unknown tool") {
        return RecoveryKind::ToolNotFound;
    }

    if has_any(
        &e,
        &[
            "action denied by user",
            "denied by user",
            "password field",
            "user types it themselves",
            "carries a credential",
            "permission denied",
        ],
    ) {
        return RecoveryKind::PermissionDenied;
    }

    if e.contains("asks before acting") {
        return RecoveryKind::ApprovalRequired;
    }

    if has_any(
        &e,
        &[
            "not valid json",
            "missing ",
            "needs a ",
            "needs two",
            "must start with",
            "must be ",
            "is empty",
            "are empty",
            "unsupported",
            "bad extension",
            "already exists",
        ],
    ) {
        return RecoveryKind::InvalidArguments;
    }

    if has_any(
        &e,
        &[
            "stale ref",
            "unknown ref",
            "for a fresh list",
            "no screenshot yet",
        ],
    ) {
        return RecoveryKind::StaleReference;
    }

    if has_any(&e, &["timed out", "timeout", "deadline exceeded"]) {
        return RecoveryKind::Timeout;
    }

    if has_any(
        &e,
        &[
            "rate-limit",
            "rate limit",
            "bot-check",
            "bot check",
            "try again in a moment",
            "try again later",
            "too many requests",
            "quota",
            "429",
        ],
    ) {
        return RecoveryKind::RateLimited;
    }

    if has_any(
        &e,
        &[
            "extension not connected",
            "not connected",
            "reconnect",
            "api key",
            "unauthorized",
            "unauthenticated",
        ],
    ) {
        return RecoveryKind::AuthenticationRequired;
    }

    if has_any(
        &e,
        &[
            "not found",
            "no such",
            "does not exist",
            "vanished",
            "item file missing",
        ],
    ) {
        return RecoveryKind::NotFound;
    }

    if has_any(
        &e,
        &[
            "failed",
            "session missing",
            "connection",
            "network",
            "temporarily",
            "try again",
            "broken pipe",
            "interrupted",
        ],
    ) {
        return RecoveryKind::Recoverable;
    }

    RecoveryKind::Fatal
}

/// Run `op` until success or the retry budget derived from each failure's
/// classification is exhausted. Returns the final outcome plus the total
/// number of attempts used (always `>= 1`). Budgets shrink when a later
/// failure classifies stricter, so repeated failures always stop.
pub async fn run_bounded<F, Fut>(mut op: F) -> (Result<String, String>, u32)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut attempts = 0u32;

    loop {
        attempts += 1;
        let res = op().await;

        let done = match &res {
            Ok(_) => true,
            Err(e) => attempts >= classify(e).max_attempts().max(1),
        };

        if done {
            return (res, attempts);
        }
    }
}

/// Outcome of one accepted tool call under the recovery policy.
pub struct RetryOutcome {
    /// Final body: success output or the last structured error. The caller
    /// turns this into the single `<tool-result>` for the model.
    pub result: Result<String, String>,
    /// Total underlying executions performed (1 when no retry applied).
    pub attempts: u32,
    /// Classification of the final error; `None` when the call succeeded.
    pub kind: Option<RecoveryKind>,
}

/// `tools::exec` with the bounded recovery policy enforced. Same signature
/// and contract as `exec`; retries are same-call/same-args and bounded by
/// `RecoveryKind::max_attempts` (at most 2 total today), so the caller's
/// one-execution-one-result invariant is preserved.
pub async fn exec_with_recovery(
    gw: &crate::gateway::Gateway,
    name: &str,
    args_json: &str,
    permission: &str,
    web: bool,
    approved: bool,
    on_term: Option<(&Channel<StreamEvent>, u32)>,
) -> RetryOutcome {
    let (result, attempts) =
        run_bounded(|| super::exec(gw, name, args_json, permission, web, approved, on_term)).await;

    let kind = match &result {
        Ok(_) => None,
        Err(e) => Some(classify(e)),
    };

    RetryOutcome {
        result,
        attempts,
        kind,
    }
}
