use serde::{Deserialize, Serialize};

/// Ten is what a person can hold in their head.
pub const MAX_PROFILES: usize = 10;

/// Always exists. A null `profile_id` is never allowed to survive.
pub const DEFAULT_ID: &str = "default";

pub const NAME_MAX: usize = 40;

/// The ladder, in order. Each rung hands over strictly more than the last,
/// and a profile only holds the rungs the user ticked.
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
    /// The custom matrix is kept underneath either way, so turning this off restores it.
    pub reach_all: bool,
    /// Carried on the row so the settings list needs no second query.
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
