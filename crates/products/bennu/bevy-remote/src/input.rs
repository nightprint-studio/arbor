//! Synthetic keyboard, mouse and gamepad input.
//!
//! Input is injected the way Bevy's own backends deliver it — as `KeyboardInput`, `MouseButtonInput`,
//! `MouseWheel`, `CursorMoved` and raw gamepad *messages* (Bevy 0.17+ calls its buffered events
//! messages) — so `ButtonInput<KeyCode>`, `Gamepad`, UI focus and every game system that reads input see
//! it exactly as if it came from a device.
//!
//! A held input is two messages with time between them ([`crate::schedule`]): writing press and release
//! in one frame would leave `pressed()` false by the time a system looks.

use bevy::input::ButtonState;
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent};
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey, NativeKeyCode};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::keys::{canonical_key, canonical_pad_axis, canonical_pad_button, is_modifier};
use crate::schedule::Queue;

const DEFAULT_HOLD_MS: u64 = 60;
const DEFAULT_DRAG_STEPS: u32 = 12;
const DEFAULT_DRAG_MS: u64 = 300;
const MAX_STEPS: u32 = 200;
const MAX_DURATION_MS: u64 = 10_000;
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone)]
pub enum Pending {
    Key { code: KeyCode, state: ButtonState },
    Character { ch: char, state: ButtonState },
    MouseButton { button: MouseButton, state: ButtonState },
    MouseMove { x: f32, y: f32 },
    Wheel { x: f32, y: f32, unit: MouseScrollUnit },
    PadButton { button: GamepadButton, value: f32 },
    PadAxis { axis: GamepadAxis, value: f32 },
}

/// Input waiting for its moment.
#[derive(Resource, Default)]
pub struct InputQueue(pub Queue<Pending>);

fn now(world: &World) -> f64 {
    world.resource::<Time<Real>>().elapsed_secs_f64()
}

fn queue(world: &mut World) -> Mut<'_, InputQueue> {
    world.get_resource_or_insert_with(InputQueue::default)
}

// ── Name → Bevy ──────────────────────────────────────────────────────────────

/// The `KeyCode` for a canonical name from [`crate::keys::KEY_NAMES`].
pub fn key_code(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    Some(match name {
        "KeyA" => KeyA, "KeyB" => KeyB, "KeyC" => KeyC, "KeyD" => KeyD, "KeyE" => KeyE, "KeyF" => KeyF,
        "KeyG" => KeyG, "KeyH" => KeyH, "KeyI" => KeyI, "KeyJ" => KeyJ, "KeyK" => KeyK, "KeyL" => KeyL,
        "KeyM" => KeyM, "KeyN" => KeyN, "KeyO" => KeyO, "KeyP" => KeyP, "KeyQ" => KeyQ, "KeyR" => KeyR,
        "KeyS" => KeyS, "KeyT" => KeyT, "KeyU" => KeyU, "KeyV" => KeyV, "KeyW" => KeyW, "KeyX" => KeyX,
        "KeyY" => KeyY, "KeyZ" => KeyZ,
        "Digit0" => Digit0, "Digit1" => Digit1, "Digit2" => Digit2, "Digit3" => Digit3, "Digit4" => Digit4,
        "Digit5" => Digit5, "Digit6" => Digit6, "Digit7" => Digit7, "Digit8" => Digit8, "Digit9" => Digit9,
        "F1" => F1, "F2" => F2, "F3" => F3, "F4" => F4, "F5" => F5, "F6" => F6,
        "F7" => F7, "F8" => F8, "F9" => F9, "F10" => F10, "F11" => F11, "F12" => F12,
        "ArrowUp" => ArrowUp, "ArrowDown" => ArrowDown, "ArrowLeft" => ArrowLeft, "ArrowRight" => ArrowRight,
        "Enter" => Enter, "Escape" => Escape, "Space" => Space, "Tab" => Tab, "Backspace" => Backspace,
        "Delete" => Delete, "Insert" => Insert, "Home" => Home, "End" => End, "PageUp" => PageUp, "PageDown" => PageDown,
        "ShiftLeft" => ShiftLeft, "ShiftRight" => ShiftRight, "ControlLeft" => ControlLeft, "ControlRight" => ControlRight,
        "AltLeft" => AltLeft, "AltRight" => AltRight, "SuperLeft" => SuperLeft, "SuperRight" => SuperRight,
        "Minus" => Minus, "Equal" => Equal, "Comma" => Comma, "Period" => Period, "Slash" => Slash,
        "Backslash" => Backslash, "Semicolon" => Semicolon, "Quote" => Quote, "Backquote" => Backquote,
        "BracketLeft" => BracketLeft, "BracketRight" => BracketRight, "CapsLock" => CapsLock,
        _ => return None,
    })
}

fn pad_button(name: &str) -> Option<GamepadButton> {
    use GamepadButton::*;
    Some(match name {
        "South" => South, "East" => East, "North" => North, "West" => West,
        "LeftTrigger" => LeftTrigger, "LeftTrigger2" => LeftTrigger2, "RightTrigger" => RightTrigger,
        "RightTrigger2" => RightTrigger2, "Select" => Select, "Start" => Start, "Mode" => Mode,
        "LeftThumb" => LeftThumb, "RightThumb" => RightThumb,
        "DPadUp" => DPadUp, "DPadDown" => DPadDown, "DPadLeft" => DPadLeft, "DPadRight" => DPadRight,
        _ => return None,
    })
}

fn pad_axis(name: &str) -> Option<GamepadAxis> {
    use GamepadAxis::*;
    Some(match name {
        "LeftStickX" => LeftStickX, "LeftStickY" => LeftStickY, "RightStickX" => RightStickX,
        "RightStickY" => RightStickY, "LeftZ" => LeftZ, "RightZ" => RightZ,
        _ => return None,
    })
}

fn mouse_button(name: Option<&str>) -> Result<MouseButton, String> {
    match name.unwrap_or("left").to_ascii_lowercase().as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        "back" => Ok(MouseButton::Back),
        "forward" => Ok(MouseButton::Forward),
        other => Err(format!("unknown mouse button `{other}` (left, right, middle, back, forward)")),
    }
}

fn key(typed: &str) -> Result<KeyCode, String> {
    canonical_key(typed)
        .and_then(key_code)
        .ok_or_else(|| format!("unknown key `{typed}` (try `a`, `Enter`, `Escape`, `F5`, `ArrowUp`, `ctrl`; see arbor.info for the list)"))
}

fn ms(value: Option<u64>, default: u64) -> f64 {
    value.unwrap_or(default).min(MAX_DURATION_MS) as f64 / 1000.0
}

// ── Keyboard ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct KeyArgs {
    pub key: String,
    /// `press` (default), `down` or `up`.
    #[serde(default)]
    pub action: Option<String>,
    /// Held while the key goes down and up: `["ctrl", "shift"]`.
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// How long a `press` is held, in milliseconds.
    #[serde(default)]
    pub hold_ms: Option<u64>,
}

pub fn send_key(world: &mut World, args: KeyArgs) -> Result<Value, String> {
    let code = key(&args.key)?;
    let mut modifiers = Vec::new();
    for m in &args.modifiers {
        let canonical = canonical_key(m).filter(|c| is_modifier(c)).ok_or_else(|| format!("`{m}` is not a modifier (ctrl, shift, alt, super)"))?;
        modifiers.push(key_code(canonical).ok_or_else(|| format!("no key code for {canonical}"))?);
    }
    let (t0, hold) = (now(world), ms(args.hold_ms, DEFAULT_HOLD_MS));
    let action = args.action.as_deref().unwrap_or("press");
    let mut q = queue(world);
    let mut at = |t: f64, code: KeyCode, state: ButtonState| q.0.push(t, Pending::Key { code, state });
    match action {
        "down" => {
            modifiers.iter().for_each(|m| at(t0, *m, ButtonState::Pressed));
            at(t0, code, ButtonState::Pressed);
        }
        "up" => {
            at(t0, code, ButtonState::Released);
            modifiers.iter().rev().for_each(|m| at(t0, *m, ButtonState::Released));
        }
        "press" => {
            modifiers.iter().for_each(|m| at(t0, *m, ButtonState::Pressed));
            at(t0, code, ButtonState::Pressed);
            at(t0 + hold, code, ButtonState::Released);
            modifiers.iter().rev().for_each(|m| at(t0 + hold, *m, ButtonState::Released));
        }
        other => return Err(format!("unknown key action `{other}` (press, down, up)")),
    }
    Ok(json!({ "key": canonical_key(&args.key), "action": action, "modifiers": args.modifiers }))
}

#[derive(Debug, Deserialize)]
pub struct TextArgs {
    pub text: String,
    /// Gap between characters, in milliseconds (default 0: one frame apart).
    #[serde(default)]
    pub interval_ms: Option<u64>,
}

pub fn send_text(world: &mut World, args: TextArgs) -> Result<Value, String> {
    let count = args.text.chars().count();
    if count == 0 || count > MAX_TEXT_CHARS {
        return Err(format!("text must be 1..={MAX_TEXT_CHARS} characters, got {count}"));
    }
    let (t0, gap) = (now(world), ms(args.interval_ms, 0));
    let mut q = queue(world);
    for (i, ch) in args.text.chars().enumerate() {
        let t = t0 + gap * i as f64;
        q.0.push(t, Pending::Character { ch, state: ButtonState::Pressed });
        q.0.push(t + 0.01, Pending::Character { ch, state: ButtonState::Released });
    }
    Ok(json!({ "typed": count }))
}

// ── Mouse ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MouseArgs {
    /// `move`, `down`, `up`, `click`, `double_click`, `drag` or `wheel`.
    pub action: String,
    /// Logical window pixels, origin top-left.
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    /// `drag` ends here.
    #[serde(default)]
    pub to_x: Option<f32>,
    #[serde(default)]
    pub to_y: Option<f32>,
    #[serde(default)]
    pub button: Option<String>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// `wheel`: scroll amounts and `line` (default) or `pixel`.
    #[serde(default)]
    pub dx: Option<f32>,
    #[serde(default)]
    pub dy: Option<f32>,
    #[serde(default)]
    pub unit: Option<String>,
}

pub fn send_mouse(world: &mut World, args: MouseArgs) -> Result<Value, String> {
    let button = mouse_button(args.button.as_deref())?;
    let t0 = now(world);
    let here = args.x.zip(args.y);
    let need = |what: &str| here.ok_or_else(|| format!("`{what}` needs `x` and `y`"));
    let mut q = queue(world);
    let mut push = |t: f64, p: Pending| q.0.push(t, p);
    match args.action.as_str() {
        "move" => {
            let (x, y) = need("move")?;
            push(t0, Pending::MouseMove { x, y });
        }
        "down" | "up" => {
            if let Some((x, y)) = here {
                push(t0, Pending::MouseMove { x, y });
            }
            let state = if args.action == "down" { ButtonState::Pressed } else { ButtonState::Released };
            push(t0, Pending::MouseButton { button, state });
        }
        "click" | "double_click" => {
            let (x, y) = need("click")?;
            push(t0, Pending::MouseMove { x, y });
            let clicks = if args.action == "double_click" { 2 } else { 1 };
            for i in 0..clicks {
                let t = t0 + 0.03 + f64::from(i) * 0.12;
                push(t, Pending::MouseButton { button, state: ButtonState::Pressed });
                push(t + 0.05, Pending::MouseButton { button, state: ButtonState::Released });
            }
        }
        "drag" => {
            let (x, y) = need("drag")?;
            let (to_x, to_y) = args.to_x.zip(args.to_y).ok_or("`drag` needs `to_x` and `to_y`")?;
            let steps = args.steps.unwrap_or(DEFAULT_DRAG_STEPS).clamp(1, MAX_STEPS);
            let span = ms(args.duration_ms, DEFAULT_DRAG_MS);
            push(t0, Pending::MouseMove { x, y });
            push(t0 + 0.03, Pending::MouseButton { button, state: ButtonState::Pressed });
            for i in 1..=steps {
                let f = i as f32 / steps as f32;
                push(t0 + 0.06 + span * f64::from(f), Pending::MouseMove { x: x + (to_x - x) * f, y: y + (to_y - y) * f });
            }
            push(t0 + 0.09 + span, Pending::MouseButton { button, state: ButtonState::Released });
        }
        "wheel" => {
            let unit = match args.unit.as_deref().unwrap_or("line") {
                "line" => MouseScrollUnit::Line,
                "pixel" => MouseScrollUnit::Pixel,
                other => return Err(format!("unknown wheel unit `{other}` (line, pixel)")),
            };
            if let Some((x, y)) = here {
                push(t0, Pending::MouseMove { x, y });
            }
            push(t0, Pending::Wheel { x: args.dx.unwrap_or(0.0), y: args.dy.unwrap_or(0.0), unit });
        }
        other => return Err(format!("unknown mouse action `{other}` (move, down, up, click, double_click, drag, wheel)")),
    }
    Ok(json!({ "action": args.action }))
}

// ── Gamepad ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PadArgs {
    /// A button (`South`, `a`, `DPadUp`, …) …
    #[serde(default)]
    pub button: Option<String>,
    /// … or an axis (`LeftStickX`) with a `value` in -1..1.
    #[serde(default)]
    pub axis: Option<String>,
    #[serde(default)]
    pub value: Option<f32>,
    /// Button: `press` (default), `down` or `up`.
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub hold_ms: Option<u64>,
}

pub fn send_pad(world: &mut World, args: PadArgs) -> Result<Value, String> {
    let t0 = now(world);
    let hold = ms(args.hold_ms, DEFAULT_HOLD_MS);
    match (&args.button, &args.axis) {
        (Some(b), None) => {
            let name = canonical_pad_button(b).ok_or_else(|| format!("unknown gamepad button `{b}`"))?;
            let button = pad_button(name).ok_or_else(|| format!("no gamepad button for {name}"))?;
            let mut q = queue(world);
            match args.action.as_deref().unwrap_or("press") {
                "down" => q.0.push(t0, Pending::PadButton { button, value: 1.0 }),
                "up" => q.0.push(t0, Pending::PadButton { button, value: 0.0 }),
                "press" => {
                    q.0.push(t0, Pending::PadButton { button, value: 1.0 });
                    q.0.push(t0 + hold, Pending::PadButton { button, value: 0.0 });
                }
                other => return Err(format!("unknown gamepad action `{other}` (press, down, up)")),
            }
            Ok(json!({ "button": name }))
        }
        (None, Some(a)) => {
            let name = canonical_pad_axis(a).ok_or_else(|| format!("unknown gamepad axis `{a}`"))?;
            let axis = pad_axis(name).ok_or_else(|| format!("no gamepad axis for {name}"))?;
            let value = args.value.ok_or("an axis needs a `value` in -1..1")?.clamp(-1.0, 1.0);
            queue(world).0.push(t0, Pending::PadAxis { axis, value });
            Ok(json!({ "axis": name, "value": value }))
        }
        _ => Err("give either `button` or `axis`".into()),
    }
}

// ── Delivery ─────────────────────────────────────────────────────────────────

fn primary_window(world: &mut World) -> Option<Entity> {
    world.query_filtered::<Entity, With<PrimaryWindow>>().iter(world).next()
}

/// Deliver what is due. Runs in `First`, before Bevy's input systems read the messages in `PreUpdate`.
pub fn deliver_due_input(world: &mut World) {
    if world.get_resource::<InputQueue>().is_none_or(|q| q.0.is_empty()) {
        return;
    }
    let t = now(world);
    let due = world.resource_mut::<InputQueue>().0.drain_due(t);
    let Some(window) = primary_window(world) else { return };
    for pending in due {
        deliver(world, window, pending);
    }
}

fn deliver(world: &mut World, window: Entity, pending: Pending) {
    match pending {
        Pending::Key { code, state } => {
            world.write_message(KeyboardInput { key_code: code, logical_key: Key::Unidentified(NativeKey::Unidentified), state, text: None, repeat: false, window });
        }
        Pending::Character { ch, state } => {
            let s = ch.to_string();
            world.write_message(KeyboardInput {
                key_code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
                logical_key: Key::Character(s.clone().into()),
                state,
                text: (state == ButtonState::Pressed).then(|| s.into()),
                repeat: false,
                window,
            });
        }
        Pending::MouseButton { button, state } => {
            world.write_message(MouseButtonInput { button, state, window });
        }
        Pending::MouseMove { x, y } => {
            let position = Vec2::new(x, y);
            let previous = world.get::<Window>(window).and_then(|w| w.cursor_position());
            if let Some(mut w) = world.get_mut::<Window>(window) {
                w.set_cursor_position(Some(position));
            }
            world.write_message(CursorMoved { window, position, delta: previous.map(|p| position - p) });
        }
        Pending::Wheel { x, y, unit } => {
            world.write_message(MouseWheel { unit, x, y, window });
        }
        Pending::PadButton { button, value } => {
            if let Some(pad) = world.query_filtered::<Entity, With<Gamepad>>().iter(world).next() {
                world.write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, button, value)));
            }
        }
        Pending::PadAxis { axis, value } => {
            if let Some(pad) = world.query_filtered::<Entity, With<Gamepad>>().iter(world).next() {
                world.write_message(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(pad, axis, value)));
            }
        }
    }
}

/// Drop everything still waiting (a client that disconnects mid-drag must not leave a button held).
pub fn cancel_pending(world: &mut World) -> usize {
    let Some(mut q) = world.get_resource_mut::<InputQueue>() else { return 0 };
    let n = q.0.len();
    q.0.clear();
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{KEY_NAMES, PAD_AXES, PAD_BUTTONS};

    #[test]
    fn every_canonical_name_has_a_bevy_counterpart() {
        for n in KEY_NAMES {
            assert!(key_code(n).is_some(), "key {n}");
        }
        for n in PAD_BUTTONS {
            assert!(pad_button(n).is_some(), "pad button {n}");
        }
        for n in PAD_AXES {
            assert!(pad_axis(n).is_some(), "pad axis {n}");
        }
    }

    #[test]
    fn a_press_with_modifiers_is_down_down_up_up_in_order() {
        let mut world = World::new();
        world.insert_resource(Time::<Real>::default());
        send_key(&mut world, KeyArgs { key: "s".into(), action: None, modifiers: vec!["ctrl".into()], hold_ms: Some(100) }).unwrap();
        let mut q = world.resource_mut::<InputQueue>();
        let now_batch = q.0.drain_due(0.0);
        assert_eq!(now_batch.len(), 2);
        let later = q.0.drain_due(1.0);
        assert!(matches!(later[0], Pending::Key { code: KeyCode::KeyS, state: ButtonState::Released }));
        assert!(matches!(later[1], Pending::Key { code: KeyCode::ControlLeft, state: ButtonState::Released }));
    }

    #[test]
    fn bad_names_say_what_was_wrong() {
        let mut world = World::new();
        world.insert_resource(Time::<Real>::default());
        let err = send_key(&mut world, KeyArgs { key: "banana".into(), action: None, modifiers: vec![], hold_ms: None }).unwrap_err();
        assert!(err.contains("unknown key `banana`"), "{err}");
        let err = send_key(&mut world, KeyArgs { key: "a".into(), action: None, modifiers: vec!["x".into()], hold_ms: None }).unwrap_err();
        assert!(err.contains("not a modifier"), "{err}");
    }

    #[test]
    fn a_drag_presses_moves_in_steps_then_releases() {
        let mut world = World::new();
        world.insert_resource(Time::<Real>::default());
        let args = MouseArgs { action: "drag".into(), x: Some(0.0), y: Some(0.0), to_x: Some(100.0), to_y: Some(0.0), button: None, steps: Some(4), duration_ms: Some(400), dx: None, dy: None, unit: None };
        send_mouse(&mut world, args).unwrap();
        let all = world.resource_mut::<InputQueue>().0.drain_due(10.0);
        // move, down, 4 steps, up
        assert_eq!(all.len(), 7);
        assert!(matches!(all[1], Pending::MouseButton { state: ButtonState::Pressed, .. }));
        assert!(matches!(all[5], Pending::MouseMove { x, .. } if (x - 100.0).abs() < 1e-3));
        assert!(matches!(all[6], Pending::MouseButton { state: ButtonState::Released, .. }));
    }
}
