//! The script half's read cache: one repository, read once, decoded once.
//!
//! Opening a repository of install scripts is the expensive thing Picus does —
//! several hundred files off a network share, each decoded from windows-1252 —
//! and every consumer of the script half wants the same bytes: the tree view, the
//! inventory, the fourteen rules, the editor. Reading them once and holding the
//! decoded text is what makes the second question cheap.
//!
//! ## What is cached, and what is deliberately not
//!
//! **The decoded text**, keyed by path, each entry carrying the digest of the
//! bytes it came from. Not the parse: a `ParsedFile` is a map of a string the
//! caller owns (`picus-parse`'s invariant), so a parse cached beside its own
//! source is a self-referential struct — the parse is therefore produced inside
//! whichever call needs it, from one isolated function.
//!
//! **Not the raw bytes**, either. Everything that writes re-reads the file from
//! disk, because "what is on disk right now" is precisely the question an apply
//! has to answer; a cached copy would be the one thing it must not trust.
//!
//! ## Invalidation is by hand, and that is the decision
//!
//! Nothing here watches the filesystem. A snapshot lives until an explicit refresh
//! or a write replaces it. A cache that expired on its own would give two different
//! answers to the same question depending on when it was asked, and a consistency
//! report that changes while nobody has changed anything is a report people stop
//! believing.
//!
//! ## What a future on-disk tier has to implement
//!
//! Every entry is content-addressed by [`CachedSource::digest`], which is what a
//! persistent tier needs and the only thing it needs from here:
//!
//! 1. a store keyed by that digest — `get(&digest) -> Option<Vec<u8>>` and
//!    `put(&digest, bytes)`, under `picus_data_dir()`, exactly as Bennu's symbol
//!    index is laid out;
//! 2. a serialisation of the *parse* (`ParsedFile` is `Serialize`; it would need
//!    `Deserialize` too), stored under the digest of the source it maps;
//! 3. one change in `picus-be`: the isolated parse function consults the store
//!    before parsing and writes back after. No other call site knows.
//!
//! Nothing in this module has to change for that to happen, which is the point of
//! keying on a digest today rather than on a path and a timestamp.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use picus_project::prelude::{LineEnding, Project, ProjectConfig, ProposalNote};

/// One script file, read and decoded.
#[derive(Debug, Clone)]
pub struct CachedSource {
    /// Project-relative path, POSIX separators — the identity of a file
    /// everywhere in Picus, including on Windows.
    pub path: String,
    /// The decoded text. Every byte range the script half produces is an offset
    /// into *this* string, never into the bytes on disk.
    pub text: String,
    /// The label the bytes were decoded with (`windows-1252`, `UTF-8`).
    pub encoding: String,
    pub eol: LineEnding,
    /// [`crate::digest::digest`] of the bytes this text was decoded from.
    pub digest: String,
}

/// One repository, as it was when it was last read.
#[derive(Debug)]
pub struct ScriptSnapshot {
    /// Absolute root, in the platform's own form.
    pub root: PathBuf,
    pub project: Project,
    pub config: ProjectConfig,
    pub notes: Vec<ProposalNote>,
    /// `true` when the repository has no `project.toml` yet, i.e. what was read is
    /// a proposal awaiting confirmation.
    pub is_new: bool,
    /// Everything the user should look at: a naming pattern that will not compile,
    /// a marker placeholder that is always empty, a file that could not be read.
    /// Reported, never fatal — refusing to open leaves nowhere to fix it from.
    pub problems: Vec<String>,
    /// Decoded sources, keyed by project-relative path.
    pub sources: BTreeMap<String, CachedSource>,
}

impl ScriptSnapshot {
    pub fn source(&self, path: &str) -> Option<&CachedSource> {
        self.sources.get(path)
    }
}

/// The repositories read so far, keyed by [`cache_key`].
///
/// Handed out as `Arc` clones so a caller can parse and analyse a snapshot for as
/// long as it takes without holding the lock — the parse of a large repository is
/// seconds of work, and a lock held across it would stall every other handler.
#[derive(Default)]
pub struct ScriptCache {
    by_root: Mutex<HashMap<String, Arc<ScriptSnapshot>>>,
}

impl std::fmt::Debug for ScriptCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let open = self.by_root.lock().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("ScriptCache").field("repositories", &open).finish()
    }
}

impl ScriptCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The snapshot for a root, if one has been read.
    pub fn get(&self, root: &Path) -> Option<Arc<ScriptSnapshot>> {
        self.lock().get(&cache_key(root)).cloned()
    }

    /// Store a freshly read snapshot, replacing any previous one.
    pub fn put(&self, snapshot: Arc<ScriptSnapshot>) {
        let key = cache_key(&snapshot.root);
        self.lock().insert(key, snapshot);
    }

    /// Forget one repository — what a refresh and every write do.
    pub fn invalidate(&self, root: &Path) {
        self.lock().remove(&cache_key(root));
    }

    /// Forget everything.
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// How many repositories are held. For diagnostics.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<ScriptSnapshot>>> {
        self.by_root.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The key a root is held under.
///
/// Canonicalised where the path exists, so `C:\repo` and `C:\repo\` and a path
/// reached through a junction are one entry rather than three copies of the same
/// repository. A root that cannot be canonicalised (it was deleted between two
/// calls) falls back to its own text, which is still stable for that caller.
pub fn cache_key(root: &Path) -> String {
    std::fs::canonicalize(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| root.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_project::prelude::{EncodingSettings, NamingScheme, CURRENT_VERSION};

    fn snapshot(root: &str) -> Arc<ScriptSnapshot> {
        Arc::new(ScriptSnapshot {
            root: PathBuf::from(root),
            project: Project { name: "PROD".into(), root: root.into(), tree: Vec::new() },
            config: ProjectConfig {
                version: CURRENT_VERSION,
                name: "PROD".into(),
                encoding: EncodingSettings::default(),
                version_table: Default::default(),
                generation: Default::default(),
                naming: NamingScheme::default(),
                folders: Vec::new(),
                files: Vec::new(),
                aliases: Vec::new(),
                analysis: Default::default(),
                destination_sets: Vec::new(),
                products: Vec::new(),
            },
            notes: Vec::new(),
            is_new: false,
            problems: Vec::new(),
            sources: BTreeMap::new(),
        })
    }

    #[test]
    fn a_snapshot_survives_until_it_is_invalidated() {
        // Nothing expires on its own: the same question must get the same answer
        // until somebody asks for a re-read.
        let cache = ScriptCache::new();
        let root = std::env::temp_dir();
        cache.put(snapshot(&root.to_string_lossy()));
        assert!(cache.get(&root).is_some());
        assert_eq!(cache.len(), 1);

        cache.invalidate(&root);
        assert!(cache.get(&root).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn a_second_read_of_the_same_root_replaces_the_first() {
        let cache = ScriptCache::new();
        let root = std::env::temp_dir();
        cache.put(snapshot(&root.to_string_lossy()));
        let mut second = snapshot(&root.to_string_lossy());
        Arc::get_mut(&mut second).unwrap().problems.push("re-read".into());
        cache.put(second);

        assert_eq!(cache.len(), 1, "one repository, one entry");
        assert_eq!(cache.get(&root).unwrap().problems, ["re-read"]);
    }

    #[test]
    fn two_spellings_of_one_root_are_one_entry() {
        // A trailing separator is the spelling difference that actually happens,
        // and two entries for one repository would let a write invalidate the
        // copy nobody is reading.
        let cache = ScriptCache::new();
        let root = std::env::temp_dir();
        cache.put(snapshot(&root.to_string_lossy()));
        let with_separator = root.join("");
        assert!(cache.get(&with_separator).is_some());
    }
}
