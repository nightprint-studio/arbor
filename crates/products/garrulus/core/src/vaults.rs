//! `vaults` — the registry of known vaults
//! (`arbor/profiles/<active>/garrulus/vaults.json`).
//!
//! **The only place absolute vault paths live** (`docs/garrulus-design.md` §3.3),
//! mirroring corvus's `repos.json`: everything else in Garrulus addresses a note
//! by a vault-relative path, so a vault that moves is one entry to fix rather than
//! a rewrite of every reference.
//!
//! Stored as JSON rather than TOML because it is a list the app maintains, not a
//! file the user hand-edits — the same call the corvus registry made.
//!
//! ## Why the id is derived and not a UUID
//!
//! A vault's id keys its index cache (`cache/garrulus/<id>/`). Deriving it from
//! the canonical path makes that mapping **stable across reinstalls and across
//! machines**, so re-adding a vault finds its cache instead of rebuilding it, and
//! it costs no `uuid` dependency for a value nothing else needs to be unique
//! against. Collisions are irrelevant here: the id is a cache key, and a collision
//! between two literally different paths would only mean a rebuilt index.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::PRODUCT_GARRULUS;
use crate::remote::RemoteConfig;

/// One known vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    /// Stable id derived from [`path`](Self::path) — see [`vault_id_for`].
    pub id: String,
    /// Absolute path of the vault root.
    pub path: String,
    /// What the vault is called in the switcher. Defaults to the folder name.
    pub display_name: String,
    /// The configured sync destination, or `None` for a local-only vault.
    ///
    /// Deliberately here and not in `<vault>/.arbor/garrulus/vault.toml`: the
    /// mirror path of a folder remote is machine-specific and would be wrong on
    /// the other PC the moment it synced — see [`crate::remote`].
    #[serde(default)]
    pub remote: Option<RemoteConfig>,
    /// Unix milliseconds of the last successful open, so the picker can sort by
    /// recency. `None` until the vault has been opened once.
    #[serde(default)]
    pub last_opened: Option<i64>,
}

/// The whole registry file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultRegistry {
    /// Known vaults, most-recently-opened first after [`upsert`](Self::upsert).
    pub vaults: Vec<VaultEntry>,
}

impl VaultRegistry {
    /// Insert or refresh an entry, keyed by id, and move it to the front.
    ///
    /// Pure — the whole of the registry's behaviour is here, so the file I/O below
    /// stays a two-liner with nothing to test.
    pub fn upsert(&mut self, entry: VaultEntry) {
        self.vaults.retain(|v| v.id != entry.id);
        self.vaults.insert(0, entry);
    }

    /// Drop a vault from the registry. Returns whether anything was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.vaults.len();
        self.vaults.retain(|v| v.id != id);
        self.vaults.len() != before
    }

    /// The entry for a vault root, if it is known.
    pub fn find_by_path(&self, path: &Path) -> Option<&VaultEntry> {
        let id = vault_id_for(path);
        self.vaults.iter().find(|v| v.id == id)
    }

    /// The entry with this id, if it is known.
    pub fn find(&self, id: &str) -> Option<&VaultEntry> {
        self.vaults.iter().find(|v| v.id == id)
    }

    /// Attach (`Some`) or clear (`None`) a vault's sync destination.
    ///
    /// Returns whether the vault was known — an unknown id is a caller bug worth
    /// reporting, not something to paper over by inserting a half-built entry
    /// whose path nobody set.
    ///
    /// Pure, like the rest of this type: the file I/O is
    /// [`set_vault_remote`], which has nothing of its own to test.
    pub fn set_remote(&mut self, id: &str, remote: Option<RemoteConfig>) -> bool {
        match self.vaults.iter_mut().find(|v| v.id == id) {
            Some(entry) => {
                entry.remote = remote;
                true
            }
            None => false,
        }
    }
}

/// The registry file: `arbor/profiles/<active>/garrulus/vaults.json`.
pub fn vaults_path() -> PathBuf {
    arbor_core::prelude::product_path(PRODUCT_GARRULUS, "vaults.json")
}

/// Read the registry. A missing / unparseable file yields an empty registry, never
/// an error — a corrupt list of vaults must not stop the window from opening.
pub fn load_vaults() -> VaultRegistry {
    if let Ok(text) = std::fs::read_to_string(vaults_path()) {
        if let Ok(reg) = serde_json::from_str::<VaultRegistry>(&text) {
            return reg;
        }
    }
    VaultRegistry::default()
}

/// Persist the registry (pretty JSON), creating the dir if needed.
pub fn save_vaults(reg: &VaultRegistry) -> Result<(), String> {
    let path = vaults_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(reg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Read a vault's configured sync destination straight from the registry.
///
/// For the caller that has an id and no entry in hand (the background probe, a
/// settings panel); the vault-open path already holds the [`VaultEntry`]
/// [`remember_vault`] handed back and should read `entry.remote` from it.
pub fn vault_remote(id: &str) -> Option<RemoteConfig> {
    load_vaults().find(id).and_then(|v| v.remote.clone())
}

/// Persist a vault's sync destination (or clear it with `None`).
///
/// Load / mutate / save against the whole file rather than a targeted patch: the
/// registry is a handful of entries, and rewriting it whole is what keeps the
/// on-disk shape a plain serialisation of [`VaultRegistry`] with no second code
/// path to keep in step.
pub fn set_vault_remote(id: &str, remote: Option<RemoteConfig>) -> Result<(), String> {
    let mut reg = load_vaults();
    if !reg.set_remote(id, remote) {
        return Err(format!("garrulus: no vault with id '{id}' in the registry"));
    }
    save_vaults(&reg)
}

/// Record a vault as known + just-opened, and hand back its entry.
///
/// `display_name` defaults to the folder name, and an existing entry keeps the
/// name the user gave it — renaming a vault is a deliberate act, not something an
/// open should undo. The configured [`RemoteConfig`] is carried forward for the
/// same reason: re-opening a vault must not silently lose its destination.
pub fn remember_vault(root: &Path, now_ms: Option<i64>) -> Result<VaultEntry, String> {
    let mut reg = load_vaults();
    let id = vault_id_for(root);
    let existing = reg.find(&id);
    let display_name =
        existing.map(|v| v.display_name.clone()).unwrap_or_else(|| folder_name(root));
    let remote = existing.and_then(|v| v.remote.clone());

    let entry = VaultEntry {
        id,
        path: root.to_string_lossy().to_string(),
        display_name,
        remote,
        last_opened: now_ms,
    };
    reg.upsert(entry.clone());
    save_vaults(&reg)?;
    Ok(entry)
}

/// Where a vault's index cache lives: `<arbor cache>/garrulus/<vault id>/`.
/// Deletable at any time — the index is rebuilt from the notes at the next open.
pub fn vault_cache_dir(id: &str) -> PathBuf {
    arbor_core::prelude::arbor_cache_dir().join(PRODUCT_GARRULUS).join(id)
}

/// The stable id of a vault root: a 64-bit FNV-1a of the path, lower-case hex.
///
/// Case- and separator-normalised first, so `C:\Notes` and `c:/notes` are the same
/// vault on Windows — where they genuinely are the same folder — and a trailing
/// separator never produces a second identity.
pub fn vault_id_for(root: &Path) -> String {
    let normalised = root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase();
    format!("{:016x}", fnv1a64(normalised.as_bytes()))
}

/// The folder name of a path, or the path itself when it has no last segment
/// (a drive root) — never empty, because it is shown in the vault switcher.
fn folder_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().to_string())
}

/// FNV-1a, 64-bit. Inlined rather than depended upon: eleven lines against a new
/// crate for a cache key.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, path: &str) -> VaultEntry {
        VaultEntry {
            id:           id.to_string(),
            path:         path.to_string(),
            display_name: "Vault".to_string(),
            remote:       None,
            last_opened:  None,
        }
    }

    #[test]
    fn id_is_stable_and_path_normalised() {
        let a = vault_id_for(Path::new("/home/c/notes"));
        assert_eq!(a, vault_id_for(Path::new("/home/c/notes")), "same path, same id");
        assert_eq!(a, vault_id_for(Path::new("/home/c/notes/")), "trailing slash is the same vault");
        assert_eq!(
            vault_id_for(Path::new("C:\\Notes")),
            vault_id_for(Path::new("c:/notes")),
            "separator + case normalised"
        );
        assert_ne!(a, vault_id_for(Path::new("/home/c/other")));
        assert_eq!(a.len(), 16, "fixed-width hex keeps the cache dir names tidy");
    }

    #[test]
    fn upsert_replaces_and_promotes() {
        let mut reg = VaultRegistry::default();
        reg.upsert(entry("a", "/a"));
        reg.upsert(entry("b", "/b"));
        assert_eq!(reg.vaults[0].id, "b", "newest first");

        reg.upsert(entry("a", "/a-moved"));
        assert_eq!(reg.vaults.len(), 2, "same id updates in place, never duplicates");
        assert_eq!(reg.vaults[0].id, "a");
        assert_eq!(reg.vaults[0].path, "/a-moved");
    }

    #[test]
    fn remove_reports_whether_it_did_anything() {
        let mut reg = VaultRegistry::default();
        reg.upsert(entry("a", "/a"));
        assert!(reg.remove("a"));
        assert!(!reg.remove("a"));
        assert!(reg.vaults.is_empty());
    }

    #[test]
    fn find_by_path_goes_through_the_same_normalisation() {
        let mut reg = VaultRegistry::default();
        reg.upsert(entry(&vault_id_for(Path::new("/home/c/notes")), "/home/c/notes"));
        assert!(reg.find_by_path(Path::new("/home/c/notes/")).is_some());
        assert!(reg.find_by_path(Path::new("/home/c/nope")).is_none());
    }

    #[test]
    fn set_remote_touches_one_vault_and_reports_an_unknown_id() {
        let mut reg = VaultRegistry::default();
        reg.upsert(entry("a", "/a"));
        reg.upsert(entry("b", "/b"));

        assert!(reg.set_remote("a", Some(RemoteConfig::git("origin"))));
        assert_eq!(reg.find("a").and_then(|v| v.remote.clone()), Some(RemoteConfig::git("origin")));
        assert!(reg.find("b").expect("still there").remote.is_none(), "only the named vault");

        assert!(reg.set_remote("a", None), "clearing is the same write");
        assert!(reg.find("a").expect("still there").remote.is_none());

        assert!(!reg.set_remote("nope", None), "an unknown id is reported, not inserted");
        assert_eq!(reg.vaults.len(), 2);
    }

    /// The registry file is the destination's only home, so its round trip — and
    /// the "written before remotes existed" case — is what has to hold.
    #[test]
    fn registry_round_trips_with_and_without_a_remote() {
        let mut reg = VaultRegistry::default();
        reg.upsert(entry("a", "/a"));
        reg.set_remote("a", Some(RemoteConfig::folder("/mnt/usb/vault")));

        let text = serde_json::to_string(&reg).expect("serialises");
        let back: VaultRegistry = serde_json::from_str(&text).expect("parses back");
        assert_eq!(back.find("a").and_then(|v| v.remote.clone()), Some(RemoteConfig::folder("/mnt/usb/vault")));

        let old = r#"{"vaults":[{"id":"a","path":"/a","display_name":"A"}]}"#;
        let old: VaultRegistry = serde_json::from_str(old).expect("an older file still loads");
        assert!(old.find("a").expect("known").remote.is_none());
        assert!(old.find("a").expect("known").last_opened.is_none());
    }

    #[test]
    fn folder_name_never_empty() {
        assert_eq!(folder_name(Path::new("/home/c/notes")), "notes");
        assert!(!folder_name(Path::new("/")).is_empty());
    }
}
