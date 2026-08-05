//! `run_config` domain — `bennu_get_run_config` / `bennu_set_run_config`.
//!
//! Per-repo persistence for the IntelliJ-style run configurations (the FE's
//! run-configuration editor: `BennuRunConfigModal` + the `run-config` store). The
//! bundle lives in `<repo>/.arbor/config.toml` under a `[bennu.run]` section — a
//! *per-repo* preference (CLAUDE.md rule 11: filesystem, never localStorage), NOT the
//! per-profile product config (`…/bennu/config.toml`) the `config_cmds` domain owns.
//!
//! We follow the same `.arbor/config.toml` precedent corvus uses (`repo_config`), but
//! bennu handlers key off `root` directly (the FE passes the project root), so there's
//! no `tab_id → workdir` resolution step.
//!
//! **Coexistence**: `.arbor/config.toml` is a shared file — a repo opened in corvus has
//! corvus's own top-level keys there. So we never rewrite the whole file from a typed
//! struct; we parse it into a dynamic `toml::Table`, replace *only* the `bennu.run`
//! sub-tree, and write it back — every unrelated section survives byte-for-byte (the
//! same merge discipline corvus's `tickets` domain uses for `[ticket_links]`).
//!
//! IDs are STABLE across restarts: the FE generates them, we persist them verbatim, and
//! never re-assign. The [`RunConfigSet`] serde round-trip is the unit-tested core; the
//! FS read/write is the thin glue around it.

use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::RunConfigSet;
use serde::Deserialize;

/// Args for [`bennu_get_run_config`] / [`bennu_set_run_config`]'s `root`.
#[derive(Deserialize)]
pub struct GetRunConfigArgs {
    /// Absolute path to the project root (the dir whose `.arbor/config.toml` holds it).
    pub root: String,
}

/// Args for [`bennu_set_run_config`].
#[derive(Deserialize)]
pub struct SetRunConfigArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The full run-config bundle to persist (ordered configs + active id).
    pub config_set: RunConfigSet,
}

/// Read the per-repo run configurations from `<root>/.arbor/config.toml` `[bennu.run]`.
/// A fresh repo (no file / no section) yields `{ configs: [], active_id: null }` — never
/// an error, so the editor opens cleanly on a project that's never had a run config.
#[arbor_rpc::handler]
fn bennu_get_run_config(_ctx: &BennuState, args: GetRunConfigArgs) -> Result<RunConfigSet, String> {
    Ok(load_run_config(&args.root))
}

/// Persist the per-repo run configurations into `<root>/.arbor/config.toml` `[bennu.run]`,
/// preserving every other section of the file. Env vars serialize as a TOML
/// array-of-tables, args as strings — the round-trip inverse of [`load_run_config`].
#[arbor_rpc::handler]
fn bennu_set_run_config(_ctx: &BennuState, args: SetRunConfigArgs) -> Result<(), String> {
    save_run_config(&args.root, &args.config_set)
}

// ── persistence (the pure-ish core: TOML-table merge over the shared file) ──────

/// `<repo>/.arbor/config.toml`.
fn config_path(root: &str) -> PathBuf {
    PathBuf::from(root).join(".arbor").join("config.toml")
}

/// Read the whole `.arbor/config.toml` as a dynamic table (empty when absent/corrupt),
/// then decode `bennu.run` into a [`RunConfigSet`]. A missing section → the default
/// (empty) set. Corruption self-heals to the default, matching the config-read
/// philosophy elsewhere (an editor pref never hard-fails a read).
fn load_run_config(root: &str) -> RunConfigSet {
    let table = read_table(root);
    match table.get("bennu").and_then(|b| b.get("run")) {
        Some(run) => run.clone().try_into().unwrap_or_default(),
        None => RunConfigSet::default(),
    }
}

/// Merge `set` into `bennu.run` of the on-disk table (creating `.arbor/` as needed) and
/// write the whole file back, so unrelated sections (e.g. corvus's own keys) survive.
fn save_run_config(root: &str, set: &RunConfigSet) -> Result<(), String> {
    let mut table = read_table(root);
    let run_value = toml::Value::try_from(set).map_err(|e| e.to_string())?;

    // Ensure `[bennu]` is a table, then set its `run` sub-tree.
    let bennu = table
        .entry("bennu".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
    let bennu_tbl = bennu
        .as_table_mut()
        .ok_or_else(|| "`.arbor/config.toml` `[bennu]` is not a table".to_string())?;
    bennu_tbl.insert("run".to_string(), run_value);

    let path = config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Parse `<root>/.arbor/config.toml` into a dynamic TOML table; a missing or unparseable
/// file yields an empty table (a corrupt sibling section shouldn't strand run configs).
fn read_table(root: &str) -> toml::value::Table {
    let path = config_path(root);
    let Ok(text) = std::fs::read_to_string(path) else {
        return toml::value::Table::new();
    };
    text.parse::<toml::Value>()
        .ok()
        .and_then(|v| v.as_table().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::{EnvVar, RunConfig};

    fn sample_set() -> RunConfigSet {
        RunConfigSet {
            configs: vec![
                RunConfig {
                    id: "rc-abc".to_string(),
                    name: "App".to_string(),
                    kind: "application".to_string(),
                    module: "services/core".to_string(),
                    main_class: "com.acme.App".to_string(),
                    program_args: "--verbose input.txt".to_string(),
                    vm_args: "-Xmx512m -Dfoo=bar".to_string(),
                    working_dir: String::new(),
                    env: vec![
                        EnvVar { key: "PROFILE".to_string(), value: "dev".to_string() },
                        EnvVar { key: "PORT".to_string(), value: "8080".to_string() },
                    ],
                    ..RunConfig::default()
                },
                RunConfig {
                    id: "rc-def".to_string(),
                    name: "All tests".to_string(),
                    kind: "junit".to_string(),
                    test_scope: "module".to_string(),
                    test_target: "sub/module".to_string(),
                    ..RunConfig::default()
                },
            ],
            active_id: Some("rc-abc".to_string()),
        }
    }

    /// The set survives a full TOML round-trip byte-shape-for-byte (ids, args, env,
    /// active pointer) through the same `Value::try_from` / `try_into` the handlers use.
    #[test]
    fn run_config_toml_round_trip() {
        let set = sample_set();
        let value = toml::Value::try_from(&set).unwrap();
        let text = toml::to_string_pretty(&value).unwrap();
        let back: RunConfigSet = toml::from_str(&text).unwrap();
        assert_eq!(back, set);
        // IDs are preserved verbatim (stability guarantee).
        assert_eq!(back.configs[0].id, "rc-abc");
        assert_eq!(back.active_id.as_deref(), Some("rc-abc"));
        // Env round-trips as key/value pairs.
        assert_eq!(back.configs[0].env[0].key, "PROFILE");
        assert_eq!(back.configs[0].env[0].value, "dev");
        // The kind survives, which is what the editor and the selector group by — and the
        // module, which is what the run classpath is built from.
        assert_eq!(back.configs[0].module, "services/core");
        assert_eq!(back.configs[1].kind, "junit");
        assert_eq!(back.configs[1].test_target, "sub/module");
    }

    /// A configuration written before kinds existed has no `kind` key. It must read back as
    /// an APPLICATION rather than failing — one unknown field cannot be allowed to cost a
    /// project every run configuration it has.
    #[test]
    fn a_config_without_a_kind_is_an_application() {
        // Root keys before the array of tables — a key after `[[configs]]` would belong to
        // that table, not to the set.
        let text = "\
active_id = \"rc-old\"

[[configs]]
id = \"rc-old\"
name = \"App\"
main_class = \"com.acme.App\"
program_args = \"\"
vm_args = \"\"
working_dir = \"\"
env = []
";
        let set: RunConfigSet = toml::from_str(text).expect("an older file must still parse");
        assert_eq!(set.configs.len(), 1);
        assert_eq!(set.configs[0].kind, "application");
        assert_eq!(set.configs[0].main_class, "com.acme.App");
        // And the new fields land empty rather than absent.
        assert_eq!(set.configs[0].test_scope, "");
    }

    /// A fresh repo — no `bennu.run` section at all — decodes to the empty default,
    /// exactly what the FE binds `{ configs: [], active_id: null }` against.
    #[test]
    fn missing_section_is_empty_default() {
        let table: toml::value::Table = "[corvus]\ndisplay_name = \"x\"\n"
            .parse::<toml::Value>()
            .unwrap()
            .as_table()
            .unwrap()
            .clone();
        let set = match table.get("bennu").and_then(|b| b.get("run")) {
            Some(run) => run.clone().try_into().unwrap_or_default(),
            None => RunConfigSet::default(),
        };
        assert!(set.configs.is_empty());
        assert!(set.active_id.is_none());
    }

    /// Merging `bennu.run` into a table that already has an unrelated section leaves that
    /// section intact (coexistence with corvus's own `.arbor/config.toml` keys).
    #[test]
    fn merge_preserves_unrelated_sections() {
        let mut table: toml::value::Table = "[corvus]\ndisplay_name = \"keepme\"\n"
            .parse::<toml::Value>()
            .unwrap()
            .as_table()
            .unwrap()
            .clone();

        let run_value = toml::Value::try_from(sample_set()).unwrap();
        let bennu = table
            .entry("bennu".to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
        bennu.as_table_mut().unwrap().insert("run".to_string(), run_value);

        let text = toml::to_string_pretty(&table).unwrap();
        // The corvus section survives the rewrite…
        assert!(text.contains("keepme"));
        // …and the bennu.run round-trips back.
        let reparsed: toml::Value = text.parse().unwrap();
        let set: RunConfigSet =
            reparsed.get("bennu").unwrap().get("run").unwrap().clone().try_into().unwrap();
        assert_eq!(set.active_id.as_deref(), Some("rc-abc"));
        assert_eq!(
            reparsed.get("corvus").unwrap().get("display_name").unwrap().as_str(),
            Some("keepme")
        );
    }
}
