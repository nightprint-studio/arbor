//! Profiles — the per-environment dimension of Arbor's on-disk layout.
//!
//! A *profile* is an isolated environment (its own settings, plugins, and
//! repos/workspaces) that the user switches between — dev vs demo vs client-X.
//! On disk it is a folder under `arbor/profiles/<name>/`, holding one
//! product-agnostic `profile.toml` plus a sub-folder per product (`corvus/`,
//! `merula/`, …) and a per-profile `plugins/` area. See
//! `docs/profiles-and-product-config.md`.
//!
//! Path helpers elsewhere ([`crate::paths`]) are pure functions; profiles add a
//! *runtime* dimension — which profile is active — without threading it through
//! every caller. This module holds a process-global **active-profile cell**
//! seeded at boot ([`init_active_profile`]) and read by the profile-aware path
//! helpers here. Switching a profile updates the cell ([`set_active_profile`]);
//! the shell is then responsible for reloading config/plugins/repos.
//!
//! The existing `arbor_config_*` helpers keep meaning "the global `arbor/`
//! root" (the `active-profile` pointer, the portable `git/` copy, OAuth client
//! overrides) — only the helpers here are profile-scoped.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use crate::paths::arbor_config_dir;

/// The implicit profile present on every install; the migration target and the
/// fallback when no `active-profile` pointer exists yet.
pub const DEFAULT_PROFILE: &str = "default";

/// On-disk product bucket names — the `<product>` segment under a profile
/// (`arbor/profiles/<profile>/<product>/`). Single source of truth so the shell
/// and the product crates agree on the literal.
pub const PRODUCT_CORVUS: &str = "corvus";
/// See [`PRODUCT_CORVUS`]. merula moves from a sibling namespace into a product
/// bucket under each profile.
pub const PRODUCT_MERULA: &str = "merula";
/// See [`PRODUCT_CORVUS`]. sitta (the file explorer) owns its own per-profile
/// bucket for its settings/data, served out-of-process by `sitta-be`.
pub const PRODUCT_SITTA: &str = "sitta";
/// See [`PRODUCT_CORVUS`]. tyto (the screen recorder) owns its own per-profile
/// bucket for its settings/data, served out-of-process by `tyto-be`.
pub const PRODUCT_TYTO: &str = "tyto";
/// See [`PRODUCT_CORVUS`]. bennu (the Java editor / analysis product) owns its own
/// per-profile bucket for its settings, served out-of-process by `bennu-be`.
pub const PRODUCT_BENNU: &str = "bennu";
/// See [`PRODUCT_CORVUS`]. picus (the SQL studio) owns its own per-profile bucket
/// for its settings, served out-of-process by `picus-be`.
pub const PRODUCT_PICUS: &str = "picus";

/// Process-global selected-profile name. Seeded to [`DEFAULT_PROFILE`] and
/// overwritten at boot by [`init_active_profile`] (or a launch argument).
static ACTIVE_PROFILE: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(DEFAULT_PROFILE.to_string()));

/// Name of the currently selected profile.
pub fn active_profile() -> String {
    ACTIVE_PROFILE
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| DEFAULT_PROFILE.to_string())
}

/// Select a profile by name. Invalid names (see [`is_valid_profile_name`]) are
/// rejected so a path segment can never escape `arbor/profiles/`. Returns
/// whether the active profile actually changed.
///
/// This only flips the cell — reloading the config/plugin/repo state that now
/// resolves to a different folder is the caller's job.
pub fn set_active_profile(name: &str) -> bool {
    if !is_valid_profile_name(name) {
        return false;
    }
    if let Ok(mut g) = ACTIVE_PROFILE.write() {
        if *g != name {
            *g = name.to_string();
            return true;
        }
    }
    false
}

/// Seed the active profile at boot. Call once, early, before any
/// profile-scoped path is resolved.
///
/// Two layers: a **build default** — debug builds start on the `dev` profile so
/// a development run never touches a release install's data (this replaces the
/// old scattered `-dev` filename suffixes) — then the on-disk pointer, which (if
/// present and valid) wins. Both the pointer file and the build default are
/// build-specific, so debug and release stay fully isolated.
pub fn init_active_profile() {
    if cfg!(debug_assertions) {
        set_active_profile("dev");
    }
    if let Ok(name) = std::fs::read_to_string(active_profile_pointer_path()) {
        let name = name.trim();
        if is_valid_profile_name(name) {
            set_active_profile(name);
        }
    }
}

/// A profile name is a single safe directory segment: non-empty, not a
/// dot-segment, no path separators or control characters. Kept liberal beyond
/// that (spaces and unicode allowed) — the only contract is "can't escape the
/// `profiles/` folder".
pub fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(|c: char| c.is_control())
}

// ── Global pointer ───────────────────────────────────────────────────────────

/// The global pointer file naming the selected profile. Lives at the `arbor/`
/// root (not inside any profile): it is what *picks* the profile. Build-specific
/// (`active-profile` for release, `active-profile-dev` for debug) so a debug run
/// and a release install track their selection independently.
pub fn active_profile_pointer_path() -> PathBuf {
    let file = if cfg!(debug_assertions) { "active-profile-dev" } else { "active-profile" };
    arbor_config_dir().join(file)
}

// ── Profile-scoped paths (active profile) ────────────────────────────────────

/// `arbor/profiles` — the parent of every profile folder.
pub fn profiles_root() -> PathBuf {
    arbor_config_dir().join("profiles")
}

/// `arbor/profiles/<active>` — the active profile's folder.
pub fn arbor_profile_dir() -> PathBuf {
    profile_dir_for(&active_profile())
}

/// Join a relative path under the active profile's folder — for
/// product-agnostic per-profile files (`profile.toml`, …).
pub fn arbor_profile_path<P: AsRef<Path>>(sub: P) -> PathBuf {
    arbor_profile_dir().join(sub)
}

/// `arbor/profiles/<active>/<product>` — a product's bucket in the active
/// profile (e.g. `product_dir("corvus")`).
pub fn product_dir(product: &str) -> PathBuf {
    product_dir_for(&active_profile(), product)
}

/// Join a relative path under a product's bucket in the active profile.
pub fn product_path<P: AsRef<Path>>(product: &str, sub: P) -> PathBuf {
    product_dir(product).join(sub)
}

/// Like [`product_path`] but propagates `None` when `dirs::config_dir()` is
/// unavailable instead of falling back to `"."` — the profile-scoped twin of
/// [`crate::paths::try_arbor_config_path`], for callers that prefer to skip
/// persistence over writing under the current working directory.
pub fn try_product_path<P: AsRef<Path>>(product: &str, sub: P) -> Option<PathBuf> {
    dirs::config_dir().map(|_| product_path(product, sub))
}

/// `arbor/profiles/<active>/plugins` — the active profile's plugin area
/// (installed set + per-plugin settings + marketplace cache + themes +
/// toolchains).
pub fn profile_plugins_dir() -> PathBuf {
    arbor_profile_dir().join("plugins")
}

/// `arbor/profiles/<active>/plugins/marketplace_plugins` — where the marketplace
/// installs plugins. The single source of truth for this segment: both the
/// marketplace crate and the product backends (corvus-be, …) resolve their plugin
/// scan root through here, so the launcher host and a product host always agree on
/// where installed plugins live.
pub fn marketplace_plugins_dir() -> PathBuf {
    profile_plugins_dir().join("marketplace_plugins")
}

// ── Explicit-profile variants ────────────────────────────────────────────────
//
// Used by the one-shot migration and by profile management (list/create/switch),
// which operate on a profile that is not (yet) the active one.

/// `arbor/profiles/<name>` for an explicitly named profile.
pub fn profile_dir_for(name: &str) -> PathBuf {
    profiles_root().join(name)
}

/// `arbor/profiles/<name>/<product>` for an explicitly named profile.
pub fn product_dir_for(name: &str, product: &str) -> PathBuf {
    profile_dir_for(name).join(product)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation_blocks_escapes() {
        assert!(is_valid_profile_name("default"));
        assert!(is_valid_profile_name("dev"));
        assert!(is_valid_profile_name("client X")); // spaces ok
        assert!(!is_valid_profile_name(""));
        assert!(!is_valid_profile_name("."));
        assert!(!is_valid_profile_name(".."));
        assert!(!is_valid_profile_name("a/b"));
        assert!(!is_valid_profile_name("a\\b"));
        assert!(!is_valid_profile_name("a\nb"));
    }

    #[test]
    fn explicit_path_composition_is_nested_under_profiles() {
        let root = profiles_root();
        let p = profile_dir_for("dev");
        assert!(p.starts_with(&root));
        assert_eq!(p.file_name().unwrap(), "dev");

        let prod = product_dir_for("dev", "corvus");
        assert!(prod.starts_with(&p));
        assert_eq!(prod.file_name().unwrap(), "corvus");
    }

    #[test]
    fn set_active_profile_rejects_invalid_and_reports_change() {
        // Rejected names never change the cell.
        let before = active_profile();
        assert!(!set_active_profile("bad/name"));
        assert_eq!(active_profile(), before);

        // A valid, different name switches and reports the change; re-setting
        // the same name reports no change.
        assert!(set_active_profile("phase1-test-profile"));
        assert_eq!(active_profile(), "phase1-test-profile");
        assert!(!set_active_profile("phase1-test-profile"));

        // Restore the default so other tests in this binary aren't perturbed.
        set_active_profile(DEFAULT_PROFILE);
    }
}
