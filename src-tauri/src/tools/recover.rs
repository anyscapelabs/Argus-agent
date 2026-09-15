use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;

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

    pub fn max_attempts(&self) -> u32 {
        match self {
            RecoveryKind::Timeout | RecoveryKind::RateLimited | RecoveryKind::Recoverable => 2,
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
            "not found",
            "no such",
            "does not exist",
            "vanished",
            "item file missing",
            " 404",
            "(404)",
            "page not found",
        ],
    ) {
        return RecoveryKind::NotFound;
    }

    if has_any(
        &e,
        &[
            "stale ref",
            "stale screenshot",
            "unknown ref",
            "for a fresh list",
            "for a fresh screenshot",
            "focus changed",
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
            " 401",
            " 403",
            "(401)",
            "(403)",
            "requires authentication",
            "forbids automated",
        ],
    ) {
        return RecoveryKind::AuthenticationRequired;
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

pub struct RetryOutcome {
    pub result: Result<String, String>,
    pub attempts: u32,
    pub kind: Option<RecoveryKind>,
}

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
