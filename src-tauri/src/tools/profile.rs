use rusqlite::Connection;
use serde_json::Value;

use crate::profiles::schema::{Reach, DEFAULT_ID};
use crate::profiles::store as pstore;
use crate::sessions::store as sstore;
use crate::tools::ToolMeta;

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "profile.list",
        desc: "List the profiles you can see and what each is working on — every chat title, when it was last touched, and how many messages it holds. Only profiles you have been granted access to appear; the ones withheld are listed as withheld, so an empty result never means everyone is idle. Pass profile_id for one profile in full. This does not say whether a turn is running right now, only how recently each chat was touched. Read a chat with profile.read.",
        args: "{\"profile_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "profile.read",
        desc: "Read the transcript of a chat that belongs to another profile, given its session_id from profile.list. Refused outright when you have not been granted read_chats on that profile — it never returns an empty transcript, because an empty one reads as an empty conversation. Use it when you need what a peer profile is actually doing, not just that it has chats.",
        args: "{\"session_id\":\"...\"}",
        mutating: false,
    },
];

/// One way. A capability on one target says nothing about any other.
pub fn may(reach: &Reach, cap: &str, target: &str) -> bool {
    reach.reach_all
        || reach
            .grants
            .iter()
            .any(|g| g.capability == cap && g.target_id == target)
}

/// Looked up from the chat, never taken from the model. A model that could name
/// its own profile could name any profile.
fn caller(conn: &Connection) -> Result<String, String> {
    let sid = crate::tools::notepad::current_session()
        .ok_or("profile tools only work from inside a conversation")?;

    Ok(pstore::of_session(conn, &sid)?.unwrap_or_else(|| DEFAULT_ID.to_string()))
}

/// Naming the capability is what makes this a refusal and not a retry.
fn refusal(cap: &str, target: &str) -> String {
    format!(
        "no grant: this profile may not {cap} {target}. The user grants that in \
         Settings, Profiles, Reach."
    )
}

fn label(id: &str, name: &str) -> String {
    let n = name.trim();

    if n.is_empty() {
        return format!("{id} (Default)");
    }

    n.to_string()
}

fn chats(conn: &Connection, owner: &str) -> Result<String, String> {
    let rows = sstore::list_profile_sessions(conn, owner, DEFAULT_ID)?;

    if rows.is_empty() {
        return Ok("no chats\n".into());
    }

    let mut out = String::new();

    for s in rows {
        let n = sstore::count_msgs(conn, &s.id)?;
        let title = if s.title.trim().is_empty() {
            "(untitled)"
        } else {
            &s.title
        };

        out.push_str(&format!(
            "- {} | {n} message{} | last touched {}\n",
            crate::tools::clip_ends(title.to_string()),
            if n == 1 { "" } else { "s" },
            s.updated_at
        ));
    }

    Ok(out)
}

pub fn list(conn: &Connection, args: &Value) -> Result<String, String> {
    let me = caller(conn)?;
    let reach = pstore::reach(conn, &me)?;
    let all = pstore::list(conn)?;

    if let Some(pid) = args["profile_id"].as_str() {
        if pid != me && !may(&reach, "see_activity", pid) {
            return Err(refusal("see", pid));
        }

        return Ok(format!(
            "{} — {} chat(s)\n{}",
            label(pid, &pstore::get(conn, pid)?.name),
            sstore::list_profile_sessions(conn, pid, DEFAULT_ID)?.len(),
            chats(conn, pid)?
        ));
    }

    let mut out = String::from(
        "Last touched is not liveness: a chat touched a moment ago may already be finished.\n",
    );
    let mut hidden: Vec<String> = Vec::new();

    for p in &all {
        if p.id == me {
            out.push_str(&format!(
                "\n## {} (you)\n{}",
                label(&p.id, &p.name),
                chats(conn, &p.id)?
            ));
        } else if may(&reach, "see_activity", &p.id) {
            out.push_str(&format!(
                "\n## {}\n{}",
                label(&p.id, &p.name),
                chats(conn, &p.id)?
            ));
        } else {
            hidden.push(label(&p.id, &p.name));
        }
    }

    if !hidden.is_empty() {
        out.push_str(&format!(
            "\nNot shown, no grant to see them: {}",
            hidden.join(", ")
        ));
    }

    Ok(out)
}

pub fn read(conn: &Connection, args: &Value) -> Result<String, String> {
    let sid = args["session_id"]
        .as_str()
        .ok_or("profile.read needs a session_id")?;

    let me = caller(conn)?;
    let s = sstore::get_session(conn, sid)?;
    let owner = s
        .profile_id
        .clone()
        .unwrap_or_else(|| DEFAULT_ID.to_string());

    if owner != me {
        let reach = pstore::reach(conn, &me)?;

        if !may(&reach, "read_chats", &owner) {
            return Err(refusal("read the chats of", &owner));
        }
    }

    let msgs = sstore::list_msgs(conn, sid)?;
    let who = pstore::get(conn, &owner).map(|p| label(&owner, &p.name))?;

    let mut out = format!(
        "## {} — {} — {}\n",
        who,
        if s.title.trim().is_empty() {
            "(untitled)"
        } else {
            &s.title
        },
        if owner == me {
            "your own chat"
        } else {
            "a peer profile's chat"
        }
    );

    if msgs.is_empty() {
        out.push_str("\n(this chat has no messages)");

        return Ok(out);
    }

    for m in msgs {
        out.push_str(&format!(
            "\n{}: {}\n",
            m.role,
            crate::tools::clip(m.content)
        ));
    }

    Ok(crate::tools::clip(out))
}
