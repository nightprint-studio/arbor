//! Driving the game: screenshots, input, actions, statistics and the presence indicator — the
//! `arbor.*` methods of the **`arbor-bevy-remote`** crate the game adds next to Bevy's own remote plugins.
//!
//! These handlers are the seam for everything that drives a game: the inspector's Screenshot, Input and
//! Stats views call them, Imber's tools call them, and — being `mcp(...)` handlers — an MCP client can too.
//! They share the inspector's connection; a game without the companion plugin answers "method not found",
//! which [`arbor`] turns into the sentence that says what to add.
//!
//! Classification: observing (screenshot, stats, info, action list) is `read`; input and presence are
//! `write` (the game reacts, nothing on disk changes); running a game's own action is `destructive`,
//! because what an action does is the game's business and may not be undoable.

use std::time::Duration;

use arbor_brp::prelude::BrpError;
use bennu_core::prelude::BennuState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{err_string, live, touch};

const NOT_INSTALLED: &str = "The game does not answer `arbor.*` methods: add the `arbor-bevy-remote` plugin to it \
(`with_arbor_methods(RemotePlugin::default())` and `ArborRemotePlugin::default()`, see the Bevy Remote docs).";
/// How long a screenshot may take to come back: it finishes a few frames after it is asked for.
const SHOT_WAIT: Duration = Duration::from_secs(8);
const SHOT_POLL: Duration = Duration::from_millis(100);

/// Call an `arbor.*` method on the connected game.
pub(super) async fn arbor(method: &str, params: Value) -> Result<Value, String> {
    let live = live()?;
    match live.client.call(method, Some(params)).await {
        Ok(v) => {
            touch();
            Ok(v)
        }
        Err(BrpError::Rpc { message, .. }) if message.to_lowercase().contains("not found") => Err(NOT_INSTALLED.to_string()),
        Err(e) => Err(err_string(e)),
    }
}

fn params<T: Serialize>(args: &T) -> Result<Value, String> {
    serde_json::to_value(args).map_err(|e| e.to_string())
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// What the game's companion plugin offers: version, methods, the key and gamepad names it accepts, the
/// actions the game registered, and the presence indicator's state.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_info",
    title = "What the running Bevy game lets you drive",
    safety = read,
))]
async fn bennu_brp_arbor_info(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.info", json!({})).await
}

/// Args for [`bennu_brp_screenshot`].
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    /// Shrink the picture to at most this many pixels wide (default: full size, capped at 4 MB). A smaller
    /// picture is cheaper to look at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    /// A window entity id; default the primary window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
}

/// A PNG handed back inline — the shape `output = image` expects — with its size.
#[derive(Serialize, JsonSchema)]
pub struct Screenshot {
    /// Always `image/png`.
    pub mime_type: String,
    /// Base64, no data-URI prefix.
    pub data: String,
    pub width: u64,
    pub height: u64,
}

/// A screenshot of the running Bevy game's window. Needs the `arbor-bevy-remote` plugin in the game.
/// Images are expensive to read: prefer `max_width` of 800–1000 unless detail matters.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_screenshot",
    title = "Screenshot of the running Bevy game",
    safety = read,
    output = image,
))]
async fn bennu_brp_screenshot(_ctx: &BennuState, args: ScreenshotArgs) -> Result<Screenshot, String> {
    let started = arbor("arbor.screenshot", params(&args)?).await?;
    let id = started["id"].as_u64().ok_or("the game did not say which screenshot it started")?;
    let deadline = std::time::Instant::now() + SHOT_WAIT;
    loop {
        tokio::time::sleep(SHOT_POLL).await;
        let r = arbor("arbor.screenshot.fetch", json!({ "id": id })).await?;
        if r["ready"].as_bool() == Some(true) {
            return Ok(Screenshot {
                mime_type: "image/png".into(),
                data: r["png_base64"].as_str().unwrap_or_default().to_string(),
                width: r["width"].as_u64().unwrap_or(0),
                height: r["height"].as_u64().unwrap_or(0),
            });
        }
        if std::time::Instant::now() >= deadline {
            return Err("the game did not finish the screenshot in time (is it paused, minimised or not rendering?)".into());
        }
    }
}

/// Args for [`bennu_brp_stats`].
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct StatsArgs {
    /// Also return the last N frame times in milliseconds (at most 600), for a graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<usize>,
}

/// Runtime statistics of the running Bevy game: frame time (average, min, max, p50 / p95 / p99), FPS, entity
/// count and every other diagnostic the game registered (system timings included), and the window size.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_stats",
    title = "Frame time, FPS and diagnostics of the running Bevy game",
    safety = read,
))]
async fn bennu_brp_stats(_ctx: &BennuState, args: StatsArgs) -> Result<Value, String> {
    arbor("arbor.stats", params(&args)?).await
}

/// The display hints the game registered with `#[derive(RemoteInspect)]`: `type path → field → hint`
/// (a colour picker here, a slider there). Not an assistant tool: it only shapes how the inspector draws.
#[arbor_rpc::handler]
async fn bennu_brp_inspect_hints(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.inspect_hints", json!({})).await
}

/// The actions the running game registered for remote clients, with their descriptions.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_actions",
    title = "Actions the running Bevy game offers",
    safety = read,
))]
async fn bennu_brp_actions(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.actions.list", json!({})).await
}

// ── Acting ───────────────────────────────────────────────────────────────────

/// Args for [`bennu_brp_key`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct KeyArgs {
    /// A key: `a`, `7`, `Enter`, `Escape`, `Space`, `F5`, `ArrowUp`, `ctrl`, … (`bevy_game_info` lists them all).
    pub key: String,
    /// `press` (default: down, held a moment, up), `down` or `up`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Modifiers held during the press: `["ctrl", "shift"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    /// How long a `press` is held, in milliseconds (default 60).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u64>,
}

/// Press a key in the running Bevy game as if on the keyboard. The input is real to the game: it acts on
/// live state with no undo.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_key",
    title = "Press a key in the running Bevy game",
    safety = write,
))]
async fn bennu_brp_key(_ctx: &BennuState, args: KeyArgs) -> Result<Value, String> {
    arbor("arbor.input.key", params(&args)?).await
}

/// Args for [`bennu_brp_text`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TextArgs {
    /// Text to type, 1–500 characters.
    pub text: String,
    /// Gap between characters in milliseconds (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
}

/// Type text into the running Bevy game (a focused text field, a console).
#[arbor_rpc::handler(mcp(
    name = "bevy_game_type",
    title = "Type text in the running Bevy game",
    safety = write,
))]
async fn bennu_brp_text(_ctx: &BennuState, args: TextArgs) -> Result<Value, String> {
    arbor("arbor.input.text", params(&args)?).await
}

/// Args for [`bennu_brp_mouse`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MouseArgs {
    /// `move`, `down`, `up`, `click`, `double_click`, `drag` or `wheel`.
    pub action: String,
    /// Logical window pixels, origin top-left (required for move, click, double_click, drag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// `drag` ends here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_y: Option<f32>,
    /// `left` (default), `right`, `middle`, `back`, `forward`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    /// `drag`: intermediate moves (default 12) over `duration_ms` (default 300).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `wheel`: scroll amounts, in `line` (default) or `pixel` units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dx: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Move, click, double-click, drag or scroll the mouse in the running Bevy game. Coordinates are logical
/// pixels of the window; take a `bevy_game_screenshot` first to know where things are.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_mouse",
    title = "Use the mouse in the running Bevy game",
    safety = write,
))]
async fn bennu_brp_mouse(_ctx: &BennuState, args: MouseArgs) -> Result<Value, String> {
    arbor("arbor.input.mouse", params(&args)?).await
}

/// Args for [`bennu_brp_gamepad`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GamepadArgs {
    /// A button: `South` (or `a`), `East`/`b`, `North`/`y`, `West`/`x`, `Start`, `DPadUp`, `LeftTrigger`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    /// `press` (default), `down` or `up` — for a button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// An axis instead of a button: `LeftStickX`, `LeftStickY`, `RightStickX`, `RightStickY`, `LeftZ`, `RightZ`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    /// The axis position, -1..1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u64>,
}

/// Press a gamepad button or set an axis in the running Bevy game. A gamepad must already be connected to it.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_gamepad",
    title = "Use the gamepad in the running Bevy game",
    safety = write,
))]
async fn bennu_brp_gamepad(_ctx: &BennuState, args: GamepadArgs) -> Result<Value, String> {
    arbor("arbor.input.gamepad", params(&args)?).await
}

/// Drop input the game has not delivered yet (a key release or a drag still in progress).
#[arbor_rpc::handler(mcp(
    name = "bevy_game_cancel_input",
    title = "Cancel pending input in the running Bevy game",
    safety = write,
))]
async fn bennu_brp_input_cancel(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.input.cancel", json!({})).await
}

/// Args for [`bennu_brp_action`].
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionArgs {
    /// An action name from `bevy_game_actions`.
    pub name: String,
    /// The action's argument, if it takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<Value>,
}

/// Run one of the actions the running game registered (`restart`, `spawn_wave`, …). What it does is the
/// game's own business and may not be undoable: read its description in `bevy_game_actions` first.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_action",
    title = "Run a named action in the running Bevy game",
    safety = destructive,
))]
async fn bennu_brp_action(_ctx: &BennuState, args: ActionArgs) -> Result<Value, String> {
    arbor("arbor.action", params(&args)?).await
}

/// Args for [`bennu_brp_presence`].
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PresenceArgs {
    /// `connected`, `paused` or `idle` (hides the indicator).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Who is attached, shown beside the border (e.g. `Imber`, `Arbor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Show or hide the coloured border and label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Set or read the on-screen indicator that a client is attached to the game. With no arguments it only reads.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_presence",
    title = "Presence indicator of the running Bevy game",
    safety = write,
))]
async fn bennu_brp_presence(_ctx: &BennuState, args: PresenceArgs) -> Result<Value, String> {
    arbor("arbor.presence", params(&args)?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_arguments_are_not_sent_as_nulls() {
        let v = params(&KeyArgs { key: "a".into(), action: None, modifiers: vec![], hold_ms: None }).unwrap();
        assert_eq!(v, json!({ "key": "a" }));
        let v = params(&MouseArgs { action: "click".into(), x: Some(1.0), y: Some(2.0), to_x: None, to_y: None, button: None, steps: None, duration_ms: None, dx: None, dy: None, unit: None }).unwrap();
        assert_eq!(v, json!({ "action": "click", "x": 1.0, "y": 2.0 }));
        assert_eq!(params(&StatsArgs::default()).unwrap(), json!({}));
    }

    #[test]
    fn the_missing_plugin_sentence_names_what_to_add() {
        assert!(NOT_INSTALLED.contains("arbor-bevy-remote") && NOT_INSTALLED.contains("with_arbor_methods"));
    }
}
