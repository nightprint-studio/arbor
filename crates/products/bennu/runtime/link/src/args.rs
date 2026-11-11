//! The arguments the game tools take, whichever link answers them.
//!
//! They used to live beside the BRP handlers; they moved here when the `dev` channel became a second way to
//! answer the same calls, so the MCP tools, Imber's tools and both links read one definition. The field
//! `///`s are the per-parameter descriptions a client reads — they are part of the tool, not commentary.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Args for a screenshot.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    /// Shrink the picture to at most this many pixels wide (default: full size, capped at 4 MB). A smaller
    /// picture is cheaper to look at. On the `dev` channel it bounds the longest side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    /// A window entity id; default the primary window. Bevy Remote Protocol only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
}

/// Args for the frame statistics.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct StatsArgs {
    /// Also return the last N frame times in milliseconds (at most 600), for a graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<usize>,
}

/// Args for a key press.
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
    /// How long a `press` is held, in milliseconds (default 60). The `dev` channel holds a press for a few frames and ignores this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<u64>,
}

/// Args for typing text.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TextArgs {
    /// Text to type, 1–500 characters.
    pub text: String,
    /// Gap between characters in milliseconds (default 0). The `dev` channel types one character per frame and ignores this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
}

/// Args for the mouse.
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
    /// `left` (default), `right`, `middle`, `back`, `forward` (the `dev` channel has the first three).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    /// `drag`: intermediate moves (default 12) over `duration_ms` (default 300).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `wheel`: scroll amounts, in `line` (default) or `pixel` units. The `dev` channel scrolls vertically (`dy`) only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dx: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Args for a named action.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionArgs {
    /// An action name from `bevy_game_actions`. On the `dev` channel there is no registry: the name is the
    /// first word of a console line, and `arg` the rest of it.
    pub name: String,
    /// The action's argument, if it takes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<Value>,
}

/// Args for a console command.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommandArgs {
    /// One line of the game's console, as typed: a verb of the game or of one of its mods and its arguments
    /// (`cozy on`, `hs start`). No leading slash.
    pub line: String,
    /// Run it as the server console (every permission group, no body in the world) instead of as the player
    /// of that process. Default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_console: Option<bool>,
}

/// Args for the HUD values.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct HudArgs {
    /// Only the values this mod wrote; default every mod.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
}

/// Args for the chat and console log.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChatArgs {
    /// How many of the latest lines (default 30, at most 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<u32>,
}

/// Args for a mod's state.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModStateArgs {
    /// The mod's id (`bevy_game_mods` lists them).
    pub mod_id: String,
    /// The state type the mod declared (`Run`, `Settings`, …).
    pub ty: String,
    /// The player it is about; omit for the world's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
}

/// Args for connecting to a game.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GameConnectArgs {
    /// `auto` (default: the `dev` channel first, then the Bevy Remote Protocol), `dev` or `brp`. The choice is
    /// kept: the game tools use that link from now on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// `dev` channel: the game's id when more than one is running (`bevy_game_status` lists them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// `dev` channel: connect to this port directly instead of looking in the discovery folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Bevy Remote Protocol: the HTTP endpoint (default `http://127.0.0.1:15702`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

/// Args for closing the link to a game.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct GameDisconnectArgs {
    /// `dev`, `brp` or `auto` (default: both). The game itself keeps running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unset_arguments_are_not_sent_as_nulls() {
        let v = serde_json::to_value(KeyArgs { key: "a".into(), action: None, modifiers: vec![], hold_ms: None }).unwrap();
        assert_eq!(v, json!({ "key": "a" }));
        assert_eq!(serde_json::to_value(StatsArgs::default()).unwrap(), json!({}));
        assert_eq!(serde_json::to_value(GameConnectArgs::default()).unwrap(), json!({}));
        assert_eq!(serde_json::to_value(HudArgs::default()).unwrap(), json!({}));
    }

    #[test]
    fn a_command_needs_only_its_line() {
        let a: CommandArgs = serde_json::from_value(json!({ "line": "cozy on" })).unwrap();
        assert_eq!(a.line, "cozy on");
        assert!(a.as_console.is_none());
        assert!(serde_json::from_value::<CommandArgs>(json!({})).is_err());
    }
}
