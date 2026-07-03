//! `config` — the typed **product** bennu configuration
//! (`arbor/profiles/<active>/bennu/config.toml`, per-profile) owned
//! **out-of-process** by `bennu-be`.
//!
//! Holds the Java-editor's persisted defaults + the IntelliJ-style *overrides* the
//! project model consults: a per-project JDK override (when the pom can't be trusted
//! / a different JDK is wanted) and a per-project / per-file encoding override (the
//! footer-style "reload in encoding X"). The auto-detected values live in the
//! project model; these are only the user's explicit overrides + editor defaults.
//!
//! Like `tyto-core`'s config, the path is **not** pushed by the shell: bennu-be
//! resolves [`bennu_config_path`](arbor_core::prelude::bennu_config_path) itself,
//! since `init_active_profile()` ran in `main` before any handler is served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`BennuConfig::default`] so operational reads never break. The
//! `get/set_bennu_config` handlers stay in bennu-be and call back into [`load`] /
//! [`save`] here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted bennu settings (product, per-profile `…/bennu/config.toml`).
///
/// Field order matters for TOML serialization: every scalar field is declared
/// before the map/table fields (`jdk_overrides` / `encoding_overrides`), or `toml`
/// fails with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BennuConfig {
    /// Default text encoding to *fall back to* when a project declares none and no
    /// override applies. `"UTF-8"` by default (the declared pom encoding always wins
    /// over this — see `bennu-project`'s encoding detection).
    pub default_encoding: String,
    /// Editor indentation width in spaces (the whitespace normalizer / display).
    pub indent_width: u32,
    /// Extra JDK install directories to search, on top of `JAVA_HOME` +
    /// `C:/Program Files/Java/*`. For a JDK installed somewhere non-standard (a portable
    /// SDK, an IDE-bundled JDK, `/usr/lib/jvm/…`), so the index can still resolve the
    /// standard library. Each is a JDK home (the dir holding `release` / `bin/java`).
    pub jdk_paths: Vec<String>,
    /// Per-project JDK override, keyed by absolute project-root path → Java version
    /// string (e.g. `"17"`). Present entries win over the pom-detected JDK.
    pub jdk_overrides: BTreeMap<String, String>,
    /// Per-project (or per-file) encoding override, keyed by absolute path → encoding
    /// label (e.g. `"Cp1252"`). Present entries win over the pom-declared encoding.
    pub encoding_overrides: BTreeMap<String, String>,
}

impl Default for BennuConfig {
    fn default() -> Self {
        Self {
            default_encoding: "UTF-8".to_string(),
            indent_width: 4,
            jdk_paths: Vec::new(),
            jdk_overrides: BTreeMap::new(),
            encoding_overrides: BTreeMap::new(),
        }
    }
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// bennu's own config file: `arbor/profiles/<active>/bennu/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("config.toml")
}

/// Read the bennu config. A missing / unparseable file yields defaults, never an
/// error — editor settings are non-critical and self-heal to defaults.
pub fn load() -> BennuConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<BennuConfig>(&text) {
            return cfg;
        }
    }
    BennuConfig::default()
}

/// Persist the bennu config to its own file (pretty TOML), creating the dir if
/// needed.
pub fn save(cfg: &BennuConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}
