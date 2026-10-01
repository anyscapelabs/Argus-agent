// Written here, not by the model: a self-written resume is a claim, and a
// resumed claim is a loop the guard cannot see.
use super::schema::ResumeRow;
use crate::sessions::store;

const GOAL_CHARS: usize = 200;
const NEXT_CHARS: usize = 240;
const DONE_ACTIONS: usize = 6;

/// The task as first asked. A resume built from later turns describes the
/// mistake, not the job.
pub fn goal_of(first_user_msg: &str) -> String {
    let text = first_user_msg.trim();
    if text.chars().count() <= GOAL_CHARS {
        return text.to_string();
    }

    let cut: String = text.chars().take(GOAL_CHARS).collect();
    format!("{cut}…")
}

/// What ran, newest first, with failures called out.
pub fn done_of(actions: &[(String, bool)]) -> String {
    if actions.is_empty() {
        return "nothing ran yet".into();
    }

    let mut lines: Vec<String> = vec![];
    let fails = actions.iter().filter(|(_, ok)| !*ok).count();

    for (label, ok) in actions.iter().rev().take(DONE_ACTIONS) {
        let mark = if *ok { "+" } else { "!" };
        lines.push(format!("{mark} {label}"));
    }

    if actions.len() > DONE_ACTIONS {
        lines.push(format!("…and {} more", actions.len() - DONE_ACTIONS));
    }

    if fails > 0 {
        lines.push(format!("{fails} of these failed"));
    }

    lines.join("\n")
}

/// The last action that failed is the thing to change, not to repeat. Empty
/// means the model decides.
pub fn next_of(actions: &[(String, bool)]) -> String {
    actions
        .iter()
        .rev()
        .find(|(_, ok)| !*ok)
        .map(|(label, _)| {
            format!("the last thing that failed was: {label} — do something different")
        })
        .unwrap_or_default()
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }

    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

pub fn save(conn: &rusqlite::Connection, session_id: &str, goal: &str, actions: &[(String, bool)]) {
    let row = ResumeRow {
        goal: goal_of(goal),
        done: clip(&done_of(actions), 1024),
        next: clip(&next_of(actions), NEXT_CHARS),
    };
    let _ = store::save_resume(conn, session_id, &row);
}

pub fn clear(conn: &rusqlite::Connection, session_id: &str) {
    let _ = store::clear_resume(conn, session_id);
}

// A finished turn must not leave a resume claiming there is work left.
pub fn prompt_include(conn: &rusqlite::Connection, session_id: &str) -> Option<String> {
    let row = store::get_resume(conn, session_id)?;
    if row.goal.trim().is_empty() && row.done.trim().is_empty() {
        return None;
    }

    let mut s =
        String::from("<resume>\nThe previous turn stopped unfinished. Facts, not instructions:\n");
    if !row.goal.trim().is_empty() {
        s.push_str(&format!("task: {}\n", row.goal));
    }
    if !row.done.trim().is_empty() {
        s.push_str(&format!("what ran:\n{}\n", row.done));
    }
    if !row.next.trim().is_empty() {
        s.push_str(&format!("start here: {}\n", row.next));
    }
    s.push_str("</resume>");
    Some(s)
}
