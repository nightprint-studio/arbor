//! Key names a remote client may use, reduced to one canonical spelling.
//!
//! A person (or a model) writes `ctrl`, `Esc`, `a`, `F5`, `ArrowUp`; Bevy's `KeyCode` variants are
//! `ControlLeft`, `Escape`, `KeyA`, `F5`, `ArrowUp`. [`canonical_key`] maps the first to the second
//! without knowing Bevy; `input.rs` maps a canonical name to the real `KeyCode`, and a test there checks
//! that every name in [`KEY_NAMES`] has one.

/// Every canonical key name this crate can press.
pub const KEY_NAMES: &[&str] = &[
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM",
    "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
    "Enter", "Escape", "Space", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown",
    "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "SuperLeft", "SuperRight",
    "Minus", "Equal", "Comma", "Period", "Slash", "Backslash", "Semicolon", "Quote", "Backquote",
    "BracketLeft", "BracketRight", "CapsLock",
];

/// The canonical name for what was typed, or `None` when it is not a key this crate knows.
pub fn canonical_key(typed: &str) -> Option<&'static str> {
    let t = typed.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "ctrl" | "control" => Some("ControlLeft"),
        "shift" => Some("ShiftLeft"),
        "alt" | "option" => Some("AltLeft"),
        "super" | "meta" | "cmd" | "command" | "win" | "windows" => Some("SuperLeft"),
        "return" => Some("Enter"),
        "esc" => Some("Escape"),
        "del" => Some("Delete"),
        "ins" => Some("Insert"),
        "pgup" => Some("PageUp"),
        "pgdn" | "pgdown" => Some("PageDown"),
        "up" => Some("ArrowUp"),
        "down" => Some("ArrowDown"),
        "left" => Some("ArrowLeft"),
        "right" => Some("ArrowRight"),
        "-" => Some("Minus"),
        "=" => Some("Equal"),
        "," => Some("Comma"),
        "." => Some("Period"),
        "/" => Some("Slash"),
        "\\" => Some("Backslash"),
        ";" => Some("Semicolon"),
        "'" => Some("Quote"),
        "`" => Some("Backquote"),
        "[" => Some("BracketLeft"),
        "]" => Some("BracketRight"),
        _ => None,
    };
    if alias.is_some() {
        return alias;
    }
    let find = |wanted: &str| KEY_NAMES.iter().copied().find(|k| k.eq_ignore_ascii_case(wanted));
    if let Some(found) = find(t) {
        return Some(found);
    }
    // A single character: a letter is `KeyX`, a digit is `DigitN`.
    let mut chars = t.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => find(&format!("Key{c}")),
        (Some(c), None) if c.is_ascii_digit() => find(&format!("Digit{c}")),
        _ => None,
    }
}

/// Every canonical gamepad button (Bevy's `GamepadButton` variant names).
pub const PAD_BUTTONS: &[&str] = &[
    "South", "East", "North", "West", "LeftTrigger", "LeftTrigger2", "RightTrigger", "RightTrigger2", "Select", "Start",
    "Mode", "LeftThumb", "RightThumb", "DPadUp", "DPadDown", "DPadLeft", "DPadRight",
];

/// Every canonical gamepad axis (Bevy's `GamepadAxis` variant names).
pub const PAD_AXES: &[&str] = &["LeftStickX", "LeftStickY", "RightStickX", "RightStickY", "LeftZ", "RightZ"];

/// The canonical gamepad button for what was typed (`a` is the south face button, as on an Xbox pad).
pub fn canonical_pad_button(typed: &str) -> Option<&'static str> {
    let lower = typed.trim().to_ascii_lowercase();
    let alias = match lower.as_str() {
        "a" | "cross" => Some("South"),
        "b" | "circle" => Some("East"),
        "y" | "triangle" => Some("North"),
        "x" | "square" => Some("West"),
        "lb" | "l1" => Some("LeftTrigger"),
        "rb" | "r1" => Some("RightTrigger"),
        "lt" | "l2" => Some("LeftTrigger2"),
        "rt" | "r2" => Some("RightTrigger2"),
        "back" => Some("Select"),
        "guide" | "home" => Some("Mode"),
        "up" => Some("DPadUp"),
        "down" => Some("DPadDown"),
        "left" => Some("DPadLeft"),
        "right" => Some("DPadRight"),
        _ => None,
    };
    alias.or_else(|| PAD_BUTTONS.iter().copied().find(|b| b.eq_ignore_ascii_case(typed.trim())))
}

pub fn canonical_pad_axis(typed: &str) -> Option<&'static str> {
    PAD_AXES.iter().copied().find(|a| a.eq_ignore_ascii_case(typed.trim()))
}

/// Whether the canonical name is a modifier (held while another key is pressed).
pub fn is_modifier(canonical: &str) -> bool {
    matches!(canonical, "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight" | "SuperLeft" | "SuperRight")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_spellings_reach_the_canonical_name() {
        for (typed, canonical) in [
            ("a", "KeyA"), ("Z", "KeyZ"), ("7", "Digit7"), ("ctrl", "ControlLeft"), ("Esc", "Escape"),
            ("return", "Enter"), ("f5", "F5"), ("ArrowUp", "ArrowUp"), ("up", "ArrowUp"), ("keyb", "KeyB"),
            ("space", "Space"), ("/", "Slash"), ("  shift ", "ShiftLeft"),
        ] {
            assert_eq!(canonical_key(typed), Some(canonical), "{typed}");
        }
    }

    #[test]
    fn unknown_or_empty_is_none() {
        for typed in ["", "  ", "F13", "banana", "ñ"] {
            assert_eq!(canonical_key(typed), None, "{typed}");
        }
    }

    #[test]
    fn every_alias_target_is_a_listed_key() {
        for typed in ["ctrl", "shift", "alt", "super", "return", "esc", "del", "ins", "pgup", "pgdn", "up", "down", "left", "right", "-", "=", ",", ".", "/", "\\", ";", "'", "`", "[", "]"] {
            let canonical = canonical_key(typed).unwrap();
            assert!(KEY_NAMES.contains(&canonical), "{typed} → {canonical}");
        }
    }

    #[test]
    fn gamepad_names_accept_face_button_letters_and_canonical_spellings() {
        assert_eq!(canonical_pad_button("A"), Some("South"));
        assert_eq!(canonical_pad_button("dpadup"), Some("DPadUp"));
        assert_eq!(canonical_pad_button("RT"), Some("RightTrigger2"));
        assert_eq!(canonical_pad_button("nope"), None);
        assert_eq!(canonical_pad_axis("leftstickx"), Some("LeftStickX"));
        assert_eq!(canonical_pad_axis("x"), None);
    }

    #[test]
    fn modifiers_are_recognised() {
        assert!(is_modifier("ControlLeft") && is_modifier("SuperRight"));
        assert!(!is_modifier("KeyA"));
    }
}
