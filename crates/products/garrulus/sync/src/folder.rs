//! [`FolderRemote`] — the vault mirrored to a plain directory.
//!
//! A USB stick, a network share, or a folder Drive/OneDrive/Dropbox already
//! syncs. It is not filler (`docs/garrulus-design.md` §4.1): it is how the whole
//! engine gets exercised without a network, it covers the "I already pay for
//! Drive" case, and — the real reason — it is what proves [`SyncRemote`] is not
//! secretly `git`.
//!
//! Concurrency is content-hash based against a manifest of the last synced
//! state, kept at `<vault>/.arbor/garrulus/folder-sync.manifest`. Delete the
//! manifest and every differing note reads as a concurrent edit — conservative
//! in the only direction that matters, since a concurrent edit costs a side
//! file and never a lost line.
//!
//! Two honest limitations, both documented rather than papered over:
//! **no history** (`capabilities.history = false`), and **no deletions** — a
//! note removed on one machine is not removed on the other, because a mirror
//! cannot tell "deleted here" from "created there" without a base, and guessing
//! deletes somebody's note.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::change::{ChangeBatch, RelPath};
use crate::conflict::{
    append_merge_daily, is_daily_note, is_side_file, merge_note, side_file_name, Conflict,
    ConflictStamp,
};
use crate::error::{SyncError, SyncResult};
use crate::files::{hash_file, hash_tree, read_note, write_note, MARKER_DIR};
use crate::remote::{
    PullOutcome, RemoteCapabilities, RemoteDescriptor, RemoteKind, Revision, SyncRemote,
};
use crate::run_blocking;
use crate::state::{classify, StateInputs, SyncState};

/// Where the last-synced state is remembered, under the vault's one dot-folder.
const MANIFEST_REL: &str = "garrulus/folder-sync.manifest";

/// A vault mirrored to a directory.
#[derive(Debug, Clone)]
pub struct FolderRemote {
    vault: PathBuf,
    mirror: PathBuf,
    device: String,
    daily_folder: Option<String>,
}

impl FolderRemote {
    /// Mirror `vault` to `mirror`, tagging conflicts with `device`.
    pub fn new(vault: impl Into<PathBuf>, mirror: impl Into<PathBuf>, device: impl Into<String>) -> Self {
        Self {
            vault: vault.into(),
            mirror: mirror.into(),
            device: device.into(),
            daily_folder: None,
        }
    }

    /// Tell the engine which folder holds daily notes, so those append-merge
    /// instead of conflicting (§4.4.5). Without it nothing is special-cased.
    pub fn with_daily_folder(mut self, folder: Option<String>) -> Self {
        self.daily_folder = folder;
        self
    }

    fn manifest_path(&self) -> PathBuf {
        self.vault.join(MARKER_DIR).join(MANIFEST_REL)
    }

    fn read_manifest(&self) -> SyncResult<BTreeMap<RelPath, u64>> {
        match fs::read_to_string(self.manifest_path()) {
            Ok(text) => Ok(parse_manifest(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(SyncError::Io(e)),
        }
    }

    fn write_manifest(&self, entries: &BTreeMap<RelPath, u64>) -> SyncResult<()> {
        let path = self.manifest_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, render_manifest(entries))?;
        Ok(())
    }

    fn require_mirror(&self) -> SyncResult<()> {
        if self.mirror.as_os_str().is_empty() {
            return Err(SyncError::NotConfigured("no mirror directory set".into()));
        }
        if !self.mirror.is_dir() {
            return Err(SyncError::Offline(format!(
                "mirror directory not available: {}",
                self.mirror.display()
            )));
        }
        Ok(())
    }

    fn probe_blocking(&self) -> SyncResult<SyncState> {
        if self.mirror.as_os_str().is_empty() {
            return Ok(SyncState::NoRemote);
        }
        if !self.mirror.is_dir() {
            return Ok(SyncState::Offline);
        }
        let manifest = self.read_manifest()?;
        let local = hash_tree(&self.vault)?;
        let remote = hash_tree(&self.mirror)?;
        let outgoing = count_changed(&local, &manifest);
        let incoming = count_changed(&remote, &manifest);
        let conflicts = local.keys().filter(|p| is_side_file(p)).count() as u32;
        Ok(classify(StateInputs {
            has_remote: true,
            reachable: true,
            dirty_notes: outgoing,
            conflicts,
            ahead_commits: 0,
            behind_commits: incoming,
        }))
    }

    fn pull_blocking(&self) -> SyncResult<PullOutcome> {
        self.require_mirror()?;
        let mut manifest = self.read_manifest()?;
        let local = hash_tree(&self.vault)?;
        let remote = hash_tree(&self.mirror)?;
        let stamp = ConflictStamp::now();
        let mut outcome = PullOutcome::default();

        for (rel, remote_hash) in &remote {
            if manifest.get(rel) == Some(remote_hash) {
                continue; // the mirror has not moved since the last sync
            }
            let local_hash = local.get(rel);
            let local_untouched = local_hash.is_none() || local_hash == manifest.get(rel);
            let vault_path = rel.to_path(&self.vault);
            let mirror_path = rel.to_path(&self.mirror);

            if local_untouched {
                let text = read_note(&mirror_path)?.unwrap_or_default();
                write_note(&vault_path, &text)?;
                outcome.applied.push(rel.clone());
                manifest.insert(rel.clone(), *remote_hash);
                continue;
            }

            // Both sides moved. There is no base — that is what a mirror costs.
            let local_text = read_note(&vault_path)?.unwrap_or_default();
            let remote_text = read_note(&mirror_path)?.unwrap_or_default();
            if is_daily_note(rel, self.daily_folder.as_deref()) {
                let merged = append_merge_daily(None, &local_text, &remote_text);
                write_note(&vault_path, &merged)?;
                outcome.applied.push(rel.clone());
            } else if let Some(merged) = merge_note(None, &local_text, &remote_text) {
                write_note(&vault_path, &merged)?;
                outcome.applied.push(rel.clone());
            } else {
                let side = side_file_name(rel, &self.device, stamp);
                write_note(&side.to_path(&self.vault), &remote_text)?;
                outcome.conflicts.push(Conflict {
                    path: rel.clone(),
                    base: None,
                    local: local_text,
                    remote: remote_text,
                    side_file: Some(side),
                });
            }
            // The mirror's version is now accounted for either way: what is in
            // the vault differs from it, so the note reads as outgoing until the
            // user sends it.
            manifest.insert(rel.clone(), *remote_hash);
        }

        self.write_manifest(&manifest)?;
        Ok(outcome)
    }

    fn push_blocking(&self, batch: &ChangeBatch) -> SyncResult<()> {
        self.require_mirror()?;
        let mut manifest = self.read_manifest()?;
        let local = hash_tree(&self.vault)?;

        let targets: Vec<RelPath> = if batch.is_empty() {
            local
                .iter()
                .filter(|(rel, h)| manifest.get(*rel) != Some(*h))
                .map(|(rel, _)| rel.clone())
                .collect()
        } else {
            batch.notes.clone()
        };

        for rel in targets {
            let from = rel.to_path(&self.vault);
            let to = rel.to_path(&self.mirror);
            let Some(text) = read_note(&from)? else { continue };
            write_note(&to, &text)?;
            if let Some(h) = hash_file(&to)? {
                manifest.insert(rel, h);
            }
        }
        self.write_manifest(&manifest)?;
        Ok(())
    }
}

#[async_trait]
impl SyncRemote for FolderRemote {
    fn descriptor(&self) -> RemoteDescriptor {
        RemoteDescriptor {
            id: self.mirror.to_string_lossy().to_string(),
            kind: RemoteKind::Folder,
            display: self
                .mirror
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.mirror.to_string_lossy().to_string()),
            capabilities: RemoteCapabilities {
                history: false,
                atomic_batch: false,
                conflicts: true,
            },
        }
    }

    async fn probe(&self) -> SyncResult<SyncState> {
        let me = self.clone();
        run_blocking(move || me.probe_blocking()).await
    }

    async fn pull(&self, _vault: &Path) -> SyncResult<PullOutcome> {
        let me = self.clone();
        run_blocking(move || me.pull_blocking()).await
    }

    async fn push(&self, _vault: &Path, batch: &ChangeBatch) -> SyncResult<()> {
        let me = self.clone();
        let batch = batch.clone();
        run_blocking(move || me.push_blocking(&batch)).await
    }

    async fn history(&self, _vault: &Path, _note: &RelPath) -> SyncResult<Vec<Revision>> {
        Err(SyncError::Unsupported("a mirror directory keeps no history"))
    }

    async fn revision(&self, _vault: &Path, _note: &RelPath, _rev: &str) -> SyncResult<String> {
        Err(SyncError::Unsupported("a mirror directory keeps no history"))
    }
}

/// How many entries differ from the manifest (or are absent from it).
fn count_changed(tree: &BTreeMap<RelPath, u64>, manifest: &BTreeMap<RelPath, u64>) -> u32 {
    tree.iter().filter(|(rel, h)| manifest.get(*rel) != Some(*h)).count() as u32
}

/// Parse the manifest: one `<hash-hex> <relative/path.md>` per line.
///
/// A hand-rollable line format rather than JSON, so the file stays diffable and
/// this crate stays free of a serialisation dependency it needs nowhere else.
/// Unparseable lines are dropped: a corrupt manifest must degrade into "assume
/// everything changed", never into a failed sync.
pub fn parse_manifest(text: &str) -> BTreeMap<RelPath, u64> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let Some((hash, path)) = line.split_once(' ') else { continue };
        let Ok(hash) = u64::from_str_radix(hash.trim(), 16) else { continue };
        if path.trim().is_empty() {
            continue;
        }
        out.insert(RelPath::new(path), hash);
    }
    out
}

/// Render the manifest. Sorted by path (a `BTreeMap`), so two machines produce
/// the same bytes for the same state.
pub fn render_manifest(entries: &BTreeMap<RelPath, u64>) -> String {
    let mut out = String::new();
    for (rel, hash) in entries {
        out.push_str(&format!("{hash:016x} {rel}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let mut m = BTreeMap::new();
        m.insert(RelPath::new("bugs/crash.md"), 0x1234u64);
        m.insert(RelPath::new("diario/2026-07-31.md"), u64::MAX);
        let text = render_manifest(&m);
        assert_eq!(
            text,
            "0000000000001234 bugs/crash.md\nffffffffffffffff diario/2026-07-31.md\n"
        );
        assert_eq!(parse_manifest(&text), m);
    }

    #[test]
    fn a_corrupt_manifest_degrades_to_empty_rather_than_failing() {
        let m = parse_manifest("spazzatura\n\nzzzz nota.md\ndeadbeef nota.md\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(&RelPath::new("nota.md")), Some(&0xdeadbeefu64));
    }

    #[test]
    fn changed_counts_new_and_differing_entries() {
        let mut manifest = BTreeMap::new();
        manifest.insert(RelPath::new("a.md"), 1u64);
        manifest.insert(RelPath::new("b.md"), 2u64);
        let mut tree = BTreeMap::new();
        tree.insert(RelPath::new("a.md"), 1u64); // untouched
        tree.insert(RelPath::new("b.md"), 9u64); // edited
        tree.insert(RelPath::new("c.md"), 3u64); // new
        assert_eq!(count_changed(&tree, &manifest), 2);
    }

    #[test]
    fn descriptor_admits_it_has_no_history() {
        let r = FolderRemote::new("/vault", "/mnt/usb/vault", "casa");
        let d = r.descriptor();
        assert_eq!(d.kind, RemoteKind::Folder);
        assert!(!d.capabilities.history);
        assert!(d.capabilities.conflicts);
    }
}
