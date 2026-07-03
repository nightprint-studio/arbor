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
//! The background build reads each `.java` **once** and shares the text between the symbol
//! index and the rename engine (no second disk pass), parses in parallel, and overlaps the
//! config-graph build with the rename-engine walk. It emits `arbor://bennu/index-progress`
//! events per phase so the FE can show a live "Indexing…" status. The class-navigator
//! entries fall out of the same parse and are cached on the slot (Go-to-Class is instant).
//!
//! A single-file edit re-extracts **just that file** and applies its records to the live
//! provider's in-memory overlay ([`IndexService::patch_file`]) — NO disk write, NO
//! provider rebuild, NO JDK re-resolve, NO whole-project re-walk on a keystroke. The
//! persisted `symbols.blob` / `names.fst` (which the provider **memory-maps** for its
//! lifetime) are only rewritten on a full build / reindex, and each such build persists
//! into a fresh **generation** subdir (`<base>/g<NNN>/`) then swaps the provider `Arc`, so
//! it never overwrites a file a live mmap still holds (Windows os error 1224).

use std::collections::HashMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use arbor_ipc::prelude::EventSink;
use bennu_index::prelude::Symbol;
use bennu_intel::prelude::{
    build_project_index_from_sources, file_records_from_source, ingest_config_graph,
    read_java_sources, ActionVerdict, CompletionItem, ConfigResolver, DeclarationLocation,
    HoverInfo as IntelHoverInfo, IntelProvider, NativeJavaProvider, Position, ReferencesResult,
    RenameEngine, RenamePlan,
};
use bennu_proto::prelude::{ClassEntry, DeclarationTarget, HoverInfo, IndexStats};
use serde_json::json;

use crate::web_discovery::discover_web_inputs;

/// The BE→FE index-progress topic. Payload:
/// `{ "root": <string>, "phase": <string>, "state": "start" | "end" }`, where `phase` is
/// one of `"project"`, `"references"`, `"config"` (start before / end after each build
/// phase) plus a terminal `{ "phase": "ready", "state": "end" }` once completion is live.
const EVT_INDEX_PROGRESS: &str = "arbor://bennu/index-progress";

/// Emit a single index-progress event (`start` / `end` of a phase) for `root`.
fn emit_progress(sink: &Arc<dyn EventSink>, root: &str, phase: &str, state: &str) {
    sink.emit(EVT_INDEX_PROGRESS, json!({ "root": root, "phase": phase, "state": state }));
}

/// The stable per-root **base** directory: `bennu_data_dir()/index/<hash-of-root>/`. The
/// actual index files live in a per-build **generation** subdir under this (see
/// [`gen_dir`]) so a rebuild never overwrites a file the live provider still has mmapped
/// (Windows os error 1224 — "user-mapped section open").
fn index_base_for(root: &str) -> PathBuf {
    // A stable, filesystem-safe per-root directory name. A simple FNV-1a hash of the
    // absolute root keeps it short and collision-resistant enough for a local cache.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in root.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    arbor_core::prelude::bennu_data_dir().join("index").join(format!("{hash:016x}"))
}

/// The generation subdir `<base>/g<NNN>` (zero-padded) that build number `gen` persists
/// into. A fresh `gen` per full build means the new `symbols.blob` / `names.fst` are
/// written to files NO live provider has mapped — the swap-then-drop-old ordering releases
/// the previous mmap before those old files are (best-effort) deleted.
fn gen_dir(base: &Path, gen: u64) -> PathBuf {
    base.join(format!("g{gen:03}"))
}

/// The `g<NNN>` gen number of an existing gen subdir name, if it parses. Used to pick the
/// next free number on open and to GC stale gens.
fn parse_gen(name: &str) -> Option<u64> {
    name.strip_prefix('g').and_then(|d| d.parse::<u64>().ok())
}

/// The next free generation number under `base`: one past the highest existing `g<NNN>`
/// (0 when none / the base doesn't exist yet). Keeps the counter monotonic across a
/// process restart, since a prior run's mapped files may still be on disk.
fn next_gen(base: &Path) -> u64 {
    let mut max: Option<u64> = None;
    if let Ok(rd) = std::fs::read_dir(base) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str().and_then(parse_gen) {
                max = Some(max.map_or(n, |m| m.max(n)));
            }
        }
    }
    max.map_or(0, |m| m + 1)
}

/// Best-effort delete every gen subdir under `base` except `keep`. A delete that fails
/// because the OS still has the file mapped (another live provider, or a not-yet-dropped
/// `Arc` on this process) is non-fatal — the dir is left for the next open's GC. Logged
/// once per stuck dir (debug-level) so a persistent leak is visible without spamming.
fn gc_old_gens(base: &Path, keep: u64) {
    let Ok(rd) = std::fs::read_dir(base) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(n) = name.to_str().and_then(parse_gen) else { continue };
        if n == keep {
            continue;
        }
        let p = e.path();
        if std::fs::remove_dir_all(&p).is_err() {
            // Still mapped elsewhere (expected on Windows for the just-swapped-out gen if
            // its Arc lingers) — leave it; the next open cleans it up.
        }
    }
}

/// One project's slot in the cache: the paths + JDK level it was opened with, plus the
/// hot-swappable provider the completion query reads.
struct ProjectSlot {
    root: PathBuf,
    /// The stable per-root base dir; each full build persists into a fresh `g<NNN>` subdir
    /// of this (never overwriting a mapped file — Windows os error 1224).
    index_base: PathBuf,
    /// The **current** generation dir the live provider/rename engine mmap from
    /// (`<index_base>/g<NNN>`). Updated on each full build; a per-keystroke patch does NOT
    /// touch disk (it updates the in-memory overlay), so this only moves on a full build.
    index_dir: RwLock<PathBuf>,
    jdk_version: String,
    /// simple name → binary name for the project's own declared types (seeds the
    /// resolver so bare project-type names resolve). Rebuilt on patch.
    simple_names: Mutex<BTreeMap<String, String>>,
    /// The "Go to Class" navigator entries, captured during the full build (same parse as
    /// the symbol index — no separate whole-project scan). Served by `bennu_class_index`
    /// instantly after the first index. Empty until the build lands; refreshed best-effort
    /// on patch.
    classes: RwLock<Vec<ClassEntry>>,
    /// The live provider. `Arc<NativeJavaProvider::new()>` (empty) until the first
    /// build completes, then swapped for the index-backed one.
    provider: RwLock<Arc<NativeJavaProvider>>,
    /// The config-graph resolver (Struts/Spring/Tiles), built off-thread on open when
    /// the project has web config. `None` until built (or for a project with no config).
    /// Drives `bennu_definition` (JSP action → class/view) + the JSP action-existence
    /// diagnostic.
    config: RwLock<Option<Arc<ConfigResolver>>>,
    /// The rename engine (whole-project reference index + resolver + source sets), built
    /// off-thread on open alongside the provider. `None` until built. Drives
    /// `bennu_rename_plan` / `bennu_rename_apply` (docs §5 #10-12) + `bennu_hover`.
    rename: RwLock<Option<Arc<RenameEngine>>>,
    /// Cached project-symbol counts from the last full build (0 until the build lands),
    /// surfaced by `bennu_index_stats` without re-walking the project.
    types: AtomicUsize,
    members: AtomicUsize,
    /// Whether the last build's provider swap-in completed (drives the `ready` flag).
    ready: AtomicBool,
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
    /// finishes. Idempotent per root — a re-open rebuilds. `sink` is the BE→FE event
    /// egress the background build emits `index-progress` events on.
    pub fn open(&'static self, root: &str, jdk_version: &str, sink: Arc<dyn EventSink>) {
        let root_path = PathBuf::from(root);
        let index_base = index_base_for(root);
        // A fresh generation dir for THIS build so its files are never the ones a prior
        // live provider still has mmapped (Windows os error 1224 on overwrite/truncate).
        let gen = next_gen(&index_base);
        let index_dir = gen_dir(&index_base, gen);
        let slot = Arc::new(ProjectSlot {
            root: root_path.clone(),
            index_base: index_base.clone(),
            index_dir: RwLock::new(index_dir.clone()),
            jdk_version: jdk_version.to_string(),
            simple_names: Mutex::new(BTreeMap::new()),
            classes: RwLock::new(Vec::new()),
            provider: RwLock::new(Arc::new(NativeJavaProvider::new())),
            config: RwLock::new(None),
            rename: RwLock::new(None),
            types: AtomicUsize::new(0),
            members: AtomicUsize::new(0),
            ready: AtomicBool::new(false),
        });
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(root_path.clone(), slot.clone());

        let jdk_version = jdk_version.to_string();
        let root_str = root.to_string();
        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&index_dir) {
                eprintln!("bennu-be: index dir {}: {e}", index_dir.display());
                return;
            }

            // ── phase "project": read every `.java` ONCE, then build + persist the symbol
            // index (parsing in parallel). The sources are shared with the rename engine
            // below — no second disk pass, and the class navigator + type map fall out of
            // the same parse (no separate whole-project scan).
            emit_progress(&sink, &root_str, "project", "start");
            let sources = read_java_sources(&root_path);
            let built = build_project_index_from_sources(&sources, &index_dir);
            if let Err(e) = built.builder.persist() {
                // A persist failure is logged ONCE and the build thread exits cleanly,
                // leaving the previous good provider (on the prior slot/gen) in place — no
                // retry loop, no corrupted slot.
                eprintln!("bennu-be: index persist failed: {e}");
                emit_progress(&sink, &root_str, "project", "end");
                return;
            }
            let (types, members) = (built.type_count, built.member_count);
            slot.types.store(types, Ordering::Relaxed);
            slot.members.store(members, Ordering::Relaxed);
            // Cache the class navigator entries (Go-to-Class is instant after this).
            *slot.classes.write().unwrap_or_else(|p| p.into_inner()) =
                built.classes.iter().map(class_entry_of).collect();
            // Seed the project's own simple names for the resolver.
            let simple = built.type_map;
            let pairs: Vec<(String, String)> =
                simple.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            *slot.simple_names.lock().unwrap_or_else(|p| p.into_inner()) = simple;
            eprintln!(
                "bennu-be: index built for {} ({types} types, {members} members)",
                root_path.display()
            );
            emit_progress(&sink, &root_str, "project", "end");

            // Build the index-backed provider and swap it in.
            match NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs) {
                Ok(p) => {
                    *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
                    slot.ready.store(true, Ordering::Relaxed);
                    eprintln!("bennu-be: completion live for {}", root_path.display());
                }
                Err(e) => eprintln!("bennu-be: provider build failed ({}): {e}", root_path.display()),
            }

            // The config-graph phase is independent of the rename engine, so overlap them:
            // config on its own thread while the rename engine's O(N) reference walk runs
            // here. Both are non-fatal.
            let config_handle = {
                let root_path = root_path.clone();
                let index_dir = index_dir.clone();
                let slot = slot.clone();
                let sink = sink.clone();
                let root_str = root_str.clone();
                std::thread::spawn(move || {
                    build_config_graph(&slot, &root_path, &index_dir, &sink, &root_str);
                })
            };

            // ── phase "references": whole-project reference index + resolver + source sets
            // for find-usages / rename. Reuses the already-read `sources` (shares text with
            // the symbol build — no second disk read). The O(N) walk runs here so
            // `bennu_rename_plan` is cheap. Non-fatal.
            emit_progress(&sink, &root_str, "references", "start");
            build_rename_engine(&slot, &root_path, &index_dir, &jdk_version, &pairs, &sources);
            emit_progress(&sink, &root_str, "references", "end");

            let _ = config_handle.join();

            // Everything for this gen is now swapped in (provider + rename + config all
            // point at `index_dir`). Best-effort GC of older gens: the previous gen's
            // files are only deletable once their provider/rename `Arc`s have dropped; a
            // still-mapped dir is left for the next open's GC (non-fatal on Windows).
            gc_old_gens(&index_base, gen);

            // Provider is live + engines built → the terminal "ready" event.
            emit_progress(&sink, &root_str, "ready", "end");
        });
    }

    /// Rebuild the index for an already-open project (by root), reusing the JDK level it
    /// was opened at. A no-op when no slot owns `root`. Called after a successful
    /// `bennu_build` so freshly-compiled `target/classes` output (and any source changes
    /// the build picked up) are reflected in completion. Returns immediately; the
    /// rebuild runs on the same background thread `open` uses.
    pub fn reindex(&'static self, root: &str, sink: Arc<dyn EventSink>) {
        let jdk = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(|s| s.jdk_version.clone())
        };
        if let Some(jdk) = jdk {
            self.open(root, &jdk, sink);
        }
    }

    /// The cached "Go to Class" navigator entries for the project rooted at `root`, or
    /// `None` when no slot owns `root` or its build hasn't captured them yet (the caller
    /// then falls back to a fresh scan). Instant after the first index — no re-parse.
    pub fn class_index(&self, root: &str) -> Option<Vec<ClassEntry>> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let slot = slots.get(&PathBuf::from(root))?;
        let cache = slot.classes.read().unwrap_or_else(|p| p.into_inner());
        if cache.is_empty() {
            return None; // build not landed yet → let the caller do a fresh scan
        }
        Some(cache.clone())
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

    /// Resolve a JSP form/link action reference to its go-to-definition target (the C1
    /// chain: the config fragment it's declared in + the resolved class FQCN + view JSP).
    /// `file` is any file inside the owning project (to pick the project's config).
    /// Returns `None` when no project owns the file, its config isn't built yet, or the
    /// action is unknown.
    pub fn definition_action(&self, file: &str, action_qname: &str) -> Option<ActionDefinition> {
        let cfg = self.config_for_file(file)?;
        let target = cfg.action_class_ref(action_qname)?;
        Some(ActionDefinition {
            config_file: target.config_file,
            class_fqcn: target.class_fqcn,
            view_jsp: target.view_jsp,
        })
    }

    /// The conservative "action inesistente" verdict for a JSP action reference.
    /// Returns `Inconclusive` (never `Missing`) whenever the config isn't built yet, so
    /// the FE never shows a false "missing" while the index is still loading.
    pub fn diagnose_action(&self, file: &str, action_qname: &str) -> ActionVerdict {
        match self.config_for_file(file) {
            Some(cfg) => cfg.diagnose_action(action_qname),
            None => ActionVerdict::Inconclusive { reason: "config not built".into() },
        }
    }

    /// Plan a rename for the symbol at `file`:`offset` → `new_name`, over the owning
    /// project's rename engine (built off-thread on open). `source` is the current
    /// (possibly-unsaved) buffer. Returns `None` when no project owns the file, the engine
    /// is still building, or the caret isn't on a renameable identifier.
    pub fn plan_rename(
        &self,
        file: &str,
        source: &str,
        offset: usize,
        new_name: &str,
    ) -> Option<RenamePlan> {
        let slot = self.slot_for_file(file)?;
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        }?;
        engine.plan(file, source, offset, new_name)
    }

    /// Find all usages of the symbol at `file`:`offset`, over the owning project's rename
    /// engine (which holds the whole-project reference index, built off-thread on open).
    /// `source` is the current (possibly-unsaved) buffer. Returns `None` when no project
    /// owns the file, the engine is still building, or the caret isn't on a referenceable
    /// symbol.
    pub fn find_usages(&self, file: &str, source: &str, offset: usize) -> Option<ReferencesResult> {
        let slot = self.slot_for_file(file)?;
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        }?;
        engine.find_usages(file, source, offset)
    }

    /// Resolve the symbol at `file`:`offset` to its DECLARATION site (go-to-declaration),
    /// over the owning project's rename engine (which shares the whole-project reference
    /// index + resolver + source sets, built off-thread on open). `source` is the current
    /// (possibly-unsaved) buffer. Returns `None` when no project owns the file, the engine
    /// is still building, the caret isn't on a resolvable symbol, or the declaration lives
    /// in a JDK / dep-jar (no project source to open). Mirrors [`find_usages`](Self::find_usages).
    pub fn declaration(&self, file: &str, source: &str, offset: usize) -> Option<DeclarationTarget> {
        let slot = self.slot_for_file(file)?;
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        }?;
        engine.declaration(file, source, offset).map(declaration_target_of)
    }

    /// Resolve the symbol at `file`:`offset` to a hover card, over the owning project's
    /// rename engine (which shares the whole-project reference index + resolver, built
    /// off-thread on open). `source` is the current (possibly-unsaved) buffer. Returns
    /// `None` when no project owns the file, the engine is still building, or the caret
    /// isn't on a symbol we can classify. Mirrors [`find_usages`](Self::find_usages).
    pub fn hover(&self, file: &str, source: &str, offset: usize) -> Option<HoverInfo> {
        let slot = self.slot_for_file(file)?;
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        }?;
        engine.hover(file, source, offset).map(hover_info_of)
    }

    /// A cheap snapshot of the index for the project rooted at `root` (the index
    /// inspector). Never errors: an unbuilt / unknown project reports zeros +
    /// `ready = false`. Symbol counts are read from the slot's cache (populated on the
    /// last full build) rather than re-walking the project; config counts come from the
    /// live [`ConfigResolver`] when built. `jar_count` is read cheaply from the cached
    /// `target/bennu-classpath.txt` (0 until a build/run has resolved the classpath — we
    /// never shell out to `mvn` here).
    pub fn index_stats(&self, root: &str) -> IndexStats {
        let root_path = PathBuf::from(root);
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&root_path).map(Arc::clone)
        };
        let Some(slot) = slot else {
            // No project open at this root — an all-zero, not-ready snapshot.
            return IndexStats {
                types: 0,
                members: 0,
                jdk_version: String::new(),
                jar_count: 0,
                actions: 0,
                beans: 0,
                relations: 0,
                ready: false,
            };
        };

        let (actions, beans, relations) = {
            let g = slot.config.read().unwrap_or_else(|p| p.into_inner());
            match g.as_ref() {
                Some(cfg) => (cfg.action_count(), cfg.bean_count(), cfg.relation_count()),
                None => (0, 0, 0),
            }
        };

        IndexStats {
            types: slot.types.load(Ordering::Relaxed),
            members: slot.members.load(Ordering::Relaxed),
            jdk_version: slot.jdk_version.clone(),
            // Cheap: count the dep jars from the cached `target/bennu-classpath.txt` that a
            // prior build/run wrote — no `mvn` shell-out here. 0 when the file is absent
            // (no build-classpath has run yet).
            jar_count: cached_jar_count(&slot.root),
            actions,
            beans,
            relations,
            ready: slot.ready.load(Ordering::Relaxed),
        }
    }

    /// The config resolver of the project owning `file`, if built.
    fn config_for_file(&self, file: &str) -> Option<Arc<ConfigResolver>> {
        let slot = self.slot_for_file(file)?;
        let g = slot.config.read().unwrap_or_else(|p| p.into_inner());
        g.as_ref().map(Arc::clone)
    }

    /// Incrementally patch one file after an edit: re-extract JUST that file and apply its
    /// records to the live provider's **in-memory overlay** — NO disk write, NO provider
    /// rebuild, NO JDK re-resolve. `source == None` means the file was deleted. Runs
    /// synchronously off the IPC read loop (the serve loop dispatches each request on its
    /// own thread), triggered by the debounced `bennu_did_change { file, text }` handler.
    ///
    /// Why memory-only: the persisted `symbols.blob` / `names.fst` are **memory-mapped** by
    /// the live provider for its whole lifetime. Overwriting/truncating them here — as a
    /// re-persist would — fails on Windows with os error 1224 ("user-mapped section open")
    /// on every keystroke, and re-resolving the JDK classpath (reopening rt.jar's ZIP dir)
    /// pegs the CPU. Instead the resolver keeps an interior-mutable overlay: this patch
    /// updates the edited file's types in that overlay so completion reflects the edit
    /// immediately, while the mmap'd files stay untouched until the next full build /
    /// reindex (which persists into a fresh generation dir and swaps in a new provider,
    /// clearing the overlay). The rename engine is NOT rebuilt here (its O(N) reference
    /// walk stays a full-build cost) — an unsaved edit's find-usages/rename runs against
    /// the current buffer over the last engine, the documented preview-first behavior.
    pub fn patch_file(&self, file: &str, source: Option<&str>) {
        let Some(slot) = self.slot_for_file(file) else { return };

        // A config (`.xml`) edit changes the config graph, not the Java index → rebuild
        // the config resolver rather than mis-parsing XML as Java. (Cheap: bounded walk +
        // parse.)
        if is_xml_config(file) {
            self.rebuild_config(&slot);
            return;
        }
        if !is_java(file) {
            return; // nothing to re-index for this file kind
        }

        let file_path = PathBuf::from(file);

        // Update the project-wide simple→binary map from the edited file's OWN type decls
        // (a renamed/added/removed type in THIS file), without re-scanning the project.
        // Cross-file type references still resolve — every other file's types are already
        // in the map from the last full build.
        let simple = {
            let mut guard = slot.simple_names.lock().unwrap_or_else(|p| p.into_inner());
            merge_file_types(&mut guard, &file_path, source);
            guard.clone()
        };

        // Re-extract only this file's records (its `Symbol`s carry the resolved members)
        // and apply them to the live provider's in-memory overlay — no disk, no rebuild.
        // A delete (`source == None`) applies an empty record set, which drops the file's
        // prior overlay entries. Keyed by the FE `file` string so the overlay's per-file
        // rename/remove bookkeeping matches on the next edit.
        let symbols: Vec<Symbol> = source
            .map(|src| {
                file_records_from_source(&file_path, src, &simple, u32::MAX / 2)
                    .into_iter()
                    .map(|r| r.symbol)
                    .collect()
            })
            .unwrap_or_default();
        {
            let provider = {
                let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
                Arc::clone(&g)
            };
            provider.apply_file_patch(file, &symbols);
        }

        // Refresh the class navigator cache for THIS file (best-effort): drop its old
        // entries and re-add from the fresh parse, so Go-to-Class reflects a rename.
        refresh_class_cache_for_file(&slot, &file_path, source);
    }

    /// Re-parse + re-ingest the project's config graph (after a struts/spring/tiles XML
    /// edit), swapping the live [`ConfigResolver`] in. Non-fatal on failure.
    fn rebuild_config(&self, slot: &Arc<ProjectSlot>) {
        let inputs = discover_web_inputs(&slot.root);
        if inputs.struts_roots.is_empty() && inputs.spring_files.is_empty() {
            return;
        }
        let (graph, _report) = bennu_web::prelude::build_web_graph(&inputs);
        // Config `config-*` files are read back into OWNED memory (no lingering mmap), so
        // re-ingesting into the current gen dir is safe to overwrite. Snapshot the path.
        let index_dir = slot.index_dir.read().unwrap_or_else(|p| p.into_inner()).clone();
        match ingest_config_graph(&graph, &index_dir) {
            Ok(cfg) => {
                *slot.config.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(cfg));
            }
            Err(e) => eprintln!("bennu-be: config rebuild failed: {e}"),
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

/// Build + swap in the rename engine for `slot`: reuse the already-read `.java` `sources`
/// (shared with the symbol-index build — no second disk pass) and read the Spring `.xml`
/// fragments (the only XML that can carry `<bean class=>`), then build the whole-project
/// reference index + resolver. Non-fatal on failure (rename then just returns "still
/// building"). Runs on the index background thread.
fn build_rename_engine(
    slot: &Arc<ProjectSlot>,
    root: &Path,
    index_dir: &Path,
    jdk_version: &str,
    simple_names: &[(String, String)],
    sources: &[(PathBuf, String)],
) {
    // Reuse the shared sources (path normalized to forward slashes to match FE file keys) —
    // this is the second consumer of the single disk read done in `open`.
    let java: Vec<(String, String)> =
        sources.iter().map(|(p, s)| (norm_path(p), s.clone())).collect();

    // Spring bean XML fragments (any `.xml` with a `<beans` root) — the class-rename
    // config-aware edit target set.
    let xml: Vec<(String, String)> = discover_web_inputs(root)
        .spring_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (norm_path(p), s)))
        .collect();

    match RenameEngine::for_project(index_dir, jdk_version, simple_names, java, xml) {
        Ok(engine) => {
            *slot.rename.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(engine));
            eprintln!("bennu-be: rename engine live for {}", root.display());
        }
        Err(e) => eprintln!("bennu-be: rename engine build failed ({}): {e}", root.display()),
    }
}

/// Build + ingest the project's config graph (Struts/Spring/Tiles) and swap the resolver
/// in, emitting the `config` progress phase around it. Non-fatal: a project with no config
/// (or a failed ingest) just gets no [`ConfigResolver`]. Runs on its own thread, overlapped
/// with the rename-engine build.
fn build_config_graph(
    slot: &Arc<ProjectSlot>,
    root: &Path,
    index_dir: &Path,
    sink: &Arc<dyn EventSink>,
    root_str: &str,
) {
    emit_progress(sink, root_str, "config", "start");
    let inputs = discover_web_inputs(root);
    if inputs.struts_roots.is_empty() && inputs.spring_files.is_empty() {
        emit_progress(sink, root_str, "config", "end");
        return; // no web config — nothing to ingest
    }
    let (graph, report) = bennu_web::prelude::build_web_graph(&inputs);
    if !report.unresolved_includes.is_empty() {
        eprintln!(
            "bennu-be: {} unresolved config include(s) (jar-resident, non-fatal)",
            report.unresolved_includes.len()
        );
    }
    match ingest_config_graph(&graph, index_dir) {
        Ok(cfg) => {
            let (a, b, r) = (cfg.action_count(), cfg.bean_count(), cfg.relation_count());
            *slot.config.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(cfg));
            eprintln!(
                "bennu-be: config graph live for {} ({a} actions, {b} beans, {r} edges)",
                root.display()
            );
        }
        Err(e) => eprintln!("bennu-be: config ingest failed: {e}"),
    }
    emit_progress(sink, root_str, "config", "end");
}

/// Map an intel [`bennu_intel::prelude::ClassDecl`] onto the wire [`ClassEntry`].
fn class_entry_of(d: &bennu_intel::prelude::ClassDecl) -> ClassEntry {
    ClassEntry { fqcn: d.fqcn.clone(), simple: d.simple.clone(), file: d.file.clone(), line: d.line }
}

/// Merge the edited `file`'s OWN current type declarations into the project-wide
/// simple→binary `map`, so an added/renamed type in this file resolves immediately. Avoids
/// a whole-project re-scan on each keystroke — every OTHER file's types are already in the
/// map from the last full build. A removed type's stale simple→binary entry (rare) is
/// corrected on the next full reindex; the common add-member / rename-type edit is exact.
/// A delete (`source == None`) contributes nothing new.
fn merge_file_types(map: &mut BTreeMap<String, String>, _file: &Path, source: Option<&str>) {
    if let Some(src) = source {
        for td in bennu_java::prelude::extract_symbols(src).types {
            map.insert(td.name, td.fqn.replace('.', "/"));
        }
    }
}

/// Refresh the class-navigator cache for a single edited `file`: drop its prior entries and
/// re-add from the fresh parse (a rename/add/remove of a top-level type shows up in
/// Go-to-Class without a full rebuild). Best-effort; a parse miss just leaves the cache as-is
/// for that file until the next full index.
fn refresh_class_cache_for_file(slot: &Arc<ProjectSlot>, file: &Path, source: Option<&str>) {
    let file_key = file.to_string_lossy().replace('\\', "/");
    let mut cache = slot.classes.write().unwrap_or_else(|p| p.into_inner());
    cache.retain(|c| c.file != file_key);
    if let Some(src) = source {
        let fs = bennu_java::prelude::extract_symbols(src);
        for td in &fs.types {
            cache.push(ClassEntry {
                fqcn: td.fqn.clone(),
                simple: td.name.clone(),
                file: file_key.clone(),
                line: decl_line_of(src, &td.name),
            });
        }
    }
}

/// The 1-based declaration line of type `name` in `source`, defaulting to 1. Mirrors the
/// intel navigator's recovery (the extractor carries no offset) — kept minimal here for the
/// single-file patch refresh.
fn decl_line_of(source: &str, name: &str) -> usize {
    for (idx, line) in source.lines().enumerate() {
        // A standalone `class|interface|enum <name>` token on this line.
        for kw in ["class", "interface", "enum"] {
            if let Some(pos) = line.find(kw) {
                let after = line[pos + kw.len()..].trim_start();
                if after.starts_with(name) {
                    let tail = &after[name.len().min(after.len())..];
                    let bounded =
                        tail.chars().next().map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);
                    if bounded {
                        return idx + 1;
                    }
                }
            }
        }
    }
    1
}

/// Count the dependency jars on the project's classpath **cheaply**, from the
/// `target/bennu-classpath.txt` file a prior `bennu_run` / build-classpath resolve wrote
/// (keyed by the Maven `-Dmdep.outputFile` the resolver uses). Only entries that exist on
/// disk and end in `.jar` are counted (unresolved / private-repo entries are excluded, like
/// [`bennu_classpath::prelude::MavenClasspath::resolved_count`]). Returns 0 when the file is
/// absent — we never shell out to `mvn` here (that would stall the inspector for seconds);
/// the real count materialises once the user has built/run the project.
fn cached_jar_count(root: &Path) -> usize {
    let cp_file = root.join("target").join("bennu-classpath.txt");
    let Ok(raw) = std::fs::read_to_string(&cp_file) else {
        return 0;
    };
    split_classpath_entries(&raw)
        .into_iter()
        .filter(|e| {
            let p = Path::new(e);
            e.to_ascii_lowercase().ends_with(".jar") && p.is_file()
        })
        .count()
}

/// Split an OS-separated classpath string into entries. Windows uses `;` (unambiguous); a
/// `:` list (Unix) must not split on a `<letter>:\`/`<letter>:/` drive-prefix colon. Mirrors
/// `bennu_classpath`'s internal splitter (kept local — that one isn't part of its prelude).
fn split_classpath_entries(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.contains(';') {
        return raw.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' {
            continue;
        }
        let prev_is_letter = i >= 1 && bytes[i - 1].is_ascii_alphabetic();
        let letter_at_entry_start = i == start + 1;
        let next_is_slash = i + 1 < bytes.len() && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'/');
        if prev_is_letter && letter_at_entry_start && next_is_slash {
            continue; // drive letter, not a separator
        }
        let e = raw[start..i].trim();
        if !e.is_empty() {
            out.push(e.to_string());
        }
        start = i + 1;
    }
    let e = raw[start..].trim();
    if !e.is_empty() {
        out.push(e.to_string());
    }
    out
}

/// Map an intel [`IntelHoverInfo`] onto the wire [`HoverInfo`] (field-for-field). Kept in
/// the be layer so the wire mapping lives at the process boundary (like `references`).
fn hover_info_of(h: IntelHoverInfo) -> HoverInfo {
    HoverInfo { signature: h.signature, kind: h.kind, container: h.container, doc: h.doc }
}

/// Map an intel [`DeclarationLocation`] onto the wire [`DeclarationTarget`] (field-for-field).
/// Kept in the be layer so the wire mapping lives at the process boundary (like `references`).
fn declaration_target_of(d: DeclarationLocation) -> DeclarationTarget {
    DeclarationTarget {
        file: d.file,
        start: d.start,
        end: d.end,
        line: d.line,
        col: d.col,
        label: d.label,
    }
}

/// Normalize a path to forward slashes (the FE keys files by forward-slash paths).
fn norm_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Whether `file` is a `.java` source (re-indexed into the Java symbol index on edit).
fn is_java(file: &str) -> bool {
    Path::new(file).extension().and_then(|e| e.to_str()) == Some("java")
}

/// Whether `file` is an `.xml` config fragment (a struts/spring/tiles edit → rebuild the
/// config graph on save rather than mis-parsing it as Java).
fn is_xml_config(file: &str) -> bool {
    Path::new(file).extension().and_then(|e| e.to_str()) == Some("xml")
}

/// A resolved go-to-definition target for a JSP action reference (the be-layer view of
/// [`bennu_intel::prelude::ActionTarget`]).
#[derive(Debug, Clone)]
pub struct ActionDefinition {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// The resolved implementation class FQCN (the C1 chain), if resolvable.
    pub class_fqcn: Option<String>,
    /// The resolved view JSP (the Tiles chain), if resolvable.
    pub view_jsp: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gen_dir_is_zero_padded_subdir_of_base() {
        let base = Path::new("/data/index/abc");
        assert_eq!(gen_dir(base, 0), base.join("g000"));
        assert_eq!(gen_dir(base, 7), base.join("g007"));
        assert_eq!(gen_dir(base, 42), base.join("g042"));
        // Above 3 digits still formats (no truncation) so the counter never collides.
        assert_eq!(gen_dir(base, 1234), base.join("g1234"));
    }

    #[test]
    fn parse_gen_round_trips_and_rejects_non_gen() {
        assert_eq!(parse_gen("g000"), Some(0));
        assert_eq!(parse_gen("g007"), Some(7));
        assert_eq!(parse_gen("g1234"), Some(1234));
        assert_eq!(parse_gen("gabc"), None);
        assert_eq!(parse_gen("000"), None); // no `g` prefix
        assert_eq!(parse_gen("symbols.blob"), None);
        assert_eq!(parse_gen(""), None);
    }

    #[test]
    fn next_gen_is_one_past_highest_existing() {
        let base = std::env::temp_dir()
            .join(format!("bennu-nextgen-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&base);
        // No base dir yet → gen 0.
        assert_eq!(next_gen(&base), 0);
        // Create g000 + g003 (+ a non-gen sibling that must be ignored) → next is 4.
        std::fs::create_dir_all(gen_dir(&base, 0)).unwrap();
        std::fs::create_dir_all(gen_dir(&base, 3)).unwrap();
        std::fs::create_dir_all(base.join("not-a-gen")).unwrap();
        assert_eq!(next_gen(&base), 4);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn gc_old_gens_keeps_current_and_removes_others() {
        let base = std::env::temp_dir()
            .join(format!("bennu-gcgen-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&base);
        for g in [0u64, 1, 2] {
            std::fs::create_dir_all(gen_dir(&base, g)).unwrap();
        }
        // A non-gen sibling must be left untouched (only `g<NNN>` dirs are GC candidates).
        std::fs::create_dir_all(base.join("keepme")).unwrap();
        gc_old_gens(&base, 2);
        assert!(!gen_dir(&base, 0).exists(), "g000 removed");
        assert!(!gen_dir(&base, 1).exists(), "g001 removed");
        assert!(gen_dir(&base, 2).exists(), "current gen kept");
        assert!(base.join("keepme").exists(), "non-gen sibling untouched");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn splits_windows_classpath_keeping_drive_letters() {
        let raw = r"C:\a\x.jar;C:\b\y.jar";
        let e = split_classpath_entries(raw);
        assert_eq!(e, vec![r"C:\a\x.jar".to_string(), r"C:\b\y.jar".to_string()]);
    }

    #[test]
    fn splits_unix_classpath_and_drive_entries() {
        assert_eq!(split_classpath_entries("/m2/a.jar:/m2/b.jar").len(), 2);
        // A `:`-joined list of Windows drive entries must not split on the drive colon.
        let e = split_classpath_entries(r"C:\a\x.jar:C:\b\y.jar");
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], r"C:\a\x.jar");
    }

    #[test]
    fn empty_and_blank_entries_dropped() {
        assert!(split_classpath_entries(";;  ;").is_empty());
        assert!(split_classpath_entries("   ").is_empty());
    }

    #[test]
    fn cached_jar_count_zero_when_file_absent() {
        let dir = std::env::temp_dir().join("bennu-no-classpath-xyz");
        assert_eq!(cached_jar_count(&dir), 0);
    }

    #[test]
    fn merge_file_types_adds_edited_files_types() {
        let mut map = BTreeMap::new();
        map.insert("Existing".to_string(), "p/Existing".to_string());
        let src = "package a;\npublic class Order { }\ninterface Repo { }\n";
        merge_file_types(&mut map, Path::new("/proj/a/Order.java"), Some(src));
        // The edited file's types are added; the pre-existing (other-file) entry is kept.
        assert_eq!(map.get("Order").map(String::as_str), Some("a/Order"));
        assert_eq!(map.get("Repo").map(String::as_str), Some("a/Repo"));
        assert_eq!(map.get("Existing").map(String::as_str), Some("p/Existing"));
    }

    #[test]
    fn merge_file_types_delete_is_noop_addition() {
        let mut map = BTreeMap::new();
        map.insert("Keep".to_string(), "p/Keep".to_string());
        merge_file_types(&mut map, Path::new("/proj/a/Gone.java"), None);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("Keep").map(String::as_str), Some("p/Keep"));
    }

    #[test]
    fn decl_line_of_locates_and_bounds() {
        let src = "package a;\n\npublic class Order {\n}\n";
        assert_eq!(decl_line_of(src, "Order"), 3);
        // `Foo` must not match `FooBar`'s declaration — falls back to 1.
        assert_eq!(decl_line_of("class FooBar {}\n", "Foo"), 1);
        assert_eq!(decl_line_of("class FooBar {}\n", "FooBar"), 1);
    }

    #[test]
    fn class_entry_of_maps_fields() {
        let d = bennu_intel::prelude::ClassDecl {
            fqcn: "com.acme.Order".into(),
            simple: "Order".into(),
            file: "/proj/Order.java".into(),
            line: 7,
        };
        let e = class_entry_of(&d);
        assert_eq!(e.fqcn, "com.acme.Order");
        assert_eq!(e.simple, "Order");
        assert_eq!(e.file, "/proj/Order.java");
        assert_eq!(e.line, 7);
    }
}
