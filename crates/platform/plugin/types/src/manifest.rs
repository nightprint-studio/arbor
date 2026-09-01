//! `plugin.toml` manifest shape + pure parsing entry point. Discovery,
//! filesystem walking, and the persisted enable/disable state file live in
//! the host (`arbor` / `arbor-plugin-core`); this crate intentionally stays
//! free of I/O.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dependency::Dependency;
use crate::hooks::Hooks;
use crate::permissions::Permissions;
use crate::provides::{CredentialSlot, LuaSection, Provides, WasmSection};
use crate::sandbox::Sandbox;
use crate::schedule::SchedulerSection;

// ---------------------------------------------------------------------------
// Manifest (plugin.toml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ── Identity ──────────────────────────────────────────────────────────────
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    #[serde(default)]
    pub license:     Option<String>,
    #[serde(default)]
    pub repository:  Option<String>,
    /// Optional homepage / docs URL — surfaced in the marketplace detail pane.
    #[serde(default)]
    pub homepage:    Option<String>,
    #[serde(default)]
    pub keywords:    Vec<String>,
    /// Free-form category for marketplace filtering. Curated values:
    /// `build` · `ci` · `git-workflow` · `language` · `ui` · `data` · `theme`.
    #[serde(default)]
    pub category:    Option<String>,
    /// Path (relative to the plugin dir) to a square SVG/PNG icon used by the
    /// marketplace + Plugin Manager. Falls back to a monogram when absent.
    #[serde(default)]
    pub icon:        Option<String>,

    // ── Compatibility ─────────────────────────────────────────────────────────
    /// Minimum Arbor version required (semver string), e.g. "0.8.0".
    /// Validated against `ARBOR_APP_VERSION` at load time — incompatible
    /// plugins are rejected with a clear error.
    #[serde(default)]
    pub min_arbor_version: Option<String>,
    /// Integer version of the Lua API contract. Bumped on breaking changes.
    /// Plugins with arbor_api > ARBOR_API_VERSION are rejected at load time.
    #[serde(default = "default_arbor_api")]
    pub arbor_api: u32,
    /// Operating systems this plugin supports. Empty = cross-platform.
    /// Recognised values: "windows", "linux", "macos". Plugins running on a
    /// non-listed OS are skipped at discovery time.
    #[serde(default)]
    pub os: Vec<String>,
    /// Products whose backend hosts this plugin, e.g. `["corvus"]`. A plugin can
    /// target more than one (`["corvus", "merula"]`). **Empty = universal**: the
    /// plugin loads on every product. Each product's plugin host loads only the
    /// plugins that target it (or are universal), so a `corvus`-only plugin never
    /// runs in a future `merula` backend. Recognised values track the product
    /// ids (`corvus`, `merula`, `sitta`, …).
    #[serde(default)]
    pub targets: Vec<String>,
    /// The Lua half of the package, when it has one. See [`Manifest::lua_entry`] for the
    /// rule that decides whether an absent section means "no Lua" or "the default entry".
    #[serde(default)]
    pub lua: Option<LuaSection>,

    // ── Documentation ────────────────────────────────────────────────────────
    /// Optional path to an HTML file (relative to plugin dir) shown in the
    /// Docs panel under "Plugins". Not required — omit to skip the Plugins section.
    #[serde(default)]
    pub doc_file: Option<String>,

    /// When true, the plugin is flagged as experimental in the Plugin Manager
    /// (orange "EXPERIMENTAL" pill next to the version). Intended for plugins
    /// that are still iterating heavily on their public surface — settings,
    /// hooks, storage formats — and may break between releases.
    #[serde(default)]
    pub experimental: bool,

    // ── Sections ──────────────────────────────────────────────────────────────
    /// What the plugin may reach. **Absent means none**, which is why this defaults rather than
    /// being required: a package that computes and nothing else — a wasm library linked by other
    /// plugins, with no network, no filesystem and no credential slots — has nothing to declare,
    /// and demanding an empty `[permissions]` from it rejected the manifest outright with a TOML
    /// error pointing at line 1. `Permissions::default()` grants nothing, so the lenient parse is
    /// also the closed one.
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub sandbox:     Sandbox,
    #[serde(default)]
    pub hooks:       Hooks,
    /// Background-scheduler opt-in. Schedule data (interval / cron / etc.)
    /// is declared from Lua via `arbor.scheduler.register`; the manifest only
    /// gates the feature on or off.
    #[serde(default)]
    pub scheduler:   SchedulerSection,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    /// Implementations of host-defined interfaces carried by this package — a Studio format
    /// backend, a cloud provider. Empty for an ordinary Lua plugin, which is every package
    /// that exists today.
    #[serde(default)]
    pub provides: Vec<Provides>,
    /// Settings shared by every module in [`Manifest::provides`]. Meaningless, and ignored,
    /// when that list is empty.
    #[serde(default)]
    pub wasm: WasmSection,
    /// Credential slots this package owns. It may create and read these and nothing else —
    /// see [`CredentialSlot`] for why that is a namespace rather than a filter.
    #[serde(default)]
    pub credentials: Vec<CredentialSlot>,

    /// Path to the plugin directory — not in TOML, filled at discovery time.
    #[serde(skip)]
    pub dir: PathBuf,
}

fn default_arbor_api() -> u32 { 1 }

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Errors that can occur when parsing a `plugin.toml` text payload.
#[derive(Debug, Error)]
pub enum ManifestParseError {
    #[error("plugin.toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

impl Manifest {
    /// The Lua entry point to load, or `None` for a package that has no Lua half.
    ///
    /// The rule, and it is one rule rather than a special case: **a package must contain at
    /// least one part.** So an absent `[lua]` section means "no Lua" only when the package
    /// provides something else; a package that provides nothing *is* a Lua plugin, and its
    /// entry point is the default it has always been.
    ///
    /// That is what keeps this change from breaking the packages that exist. None of them
    /// declares an entry point today — they all rely on the default — and none of them
    /// provides anything, so all of them keep loading `main.lua` without being touched.
    pub fn lua_entry(&self) -> Option<&str> {
        match (&self.lua, self.provides.is_empty()) {
            (Some(section), _) => Some(section.entry.as_str()),
            // Provides nothing and says nothing: it can only be a Lua plugin.
            (None, true) => Some("main.lua"),
            // Provides something and claims no Lua half. Believe it.
            (None, false) => None,
        }
    }

    /// Whether installing this package requires the release channel rather than a source
    /// archive. Carrying a built artifact is what forces the stricter path — see
    /// `docs/extension-repo-layout.md`.
    pub fn has_binary_parts(&self) -> bool {
        !self.provides.is_empty()
    }

    /// Parse a `plugin.toml` text payload and stamp the plugin directory on the
    /// resulting [`Manifest`]. No filesystem access — callers (the host's
    /// `discover_plugins`, the marketplace installer, etc.) read the file and
    /// pass the resulting string in.
    pub fn from_toml_str(content: &str, dir: &Path) -> Result<Self, ManifestParseError> {
        let mut manifest: Manifest = toml::from_str(content)?;
        manifest.dir = dir.to_path_buf();
        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------
// Discovery-time failure record
// ---------------------------------------------------------------------------

/// A plugin folder whose `plugin.toml` could not be parsed. Kept separate
/// from `Manifest` because we don't have a typed manifest to attach the error
/// to — the folder name is the best stand-in for the plugin name.
#[derive(Debug, Clone)]
pub struct ManifestParseFailure {
    pub folder_name: String,
    pub error:       String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The head every fixture below needs — the fields with no default.
    const HEAD: &str = "\
name        = \"x\"
version     = \"1.0.0\"
description = \"d\"
author       = \"a\"

[permissions]
";

    fn parse(extra: &str) -> Manifest {
        Manifest::from_toml_str(&format!("{HEAD}{extra}"), Path::new("/p")).expect("parse")
    }

    /// A package with nothing to declare — no `[permissions]` at all. This is what a pure-wasm
    /// library linked by other plugins looks like, and requiring the section rejected it with a
    /// TOML error at line 1 that named the whole file rather than the missing block.
    #[test]
    fn a_manifest_without_a_permissions_section_parses_and_grants_nothing() {
        let m = Manifest::from_toml_str(
            "name = \"x\"\nversion = \"1.0.0\"\ndescription = \"d\"\nauthor = \"a\"\n",
            Path::new("/p"),
        )
        .expect("a manifest that grants nothing is still a manifest");
        assert!(m.permissions.network.is_empty());
        assert_eq!(m.permissions.fs, crate::permissions::AccessLevel::None);
    }

    #[test]
    fn a_plugin_that_says_nothing_still_loads_main_lua() {
        // Every package that exists today looks like this: no `[lua]`, no `[[provides]]`.
        // If this ever fails, the change broke all twenty of them at once.
        assert_eq!(parse("").lua_entry(), Some("main.lua"));
    }

    #[test]
    fn a_lua_section_can_name_a_different_entry() {
        assert_eq!(parse("\n[lua]\nentry = \"init.lua\"\n").lua_entry(), Some("init.lua"));
    }

    #[test]
    fn a_lua_section_without_an_entry_still_means_main_lua() {
        assert_eq!(parse("\n[lua]\n").lua_entry(), Some("main.lua"));
    }

    #[test]
    fn a_package_that_provides_something_and_claims_no_lua_has_none() {
        let m = parse(
            "\n[[provides]]\ninterface = \"cloud-provider\"\nversion = 1\n\
             id = \"gcs\"\nmodule = \"cloud_gcs.wasm\"\n",
        );
        assert_eq!(m.lua_entry(), None);
        assert!(m.has_binary_parts());
    }

    #[test]
    fn a_package_can_have_both_halves() {
        let m = parse(
            "\n[lua]\nentry = \"main.lua\"\n\n[[provides]]\ninterface = \"cloud-provider\"\n\
             version = 1\nid = \"gcs\"\nmodule = \"cloud_gcs.wasm\"\n",
        );
        assert_eq!(m.lua_entry(), Some("main.lua"));
        assert!(m.has_binary_parts());
    }

    #[test]
    fn an_ordinary_plugin_does_not_need_the_release_channel() {
        assert!(!parse("").has_binary_parts());
    }

    #[test]
    fn credentials_are_declared_slots_rather_than_a_flag() {
        // The consent dialog has to be able to say WHAT will be stored.
        let m = parse(
            "\n[[credentials]]\nkey = \"oauth\"\nlabel = \"Google account\"\n\
             \n[[credentials]]\nkey = \"hmac\"\nlabel = \"S3 access key\"\n",
        );
        assert_eq!(m.credentials.len(), 2);
        assert_eq!(m.credentials[0].key, "oauth");
        assert_eq!(m.credentials[1].label, "S3 access key");
    }

    #[test]
    fn a_package_declaring_no_credentials_owns_none() {
        // The default has to be the empty set, not "unspecified" — a plugin that said
        // nothing must reach nothing.
        assert!(parse("").credentials.is_empty());
    }
}
