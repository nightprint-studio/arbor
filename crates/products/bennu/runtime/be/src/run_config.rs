//! `run_config` domain — `bennu_get_run_config` / `bennu_set_run_config`.
//!
//! Per-repo persistence for the IntelliJ-style run configurations (the FE's
//! run-configuration editor: `BennuRunConfigModal` + the `run-config` store). The
//! bundle lives in `<repo>/.arbor/bennu/config.toml` under a `[run]` section — a *per-repo*
//! preference (CLAUDE.md rule 11: filesystem, never localStorage), NOT the per-profile
//! product config (`…/bennu/config.toml`) the `config_cmds` domain owns.
//!
//! Handlers key off `root` directly (the FE passes the project root), so there is no
//! `tab_id → workdir` resolution step. The file itself — where it is, how a section is
//! merged into it, and how a project configured before bennu had a file of its own is still
//! read — is [`crate::repo_config`], which every per-repo section goes through.
//!
//! IDs are STABLE across restarts: the FE generates them, we persist them verbatim, and
//! never re-assign. The [`RunConfigSet`] serde round-trip is the unit-tested core; the
//! FS read/write is the thin glue around it.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::RunConfigSet;
use serde::Deserialize;

/// Args for [`bennu_get_run_config`] / [`bennu_set_run_config`]'s `root`.
#[derive(Deserialize)]
pub struct GetRunConfigArgs {
    /// Absolute path to the project root (the dir whose `.arbor/bennu/config.toml` holds it).
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

/// Read the per-repo run configurations from `<root>/.arbor/bennu/config.toml` `[run]`.
/// A fresh repo (no file / no section) yields `{ configs: [], active_id: null }` — never
/// an error, so the editor opens cleanly on a project that's never had a run config.
#[arbor_rpc::handler]
fn bennu_get_run_config(_ctx: &BennuState, args: GetRunConfigArgs) -> Result<RunConfigSet, String> {
    Ok(load_run_config(&args.root))
}

/// Persist the per-repo run configurations into `<root>/.arbor/bennu/config.toml` `[run]`,
/// preserving every other section of the file. Env vars serialize as a TOML
/// array-of-tables, args as strings — the round-trip inverse of [`load_run_config`].
#[arbor_rpc::handler]
fn bennu_set_run_config(_ctx: &BennuState, args: SetRunConfigArgs) -> Result<(), String> {
    save_run_config(&args.root, &args.config_set)
}

// ── persistence ────────────────────────────────────────────────────────────────

/// Read `[run]`. A fresh repo (no file / no section) yields the empty default rather
/// than an error — see [`crate::repo_config::load`].
fn load_run_config(root: &str) -> RunConfigSet {
    crate::repo_config::load(root, "run")
}

/// Persist `[run]`, leaving every other section of the file intact.
fn save_run_config(root: &str, set: &RunConfigSet) -> Result<(), String> {
    crate::repo_config::save(root, "run", set)
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

    /// A scratch project root, cleaned on the way in.
    fn scratch(tag: &str) -> String {
        let dir = std::env::temp_dir().join(format!("bennu-runcfg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.display().to_string()
    }

    /// A fresh repo — no file at all — decodes to the empty default, exactly what the FE binds
    /// `{ configs: [], active_id: null }` against.
    #[test]
    fn a_repo_with_no_config_is_the_empty_default() {
        let root = scratch("fresh");
        let set = load_run_config(&root);
        assert!(set.configs.is_empty());
        assert!(set.active_id.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The real save/load pair, through the file the FE's editor writes.
    #[test]
    fn the_bundle_round_trips_through_the_repo_file() {
        let root = scratch("roundtrip");
        save_run_config(&root, &sample_set()).unwrap();
        assert_eq!(load_run_config(&root), sample_set());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A project whose run configurations were written before bennu had a file of its own
    /// still opens with them. Losing somebody's run configurations to a relocation is the one
    /// outcome the fallback exists to prevent.
    #[test]
    fn configurations_written_before_the_move_are_still_found() {
        let root = scratch("legacy");
        let legacy = std::path::PathBuf::from(&root).join(".arbor");
        std::fs::create_dir_all(&legacy).unwrap();
        let mut table = toml::value::Table::new();
        let mut bennu = toml::value::Table::new();
        bennu.insert("run".into(), toml::Value::try_from(sample_set()).unwrap());
        table.insert("corvus".into(), toml::Value::String("x".into()));
        table.insert("bennu".into(), toml::Value::Table(bennu));
        std::fs::write(legacy.join("config.toml"), toml::to_string_pretty(&table).unwrap())
            .unwrap();

        assert_eq!(load_run_config(&root), sample_set());
        let _ = std::fs::remove_dir_all(&root);
    }
}
