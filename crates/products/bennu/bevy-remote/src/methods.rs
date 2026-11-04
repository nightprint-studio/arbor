//! The BRP methods this crate adds, all under the `arbor.` prefix so they can never shadow a stock one.
//!
//! | Method | Does |
//! |---|---|
//! | `arbor.info` | version, methods, key / button names, registered actions, presence |
//! | `arbor.screenshot` / `arbor.screenshot.fetch` | start a capture; fetch the PNG (base64) |
//! | `arbor.input.key` / `.text` / `.mouse` / `.gamepad` / `.cancel` | synthetic input |
//! | `arbor.action` / `arbor.actions.list` | trigger / list the game's registered actions |
//! | `arbor.stats` | frame-time percentiles, diagnostics, window |
//! | `arbor.presence` | declare (or read) the indicator's state |
//! | `arbor.inspect_hints` | the display hints registered with `#[derive(RemoteInspect)]` |
//!
//! Methods that act on the game mark the presence indicator *acting*; reads do not.

use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, RemotePlugin};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::RemotePresence;
use crate::actions::{RemoteActions, run_action};
use crate::inspect::InspectRegistry;
use crate::input::{self, KeyArgs, MouseArgs, PadArgs, TextArgs};
use crate::keys::{KEY_NAMES, PAD_AXES, PAD_BUTTONS};
use crate::presence::PresenceState;
use crate::screenshot::{self, FetchArgs, ShotArgs};
use crate::telemetry::{self, StatsArgs};

/// JSON-RPC's "invalid params" and "internal error".
const INVALID_PARAMS: i16 = -32602;
const INTERNAL: i16 = -32603;

pub const METHODS: &[&str] = &[
    "arbor.info", "arbor.screenshot", "arbor.screenshot.fetch", "arbor.input.key", "arbor.input.text",
    "arbor.input.mouse", "arbor.input.gamepad", "arbor.input.cancel", "arbor.action", "arbor.actions.list",
    "arbor.stats", "arbor.presence", "arbor.inspect_hints",
];

/// Add every `arbor.*` method to a `RemotePlugin`:
/// `with_arbor_methods(RemotePlugin::default())`.
pub fn with_arbor_methods(remote: RemotePlugin) -> RemotePlugin {
    remote
        .with_method("arbor.info", info)
        .with_method("arbor.screenshot", screenshot_start)
        .with_method("arbor.screenshot.fetch", screenshot_fetch)
        .with_method("arbor.input.key", input_key)
        .with_method("arbor.input.text", input_text)
        .with_method("arbor.input.mouse", input_mouse)
        .with_method("arbor.input.gamepad", input_gamepad)
        .with_method("arbor.input.cancel", input_cancel)
        .with_method("arbor.action", action)
        .with_method("arbor.actions.list", actions_list)
        .with_method("arbor.stats", stats)
        .with_method("arbor.presence", presence)
        .with_method("arbor.inspect_hints", inspect_hints)
}

// ── Plumbing ─────────────────────────────────────────────────────────────────

fn parse<T: DeserializeOwned>(params: Option<Value>) -> Result<T, BrpError> {
    serde_json::from_value(params.unwrap_or_else(|| json!({})))
        .map_err(|e| BrpError { code: INVALID_PARAMS, message: format!("invalid params: {e}"), data: None })
}

fn failed(message: String) -> BrpError {
    BrpError { code: INTERNAL, message, data: None }
}

fn mark_acting(world: &mut World) {
    let now = world.get_resource::<Time<Real>>().map_or(0.0, Time::elapsed_secs_f64);
    if let Some(mut p) = world.get_resource_mut::<RemotePresence>() {
        p.0.mark_acting(now);
    }
}

/// Parse, mark the client as acting, run.
fn act<T: DeserializeOwned>(world: &mut World, params: Option<Value>, f: impl FnOnce(&mut World, T) -> Result<Value, String>) -> BrpResult {
    let args = parse::<T>(params)?;
    mark_acting(world);
    f(world, args).map_err(failed)
}

/// Parse and run, without marking presence: a read.
fn read<T: DeserializeOwned>(world: &mut World, params: Option<Value>, f: impl FnOnce(&mut World, T) -> Result<Value, String>) -> BrpResult {
    f(world, parse::<T>(params)?).map_err(failed)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn info(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let actions: Vec<Value> = world
        .get_resource::<RemoteActions>()
        .map(|a| a.list().into_iter().map(|(n, d)| json!({ "name": n, "description": d })).collect())
        .unwrap_or_default();
    let presence = world.get_resource::<RemotePresence>().map(|p| json!({ "enabled": p.0.enabled, "state": p.0.base_state(), "label": p.0.label }));
    Ok(json!({
        "crate": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "bevy": "0.18",
        "methods": METHODS,
        "keys": KEY_NAMES,
        "gamepad_buttons": PAD_BUTTONS,
        "gamepad_axes": PAD_AXES,
        "actions": actions,
        "presence": presence,
    }))
}

fn screenshot_start(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act(world, params, screenshot::start)
}

fn screenshot_fetch(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    read::<FetchArgs>(world, params, screenshot::fetch)
}

fn input_key(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act::<KeyArgs>(world, params, input::send_key)
}

fn input_text(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act::<TextArgs>(world, params, input::send_text)
}

fn input_mouse(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act::<MouseArgs>(world, params, input::send_mouse)
}

fn input_gamepad(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act::<PadArgs>(world, params, input::send_pad)
}

fn input_cancel(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    Ok(json!({ "cancelled": input::cancel_pending(world) }))
}

#[derive(Deserialize)]
struct ActionArgs {
    name: String,
    #[serde(default)]
    arg: Option<Value>,
}

fn action(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    act::<ActionArgs>(world, params, |world, a| run_action(world, &a.name, a.arg))
}

fn actions_list(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let list: Vec<Value> = world
        .get_resource::<RemoteActions>()
        .map(|a| a.list().into_iter().map(|(n, d)| json!({ "name": n, "description": d })).collect())
        .unwrap_or_default();
    Ok(json!({ "actions": list }))
}

fn stats(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    read::<StatsArgs>(world, params, telemetry::read)
}

/// The display hints the game registered with `#[derive(RemoteInspect)]`: `type path → field → hint`.
fn inspect_hints(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    Ok(world.get_resource::<InspectRegistry>().map_or_else(|| json!({}), InspectRegistry::to_json))
}

#[derive(Deserialize, Default)]
struct PresenceArgs {
    #[serde(default)]
    state: Option<PresenceState>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

fn presence(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    read::<PresenceArgs>(world, params, |world, a| {
        let mut p = world.get_resource_mut::<RemotePresence>().ok_or("the presence indicator is not installed")?;
        if let Some(enabled) = a.enabled {
            p.0.enabled = enabled;
        }
        if let Some(state) = a.state {
            p.0.declare(state, a.label);
        } else if let Some(label) = a.label {
            p.0.label = label;
        }
        Ok(json!({ "enabled": p.0.enabled, "state": p.0.base_state(), "label": p.0.label }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_params_are_an_invalid_params_error_that_names_the_problem() {
        let err = parse::<KeyArgs>(Some(json!({ "modifiers": [] }))).unwrap_err();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("key"), "{}", err.message);
    }

    #[test]
    fn missing_params_parse_as_an_empty_object() {
        assert!(parse::<StatsArgs>(None).is_ok());
    }

    #[test]
    fn the_listed_methods_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for m in METHODS {
            assert!(m.starts_with("arbor.") && seen.insert(*m), "{m}");
        }
    }
}
