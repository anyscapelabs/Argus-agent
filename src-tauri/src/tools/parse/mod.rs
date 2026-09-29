// Reading a tool call back out of a model's text.
//
// The formats here are not one format. A model may write the template XML, a
// bare tag, a native call, or a partial one that was cut off mid-write. Every
// parser below takes a string a model produced and returns what it could
// recover, and none of them fails: an unparseable fragment yields nothing
// rather than an error, because the loop above already treats a reply that
// describes an action without running one as something to ask about.
//
// The files are the pipeline in order: `argtext` and `salvage` recover pieces,
// `normalize` rewrites them into the canonical syntax, `actions` reads that
// syntax, and `ingress` decides what the reply actually ran.

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
