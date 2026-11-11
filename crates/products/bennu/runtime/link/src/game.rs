//! A game Arbor is attached to, whichever wire it speaks.
//!
//! [`GameLink`] is the seam the game tools (the MCP `bevy_game_*` family and Imber's) call through. Two
//! things implement it: [`crate::dev::DevChannelLink`] here, and the Bevy Remote Protocol link in the
//! backend (`crate::brp::link`), which is the pre-existing BRP code put behind the trait without changing
//! what it does.
//!
//! Every method has a default that answers *"this link has no <capability>"* with the sentence
//! [`unsupported`] builds — so a link implements what it can and declares the same set in
//! [`LiveTarget::capabilities`], and the caller never meets a bare "method not found". A test per link holds
//! the two lists together (what it declares is what it answers).
//!
//! Results are `serde_json::Value`s of a documented shape rather than typed structs: they cross straight to
//! an MCP client or to Imber's text formatter, and the two wires would otherwise need a third, neutral
//! model that only exists to be converted back.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::args::{ActionArgs, AnalyzeArgs, CommandArgs, KeyArgs, MouseArgs, PoseArgs, ScreenshotArgs, StatsArgs, TextArgs, UiArgs};
use crate::target::{unsupported, Capability, LiveTarget};

/// A PNG handed back inline — the shape the MCP `output = image` expects — with its size.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Shot {
    /// Always `image/png`.
    pub mime_type: String,
    /// Base64, no data-URI prefix.
    pub data: String,
    pub width: u64,
    pub height: u64,
}

#[async_trait]
pub trait GameLink: LiveTarget {
    /// What the link is and what the game offers: `{protocol, …}`.
    async fn info(&self) -> Result<Value, String> {
        Err(unsupported(self, Capability::Info, ""))
    }

    async fn screenshot(&self, _args: ScreenshotArgs) -> Result<Shot, String> {
        Err(unsupported(self, Capability::Screenshot, ""))
    }

    async fn key(&self, _args: KeyArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Input, ""))
    }

    async fn text(&self, _args: TextArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Input, ""))
    }

    async fn mouse(&self, _args: MouseArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Input, ""))
    }

    /// Frame time, FPS and the game's diagnostics.
    async fn stats(&self, _args: StatsArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Stats, ""))
    }

    /// The registry of named actions the game published: `{actions: [{name, description}]}`.
    async fn actions(&self) -> Result<Value, String> {
        Err(unsupported(self, Capability::Actions, "Run a console command with bevy_game_command instead."))
    }

    /// Run one named action. A link with a command line runs it as one (`name arg`).
    async fn action(&self, _args: ActionArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Actions, ""))
    }

    /// Run one console line: `{ok, lines}`.
    async fn command(&self, _args: CommandArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Command, "Use bevy_game_action for the actions the game registered."))
    }

    /// The HUD values the mods wrote: `{entries: [{mod, key, value, writes}]}`.
    async fn hud(&self, _mod_id: Option<String>) -> Result<Value, String> {
        Err(unsupported(self, Capability::Hud, ""))
    }

    /// The latest chat and console lines: `{chat: [{to, key, params}], console: [..]}`.
    async fn chat(&self, _last: u32) -> Result<Value, String> {
        Err(unsupported(self, Capability::Chat, ""))
    }

    /// The loaded mods: `[{id, enabled, note, hooks, faults}]`.
    async fn mods(&self) -> Result<Value, String> {
        Err(unsupported(self, Capability::Mods, ""))
    }

    /// The world: `{name, seed, dimension, players: [{name, pos}]}`.
    async fn world(&self) -> Result<Value, String> {
        Err(unsupported(self, Capability::World, ""))
    }

    /// The UI as data, without a picture: `{viewport, scale, flags, count, nodes: [{n: "scope/id", r: [x, y, w, h], t?, v?, a?, bg?, ink?}]}`.
    async fn ui_tree(&self, _args: UiArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::UiTree, "Take a bevy_game_screenshot (small, with max_width) to see the interface instead."))
    }

    /// The pose of a node or entity right now: `{target, kind, pose: {key: value}, anims: [{name, t, length, waiting}], notes}`.
    async fn pose(&self, _args: PoseArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Pose, "Take a bevy_game_screenshot (small, with max_width) to see the motion instead."))
    }

    /// The pose sampled over several frames, with a summary: `{target, kind, keys, rows: [[frame, secs, v…]], anims, summary, notes}`.
    async fn pose_series(&self, _args: PoseArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::Pose, "Take a bevy_game_screenshot (small, with max_width) to see the motion instead."))
    }

    /// Questions about the screen answered in the game, without the picture crossing: `{results: [{op, summary?, data?}]}`.
    async fn analyze(&self, _args: AnalyzeArgs) -> Result<Value, String> {
        Err(unsupported(self, Capability::ImageAnalysis, "Take a bevy_game_screenshot (small, with max_width) and look at it instead."))
    }

    /// The state a mod keeps: `{mod, ty, state}`.
    async fn mod_state(&self, _mod_id: String, _ty: String, _who: Option<String>) -> Result<Value, String> {
        Err(unsupported(self, Capability::ModState, ""))
    }
}
