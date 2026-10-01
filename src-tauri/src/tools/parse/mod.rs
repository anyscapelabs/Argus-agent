// Reading a tool call back out of a model's text. None of these parsers fails:
// an unreadable fragment yields nothing, and the loop above already treats a
// reply that describes an action without running one as something to ask about.

mod actions;
mod argtext;
mod ingress;
mod normalize;
mod salvage;

pub use actions::{close_dangling_actions, parse_actions};
pub use ingress::{
    build_executions, build_executions_styled, has_native_text_duplicate,
    has_orphaned_action_block, render_actions, split_commit, strip_actions, FINAL_MARKER,
};
pub use normalize::normalize_actions;
