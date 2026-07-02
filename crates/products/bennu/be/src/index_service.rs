//! The per-project **index lifecycle**: build the symbol index off the IPC thread,
//! cache the resulting native provider, and serve completion from it.
//!
//! On `bennu_open_project` the be layer calls [`IndexService::open`], which spawns a
//! **background std thread** to walk the project's `.java` sources, persist the
//! fst+rkyv index under `bennu_data_dir()`, and build a [`NativeJavaProvider`] over it
//! + the resolved JDK. The IPC dispatcher never blocks on the build (the CLAUDE.md
//! gotcha: framed-IPC work that can stall must not run on the dispatcher / a tokio
//! worker — a plain background thread is the cleanest fit here, since the build does no
//! reverse-channel round-trips). While the build runs, `bennu_completion` serves the
//! **empty** provider (the FE shows nothing gracefully); the moment the build lands,
//! the cached provider is swapped in and completion goes live.
//!
//! A single-file edit re-extracts just that file and patches the persisted index
//! ([`IndexService::patch_file`]), then rebuilds the (cheap) provider handle — no
//! whole-project re-parse.

use std::collections::HashMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use bennu_intel::prelude::{
    build_project_index, file_records_from_source, CompletionItem, IntelProvider,
    NativeJavaProvider, Position,
};

/// Where each project's index files live: `bennu_data_dir()/index/<hash-of-root>/`.
fn index_dir_for(root: &str) -> PathBuf {
    // A stable, filesystem-safe per-root directory name. A simple FNV-1a hash of the
    // absolute root keeps it short and collision-resistant enough for a local cache.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in root.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir().join("index").join(format!("{hash:016x}"))
}

/// One project's slot in the cache: the paths + JDK level it was opened with, plus the
/// hot-swappable provider the completion query reads.
struct ProjectSlot {
    root: PathBuf,
    index_dir: PathBuf,
    jdk_version: String,
    /// simple name → binary name for the project's own declared types (seeds the
    /// resolver so bare project-type names resolve). Rebuilt on patch.
    simple_names: Mutex<BTreeMap<String, String>>,
    /// The live provider. `Arc<NativeJavaProvider::new()>` (empty) until the first
    /// build completes, then swapped for the index-backed one.
    provider: RwLock<Arc<NativeJavaProvider>>,
}

/// The process-wide index service (one per `bennu-be`).
pub struct IndexService {
    slots: Mutex<HashMap<PathBuf, Arc<ProjectSlot>>>,
}

static SERVICE: OnceLock<IndexService> = OnceLock::new();

impl IndexService {
    /// The global service, created on first use.
    pub fn global() -> &'static IndexService {
        SERVICE.get_or_init(|| IndexService { slots: Mutex::new(HashMap::new()) })
    }

    /// Kick off (or restart) the index build for `root` at JDK `jdk_version`, on a
    /// background thread. Returns immediately; the provider goes live when the build
    /// finishes. Idempotent per root — a re-open rebuilds.
    pub fn open(&'static self, root: &str, jdk_version: &str) {
        let root_path = PathBuf::from(root);
        let index_dir = index_dir_for(root);
        let slot = Arc::new(ProjectSlot {
            root: root_path.clone(),
            index_dir: index_dir.clone(),
            jdk_version: jdk_version.to_string(),
            simple_names: Mutex::new(BTreeMap::new()),
            provider: RwLock::new(Arc::new(NativeJavaProvider::new())),
        });
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(root_path.clone(), slot.clone());

        let jdk_version = jdk_version.to_string();
        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&index_dir) {
                eprintln!("bennu-be: index dir {}: {e}", index_dir.display());
                return;
            }
            // Build + persist the project index.
            let (builder, types, members) = build_project_index(&root_path, &index_dir);
            if let Err(e) = builder.persist() {
                eprintln!("bennu-be: index persist failed: {e}");
                return;
            }
            eprintln!(
                "bennu-be: index built for {} ({types} types, {members} members)",
                root_path.display()
            );

            // Seed the project's own simple names for the resolver.
            let simple = bennu_intel::prelude::project_type_map(&root_path);
            let pairs: Vec<(String, String)> =
                simple.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            *slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()) = simple;

            // Build the index-backed provider and swap it in.
            match NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs) {
                Ok(p) => {
                    *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
                    eprintln!("bennu-be: completion live for {}", root_path.display());
                }
                Err(e) => eprintln!("bennu-be: provider build failed ({}): {e}", root_path.display()),
            }
        });
    }

    /// Serve completion at `file`:`offset` from the owning project's provider (matched
    /// by longest root prefix). Returns `[]` when no project owns the file, or its
    /// index is still building.
    pub fn completion(&self, file: &str, offset: usize) -> Vec<CompletionItem> {
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        let at = Position { file: file.to_string(), offset };
        provider.completion(&at).unwrap_or_default()
    }

    /// Incrementally patch one file after an edit: re-extract it, patch the persisted
    /// index, and rebuild the provider handle. `source == None` means the file was
    /// deleted. Runs synchronously (a single-file extract + persist is sub-ms — the
    /// disposable measured ~2 ms for a full project) but is cheap enough to call from a
    /// debounced save handler; move to a thread if a huge project ever makes it slow.
    ///
    /// Wired but not yet triggered: the Phase-1 contract has no edit/save wire method
    /// (the FE is built against `bennu_completion` / `bennu_open_project`). A future
    /// `bennu_did_change { file, text }` handler calls this; keeping the capability here
    /// means that wave is a one-line handler, not new plumbing.
    #[allow(dead_code)]
    pub fn patch_file(&self, file: &str, source: Option<&str>) {
        let Some(slot) = self.slot_for_file(file) else { return };
        let file_path = PathBuf::from(file);
        let simple = { slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()).clone() };

        // Rebuild the builder's per-file record set from the current project, then patch
        // this one file. (The builder is re-derived rather than held in the slot to keep
        // the slot small; the project walk is bounded and the persist is fast.)
        let (mut builder, _t, _m) = build_project_index(&slot.root, &slot.index_dir);
        let records = source.map(|src| {
            file_records_from_source(&file_path, src, &simple, u32::MAX / 2)
        });
        if let Err(e) = builder.patch_file(file_path, records) {
            eprintln!("bennu-be: index patch failed: {e}");
            return;
        }
        // Refresh the simple-name map + provider so the new members resolve.
        let new_simple = bennu_intel::prelude::project_type_map(&slot.root);
        let pairs: Vec<(String, String)> =
            new_simple.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        *slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()) = new_simple;
        if let Ok(p) = NativeJavaProvider::for_project(&slot.index_dir, &slot.jdk_version, &pairs) {
            *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
        }
    }

    /// The slot whose root is the longest prefix of `file`.
    fn slot_for_file(&self, file: &str) -> Option<Arc<ProjectSlot>> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let mut best: Option<&Arc<ProjectSlot>> = None;
        for slot in slots.values() {
            if Path::new(file).starts_with(&slot.root) {
                match best {
                    Some(b) if b.root.as_os_str().len() >= slot.root.as_os_str().len() => {}
                    _ => best = Some(slot),
                }
            }
        }
        best.cloned()
    }
}
