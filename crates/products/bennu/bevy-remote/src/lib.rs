//! # arbor-bevy-remote
//!
//! A Bevy plugin that extends the stock Bevy Remote Protocol server with what an external tool needs to
//! *drive* a game, not just inspect it: screenshots, keyboard / mouse / gamepad input, named actions,
//! runtime statistics, and an on-screen indicator that says someone is attached.
//!
//! ```ignore
//! use arbor_bevy_remote::prelude::*;
//! use bevy::prelude::*;
//! use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};
//!
//! App::new()
//!     .add_plugins((
//!         DefaultPlugins,
//!         with_arbor_methods(RemotePlugin::default()),
//!         RemoteHttpPlugin::default(),
//!         ArborRemotePlugin::default(),
//!     ))
//!     .register_remote_action("restart", "Start a new game", |_world, _arg| Ok(serde_json::json!("ok")))
//!     .run();
//! ```
//!
//! Everything public is re-exported from [`prelude`]; the other modules are public for rustdoc only.

pub mod actions;
pub mod base64;
pub mod inspect;
pub mod input;
pub mod keys;
pub mod methods;
pub mod png;
pub mod prelude;
pub mod presence;
pub mod schedule;
pub mod screenshot;
pub mod stats;
pub mod telemetry;

#[cfg(feature = "overlay")]
mod overlay;

use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

/// The presence state as a resource; the indicator and `arbor.presence` read and write it.
#[derive(Resource, Default)]
pub struct RemotePresence(pub presence::Presence);

/// The systems and resources behind the methods. Add it **alongside** `with_arbor_methods(..)`: the
/// methods are registered on the `RemotePlugin` (which only accepts them at construction), this plugin
/// is what they run against.
#[derive(Debug, Clone)]
pub struct ArborRemotePlugin {
    /// Draw the presence indicator (needs the `overlay` feature). Default `true`.
    pub indicator: bool,
}

impl Default for ArborRemotePlugin {
    fn default() -> Self {
        Self { indicator: true }
    }
}

impl ArborRemotePlugin {
    /// No on-screen indicator; every method still works.
    pub fn without_indicator() -> Self {
        Self { indicator: false }
    }
}

impl Plugin for ArborRemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<actions::RemoteActions>()
            .init_resource::<input::InputQueue>()
            .init_resource::<screenshot::Shots>()
            .init_resource::<telemetry::FrameTimes>()
            .init_resource::<RemotePresence>()
            .init_resource::<inspect::InspectRegistry>()
            .add_systems(First, input::deliver_due_input)
            .add_systems(Last, telemetry::record_frame);
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        app.world_mut().resource_mut::<RemotePresence>().0.enabled = self.indicator;
        #[cfg(feature = "overlay")]
        if self.indicator {
            app.add_systems(Startup, overlay::spawn_overlay).add_systems(Update, overlay::update_overlay);
        }
    }
}
