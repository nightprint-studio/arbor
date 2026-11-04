//! The crate's public API: `use arbor_bevy_remote::prelude::*;`.

pub use crate::actions::{RemoteActions, RemoteActionsExt, run_action};
// The derive and the trait share a name, like serde's `Serialize`: one is a macro, the other a trait.
pub use arbor_bevy_remote_derive::RemoteInspect;
pub use crate::inspect::{Hint, InspectRegistry, RemoteInspect, RemoteInspectExt, hint_json};
pub use crate::input::{InputQueue, KeyArgs, MouseArgs, PadArgs, Pending, TextArgs, cancel_pending, key_code};
pub use crate::keys::{KEY_NAMES, PAD_AXES, PAD_BUTTONS, canonical_key, canonical_pad_axis, canonical_pad_button, is_modifier};
pub use crate::methods::{METHODS, with_arbor_methods};
pub use crate::png::{Order, Shot, encode_capped, encode_rgb};
pub use crate::presence::{ACTING_HOLD, Presence, PresenceColors, PresenceState, Rgba};
pub use crate::schedule::Queue;
pub use crate::screenshot::{DEFAULT_MAX_BYTES, FetchArgs, ShotArgs, Shots};
pub use crate::stats::{FrameHistory, Summary};
pub use crate::telemetry::{FrameTimes, StatsArgs};
pub use crate::{ArborRemotePlugin, RemotePresence};
