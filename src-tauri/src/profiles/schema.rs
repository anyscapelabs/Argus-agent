use serde::{Deserialize, Serialize};

/// Past this the picker is a list nobody reads and the prompt layer is ten
/// near-copies of itself. Ten is what a person can hold in their head.
pub const MAX_PROFILES: usize = 10;

/// The one that always exists and never goes. Seeded at migration with an empty
/// name; a null `profile_id` is never allowed to survive, so the UI reads one
/// uniform list.
pub const DEFAULT_ID: &str = "default";

pub const NAME_MAX: usize = 40;

/// The ladder, in order. Each rung hands over strictly more than the last, and
/// a profile only holds the rungs the user ticked — there is no "seniority"
/// field that implies the rest. The names are what the code and the database
/// say; the settings page puts plain words next to them.
pub const CAPABILITIES: [&str; 6] = [
    "see_activity",
    "read_chats",
    "write_prompts",
    "interrupt",
    "edit",
    "change_access",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub instructions: String,
    /// True means every other profile, present and future. The custom matrix is
    /// kept underneath either way, so turning this back off restores it.
    pub reach_all: bool,
    /// How many cells the custom matrix holds. Carried on the row so the
    /// settings list can say a profile reaches out without a second query.
    pub grants: i64,
    pub created_at: String,
}

/// One cell of the matrix: this profile may do this thing to that profile.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub capability: String,
    pub target_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reach {
    pub reach_all: bool,
    pub grants: Vec<Grant>,
}
