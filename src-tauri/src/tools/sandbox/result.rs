use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Termination {
    Completed,
    Failed,
    LimitHit,
    TimedOut,
    Cancelled,
}

impl Termination {
    pub fn as_str(&self) -> &'static str {
        match self {
            Termination::Completed => "completed",
            Termination::Failed => "failed",
            Termination::LimitHit => "limitHit",
            Termination::TimedOut => "timedOut",
            Termination::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub exit: i64,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub termination: Termination,
    pub truncated: bool,
}

impl Outcome {
    pub fn succeeded(&self) -> bool {
        matches!(self.termination, Termination::Completed) && self.exit == 0
    }

    pub fn combined(&self) -> String {
        let mut text = self.stdout.clone();

        if !self.stderr.is_empty() {
            text.push('\n');
            text.push_str(&self.stderr);
        }

        if self.truncated {
            text.push_str("\n...[output truncated]");
        }

        text
    }
}
