//! Canonical entry point for `merula-core`'s public API.
//!
//! Workspace convention: call sites (in `merula-be`) reach this crate's surface
//! through `merula_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::MerulaState;

// The session substrate the audio-command handlers drive against
// `MerulaState::session()`'s guard, plus the typed last-evaluation slot the
// eval / query / scenes domains read and write.
pub use crate::session::{
    ensure, live_handles, send_if_live, set_latest, shutdown, with_latest, LiveHandles, Latest,
    Session,
};

// The control channel the command layer posts down, and the off-thread-decoded
// registry it hands the audio thread.
pub use crate::control::{MerulaControl, Prepared};

// The typed global config the eval / render / audio / packs / models domains read.
pub use crate::config::{MerulaConfig, MerulaRenderConfig};

// The frozen BE→FE event contract (the EVT_* topic consts + the typed payload
// structs the domain handlers emit) + the `emit` helper.
pub use crate::events::*;
