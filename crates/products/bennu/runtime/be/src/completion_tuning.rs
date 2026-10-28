//! Pushes the user's smart-completion settings (Settings → Completion → Smart suggestions) into the
//! ranking.
//!
//! The ranking reads one process-wide [`Tuning`] rather than the config file: completion is the hot
//! path, and parsing TOML per keystroke to answer a handful of booleans would pay for the switches
//! on every character typed. So the value is refreshed where the settings can change — when the
//! config is saved, and when a project opens (a file edited by hand outside the app is picked up
//! too) — the same two moments the import-census flag is refreshed at.

use bennu_core::config::BennuConfig;
use bennu_query::prelude::{set_completion_tuning, Tuning};

/// The ranking's view of the persisted settings.
pub(crate) fn tuning_from(config: &BennuConfig) -> Tuning {
    Tuning {
        name_affinity: config.completion_smart_name_affinity,
        project_usage: config.completion_smart_project_usage,
        sequence: config.completion_smart_sequence,
        chain: config.completion_smart_chain,
        context_filters: config.completion_smart_context,
        statements: config.completion_smart_statements,
        ghost_text: config.completion_smart_ghost_text,
        strength: config.completion_smart_strength,
    }
}

/// Re-read the settings from disk and hand them to the ranking.
pub(crate) fn refresh_completion_tuning_setting() {
    set_completion_tuning(tuning_from(&bennu_core::config::load()));
}

/// Hand the ranking the settings the caller already holds — the save handler has them in memory,
/// and reading the file back would be a second parse to learn what it just wrote.
pub(crate) fn apply_completion_tuning(config: &BennuConfig) {
    set_completion_tuning(tuning_from(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_turns_every_signal_on_at_full_strength() {
        assert_eq!(tuning_from(&BennuConfig::default()), Tuning::DEFAULT);
    }

    #[test]
    fn each_switch_maps_to_its_own_signal() {
        let config = BennuConfig {
            completion_smart_name_affinity: false,
            completion_smart_chain: false,
            completion_smart_ghost_text: false,
            completion_smart_strength: 150,
            ..BennuConfig::default()
        };
        let t = tuning_from(&config);
        assert!(!t.name_affinity && !t.chain && !t.ghost_text);
        assert!(t.project_usage && t.sequence && t.context_filters && t.statements);
        assert_eq!(t.strength, 150);
    }

    /// A config written before the settings existed has none of the keys; they must read as "on",
    /// not as a bare-`bool` `false` and a bare-`u32` `0` that would switch the feature off.
    #[test]
    fn a_config_without_the_keys_reads_as_everything_on() {
        let old: BennuConfig = toml::from_str("indent_width = 4\n").expect("an old config parses");
        assert_eq!(tuning_from(&old), Tuning::DEFAULT);
    }
}
