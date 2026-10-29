//! Loading the model table: the one that ships, overlaid with the user's own.
//!
//! The format and the arithmetic are in `bennu-assistant`; this is the part that touches the file system
//! and the TOML parser. A broken user file never takes the assistant down — it is reported, and the
//! built-in table is used — because a typo in a price list is no reason to lose the chat.

use std::path::PathBuf;

use bennu_assistant::prelude::{merge_models, ModelFile, ModelInfo, DEFAULT_MODELS_TOML};
use serde_json::{json, Value};

pub(super) fn user_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("assistant-models.toml")
}

pub(super) fn parse(text: &str) -> Result<Vec<ModelInfo>, String> {
    toml::from_str::<ModelFile>(text).map(|f| f.model).map_err(|e| e.to_string())
}

/// The built-in table overlaid with `user` (the text of the user's file), and the problem with that
/// file if it had one.
pub(super) fn from_texts(user: Option<&str>) -> (Vec<ModelInfo>, Option<String>) {
    let defaults = parse(DEFAULT_MODELS_TOML).unwrap_or_default();
    let Some(text) = user else { return (defaults, None) };
    match parse(text) {
        Ok(mine) => (merge_models(defaults, mine), None),
        Err(e) => (defaults, Some(format!("{}: {e}", user_path().display()))),
    }
}

pub(super) fn load() -> (Vec<ModelInfo>, Option<String>) {
    let text = std::fs::read_to_string(user_path()).ok();
    from_texts(text.as_deref())
}

/// A model as the panel reads it: camelCase, with the base prices the picker shows.
pub(super) fn wire(m: &ModelInfo) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "tagline": m.tagline,
        "contextMax": m.context_max,
        "efforts": m.efforts.iter().map(|e| e.wire()).collect::<Vec<_>>(),
        "inputPrice": m.price.input,
        "outputPrice": m.price.output,
    })
}

/// Write the built-in table to the user's file, if there is none yet, as something to start from.
/// Never overwrites: the file is theirs.
pub(super) fn write_template() -> Result<PathBuf, String> {
    let path = user_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, DEFAULT_MODELS_TOML).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_table_parses_and_has_the_current_models() {
        let (models, err) = from_texts(None);
        assert!(err.is_none());
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        for id in ["claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5", "claude-haiku-4-5"] {
            assert!(ids.contains(&id), "{id} is missing from models.toml");
        }
    }

    #[test]
    fn haiku_5_5_is_dearer_over_a_hundred_thousand_tokens() {
        let (models, _) = from_texts(None);
        let h = models.iter().find(|m| m.id == "claude-haiku-5-5").unwrap();
        let long = h.long_prompt.as_ref().expect("a long-prompt tier");
        assert_eq!(long.over_tokens, Some(100_000));
        assert!(long.input > h.price.input && long.output > h.price.output);
    }

    #[test]
    fn every_built_in_entry_names_a_price_and_a_window() {
        let (models, _) = from_texts(None);
        for m in &models {
            assert!(m.price.input > 0.0 && m.price.output > 0.0, "{}", m.id);
            assert!(m.context_max >= 100_000, "{}", m.id);
        }
    }

    #[test]
    fn a_user_file_overrides_a_price_and_adds_a_model() {
        let mine = r#"
            [[model]]
            id = "claude-opus-5-5"
            name = "Opus 5.5 (negotiated)"
            context_max = 1000000
            [model.price]
            input = 3.0
            output = 15.0

            [[model]]
            id = "claude-future-6"
            name = "Future 6"
            context_max = 500000
            [model.price]
            input = 1.0
            output = 2.0
        "#;
        let (models, err) = from_texts(Some(mine));
        assert!(err.is_none(), "{err:?}");
        let opus = models.iter().find(|m| m.id == "claude-opus-5-5").unwrap();
        assert_eq!(opus.price.input, 3.0);
        assert_eq!(opus.name, "Opus 5.5 (negotiated)");
        assert!(models.iter().any(|m| m.id == "claude-future-6"));
        assert!(models.iter().any(|m| m.id == "claude-haiku-5-5"), "the others are untouched");
    }

    #[test]
    fn a_broken_user_file_falls_back_to_the_built_in_table_and_says_why() {
        let (models, err) = from_texts(Some("[[model]]\nid = 12"));
        assert!(err.is_some());
        assert!(models.len() >= 5);
    }

    #[test]
    fn the_panel_gets_camel_case_and_the_efforts_as_words() {
        let (models, _) = from_texts(None);
        let w = wire(models.iter().find(|m| m.id == "claude-opus-5-5").unwrap());
        assert_eq!(w["contextMax"], 1_000_000);
        assert_eq!(w["inputPrice"], 4.0);
        assert_eq!(w["efforts"][3], "xhigh");
    }
}
