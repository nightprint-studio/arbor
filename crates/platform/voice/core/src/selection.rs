//! Which model is "in use": one pure rule set, so the backend, the settings page and the error
//! banner can never disagree.
//!
//! The persisted `VoiceConfig` is the single source of truth (`model` = a catalogue id,
//! `custom_model_path` = the user's own file, which wins). What is *effective* also depends on what
//! is on disk: a selected model whose file is gone is not in use.

use serde::Serialize;

/// What dictation will actually run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Selection {
    /// Nothing usable is selected.
    None,
    /// A catalogue model that is installed.
    Model(String),
    /// The user's own ggml file.
    Custom,
}

impl Selection {
    /// The catalogue id, `""` for none / custom.
    pub fn model_id(&self) -> &str {
        match self {
            Selection::Model(id) => id,
            _ => "",
        }
    }

    pub fn is_usable(&self) -> bool {
        !matches!(self, Selection::None)
    }
}

/// The selection that is really in force given what is installed.
pub fn effective(model: &str, custom_path: &str, installed: &[&str]) -> Selection {
    if !custom_path.trim().is_empty() {
        return Selection::Custom;
    }
    if !model.is_empty() && installed.contains(&model) {
        Selection::Model(model.to_string())
    } else {
        Selection::None
    }
}

/// A model finished installing (`installed` already contains it): the id to select now, if the
/// user has nothing usable selected yet. Never overrides a working choice.
pub fn after_install(model: &str, custom_path: &str, installed: &[&str], just_installed: &str) -> Option<String> {
    if effective(model, custom_path, installed).is_usable() {
        return None;
    }
    installed.contains(&just_installed).then(|| just_installed.to_string())
}

/// A model was removed: the new `model` value when the removed one was the selection (the best
/// remaining installed model, `""` when none is left), `None` when the selection is unaffected.
/// `ranked_installed` is best-first and may still list the removed id.
pub fn after_remove(model: &str, removed: &str, ranked_installed: &[&str]) -> Option<String> {
    if model != removed {
        return None;
    }
    Some(ranked_installed.iter().find(|id| **id != removed).map(|id| id.to_string()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_model_must_be_installed() {
        assert_eq!(effective("small", "", &["small"]), Selection::Model("small".into()));
        assert_eq!(effective("small", "", &["base"]), Selection::None);
        assert_eq!(effective("", "", &["base"]), Selection::None);
        assert_eq!(effective("", "C:/m.bin", &[]), Selection::Custom);
        assert_eq!(effective("small", "  ", &["small"]).model_id(), "small");
    }

    #[test]
    fn install_selects_only_when_nothing_is_in_use() {
        assert_eq!(after_install("", "", &["small"], "small"), Some("small".into()));
        // The previous choice vanished from disk: the new install takes over.
        assert_eq!(after_install("base", "", &["small"], "small"), Some("small".into()));
        // A working choice or a custom file is never overridden.
        assert_eq!(after_install("base", "", &["base", "small"], "small"), None);
        assert_eq!(after_install("", "C:/m.bin", &["small"], "small"), None);
        // Not actually installed: nothing to select.
        assert_eq!(after_install("", "", &[], "small"), None);
    }

    #[test]
    fn remove_falls_back_to_the_best_remaining() {
        assert_eq!(after_remove("small", "small", &["small", "base"]), Some("base".into()));
        assert_eq!(after_remove("small", "small", &["small"]), Some(String::new()));
        assert_eq!(after_remove("small", "base", &["small", "base"]), None);
    }
}
