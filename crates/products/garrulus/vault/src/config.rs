//! `.arbor/garrulus/vault.toml` — everything that describes *this vault* rather
//! than *this user's preferences*.
//!
//! Why it lives with the vault and not in the profile: the second machine has to
//! inherit the attachments folder, the daily-note pattern and the link style, or
//! the same vault behaves differently depending on which PC you are sitting at —
//! which is the class of surprise a two-machine product exists to remove. The
//! per-user settings (device name, sync cadence, editor preferences) stay in the
//! profile's `garrulus/config.toml`; nothing is in both files.
//!
//! Why `.arbor/garrulus/` and not `.garrulus/`: Arbor already owns `.arbor/` in a
//! project, and namespacing per product inside it means a vault that is also a
//! Corvus repository has one dot-folder, not two.
//!
//! **Absent is normal.** A folder full of markdown that has never been opened in
//! Garrulus is a perfectly good vault-to-be; [`load`] returns `Ok(None)` for it
//! rather than an error, and the caller offers to create the marker.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{VaultError, VaultResult};

/// The one dot-folder, relative to the vault root.
pub const MARKER_RELATIVE_PATH: &str = ".arbor/garrulus";

/// Where the vault-scoped settings sit, relative to the vault root.
pub const VAULT_CONFIG_RELATIVE_PATH: &str = ".arbor/garrulus/vault.toml";

/// Where the note types sit, relative to the vault root.
pub const TYPES_RELATIVE_PATH: &str = ".arbor/garrulus/types";

/// Where deleted notes wait, relative to the vault root.
pub const TRASH_RELATIVE_PATH: &str = ".arbor/garrulus/trash";

/// The highest schema version this build understands.
pub const CURRENT_VERSION: u32 = 1;

/// The one dot-folder, resolved against a root.
///
/// Built from [`MARKER_RELATIVE_PATH`] rather than re-spelling the segments, so
/// there is exactly one place that knows where Garrulus writes.
pub fn marker_dir(root: &Path) -> PathBuf {
    resolve(root, MARKER_RELATIVE_PATH)
}

/// `<root>/.arbor/garrulus/vault.toml`.
pub fn config_path(root: &Path) -> PathBuf {
    resolve(root, VAULT_CONFIG_RELATIVE_PATH)
}

/// `<root>/.arbor/garrulus/types`.
pub fn types_dir(root: &Path) -> PathBuf {
    resolve(root, TYPES_RELATIVE_PATH)
}

/// `<root>/.arbor/garrulus/trash`.
pub fn trash_dir(root: &Path) -> PathBuf {
    resolve(root, TRASH_RELATIVE_PATH)
}

fn resolve(root: &Path, relative: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for segment in relative.split('/') {
        out.push(segment);
    }
    out
}

/// How a note refers to another note when Garrulus writes the link itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStyle {
    /// `[[Note]]` — the Obsidian dialect, and the default because the vault has
    /// to stay readable in Obsidian.
    #[default]
    Wiki,
    /// `[Note](./note.md)` — portable to any markdown renderer, at the cost of
    /// breaking when a note moves.
    Markdown,
}

/// The daily note: the one file two machines fight over most.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DailySettings {
    /// Folder new daily notes land in.
    pub folder: String,
    /// Filename pattern, expanded by [`crate::naming::file_name`].
    pub naming: String,
    /// The note type applied to a freshly created daily note, when one exists.
    pub type_id: String,
}

impl Default for DailySettings {
    fn default() -> Self {
        DailySettings {
            folder: "daily".to_string(),
            naming: "{{date}}".to_string(),
            type_id: "daily".to_string(),
        }
    }
}

/// The whole vault file.
///
/// Field order is load-bearing: `toml` refuses to emit a plain value after a
/// table, so every scalar and plain array is declared before [`DailySettings`].
/// Reordering this struct silently breaks `save`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VaultConfig {
    /// Schema version of this file, so a future shape is recognised rather than
    /// silently half-read.
    #[serde(default = "default_version")]
    pub version: u32,
    /// What the vault is called in the switcher. Free text; the folder name is
    /// only the initial suggestion.
    #[serde(default)]
    pub name: String,
    /// Where a pasted image is written. Kept inside the vault so it syncs with
    /// the note that embeds it.
    #[serde(default = "default_attachments")]
    pub attachments: String,
    /// How Garrulus spells a link it writes itself. It never rewrites links the
    /// user wrote in the other style.
    #[serde(default)]
    pub link_style: LinkStyle,
    /// Folder globs never scanned, indexed or synced. Dot-directories are always
    /// skipped and do not need to be listed.
    #[serde(default)]
    pub excluded: Vec<String>,
    /// The daily note. A table, so it comes last — see the note on field order.
    #[serde(default)]
    pub daily: DailySettings,
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

fn default_attachments() -> String {
    "attachments".to_string()
}

impl Default for VaultConfig {
    fn default() -> Self {
        VaultConfig {
            version: CURRENT_VERSION,
            name: String::new(),
            attachments: default_attachments(),
            link_style: LinkStyle::default(),
            excluded: Vec::new(),
            daily: DailySettings::default(),
        }
    }
}

impl VaultConfig {
    /// Parse the file's text. Kept separate from [`load`] so the shape can be
    /// tested without a filesystem.
    pub fn parse(text: &str) -> Result<VaultConfig, toml::de::Error> {
        toml::from_str(text)
    }

    /// Render the file's text.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Drop empty entries and sort what has no meaningful order, so a git diff of
    /// this file reads as the decisions that changed rather than as churn.
    pub fn tidy(&mut self) {
        self.excluded.retain(|glob| !glob.trim().is_empty());
        self.excluded.sort();
        self.excluded.dedup();
        self.name = self.name.trim().to_string();
    }

    /// Read `<root>/.arbor/garrulus/vault.toml`.
    ///
    /// `Ok(None)` when the file is absent — "not set up yet" is an ordinary state
    /// and is exactly what makes the caller offer to create the vault.
    pub fn load(root: &Path) -> VaultResult<Option<VaultConfig>> {
        let path = config_path(root);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| VaultError::io(&path, e))?;
        let config = VaultConfig::parse(&text).map_err(|e| VaultError::malformed(&path, e))?;
        if config.version > CURRENT_VERSION {
            return Err(VaultError::Malformed {
                path,
                reason: format!(
                    "it was written by a newer Garrulus (version {}, this build reads up to {CURRENT_VERSION})",
                    config.version
                ),
            });
        }
        Ok(Some(config))
    }

    /// Write `<root>/.arbor/garrulus/vault.toml`, creating the dot-folder.
    ///
    /// Tidies first: the file is committed and shared with the other machine, so
    /// it is written in its canonical form every time rather than accumulating
    /// whichever order the last edit happened to leave.
    pub fn save(&mut self, root: &Path) -> VaultResult<PathBuf> {
        self.tidy();
        let path = config_path(root);
        let text = self.to_toml().map_err(|e| VaultError::malformed(&path, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VaultError::io(parent, e))?;
        }
        std::fs::write(&path, text).map_err(|e| VaultError::io(&path, e))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_a_complete_config() {
        let config = VaultConfig::parse("").expect("an empty vault file is legal");
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(config.attachments, "attachments");
        assert_eq!(config.daily.folder, "daily");
        assert_eq!(config.link_style, LinkStyle::Wiki);
    }

    #[test]
    fn the_file_round_trips_through_toml() {
        let mut config = VaultConfig { name: "Appunti".to_string(), ..VaultConfig::default() };
        config.excluded.push("archivio/**".to_string());
        let text = config.to_toml().expect("the config serialises");
        let back = VaultConfig::parse(&text).expect("and reads back");
        assert_eq!(back, config);
    }

    #[test]
    fn tidy_drops_blank_globs_and_sorts_the_rest() {
        let mut config = VaultConfig::default();
        config.name = "  Appunti  ".to_string();
        config.excluded =
            vec!["z/**".into(), "  ".into(), "a/**".into(), "a/**".into(), "".into()];
        config.tidy();
        assert_eq!(config.name, "Appunti");
        assert_eq!(config.excluded, vec!["a/**".to_string(), "z/**".to_string()]);
    }

    #[test]
    fn every_garrulus_path_hangs_off_the_one_dot_folder() {
        let root = Path::new("/vault");
        assert!(config_path(root).starts_with(marker_dir(root)));
        assert!(types_dir(root).starts_with(marker_dir(root)));
        assert!(trash_dir(root).starts_with(marker_dir(root)));
    }
}
