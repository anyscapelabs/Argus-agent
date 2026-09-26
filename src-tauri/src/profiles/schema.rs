use serde::{Deserialize, Serialize};

/// Past this the picker is a list nobody reads and the prompt layer is ten
/// near-copies of itself. Ten is what a person can hold in their head.
pub const MAX_PROFILES: usize = 10;

/// The one that always exists and never goes. Seeded at migration with an empty
/// name; a null `profile_id` is never allowed to survive, so the UI reads one
/// uniform list.
pub const DEFAULT_ID: &str = "default";

pub const NAME_MAX: usize = 40;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub created_at: String,
}
