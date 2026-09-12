//! `dtolab_values` — the test values the DTO Lab fills fields with: the built-ins, and the user's own.
//!
//! What a rule is and how one is matched are `bennu-dtolab`'s ([`bennu_dtolab::values`]). This keeps the
//! user's rules — `bennu/dtolab/values.toml` in the profile, beside the templates, so every project sees
//! them — and serves the settings page that edits them.

use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use bennu_dtolab::prelude::{builtin_rules, check_rules, effective_rules, ValueRule};
use serde::{Deserialize, Serialize};

/// The file as written: which built-ins are switched off, then the user's rules in order.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    disabled: Vec<String>,
    #[serde(default, rename = "rule")]
    rules: Vec<ValueRule>,
}

fn path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("dtolab").join("values.toml")
}

/// A file that does not parse is logged and read as empty: the built-ins still apply, and a generation
/// is not the place to fail over a settings file.
fn read() -> Stored {
    let path = path();
    let Ok(text) = std::fs::read_to_string(&path) else { return Stored::default() };
    toml::from_str(&text).unwrap_or_else(|e| {
        eprintln!("[dtolab] {}: {e}", path.display());
        Stored::default()
    })
}

/// The rules a generation and a payload sketch use: the user's first, then the built-ins kept.
pub(crate) fn rules() -> Vec<ValueRule> {
    let stored = read();
    effective_rules(&stored.rules, &stored.disabled)
}

#[derive(Deserialize)]
pub struct ValueRulesArgs {}

#[derive(Serialize)]
pub struct ValueRulesView {
    pub rules: Vec<ValueRule>,
    pub disabled: Vec<String>,
    pub builtins: Vec<ValueRule>,
    /// Where the user's rules are kept.
    pub path: String,
}

/// The user's test values, the built-ins switched off, and the built-ins themselves.
#[arbor_rpc::handler]
fn bennu_dtolab_value_rules(_ctx: &BennuState, _args: ValueRulesArgs) -> Result<ValueRulesView, String> {
    let stored = read();
    Ok(ValueRulesView {
        rules: stored.rules,
        disabled: stored.disabled,
        builtins: builtin_rules(),
        path: path().display().to_string(),
    })
}

#[derive(Deserialize)]
pub struct SaveValueRulesArgs {
    pub rules: Vec<ValueRule>,
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// Replace the user's test values. Refused whole when one rule is malformed, so the file never holds a
/// rule the settings page could not show.
#[arbor_rpc::handler]
fn bennu_dtolab_save_value_rules(_ctx: &BennuState, args: SaveValueRulesArgs) -> Result<(), String> {
    check_rules(&args.rules)?;
    let path = path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = toml::to_string_pretty(&Stored { disabled: args.disabled, rules: args.rules })
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
