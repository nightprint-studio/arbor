//! Where Imber's choices come from in Corvus: the shared ones, read, and the one that is Corvus's own.
//!
//! **Shared with Bennu, never copied.** The model, the effort, the spending limits and the model price
//! list are the `[assistant]` table of Bennu's config and its `assistant-models.toml`; Corvus reads those
//! files as they are, so a change made in Bennu's settings is what the next Corvus run uses. The API key
//! is not in either: it sits in the shell's vault and is asked for, per request, over the reverse channel
//! (`__assistant_key`), exactly as Bennu's backend does — it is never stored, cached or logged here.
//!
//! **Corvus's own:** the mode (manual or auto). Bennu's modes are about editing files; this one is about
//! git, so it keeps its own file, `imber.toml`, in Corvus's config folder.

use std::path::PathBuf;

use corvus_core::prelude::CorvusState;
use imber_core::prelude::{find_model, merge_models, AssistantConfig, ModelFile, ModelInfo, DEFAULT_MODELS_TOML};
use serde_json::json;

use super::permission::Mode;

/// The provider this build talks to. The vault keeps one key per provider id.
pub const PROVIDER: &str = "anthropic";

fn bennu_config_file() -> PathBuf {
    arbor_core::prelude::bennu_config_path("config.toml")
}

fn models_file() -> PathBuf {
    arbor_core::prelude::bennu_config_path("assistant-models.toml")
}

/// The `[assistant]` table out of the text of Bennu's config; the defaults when it is missing or broken —
/// a typo in a setting is no reason to lose the assistant.
pub fn parse_assistant(text: &str) -> AssistantConfig {
    toml::from_str::<toml::Table>(text)
        .ok()
        .and_then(|mut table| table.remove("assistant"))
        .and_then(|section| section.try_into::<AssistantConfig>().ok())
        .unwrap_or_default()
}

pub fn assistant() -> AssistantConfig {
    std::fs::read_to_string(bennu_config_file()).map(|t| parse_assistant(&t)).unwrap_or_default()
}

/// The price list: the one that ships, overlaid with the user's own file. A broken user file is ignored.
pub fn models_from(user: Option<&str>) -> Vec<ModelInfo> {
    let parse = |text: &str| toml::from_str::<ModelFile>(text).map(|f| f.model).ok();
    let defaults = parse(DEFAULT_MODELS_TOML).unwrap_or_default();
    match user.and_then(parse) {
        Some(mine) => merge_models(defaults, mine),
        None => defaults,
    }
}

/// The price list as the user has it: the shipped one overlaid with their file.
pub fn models() -> Vec<ModelInfo> {
    models_from(std::fs::read_to_string(models_file()).ok().as_deref())
}

pub fn model_info(id: &str) -> Option<ModelInfo> {
    let table = models();
    find_model(&table, id).cloned()
}

/// The API key, from the vault. The error is what the panel shows when there is none.
pub fn api_key(state: &CorvusState) -> Result<String, String> {
    let key = state.host_call("__assistant_key", json!(PROVIDER))?;
    match key.as_str().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Ok(k.to_string()),
        None => Err("No API key is set. Add one in Settings \u{203a} Imber (it is shared with Bennu).".into()),
    }
}

fn mode_file(state: &CorvusState) -> Result<PathBuf, String> {
    Ok(PathBuf::from(crate::corvus_config::corvus_config_dir(state)?).join("imber.toml"))
}

pub fn parse_mode(text: &str) -> Mode {
    let word = toml::from_str::<toml::Table>(text).ok().and_then(|t| t.get("mode").and_then(|m| m.as_str().map(str::to_string)));
    Mode::parse(word.as_deref().unwrap_or(""))
}

/// The saved mode; manual when nothing was ever chosen.
pub fn mode(state: &CorvusState) -> Mode {
    mode_file(state).ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse_mode(&t)).unwrap_or(Mode::Manual)
}

pub fn set_mode(state: &CorvusState, mode: Mode) -> Result<(), String> {
    let path = mode_file(state)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, format!("mode = \"{}\"\n", mode.word())).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assistant_table_is_read_out_of_the_bennu_config() {
        let text = "font_size = 14\n[assistant]\nmodel = \"claude-sonnet-5-5\"\neffort = \"low\"\n[assistant.limits]\nsession_eur = 2.5\n";
        let cfg = parse_assistant(text);
        assert_eq!(cfg.model, "claude-sonnet-5-5");
        assert_eq!(cfg.effort, "low");
        assert_eq!(cfg.limits.session_eur, 2.5);
        assert_eq!(cfg.limits.day_eur, AssistantConfig::default().limits.day_eur, "unset keys keep their defaults");
    }

    #[test]
    fn a_missing_or_broken_config_gives_the_defaults() {
        assert_eq!(parse_assistant(""), AssistantConfig::default());
        assert_eq!(parse_assistant("[assistant]\nmodel = 12"), AssistantConfig::default());
        assert_eq!(parse_assistant("not toml ["), AssistantConfig::default());
    }

    #[test]
    fn the_shipped_price_list_has_the_default_model() {
        let table = models_from(None);
        assert!(find_model(&table, &AssistantConfig::default().model).is_some());
    }

    #[test]
    fn a_user_price_list_overrides_and_a_broken_one_is_ignored() {
        let mine = "[[model]]\nid = \"claude-opus-5-5\"\nname = \"Mine\"\ncontext_max = 1000000\n[model.price]\ninput = 1.0\noutput = 2.0\n";
        let table = models_from(Some(mine));
        assert_eq!(find_model(&table, "claude-opus-5-5").map(|m| m.name.as_str()), Some("Mine"));
        assert!(find_model(&models_from(Some("[[model]]\nid = 12")), "claude-opus-5-5").is_some());
    }

    #[test]
    fn the_mode_file_reads_back_and_defaults_to_manual() {
        assert_eq!(parse_mode("mode = \"auto\"\n"), Mode::Auto);
        assert_eq!(parse_mode("mode = \"manual\"\n"), Mode::Manual);
        assert_eq!(parse_mode(""), Mode::Manual);
        assert_eq!(parse_mode("mode = 3"), Mode::Manual);
    }
}
