//! Driving the game over the Bevy Remote Protocol: screenshots, input, actions, statistics and the presence
//! indicator — the `arbor.*` methods of the **`arbor-bevy-remote`** crate the game adds next to Bevy's own
//! remote plugins.
//!
//! These handlers are the **BRP-exact** seam: the inspector's Screenshot, Input and Stats views call them, and
//! so does [`super::link::BrpLink`], the BRP side of the neutral `GameLink` trait. They share the inspector's
//! connection; a game without the companion plugin answers "method not found", which [`arbor`] turns into the
//! sentence that says what to add.
//!
//! ## What is an MCP tool and what is not
//!
//! Since the `dev` channel became a second way to drive a game (11 nov 2026), the MCP tools `bevy_game_info`,
//! `_screenshot`, `_stats`, `_actions`, `_key`, `_type`, `_mouse` and `_action` are published by
//! `crate::link::tools`, which routes each call to whichever link is open. They used to be the `mcp(...)`
//! attribute of the handlers below; the handlers stay — as plain handlers — for the panel. What only the
//! companion plugin has stays here with its MCP name: the gamepad, cancelling pending input, and the presence
//! indicator.
//!
//! Classification of the tools that remain: input and presence are `write` (the game reacts, nothing on disk
//! changes).

use std::time::Duration;

use arbor_brp::prelude::BrpError;
use bennu_core::prelude::BennuState;
// The arguments of the calls both links answer live in `bennu-link` (the `dev` channel reads the same ones).
use bennu_link::prelude::{ActionArgs, KeyArgs, MouseArgs, ScreenshotArgs, Shot, StatsArgs, TextArgs};
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

pub(super) fn params<T: Serialize>(args: &T) -> Result<Value, String> {
    serde_json::to_value(args).map_err(|e| e.to_string())
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// What the game's companion plugin offers: version, methods, the key and gamepad names it accepts, the
/// actions the game registered, and the presence indicator's state.
#[arbor_rpc::handler]
async fn bennu_brp_arbor_info(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.info", json!({})).await
}

/// A screenshot of the running Bevy game's window. Needs the `arbor-bevy-remote` plugin in the game.
/// Images are expensive to read: prefer `max_width` of 800–1000 unless detail matters.
#[arbor_rpc::handler]
async fn bennu_brp_screenshot(_ctx: &BennuState, args: ScreenshotArgs) -> Result<Shot, String> {
    take_screenshot(args).await
}

/// Ask the game for a picture and wait for it: it finishes a few frames after it is asked for. The one
/// implementation behind the panel's handler and [`super::link::BrpLink`].
pub(super) async fn take_screenshot(args: ScreenshotArgs) -> Result<Shot, String> {
    let started = arbor("arbor.screenshot", params(&args)?).await?;
    let id = started["id"].as_u64().ok_or("the game did not say which screenshot it started")?;
    let deadline = std::time::Instant::now() + SHOT_WAIT;
    loop {
        tokio::time::sleep(SHOT_POLL).await;
        let r = arbor("arbor.screenshot.fetch", json!({ "id": id })).await?;
        if r["ready"].as_bool() == Some(true) {
            return Ok(Shot {
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

/// Runtime statistics of the running Bevy game: frame time (average, min, max, p50 / p95 / p99), FPS, entity
/// count and every other diagnostic the game registered (system timings included), and the window size.
#[arbor_rpc::handler]
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
#[arbor_rpc::handler]
async fn bennu_brp_actions(_ctx: &BennuState) -> Result<Value, String> {
    arbor("arbor.actions.list", json!({})).await
}

// ── Acting ───────────────────────────────────────────────────────────────────

/// Press a key in the running Bevy game as if on the keyboard. The input is real to the game: it acts on
/// live state with no undo.
#[arbor_rpc::handler]
async fn bennu_brp_key(_ctx: &BennuState, args: KeyArgs) -> Result<Value, String> {
    arbor("arbor.input.key", params(&args)?).await
}

/// Type text into the running Bevy game (a focused text field, a console).
#[arbor_rpc::handler]
async fn bennu_brp_text(_ctx: &BennuState, args: TextArgs) -> Result<Value, String> {
    arbor("arbor.input.text", params(&args)?).await
}

/// Move, click, double-click, drag or scroll the mouse in the running Bevy game. Coordinates are logical
/// pixels of the window; take a `bevy_game_screenshot` first to know where things are.
#[arbor_rpc::handler]
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

/// Run one of the actions the running game registered (`restart`, `spawn_wave`, …). What it does is the
/// game's own business and may not be undoable: read its description in `bevy_game_actions` first.
#[arbor_rpc::handler]
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
