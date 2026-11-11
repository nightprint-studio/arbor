//! # bennu-link
//!
//! The seam between Bennu's tools and the **live things** Arbor attaches to — a debugged program, a running
//! game — kept out of the backend (`bennu-be`) so it can be tested on its own, against a fake game, without
//! Tauri, Bennu's state or a window.
//!
//! | Module | What |
//! |---|---|
//! | [`target`] | [`target::LiveTarget`] and [`target::Capability`]: who a target is and what it can do. Shared by the debugger and the game. |
//! | [`game`] | [`game::GameLink`]: the operations of a *game* (screenshot, input, command, HUD…), each defaulting to "this link has no …". |
//! | [`pick`] | which link a call is routed to: `auto` (`dev` first, then BRP), `dev`, `brp`. |
//! | [`dev`] | [`dev::DevChannelLink`]: the link over the fulcrum `dev` channel (`fulcrum-dev-channel`, topic `dev`), and the discovery of games. |
//! | [`session`] | the one `dev` session of the process and the protocol the user chose. |
//! | [`args`] | the argument structs of the game tools (with the schema descriptions an MCP client reads). |
//! | [`pose`] | the numeric pose of a node or an entity (now, or sampled over N frames with a summary), shaped for an assistant (pure). |
//! | [`vision`] | the picture-free ways to look at a game: the UI tree and image analysis, shaped for an assistant (pure; PNG analysis of a file on disk). |
//!
//! The Bevy Remote Protocol link is **not** here: it is the inspector's existing connection, owned by
//! `bennu-be::brp`, and it implements [`game::GameLink`] from there. See `docs/arbor-gioco-collegato.md` of
//! Voriz for the whole design.

pub mod args;
pub mod dev;
pub mod game;
pub mod pick;
pub mod pose;
pub mod session;
pub mod target;
pub mod vision;

pub mod prelude {
    pub use crate::args::*;
    pub use crate::dev::{list_games, DevChannelLink, DevTarget, FoundDevGame};
    pub use crate::game::{GameLink, Shot};
    pub use crate::pick::{pick, Pick, Protocol, NOT_CONNECTED};
    pub use crate::vision::{analyze_file, parse_ops, results_json, scratch_path, ui_json, Detail, MAX_OPS};
    pub use crate::target::{summarize, unsupported, Capability, LiveTarget, TargetKind, TargetSummary};
}
