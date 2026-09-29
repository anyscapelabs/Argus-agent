// The turn loop: one user message, run to completion, stop, or run out of
// budget.
//
// The whole of a turn's mutable state is `Turn`. Nothing in here is on a hot
// path in the sense of being called often — it runs once per user message, for
// up to MAX_STEPS model calls — so the struct is plain and unshared, and the
// loop is written to be read top to bottom: build the request, spend budget,
// call the model, decide whether this is the last step, run the tools, record
// what ran.

use crate::tools::sandbox::Origin;

// Each of these answers one question the loop asks, and none of them can reach
// the loop's locals — that is what makes the split free rather than a reshuffle.
mod budget;
mod persist;
mod request;
mod run;
mod style;
mod truncation;

/// What a cut-off reply means for the rest of the step.
enum Truncation {
    /// Not truncated; carry on.
    None,
    /// Truncated past the limit with work still pending. The step continues, but
    /// the next request is already over budget.
    Overflow,
    /// Ask the model to finish the sentence and take another step.
    Retry,
    /// The turn is over.
    Finish,
}

/// Everything a single turn mutates. The loop reads and writes these across
/// iterations; nothing else crosses a step boundary.
pub struct Turn {
    pub tok_in_sum: i64,
    pub act_base: usize,
    pub nudge: Option<String>,
    pub claim_nudges: usize,
    pub forced_summary: bool,
    pub reflect_nudges: usize,
    pub trunc_conts: usize,
    pub empty_retries: usize,
    pub finished: bool,
    pub acts_run: usize,
    pub recent: Vec<(String, String)>,
    pub recent_out: Vec<bool>,
    pub turn_origin: Option<Origin>,
    pub budget_warned: bool,
    pub turn_actions: Vec<(String, bool)>,
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}

impl Turn {
    pub fn new() -> Self {
        Self {
            tok_in_sum: 0,
            act_base: 0,
            nudge: None,
            claim_nudges: 0,
            forced_summary: false,
            reflect_nudges: 0,
            trunc_conts: 0,
            empty_retries: 0,
            finished: false,
            acts_run: 0,
            recent: vec![],
            recent_out: vec![],
            turn_origin: None,
            budget_warned: false,
            turn_actions: vec![],
        }
    }
}
