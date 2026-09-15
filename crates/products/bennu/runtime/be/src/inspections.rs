//! `inspections` domain — which checks a project reports, and how loudly.
//!
//! The policy itself is [`bennu_check::prelude::Inspections`], a pure type that knows nothing about
//! projects. This is the glue: where the config lives (`<repo>/.arbor/bennu/config.toml`
//! `[inspections]`, like every other per-repo section), which project owns a file, and the cache
//! that keeps a project-wide validation from re-reading the same TOML once per file.
//!
//! ## The cache is invalidated by the writer, not by a clock
//!
//! Same reasoning as the naming pack's, and for the same reason: a TTL would produce exactly the
//! bug worth avoiding — "I turned that check off and it is still there."
//!
//! ## Where it is applied
//!
//! At the diagnostics funnel, once, over whatever the checks produced — see
//! [`policy_for_file`]. Applying it inside the checks would mean seventy places that each have to
//! remember to ask; applying it at the funnel means a check cannot forget, and a check added
//! tomorrow is configurable the day it lands.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bennu_check::prelude::{CheckId, Inspections};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// The TOML section this domain owns.
const SECTION: &str = "inspections";

/// The `[inspections]` section: a level per check code.
///
/// A map rather than a list of structs so the file reads as what it is —
/// `unused-import = "off"` — and so a code the current build has never heard of survives a
/// round-trip instead of being dropped by a stricter shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InspectionConfig {
    /// `code` → `"error"` | `"warning"` | `"weak"` | `"off"`. Absent → the check's own default.
    #[serde(default)]
    pub severity: HashMap<String, String>,
}

/// One check kind, as a settings screen needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInfo {
    /// The stable slug — the key the config is written under, and the one a `@SuppressWarnings`
    /// names.
    pub code: String,
    /// A readable name, derived from the code rather than written out seventy-one times.
    ///
    /// Derived on purpose: a hand-written title per kind is seventy-one strings that drift from the
    /// codes beside them, and the drift is invisible because nothing compares the two. `code` is
    /// already the name — this is that name with its hyphens taken out.
    pub title: String,
    /// What the check reports at when the project says nothing.
    pub default_severity: String,
    /// The level configured for this project, when it differs from the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured: Option<String>,
}

/// Args for [`bennu_get_inspection_config`] and [`bennu_inspection_catalog`].
#[derive(Deserialize)]
pub struct RootArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// Args for [`bennu_set_inspection_config`].
#[derive(Deserialize)]
pub struct SetArgs {
    pub root: String,
    /// The whole `[inspections]` section to persist.
    pub config: InspectionConfig,
}

/// Read `[inspections]`. A project that never configured it yields the default — every check at its
/// own severity.
#[arbor_rpc::handler]
fn bennu_get_inspection_config(
    _ctx: &BennuState,
    args: RootArgs,
) -> Result<InspectionConfig, String> {
    Ok(config_for_root(&args.root))
}

/// Persist `[inspections]`, leaving every other section of the file intact, and drop the cached copy
/// so the next validation uses it.
#[arbor_rpc::handler]
fn bennu_set_inspection_config(_ctx: &BennuState, args: SetArgs) -> Result<(), String> {
    crate::repo_config::save(&args.root, SECTION, &args.config)?;
    invalidate(&args.root);
    Ok(())
}

/// Every check kind, with what it reports at here — what a settings screen renders.
#[arbor_rpc::handler]
fn bennu_inspection_catalog(_ctx: &BennuState, args: RootArgs) -> Result<Vec<CheckInfo>, String> {
    let cfg = config_for_root(&args.root);
    let mut out: Vec<CheckInfo> = CheckId::ALL
        .iter()
        .map(|id| {
            let code = id.code().to_string();
            CheckInfo {
                title: title_of(&code),
                default_severity: id.severity().to_string(),
                configured: cfg.severity.get(&code).cloned(),
                code,
            }
        })
        .collect();
    out.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(out)
}

/// `non-exhaustive-enum-switch` → `Non exhaustive enum switch`.
fn title_of(code: &str) -> String {
    let spaced = code.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

/// The policy in force for `file`, ready to apply to its diagnostics.
///
/// `Inspections::default()` — a policy that changes nothing — when no project owns the file or the
/// project configured nothing, which is the common case and costs one cache read.
pub(crate) fn policy_for_file(file: &str) -> Inspections {
    let Some(root) = crate::index_service::IndexService::global().root_for_file(file) else {
        return Inspections::default();
    };
    let cfg = config_for_root(&root);
    Inspections::from_pairs(cfg.severity.iter().map(|(k, v)| (k.as_str(), v.as_str())))
}

fn cache() -> &'static Mutex<HashMap<String, InspectionConfig>> {
    static CACHE: OnceLock<Mutex<HashMap<String, InspectionConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn config_for_root(root: &str) -> InspectionConfig {
    let key = root.replace('\\', "/");
    if let Ok(guard) = cache().lock() {
        if let Some(hit) = guard.get(&key) {
            return hit.clone();
        }
    }
    let loaded: InspectionConfig = crate::repo_config::load(root, SECTION);
    if let Ok(mut guard) = cache().lock() {
        guard.insert(key, loaded.clone());
    }
    loaded
}

/// Drop the cached copy for `root`.
fn invalidate(root: &str) {
    if let Ok(mut guard) = cache().lock() {
        guard.remove(&root.replace('\\', "/"));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_code_made_readable() {
        assert_eq!(title_of("non-exhaustive-enum-switch"), "Non exhaustive enum switch");
        assert_eq!(title_of("unused-import"), "Unused import");
    }

    #[test]
    fn an_empty_code_does_not_panic() {
        assert_eq!(title_of(""), "");
    }
}
