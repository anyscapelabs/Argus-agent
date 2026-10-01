// How the assembled prompt breaks down, and what a request must be cut down to
// when the model has a small context.
use super::config;
use super::types::Projection;

pub fn budget(system: &str) -> Vec<(&'static str, i64)> {
    let mut out: Vec<(&'static str, i64)> = vec![];
    let mut rest = system;
    let mut head = "stable";

    for (mark, name) in [
        ("<user-preferences>", "user-preferences"),
        ("<learned-preferences>", "learned"),
        ("<model-playbook>", "playbook"),
        ("<environment-notes>", "environment"),
        ("<resume>", "resume"),
        ("<session-summary>", "session-summary"),
        ("<working-notes>", "notepad"),
    ] {
        if let Some(idx) = rest.find(mark) {
            let est = config::est_tokens(&rest[..idx]);

            if est > 0 {
                out.push((head, est));
            }

            rest = &rest[idx..];
            head = name;
        }
    }

    out.push((head, config::est_tokens(rest)));

    out
}

pub fn tools_budget(web: bool) -> i64 {
    // Native wording; the template variant is the same order of magnitude.
    let section = crate::tools::section(web, crate::tools::ToolCallStyle::Native);
    let specs: i64 = crate::tools::tool_specs(web)
        .iter()
        .map(|t| config::est_tokens(&serde_json::to_string(&t.parameters).unwrap_or_default()))
        .sum();

    config::est_tokens(&section) + specs
}

pub struct PromptBudget {
    pub stable: i64,
    pub tools: i64,
    pub preferences: i64,
    pub learned: i64,
    /// Model lessons and environment facts, reported as one tier.
    pub playbook: i64,
    pub summary: i64,
    pub notepad: i64,
    pub skill: i64,
    pub memory: i64,
    pub conversation: i64,
    pub total: i64,
}

pub fn full_budget(p: &Projection, web: bool) -> PromptBudget {
    let mut b = PromptBudget {
        stable: 0,
        tools: tools_budget(web),
        preferences: 0,
        learned: 0,
        playbook: 0,
        summary: 0,
        notepad: 0,
        skill: 0,
        memory: 0,
        conversation: 0,
        total: 0,
    };

    for (name, est) in budget(&p.system) {
        match name {
            "stable" => b.stable = est,
            "user-preferences" => b.preferences = est,
            "learned" => b.learned = est,
            "playbook" | "environment" | "resume" => b.playbook += est,
            "session-summary" => b.summary = est,
            "notepad" => b.notepad = est,
            _ => {}
        }
    }

    b.conversation = p.msgs.iter().map(|m| config::est_tokens(&m.content)).sum();
    b.total = b.stable
        + b.tools
        + b.preferences
        + b.learned
        + b.playbook
        + b.summary
        + b.notepad
        + b.skill
        + b.memory
        + b.conversation;

    b
}
