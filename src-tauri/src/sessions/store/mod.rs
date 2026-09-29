// One SQLite handle, five concerns. The submodules own their tables; this
// file only routes, so callers keep reaching everything at `store::`.
//
// Why not one big file: sessions, messages, events, and folders evolve on
// different clocks — a schema change to `tool_events` should not have to
// survive a re-read of session lifecycle code to get reviewed.

pub mod events_resume;
pub mod folders_export;
pub mod messages;
pub mod migrate;
pub mod sessions;

pub use events_resume::*;
pub use folders_export::*;
pub use messages::*;
pub use migrate::*;
pub use sessions::*;
