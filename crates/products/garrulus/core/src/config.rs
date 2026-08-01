//! `config` — the typed **global** garrulus configuration
//! (`arbor/profiles/<active>/garrulus/config.toml`, per-profile) owned
//! **out-of-process** by `garrulus-be`.
//!
//! Holds the three things that are about *this machine* rather than about the
//! vault: the **device name** (which rides in every sync commit and in every UI
//! string that says "the other PC"), the **sync cadence** (the read-only
//! background probe — nothing here ever makes it write), and the editor
//! preferences. Everything about a *vault* — note types, templates, attachment
//! folder, link style — deliberately lives inside the vault under
//! `<vault>/.arbor/garrulus/`, so it syncs to the second machine with the notes.
//!
//! Like sitta's, the path is **not** pushed by the shell: garrulus-be resolves it
//! itself, since `init_active_profile()` ran in `main` before any handler is
//! served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`GarrulusConfig::default`] so operational reads never break.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The on-disk product bucket name — the `<product>` segment under a profile.
///
/// `arbor-core` has a `PRODUCT_*` const and a `<product>_config_path` helper per
/// product; garrulus has neither yet, so the literal lives here and every path in
/// this crate goes through it. Collapse this into `arbor_core::prelude` (a
/// `PRODUCT_GARRULUS` + `garrulus_config_path` pair) when foundation is next
/// touched — this const is the only thing that has to change.
pub const PRODUCT_GARRULUS: &str = "garrulus";

/// Persisted garrulus settings (global, per-profile `…/garrulus/config.toml`).
///
/// Field order matters for TOML serialization: every scalar / value-array field is
/// declared before the array-of-tables / table fields (`editor`), or `toml` fails
/// with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GarrulusConfig {
    /// How this machine identifies itself: sync commit authorship
    /// (`Garrulus (<device>)`), the conflict side-file name, `devices.json`, and
    /// every "3 note in arrivo da <device>" string. Defaults to the OS host name
    /// so a fresh install already reads sensibly on two PCs.
    pub device_name: String,
    /// Re-open the last vault at startup instead of showing the vault picker.
    pub open_last_vault: bool,
    /// Absolute path of the vault to re-open when [`open_last_vault`] is set.
    /// Empty until a vault has been opened once.
    pub last_vault: String,
    /// Seconds between background sync **probes**. The probe is read-only by
    /// design (`docs/garrulus-design.md` §4.2): it fetches and updates the state
    /// indicator, and never commits, pulls or pushes. `0` disables it.
    pub sync_probe_secs: u32,
    /// Only probe while the Garrulus window has focus — the scheduler's per-tick
    /// focus gate. Off means probe on a timer regardless.
    pub sync_probe_focus_only: bool,
    /// Quiet period, in milliseconds, the vault watcher waits before reporting a
    /// burst of filesystem changes as one `garrulus:vault-changed` event. A sync
    /// pull or an Obsidian save touches many files at once; without this the
    /// frontend would re-read the tree dozens of times per second.
    pub watch_debounce_ms: u64,
    /// Editor preferences. **Declared last**: it is a TOML table.
    pub editor: GarrulusEditorConfig,
}

impl Default for GarrulusConfig {
    fn default() -> Self {
        Self {
            device_name:           default_device_name(),
            open_last_vault:       true,
            last_vault:            String::new(),
            sync_probe_secs:       60,
            sync_probe_focus_only: true,
            watch_debounce_ms:     300,
            editor:                GarrulusEditorConfig::default(),
        }
    }
}

/// Editor preferences — the subset of the markdown editor's capability set that
/// is a user preference rather than a per-vault or per-note property.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GarrulusEditorConfig {
    /// Obsidian-style live preview (conceal the syntax of the inline component
    /// under the cursor). Off means plain source with syntax highlighting.
    pub live_preview: bool,
    /// Show the formatting toolbar above the editor. Every button is a keybinding
    /// first, so hiding it costs nothing but discoverability.
    pub toolbar: bool,
    /// Render the YAML frontmatter as a form card at the top of the note instead
    /// of as raw source.
    pub frontmatter_form: bool,
    /// Run the prose spell checker (EN + IT) over the note body.
    pub spell_check: bool,
}

impl Default for GarrulusEditorConfig {
    fn default() -> Self {
        Self { live_preview: true, toolbar: true, frontmatter_form: true, spell_check: false }
    }
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// garrulus's own config file: `arbor/profiles/<active>/garrulus/config.toml`.
/// Resolved directly (not pushed by the shell) — `init_active_profile()` ran in
/// `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::product_path(PRODUCT_GARRULUS, "config.toml")
}

/// Read the garrulus config. A missing / unparseable file yields defaults, never
/// an error — a broken settings file must not stop a vault from opening.
pub fn load() -> GarrulusConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<GarrulusConfig>(&text) {
            return cfg;
        }
    }
    GarrulusConfig::default()
}

/// Persist the garrulus config to its own file (pretty TOML), creating the dir if
/// needed.
pub fn save(cfg: &GarrulusConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// This machine's name, best-effort, for the sync identity.
///
/// No new dependency for one string: the host name is in the environment on every
/// platform Arbor targets (`COMPUTERNAME` on Windows, `HOSTNAME` on the unices —
/// and where a login shell did not export it, the fallback is a name the user can
/// change in settings rather than an error).
fn default_device_name() -> String {
    for key in ["COMPUTERNAME", "HOSTNAME", "HOST"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    "questo-pc".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The TOML round-trip is the only thing that can silently break the settings
    /// file, and it breaks on field ORDER (tables must come last), so it is worth
    /// a test rather than a comment.
    #[test]
    fn config_round_trips_through_toml() {
        let mut cfg = GarrulusConfig::default();
        cfg.device_name = "casa".to_string();
        cfg.sync_probe_secs = 15;
        cfg.editor.spell_check = true;

        let text = toml::to_string_pretty(&cfg).expect("serialises");
        let back: GarrulusConfig = toml::from_str(&text).expect("parses back");

        assert_eq!(back.device_name, "casa");
        assert_eq!(back.sync_probe_secs, 15);
        assert!(back.editor.spell_check);
        assert!(back.editor.live_preview, "untouched fields keep their default");
    }

    /// A config file written by an older build (or hand-edited down to one key)
    /// must still load — `#[serde(default)]` is what guarantees it.
    #[test]
    fn partial_config_fills_in_defaults() {
        let cfg: GarrulusConfig = toml::from_str("device_name = \"ufficio\"").expect("parses");
        assert_eq!(cfg.device_name, "ufficio");
        assert_eq!(cfg.sync_probe_secs, 60);
        assert_eq!(cfg.watch_debounce_ms, 300);
    }

    #[test]
    fn device_name_is_never_empty() {
        assert!(!default_device_name().is_empty());
    }
}
