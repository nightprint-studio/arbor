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

/// Args for the UI tree.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UiArgs {
    /// Which part of the interface: a screen (`pausa`), one node (`pausa/riprendi`) or a prefix of ids
    /// (`hud/matassa*`). Default: every screen. Narrow it: a whole HUD is hundreds of nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    /// Also list the nodes that are switched off or hidden (default false: only what the player can see).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_hidden: Option<bool>,
}

/// Args for the numeric pose of a node or an entity.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PoseArgs {
    /// What to read: a UI node `scope/id` (`hud/banner`), an entity by name (`entity:cristallo`), or whatever the game
    /// recognises (`player`).
    pub target: String,
    /// Sample this many frames in one round trip (2 to 2000) and get the table with a summary (peak, overshoot, settling).
    /// Omit it for the pose right now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticks: Option<u32>,
    /// With `ticks`: take one sample every this many frames (default 1, at most 600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<u32>,
}

/// Args for analysing the picture on screen.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AnalyzeArgs {
    /// What to compute, one string each (at most 12): `stats`, `palette[:k]`, `count:#rrggbb[~tol]`,
    /// `bbox:#rrggbb[~tol]`, `edges[:min_run]`, `grid[:CxR]`, `thumb[:side[:bytes]]`, `diff:other.png[:threshold]`.
    /// Each can carry its own region after an `@` (`stats@ui:cozy/pill`). Default: `["stats"]`.
    #[serde(default)]
    pub ops: Vec<String>,
    /// The region every operation without its own looks at: `x0,y0,x1,y1` as fractions 0..1 of the picture
    /// (`0.7,0,1,0.15` is the top-right corner), `px:x,y,w,h`, or `ui:scope/id` for a UI node. Default: the whole picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// `summary` (default: one sentence per operation), `json` (the structured result too) or `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Args for analysing a PNG on disk.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ImageAnalyzeArgs {
    /// The PNG. With `root` it is a path inside that project (scope-checked); without, an absolute path to a `.png`
    /// in the system temporary folder (where screenshots and scratch pictures live).
    pub file: String,
    /// A project root the file belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// As in `bevy_game_analyze`: `stats`, `palette[:k]`, `count:#rrggbb[~tol]`, `bbox:…`, `edges`, `grid[:CxR]`, `thumb[:side]`, `diff:other.png`.
    /// A `ui:` region needs a running game and is refused here.
    #[serde(default)]
    pub ops: Vec<String>,
    /// Region for the operations without their own: fractions `x0,y0,x1,y1` or `px:x,y,w,h`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// `summary` (default), `json` or `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Args for the summary of a region of the live world.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct RegionArgs {
    /// One corner of the box, in blocks `[x, y, z]` (the other is `max`). Both corners are inside the box. Omit both to look around the player instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<[i32; 3]>,
    /// The opposite corner, in blocks `[x, y, z]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<[i32; 3]>,
    /// Without `min` and `max`: the box around the local player, this many blocks to each side (default 16, at most 64), 8 below and 24 above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub around_player: Option<i32>,
    /// Only these parts of the summary, which makes it cheaper: `counts`, `layers`, `heights`, `sections`, `voids`, `marks`. Default: all.
    #[serde(default)]
    pub parts: Vec<String>,
}

/// Args for the models and props placed in the world.
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PropsArgs {
    /// A fragment of the name to keep (case-insensitive), or `mod:<id>` for the models one mod placed. Default: all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// Args for the summary of a map file (no game needed).
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MapSchedaArgs {
    /// The map: an absolute path to a `.fmap.ron` (or to the cooked `.fmap` next to it); with `root`, a path inside that project.
    pub path: String,
    /// A project root the map belongs to (scope-checked, like `image_analyze`). Without it `path` must be absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// A second map, or a summary saved earlier (`.json`): the answer is then what changed from `path` (before) to this (after).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<String>,
    /// Only these parts: `counts`, `layers`, `heights`, `sections`, `voids`, `marks`. Default: all.
    #[serde(default)]
    pub parts: Vec<String>,
    /// Columns per character of the height profile (default: the smallest that fits 72 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<u32>,
}

/// Args for "can the player walk from A to B" on a map file (no game needed).
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MapPercorsoArgs {
    /// The map: an absolute path to a `.fmap.ron` (or to the cooked `.fmap` next to it); with `root`, a path inside that project.
    pub path: String,
    /// A project root the map belongs to (scope-checked, like `image_analyze`). Without it `path` must be absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// Where from: a mark name (`start`) or a position `x,y,z` (x and z are cell indices, y is in blocks).
    pub from: String,
    /// Where to: a mark, or a chain joined with `+` (`checkpoint:*+finish`; the `*` takes every mark with that prefix, in numeric order).
    pub to: String,
    /// The highest jump in blocks (default 1.26: the game's jump speed 8.4 with gravity 28).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<f32>,
    /// The reach of a running jump on level ground, in blocks (default 3.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<f32>,
    /// The step walked up without jumping (default 0.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f32>,
    /// Slack added to the gap of a jump (default 0: strict; the body overhangs 0.3 per side, so the real game grants up to 0.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<f32>,
    /// Also return each leg's route as a polyline of `[x, y, z]` points (default false: for a long route it is half the bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polyline: Option<bool>,
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
    fn the_new_game_tools_send_no_nulls() {
        assert_eq!(serde_json::to_value(UiArgs::default()).unwrap(), json!({}));
        assert_eq!(serde_json::to_value(AnalyzeArgs::default()).unwrap(), json!({ "ops": [] }));
        let a: AnalyzeArgs = serde_json::from_value(json!({ "ops": ["stats"], "region": "0,0,1,1" })).unwrap();
        assert_eq!(a.ops, vec!["stats"]);
        assert!(serde_json::from_value::<ImageAnalyzeArgs>(json!({})).is_err());
    }

    #[test]
    fn the_world_as_numbers_tools_send_no_nulls() {
        assert_eq!(serde_json::to_value(RegionArgs::default()).unwrap(), json!({ "parts": [] }));
        assert_eq!(serde_json::to_value(PropsArgs::default()).unwrap(), json!({}));
        let a: MapPercorsoArgs = serde_json::from_value(json!({ "path": "/x.fmap.ron", "from": "start", "to": "finish" })).unwrap();
        assert!(a.jump.is_none() && a.pad.is_none());
        assert!(serde_json::from_value::<MapSchedaArgs>(json!({})).is_err());
        assert!(serde_json::from_value::<MapPercorsoArgs>(json!({ "path": "/x" })).is_err());
    }

    #[test]
    fn a_command_needs_only_its_line() {
        let a: CommandArgs = serde_json::from_value(json!({ "line": "cozy on" })).unwrap();
        assert_eq!(a.line, "cozy on");
        assert!(a.as_console.is_none());
        assert!(serde_json::from_value::<CommandArgs>(json!({})).is_err());
    }
}
