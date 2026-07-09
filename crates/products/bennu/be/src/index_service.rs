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
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::{EventSink, HostCaller};
use bennu_index::prelude::Symbol;

use crate::jobs::JobHandle;
use bennu_intel::prelude::{
    build_project_index_from_sources, collect_annotation_beans, file_records_from_source,
    ingest_config_graph, read_java_sources, ActionVerdict, CompletionItem, ConfigResolver,
    DeclarationLocation, HoverInfo as IntelHoverInfo, IntelProvider, NativeJavaProvider,
    NonCompliantSource, Position, ProjectSources, ReferencesResult, RenameEngine, RenamePlan,
};
use bennu_query::prelude::InheritedMember as IntelInheritedMember;
use bennu_project::prelude::source_encoding_label;
use bennu_proto::prelude::{
    ClassEntry, DeclarationTarget, EncodingIssue, FileDiagnostics, FileValidationStat, HoverInfo,
    IndexEntry, IndexStats, InheritedMember, InheritedSource, JdkStatus, UsageHit,
    ValidationContext,
};
use bennu_web::prelude::{file_stamp, IncludeGraph, IncludeGraphCache};
use serde_json::json;

use crate::web_discovery::{discover_jsp_files, discover_web_inputs};

/// The BE→FE index-progress topic. Payload:
/// `{ "root": <string>, "phase": <string>, "state": "start" | "end" }`, where `phase` is
/// one of `"project"`, `"references"`, `"config"` (start before / end after each build
/// phase) plus a terminal `{ "phase": "ready", "state": "end" }` once completion is live.
const EVT_INDEX_PROGRESS: &str = "arbor://bennu/index-progress";

/// Emitted when a "Download sources" fetch finishes. Payload `{ "path": <decompiled-tab path>,
/// "ok": <bool> }` — the FE clears the tab's download spinner and, on `ok`, reloads it from disk
/// (its content flipped from stub to real source).
const EVT_SOURCES_READY: &str = "arbor://bennu/sources-ready";

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

/// Where a decompiled-from-bytecode stub for `binary` is cached: `bennu_data_dir()/decompiled/<pkg
/// dirs>/<Simple>.java`. Laying it out by package + simple name (not the raw binary) keeps the file
/// stem matching the declared type, so the editor opens it as a normal `.java` (no name-mismatch
/// noise). `$`-nested types collapse to their innermost simple name (a rare cosmetic case).
/// Whether `file` is a generated decompiled-from-bytecode stub (under `bennu_data_dir()/decompiled/`).
/// Such files are read-only JDK/dependency views, never validated. `Path::starts_with` matches
/// component-wise, so a forward-slashed FE path and the native data dir still compare correctly.
fn is_decompiled_stub(file: &str) -> bool {
    let root = arbor_core::prelude::bennu_data_dir().join("decompiled");
    Path::new(file).starts_with(&root)
}

fn decompiled_cache_path(binary: &str) -> PathBuf {
    let simple = binary.rsplit(['/', '$']).next().unwrap_or(binary);
    let mut path = arbor_core::prelude::bennu_data_dir().join("decompiled");
    if let Some((pkg, _)) = binary.rsplit_once('/') {
        for seg in pkg.split('/').filter(|s| !s.is_empty()) {
            path = path.join(seg);
        }
    }
    path.join(format!("{simple}.java"))
}

/// The OUTER (enclosing) binary name for a source view — `java/util/Map$Entry` → `java/util/Map`.
/// Real source lives in the enclosing compilation unit, so the cached file is named after the type
/// that declares it (not the inner class). A no-`$` name is returned unchanged.
fn outer_binary(binary: &str) -> String {
    binary.split('$').next().unwrap_or(binary).to_string()
}

/// Write a source view's `text` to its cache file (named after `file_binary`), rewriting only when
/// missing/changed (a warm view opens instantly and keeps a stable mtime). Returns the
/// forward-slashed path, or `None` on an I/O failure.
fn write_view(file_binary: &str, text: &str) -> Option<String> {
    let path = decompiled_cache_path(file_binary);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let fresh = std::fs::read_to_string(&path).map(|c| c == text).unwrap_or(false);
    if !fresh {
        std::fs::write(&path, text).ok()?;
    }
    Some(path.to_string_lossy().replace('\\', "/"))
}

/// The byte offset to land on in a served source `text`: the member declaration's name token (for a
/// member access), else the type declaration's name, else the top of the file. `file_binary` names
/// the owning type. Member-precise landing is best-effort — an inherited member not declared in
/// `text` (declared in a supertype) falls back to the type declaration.
fn member_jump_offset(
    text: &str,
    file_binary: &str,
    member: Option<&bennu_intel::prelude::LibraryMember>,
) -> usize {
    use bennu_intel::prelude::{find_member_name_span, find_type_name_span, DeclKey};
    if let Some(m) = member {
        let key = if m.is_field {
            DeclKey::Field { owner: file_binary.to_string(), name: m.name.clone() }
        } else {
            DeclKey::Method { owner: file_binary.to_string(), name: m.name.clone() }
        };
        if let Some((start, _)) = find_member_name_span(text, &key) {
            return start;
        }
    }
    let simple = file_binary.rsplit(['/', '$']).next().unwrap_or(file_binary);
    find_type_name_span(text, simple).map(|(s, _)| s).unwrap_or(0)
}

/// The result of resolving a library/JDK type to an on-disk source view (real source or a decompiled
/// stub) — the wire shape behind go-to-into-a-library-class and the "Download sources" banner.
#[derive(Debug, Clone)]
pub struct DecompiledView {
    /// Absolute (forward-slashed) path of the cached `.java` view.
    pub file: String,
    /// Byte offset to jump to (currently the top of the file).
    pub offset: usize,
    /// `true` when a signatures-only stub was served for a third-party dependency — the FE then
    /// offers "Download sources". `false` for real source (JDK / already-downloaded dep) or a JDK
    /// stub (no downloadable artifact).
    pub can_download: bool,
}

/// The serialized-schema version of the persisted JDK member index. **Bump this whenever the shape
/// of the decoded `ClassMembers`/`Member` changes** (a new field like `throws` or `type_params`, a
/// renamed field, …). It's part of the cache filename, so a bump makes every project re-decode the
/// JDK from bytecode instead of loading a stale memo whose new fields would `serde(default)` to empty
/// — the reason, e.g., method `throws` clauses went missing from decompiled stubs after `throws` was
/// added without invalidating the old cache.
///   v2: added `Member::throws` + `ClassMembers::type_params`.
const JDK_INDEX_SCHEMA: u32 = 2;

/// The **shared, cross-session** JDK member-index path for `jdk_version`:
/// `bennu_data_dir()/jdk-index/v<schema>-<major>-<hash-of-home>.json`. Keyed by the schema version +
/// the RESOLVED JDK (home + major) — not the requested version — so every project that resolves to
/// the same JDK reuses one memo, two different JDKs never share, and a schema change invalidates old
/// memos. `None` when no JDK resolves (the index then stays in-memory, re-parsing as today).
fn jdk_index_path(jdk_version: &str) -> Option<PathBuf> {
    let status = bennu_classpath::prelude::jdk_status(jdk_version);
    let home = status.resolved_home?;
    let major = status.resolved_major.unwrap_or(0);
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in home.to_string_lossy().as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(
        arbor_core::prelude::bennu_data_dir()
            .join("jdk-index")
            .join(format!("v{JDK_INDEX_SCHEMA}-{major}-{hash:016x}.json")),
    )
}

/// A stable, filesystem-safe FNV-1a hash of an absolute root — the per-root cache dir name.
fn root_hash(root: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in root.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// The on-disk include-graph cache file for `root`:
/// `bennu_data_dir()/include-graph/<hash-of-root>.json`. A small JSON (per-file edge lists) that
/// warm-starts the form analysis across app restarts — a corrupt/missing file just means a cold
/// (full) rebuild, never an error.
fn include_cache_path(root: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("include-graph")
        .join(format!("{}.json", root_hash(root)))
}

/// Load the persisted include-graph cache for `root`, rebuilding its derived graph. `None` on a
/// missing / unreadable / stale-format file (→ the caller starts cold).
fn load_include_cache(root: &str) -> Option<IncludeGraphCache> {
    let bytes = std::fs::read(include_cache_path(root)).ok()?;
    let mut cache: IncludeGraphCache = serde_json::from_slice(&bytes).ok()?;
    cache.rebuild_after_load();
    Some(cache)
}

/// Persist the include-graph cache for `root` (best-effort — a write failure is non-fatal, the
/// in-memory cache still serves this session).
fn save_include_cache(root: &str, cache: &IncludeGraphCache) {
    let path = include_cache_path(root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec(cache) {
        let _ = std::fs::write(path, bytes);
    }
}

/// The current JSP set of `root` as `(path, mtime, size)` triples (a full discovery walk + stat).
fn jsp_stamps(root: &str) -> Vec<(PathBuf, u64, u64)> {
    discover_jsp_files(Path::new(root))
        .into_iter()
        .map(|p| {
            let (m, s) = file_stamp(&p);
            (p, m, s)
        })
        .collect()
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

/// Best-effort delete every gen subdir under `base` **strictly older** than `keep`. Newer gens
/// (`>= keep`) are left alone: gen numbers are monotonic, so a higher one is a CONCURRENT open's
/// in-progress build — deleting it would pull the directory out from under that build's persist
/// (the reported `os error 3`). A delete that fails because the OS still has the file mapped
/// (another live provider, or a not-yet-dropped `Arc`) is non-fatal — the dir is left for the next
/// open's GC.
fn gc_old_gens(base: &Path, keep: u64) {
    let Ok(rd) = std::fs::read_dir(base) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(n) = name.to_str().and_then(parse_gen) else { continue };
        if n >= keep {
            continue; // the current gen, or a newer concurrent build's — never GC it
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
    /// The project's declared source encoding (Maven `sourceEncoding`, else the config
    /// default) — every source is decoded in this for indexing, and a file whose bytes don't
    /// fit lands in `non_compliant`. Held so `reindex` re-uses it.
    encoding_label: String,
    /// Source files whose bytes weren't valid in `encoding_label` (recovered + indexed, but
    /// flagged) — captured on each full build, served by `bennu_encoding_report` for a future
    /// "non-compliant files" UI.
    non_compliant: RwLock<Vec<EncodingIssue>>,
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
    /// The dependency jars the resolver actually loaded for this project (absolute paths,
    /// forward slashes) — the index's own dependency tier, NOT the Build's `target/` artifact.
    /// Set on each build when Maven dep resolution succeeds (empty for a dep-less project or a
    /// failed resolve). Read by the index inspector's Jars stat/list so the count reflects what
    /// completion/validation resolve against, independent of whether `mvn` re-ran this session.
    dep_jars: RwLock<Vec<String>>,
    /// Cached project-symbol counts from the last full build (0 until the build lands),
    /// surfaced by `bennu_index_stats` without re-walking the project.
    types: AtomicUsize,
    members: AtomicUsize,
    /// Whether the last build FULLY completed — provider + reference walk + config graph all
    /// swapped in. Set only at the end of the build thread (NOT when completion first goes
    /// live), so `index_stats.ready` — which the FE's "Indexing" card finishes on — stays
    /// false through the O(N) reference walk and the References-index step remains visible.
    ready: AtomicBool,
}

/// The process-wide index service (one per `bennu-be`).
pub struct IndexService {
    slots: Mutex<HashMap<PathBuf, Arc<ProjectSlot>>>,
    /// Per-project include-graph cache (keyed by forward-slashed root) for the form analysis —
    /// avoids re-parsing every JSP on each tab switch. Loaded from disk on first use, refreshed
    /// incrementally, persisted back. Behind an inner `Mutex` so a build holds the lock without
    /// blocking OTHER projects' caches.
    include_caches: Mutex<HashMap<String, Arc<Mutex<IncludeGraphCache>>>>,
    /// Roots that already got a full include-graph freshness pass THIS process run — the first
    /// analysis per root does a full sync (a warm disk cache may be stale after external edits),
    /// subsequent ones only refresh the focus file.
    include_synced: Mutex<HashSet<String>>,
    /// Monotonic per-root **build generation** for SUPERSESSION. Each [`open`](Self::open) bumps the
    /// root's counter and hands the build thread its own gen; a thread that finds a NEWER gen has
    /// started for the same root bails — it stops the (expensive) remaining work AND suppresses its
    /// terminal `ready` event. Without this, a rebuild launched while a prior build is still warming
    /// up lets the OLD thread's stale `ready` close the NEW build's progress card early, leaving the
    /// new warm-up running unnotified ("card done but CPU still at ~70%"). Separate from the on-disk
    /// `g<NNN>` gen dir counter (which is filesystem-collision-safe across process restarts).
    build_gen: Mutex<HashMap<PathBuf, u64>>,
    /// Per-file **out-of-code-block** validation cache (keyed by the FE file path): the diagnostics of
    /// each method / constructor body, so re-validating the live buffer after an edit re-runs only the
    /// body that changed. Threaded through [`IndexService::validate_java`] (the resolved tier); the
    /// whole-project run doesn't use it. Taken out for the duration of a validation and put back after,
    /// so concurrent validations of the SAME file simply fall back to a full run (never a wrong result).
    incremental: Mutex<HashMap<PathBuf, bennu_check::prelude::IncrementalCache>>,
    /// Buffer-edit counters that make an incremental cache safe across files: `total` bumps on every
    /// Java `patch_file`, and `per_file[f]` counts f's own patches. A file A's "resolver revision" is
    /// `build_gen[root]` combined with `total − per_file[A]` = the number of edits to OTHER files since
    /// A was last edited — so editing A never invalidates A's own cache (its structure is captured
    /// separately), but editing another file A depends on does.
    patch_counts: Mutex<PatchCounts>,
    /// The reverse channel back to the shell, set from a handler's [`BennuState`] on project open
    /// (bennu is one window per backend, so a single host is stable). Used by the background analysis
    /// warm-up to register itself as a tracked JOB in the shell's registry (→ the bennu Jobs overlay).
    /// `None` until the first open, or in the (unused) in-process path — the warm-up then runs
    /// untracked.
    host: RwLock<Option<Arc<dyn HostCaller>>>,
    /// Per-project pool of opened dependency `-sources.jar`s (keyed by forward-slashed root), for the
    /// decompiled-tab "go to source" — consulted for REAL library source before falling back to a
    /// stub. Built lazily on first library go-to (only the sources jars already on disk), and cleared
    /// after a "Download sources" fetch so the freshly-downloaded jar is picked up without a reindex.
    dep_sources: Mutex<HashMap<String, Arc<Vec<bennu_classpath::prelude::JavaSourceZip>>>>,
}

/// Buffer-edit bookkeeping for [`IndexService::patch_counts`] — see that field.
#[derive(Default)]
struct PatchCounts {
    total: u64,
    per_file: HashMap<PathBuf, u64>,
}

/// Resolve the source encoding label to index a project at `root` in: a per-project config
/// override wins, else the pom's declared `sourceEncoding`, else the config default. Shared by
/// project-open and the fresh-scan handlers (`bennu_class_index` / `bennu_main_classes`) so a
/// legacy tree is decoded identically everywhere.
pub fn resolve_index_encoding(root: &str) -> String {
    let cfg = bennu_core::config::load();
    cfg.encoding_overrides
        .get(root)
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| source_encoding_label(Path::new(root), &cfg.default_encoding))
}

static SERVICE: OnceLock<IndexService> = OnceLock::new();

impl IndexService {
    /// The global service, created on first use.
    pub fn global() -> &'static IndexService {
        SERVICE.get_or_init(|| IndexService {
            slots: Mutex::new(HashMap::new()),
            include_caches: Mutex::new(HashMap::new()),
            include_synced: Mutex::new(HashSet::new()),
            build_gen: Mutex::new(HashMap::new()),
            incremental: Mutex::new(HashMap::new()),
            patch_counts: Mutex::new(PatchCounts::default()),
            host: RwLock::new(None),
            dep_sources: Mutex::new(HashMap::new()),
        })
    }

    /// Attach (or refresh) the reverse channel back to the shell, so the background analysis warm-up
    /// can register itself as a tracked job. Called from the open / reindex handlers, which hold the
    /// [`BennuState`]. Cheap and idempotent — bennu is one window per backend.
    pub fn set_host(&self, host: Option<Arc<dyn HostCaller>>) {
        *self.host.write().unwrap_or_else(|p| p.into_inner()) = host;
    }

    /// A clone of the reverse channel, if wired. `None` before the first open / in-process.
    fn host(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The include graph for the project at `root`, served from the incremental cache. Cheap on
    /// the common per-tab path: the cache is built once (a full walk on the FIRST call per root
    /// this run, warm-started from disk), then only `focus`'s edges are refreshed on later calls.
    /// `force_full` (the Refresh button) triggers a full freshness pass — the way a newly-added
    /// file or a newly-added parent include is picked up. Returns a snapshot clone (so the
    /// analysis reads files without holding the cache lock).
    pub fn include_graph(&self, root: &str, focus: &str, force_full: bool) -> IncludeGraph {
        let cell = {
            let mut caches = self.include_caches.lock().unwrap();
            caches
                .entry(root.to_string())
                .or_insert_with(|| {
                    // Warm-start from the persisted cache if present, else start cold.
                    Arc::new(Mutex::new(load_include_cache(root).unwrap_or_default()))
                })
                .clone()
        };

        // First analysis for this root this run → force a full sync even off a warm disk cache
        // (the project may have changed externally since it was written).
        let first_this_run = self.include_synced.lock().unwrap().insert(root.to_string());
        let want_full = force_full || first_this_run;

        let mut cache = cell.lock().unwrap();
        let mut changed = false;
        if want_full || cache.is_empty() {
            changed = cache.sync(&jsp_stamps(root));
        } else {
            let (m, s) = file_stamp(Path::new(focus));
            if cache.refresh_file(Path::new(focus), m, s) {
                cache.commit();
                changed = true;
            }
        }
        let graph = cache.graph().clone();
        if changed {
            save_include_cache(root, &cache);
        }
        graph
    }

    /// Kick off (or restart) the index build for `root` at JDK `jdk_version`, decoding sources
    /// in `encoding_label` (the project's Maven `sourceEncoding`), on a background thread.
    /// Returns immediately; the provider goes live when the build finishes. Idempotent per
    /// root — a re-open rebuilds. `sink` is the BE→FE event egress the background build emits
    /// `index-progress` events on.
    pub fn open(
        &'static self,
        root: &str,
        jdk_version: &str,
        encoding_label: &str,
        sink: Arc<dyn EventSink>,
    ) {
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
            encoding_label: encoding_label.to_string(),
            non_compliant: RwLock::new(Vec::new()),
            simple_names: Mutex::new(BTreeMap::new()),
            classes: RwLock::new(Vec::new()),
            provider: RwLock::new(Arc::new(NativeJavaProvider::new())),
            config: RwLock::new(None),
            rename: RwLock::new(None),
            dep_jars: RwLock::new(Vec::new()),
            types: AtomicUsize::new(0),
            members: AtomicUsize::new(0),
            ready: AtomicBool::new(false),
        });
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(root_path.clone(), slot.clone());

        // Claim a fresh build generation for this root: a later `open` (a rebuild, or a re-open)
        // bumps it again, and THIS thread bails the moment it sees a newer gen — so a superseded
        // build never burns CPU behind the newer one nor emits a stale terminal `ready`.
        let my_gen = {
            let mut g = self.build_gen.lock().unwrap_or_else(|p| p.into_inner());
            let n = g.entry(root_path.clone()).or_insert(0);
            *n += 1;
            *n
        };

        let svc: &'static IndexService = self;
        let jdk_version = jdk_version.to_string();
        let encoding_label = encoding_label.to_string();
        let root_str = root.to_string();
        // The reverse channel for the analysis warm-up's tracked job (registered inside the thread).
        let host = self.host();
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
            let ProjectSources { sources, non_compliant } =
                read_java_sources(&root_path, &encoding_label);
            // Record the files that weren't valid in the declared encoding (recovered, not
            // dropped) for the encoding report — visible, never a silent skip.
            if !non_compliant.is_empty() {
                eprintln!(
                    "bennu-be: {} source file(s) not valid in declared encoding {} for {}",
                    non_compliant.len(),
                    encoding_label,
                    root_path.display()
                );
            }
            *slot.non_compliant.write().unwrap_or_else(|p| p.into_inner()) =
                non_compliant.iter().map(encoding_issue_of).collect();
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

            // A newer build (a rebuild / re-open) already superseded this one → stop NOW: skip the
            // Maven resolve, the provider build, the O(N) reference walk, and the warm-up entirely,
            // so a stale thread never burns CPU behind the newer build nor emits the terminal `ready`
            // that would close the new build's progress card early.
            if svc.superseded(&root_path, my_gen) {
                eprintln!(
                    "bennu-be: build gen {my_gen} for {} superseded — bailing after project phase",
                    root_path.display()
                );
                return;
            }

            // Resolve the project's dependency jars (Maven `~/.m2`, cached across sessions by pom
            // mtime) so validation + completion resolve LIBRARY types (Spring, servlet, Hibernate,
            // Struts, …), not just the JDK + project. Non-fatal and lazy: a dep-less project, a
            // missing `mvn`, or a resolve failure yields `None` and the provider degrades to JDK +
            // project. The decoded members are memoized to a per-project file; the jar list itself is
            // disk-cached, so `mvn dependency:build-classpath` runs at most once per pom.
            emit_progress(&sink, &root_str, "dependencies", "start");
            let deps = crate::dep_classpath::resolve_dep_classpath(&root_path, &jdk_version).map(|d| {
                eprintln!(
                    "bennu-be: dependency classpath resolved for {} ({} jars)",
                    root_path.display(),
                    d.jars.len()
                );
                // Record the resolved jars on the slot so the index inspector's Jars count reflects
                // what the resolver loaded — independent of whether `mvn` re-ran (the jar list is
                // disk-cached, so a cached open skips mvn but still indexes the same jars).
                *slot.dep_jars.write().unwrap_or_else(|p| p.into_inner()) = d.jars;
                (d.source, d.memo_path)
            });
            emit_progress(&sink, &root_str, "dependencies", "end");

            // Build the index-backed provider and swap it in. The JDK member index is persistent
            // (shared across projects/sessions, keyed by the resolved JDK), so JDK classes are
            // parsed from bytecode at most once ever; the dependency tier (when present) is memoized
            // per-project.
            match NativeJavaProvider::for_project(&index_dir, &jdk_version, &pairs, jdk_index_path(&jdk_version), deps) {
                Ok(p) => {
                    *slot.provider.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(p);
                    // NB: completion is live here, but the index is NOT fully built yet — the
                    // O(N) reference walk still runs below. `slot.ready` (→ `index_stats.ready`,
                    // which the FE poll finishes the "Indexing" card on) is set only at the very
                    // end, so the References-index step stays visibly active through the walk.
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
                // Share the already-read `.java` sources with the config build so the
                // annotation-bean collection reuses them (no third disk walk). Cloned because
                // the rename-engine build below also borrows `sources` concurrently.
                let sources = sources.clone();
                std::thread::spawn(move || {
                    build_config_graph(&slot, &root_path, &index_dir, &sources, &sink, &root_str);
                })
            };

            // ── phase "references": whole-project reference index + resolver + source sets
            // for find-usages / rename. Reuses the already-read `sources` (shares text with
            // the symbol build — no second disk read). The O(N) walk runs here so
            // `bennu_rename_plan` is cheap. Non-fatal.
            emit_progress(&sink, &root_str, "references", "start");
            build_rename_engine(&slot, &root_path, &index_dir, &jdk_version, &pairs, &sources, &sink, &root_str);
            emit_progress(&sink, &root_str, "references", "end");

            let _ = config_handle.join();

            // Everything for this gen is now swapped in (provider + rename + config all
            // point at `index_dir`). Best-effort GC of older gens: the previous gen's
            // files are only deletable once their provider/rename `Arc`s have dropped; a
            // still-mapped dir is left for the next open's GC (non-fatal on Windows).
            gc_old_gens(&index_base, gen);

            // A rebuild landed during the reference walk → this gen is stale; the newer build owns
            // the progress card and will emit its own `ready`. Bail before marking ready.
            if svc.superseded(&root_path, my_gen) {
                eprintln!(
                    "bennu-be: build gen {my_gen} for {} superseded — bailing before ready",
                    root_path.display()
                );
                return;
            }

            // Index + engines are swapped in and completion / navigation are ALREADY live → mark the
            // project READY NOW, BEFORE the (minutes-long on a big project) whole-project validation
            // warm-up. The editor must be usable immediately; project-wide validation is a background
            // citizen that catches up after, exactly like IntelliJ's daemon (editor first, analysis
            // streams in). Gating `ready` on the warm-up made a 1.3k-file project feel like a 2-minute
            // boot for zero interactive benefit — completion / go-to don't depend on it, and per-file
            // diagnostics on open are served from (or lazily fill) the same cache regardless.
            slot.ready.store(true, Ordering::Relaxed);
            emit_progress(&sink, &root_str, "ready", "end");

            // Whole-project VALIDATION warm-up (opt-in `validate_on_open`, default on) — now a pure
            // BACKGROUND pass AFTER `ready`, on this same build thread. It pre-fills the persisted
            // diagnostic cache (so opening a file + the explicit "Validate" are instant) and warms the
            // persistent JDK / dependency member cache as a side effect. Skipped when the project is
            // byte-for-byte unchanged since the last warm-up (the persisted cache is still valid) or
            // when a rebuild has already superseded this gen. Cancellable: a rebuild bumps the gen and
            // this thread stops. Leaves ~2 cores free. The `validation` progress phase is still emitted
            // but is harmless — the FE has already latched `ready` and drops non-`ready` events — and
            // is the hook for a future non-blocking "analyzing project…" chip.
            if bennu_core::config::load().validate_on_open
                && !svc.superseded(&root_path, my_gen)
                && !svc.warmup_up_to_date(&root_str, &sources)
            {
                // Track the warm-up as a JOB in the shell registry so it shows in the bennu Jobs
                // overlay (a background citizen, self-purged shortly after it completes). `None` when
                // no reverse channel is wired — the warm-up still runs, just untracked.
                let job = host.as_ref().and_then(|h| register_warmup_job(h, &sink, &root_str));
                emit_progress(&sink, &root_str, "validation", "start");
                warm_up_validation_cache(&root_str, &sources);
                // Only stamp "up to date" when this gen is still current — a warm-up superseded
                // mid-run must not record a marker the winning build would trust and then skip its
                // own warm-up over possibly-different sources.
                if !svc.superseded(&root_path, my_gen) {
                    svc.record_warmup_stamp(&root_str, &sources);
                }
                emit_progress(&sink, &root_str, "validation", "end");
                finish_warmup_job(&sink, job);
            }

            // Diagnostic (idle-CPU investigation): confirms the build thread reaches its end and
            // exits. If bennu-be still burns CPU after this line logs, the spinner is NOT this thread.
            eprintln!("bennu-be: index build thread exiting for {}", root_path.display());
        });
    }

    /// Whether a NEWER build has been started for `root` since the caller's build began (`my_gen`) —
    /// the bail signal for a superseded build thread. Bailing stops the remaining (expensive) work
    /// and, crucially, suppresses the terminal `ready` event, so an old thread finishing after a
    /// rebuild can't close the new build's progress card (leaving its warm-up running unnotified).
    fn superseded(&self, root: &Path, my_gen: u64) -> bool {
        self.build_gen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(root)
            .is_some_and(|&g| g != my_gen)
    }

    /// Rebuild the index for an already-open project (by root), reusing the JDK level it
    /// was opened at. A no-op when no slot owns `root`. Called after a successful
    /// `bennu_build` so freshly-compiled `target/classes` output (and any source changes
    /// the build picked up) are reflected in completion. Returns immediately; the
    /// rebuild runs on the same background thread `open` uses.
    pub fn reindex(&'static self, root: &str, sink: Arc<dyn EventSink>) {
        let opened_with = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots
                .get(&PathBuf::from(root))
                .map(|s| (s.jdk_version.clone(), s.encoding_label.clone()))
        };
        if let Some((jdk, encoding_label)) = opened_with {
            // A manual rebuild is authoritative: drop the incremental reference cache so the
            // reopen re-walks every file from scratch (not just the changed ones), and the
            // diagnostic cache so a fresh full validation runs (the classpath may have changed).
            let base = index_base_for(root);
            bennu_intel::prelude::clear_ref_cache(&bennu_intel::prelude::ref_cache_path(&base));
            bennu_intel::prelude::clear_diag_cache(&bennu_intel::prelude::diag_cache_path(&base));
            self.open(root, &jdk, &encoding_label, sink);
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

    /// The source files that weren't valid in the project's declared encoding (recovered +
    /// indexed, but flagged) for the project rooted at `root`. Empty when no slot owns `root`,
    /// the build hasn't landed, or every file was compliant. Served by `bennu_encoding_report`.
    pub fn encoding_issues(&self, root: &str) -> Vec<EncodingIssue> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let Some(slot) = slots.get(&PathBuf::from(root)) else {
            return Vec::new();
        };
        // Bind to a local so the RwLockReadGuard temporary drops here (`;`), before `slots`
        // does — otherwise the guard would outlive the borrow it holds into `slots`.
        let issues = slot.non_compliant.read().unwrap_or_else(|p| p.into_inner()).clone();
        issues
    }

    /// How the JDK resolved for the project at `root` — the exact-vs-fallback / none status
    /// the FE surfaces as a titlebar warning or a Problems entry. `None` when no slot owns
    /// `root`. The FS probe runs AFTER the slots lock is released (never holds it across IO).
    pub fn jdk_report(&self, root: &str) -> Option<JdkStatus> {
        let jdk_version = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(|s| s.jdk_version.clone())
        }?;
        Some(jdk_status_of(bennu_classpath::prelude::jdk_status(&jdk_version)))
    }

    /// Serve completion at `file`:`offset` from the owning project's provider (matched
    /// by longest root prefix). Returns `[]` when no project owns the file, or its
    /// index is still building.
    pub fn completion(&self, file: &str, offset: usize, source: Option<&str>) -> Vec<CompletionItem> {
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        let at = Position { file: file.to_string(), offset };
        provider.completion(&at, source).unwrap_or_default()
    }

    /// Validate a Java `file` over its owning project's provider (AST checks + the resolver-backed
    /// unknown-member check when the index is built). `source` is the live buffer. Falls back to the
    /// pure AST checks when no project owns the file / its index isn't built yet.
    ///
    /// `resolved` picks the tier: `false` runs ONLY the fast pure-AST checks (syntax / structure /
    /// unused imports — cheap, no type inference), `true` runs the full resolver-backed pass on top.
    /// The FE fires the fast tier on a short debounce and the full tier on a longer idle debounce, so
    /// a large file paints syntax squiggles immediately while the ~0.7s semantic pass catches up.
    pub fn validate_java(&self, file: &str, source: &str, resolved: bool) -> Vec<bennu_proto::prelude::Diagnostic> {
        // A decompiled-from-bytecode stub — a JDK / dependency class opened via Ctrl+B — is a
        // signature-only, body-less, read-only view. Running the validator on it only produces useless
        // noise (missing returns for body-less methods, unresolved references to types we didn't
        // decompile, …), so skip validation on it entirely.
        if is_decompiled_stub(file) {
            return Vec::new();
        }
        let path = Path::new(file);
        let file_stem = path.file_stem().and_then(|s| s.to_str()).map(str::to_string);
        // Expected package from the file's location under its source root (`src/main/java/...`).
        let expected_package = path.parent().and_then(bennu_java::prelude::infer_package);

        let Some(slot) = self.slot_for_file(file) else {
            // No owning project: still run every source-only check (no target version to gate on).
            let ctx = bennu_check::prelude::FileContext {
                file_stem,
                expected_package,
                java_major: None,
                classpath_complete: false,
            };
            return bennu_check::prelude::check_file(source, &ctx);
        };
        let status = bennu_classpath::prelude::jdk_status(&slot.jdk_version);
        let ctx = bennu_check::prelude::FileContext {
            file_stem,
            expected_package,
            java_major: status.requested_major,
            // The dependency classpath is known-complete only when Maven resolved its jars (recorded
            // on the slot). Absent → the unresolved-import check adjudicates only `java.*`.
            classpath_complete: !slot.dep_jars.read().unwrap_or_else(|p| p.into_inner()).is_empty(),
        };
        // Fast tier: only the pure-AST checks — no provider / resolver / inference. This is the cheap
        // pass the FE fires on a short debounce for instant syntax squiggles on a big file.
        if !resolved {
            return bennu_check::prelude::check_file(source, &ctx);
        }
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        // Full tier: reuse the per-file out-of-code-block cache so a re-validation after typing inside
        // one method re-runs only that method's body. Take the cache out for the run and put it back
        // after — a concurrent validation of the SAME file just finds none and does a full run (still
        // correct). Keyed by the FE `file` string, exactly as `patch_file` counts edits.
        let file_key = PathBuf::from(file);
        let resolver_rev = self.resolver_rev_for(&slot.root, &file_key);
        let mut cache = {
            let mut g = self.incremental.lock().unwrap_or_else(|p| p.into_inner());
            g.remove(&file_key).unwrap_or_default()
        };
        let out = provider.validate_incremental(source, &ctx, status.any_installed, resolver_rev, &mut cache);
        {
            let mut g = self.incremental.lock().unwrap_or_else(|p| p.into_inner());
            g.insert(file_key, cache);
        }
        out
    }

    /// The "resolver revision" for `file` — combined with the per-body text hash + the structural hash,
    /// it gates reuse of the out-of-code-block cache. It changes when the project index rebuilds
    /// (`build_gen`) OR when a file OTHER than `file` is edited (`total − per_file[file]`), so a cached
    /// body is never replayed against a type that has since moved. Editing `file` itself does NOT
    /// change it (that file's own structure is captured by the cache's structural hash).
    fn resolver_rev_for(&self, root: &Path, file: &Path) -> u64 {
        let build = self
            .build_gen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(root)
            .copied()
            .unwrap_or(0);
        let cross = {
            let g = self.patch_counts.lock().unwrap_or_else(|p| p.into_inner());
            g.total.wrapping_sub(g.per_file.get(file).copied().unwrap_or(0))
        };
        build.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(cross)
    }

    /// Validate every file in `files` (`(path, source)`) for a WHOLE-PROJECT run, in PARALLEL,
    /// consulting the read-only diagnostic `cache`. Each file is either served from the cache (its
    /// recorded project dependencies re-checked against the live resolver — own bytes unchanged,
    /// every project type it read still has the same members, every bare name still resolves the
    /// same, every absent name still absent) or re-validated while recording fresh dependencies.
    /// The caller folds the results back into the cache (`deps` is `Some` only for a re-validated
    /// file). Returns one [`BatchResult`] per input file, in input order.
    ///
    /// Parallel is safe here: the pass is read-only over the cache + the `Arc`-shared resolver
    /// (whose internals are `RwLock`-guarded) and dependency recording is thread-local, so each
    /// worker records its own file with no shared state. The work-stealing pool leaves ~2 cores
    /// free for the interactive path (completion / go-to), so a background warm-up never pegs the
    /// machine. When no resolver is built for the project, files are validated pure-AST and nothing
    /// is cached.
    pub fn validate_project_batch(
        &self,
        root: &str,
        files: &[(PathBuf, String)],
        cache: &bennu_intel::prelude::DiagCache,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Vec<BatchResult> {
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(Arc::clone)
        };
        let (provider, jdk_available, classpath_complete, requested_major) = match slot {
            Some(slot) => {
                let status = bennu_classpath::prelude::jdk_status(&slot.jdk_version);
                let provider = {
                    let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
                    Arc::clone(&g)
                };
                // Deps known-complete only when Maven resolved jars (recorded on the slot).
                let complete = !slot.dep_jars.read().unwrap_or_else(|p| p.into_inner()).is_empty();
                (provider, status.any_installed, complete, status.requested_major)
            }
            // No project owns this root — validate pure-AST over an empty provider, no caching.
            None => (Arc::new(NativeJavaProvider::new()), false, false, None),
        };
        validate_files_parallel(&provider, jdk_available, classpath_complete, requested_major, files, cache, on_progress)
    }

    /// Request cancellation of the currently-running whole-project validation sweep (the FE's Cancel
    /// on the "Validating…" operation card). The parallel loop stops doing real work and drains the
    /// rest cheaply; the run then discards its partial results instead of persisting them. A no-op
    /// when nothing is validating (the next run clears the flag on start).
    pub fn request_cancel_validation(&self) {
        CANCEL_VALIDATION.store(true, Ordering::Release);
    }

    /// Whether a cancel was requested for the in-flight validation run — the caller checks this after
    /// the batch to skip persisting a partial result.
    pub fn validation_cancelled(&self) -> bool {
        CANCEL_VALIDATION.load(Ordering::Acquire)
    }

    /// Whether the project at `root` has a built resolver (so the diagnostic cache can check
    /// freshness). The background warm-up skips validation until this is true — validating pure-AST
    /// with nothing to cache would be wasted work on every open.
    pub fn has_resolver(&self, root: &str) -> bool {
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(Arc::clone)
        };
        let Some(slot) = slot else { return false };
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        provider.project_view().is_some()
    }

    /// Run a whole-project validation for `root` (decoding sources in `label`): read every source,
    /// validate in parallel against the persisted diagnostic cache, refill + prune + persist the
    /// cache, and fold the per-file results into a [`RunOutcome`] (grouped diagnostics + per-file
    /// stats + aggregate counts, all UNcapped — the caller applies its own caps). `on_progress` is
    /// invoked with the running count as workers advance (a no-op for a silent run). The single
    /// place both the explicit "Validate (no compile)" and the silent on-save refresh share.
    pub fn validate_project_collect(
        &self,
        root: &str,
        label: &str,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> RunOutcome {
        let sources = read_java_sources(Path::new(root), label).sources;
        let mut cache = self.diag_cache_load(root);
        let run = std::time::Instant::now();
        let results = self.validate_project_batch(root, &sources, &cache, on_progress);
        let wall_ms = run.elapsed().as_millis() as u64;

        // Cancelled mid-sweep → discard the partial batch: don't persist a partial cache, don't
        // report partial diagnostics. The FE already dismissed the card.
        if self.validation_cancelled() {
            return RunOutcome::default();
        }

        let validated = results.len();
        let mut diagnostics: Vec<FileDiagnostics> = Vec::new();
        let mut stats: Vec<FileValidationStat> = Vec::with_capacity(validated);
        let mut sum_ms = 0u64;
        let mut max_ms = 0u64;
        let mut max_file: Option<String> = None;
        let mut error_count = 0usize;
        let mut warning_count = 0usize;
        let mut cached_hits = 0usize;
        let mut seen: HashSet<String> = HashSet::with_capacity(validated);

        for r in results {
            seen.insert(r.file.clone());
            if r.hit {
                cached_hits += 1;
            }
            sum_ms += r.ms;
            if r.ms > max_ms {
                max_ms = r.ms;
                max_file = Some(r.file.clone());
            }
            let errors = r.diags.iter().filter(|d| d.severity == "error").count();
            let warnings = r.diags.iter().filter(|d| d.severity == "warning").count();
            error_count += errors;
            warning_count += warnings;
            stats.push(FileValidationStat { file: r.file.clone(), ms: r.ms, errors, warnings });
            if let Some(deps) = r.deps {
                cache.put(&r.file, deps, r.diags.clone());
            }
            if !r.diags.is_empty() {
                diagnostics.push(FileDiagnostics { file: r.file, diagnostics: r.diags });
            }
        }
        cache.files.retain(|k, _| seen.contains(k));
        self.diag_cache_save(root, &cache);

        RunOutcome {
            diagnostics,
            stats,
            wall_ms,
            sum_ms,
            error_count,
            warning_count,
            validated,
            cached_hits,
            max_ms,
            max_file,
        }
    }

    /// Load the persisted diagnostic cache for the project at `root`, keyed to the current
    /// classpath/JDK epoch (a change drops it wholesale). Empty when none is on disk. The
    /// whole-project validation loads it once, serves + fills it, then
    /// [`diag_cache_save`](Self::diag_cache_save)s it.
    pub fn diag_cache_load(&self, root: &str) -> bennu_intel::prelude::DiagCache {
        let base = index_base_for(root);
        let epoch = self.diag_epoch(root);
        bennu_intel::prelude::DiagCache::load_or_new(
            &bennu_intel::prelude::diag_cache_path(&base),
            epoch,
        )
    }

    /// Persist the diagnostic `cache` for the project at `root` (best-effort).
    pub fn diag_cache_save(&self, root: &str, cache: &bennu_intel::prelude::DiagCache) {
        let base = index_base_for(root);
        bennu_intel::prelude::save_diag_cache(&bennu_intel::prelude::diag_cache_path(&base), cache);
    }

    /// A content stamp of the whole project's `.java` set for the warm-up short-circuit: the
    /// classpath/JDK epoch + a hash over every file's (path, content-hash). Two opens produce the
    /// SAME stamp iff nothing that could change a diagnostic changed — same files, same bytes, same
    /// classpath — so the warm-up can be skipped entirely (no re-validation, no progress card) rather
    /// than re-serving the persisted cache file-by-file on every startup. Any edit (content), added /
    /// removed file, or JDK/dependency change flips it, so a real change still re-warms.
    fn warmup_stamp(&self, root: &str, sources: &[(PathBuf, String)]) -> u64 {
        let mut entries: Vec<(String, u64)> = sources
            .iter()
            .map(|(p, s)| (norm_path(p), bennu_intel::prelude::source_hash(s)))
            .collect();
        entries.sort_unstable();
        let mut seed = format!("{}\0", self.diag_epoch(root));
        for (p, h) in &entries {
            seed.push_str(p);
            seed.push('\0');
            seed.push_str(&h.to_string());
            seed.push('\0');
        }
        bennu_intel::prelude::source_hash(&seed)
    }

    /// The on-disk warm-up stamp path for `root` (stable, under the index base like the diag cache).
    fn warmup_stamp_path(&self, root: &str) -> PathBuf {
        index_base_for(root).join("warmup-stamp")
    }

    /// Whether the warm-up for `root` can be SKIPPED: the current project stamp equals the one
    /// recorded after the last successful warm-up. `false` (→ warm up) on any mismatch / missing
    /// stamp / read error.
    pub fn warmup_up_to_date(&self, root: &str, sources: &[(PathBuf, String)]) -> bool {
        let stamp = self.warmup_stamp(root, sources);
        std::fs::read_to_string(self.warmup_stamp_path(root))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            == Some(stamp)
    }

    /// Record the current project stamp after a successful warm-up (best-effort), so the next open
    /// with an unchanged project skips the warm-up.
    pub fn record_warmup_stamp(&self, root: &str, sources: &[(PathBuf, String)]) {
        let path = self.warmup_stamp_path(root);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.warmup_stamp(root, sources).to_string());
    }

    /// The classpath/JDK epoch for the project at `root` — a hash of the JDK level it was opened at
    /// plus the **resolved dependency jar set** (sorted, so order-independent). A JDK switch or a
    /// dependency-set change bumps it, dropping the diagnostic cache so nothing computed against the
    /// old classpath is ever served; an UNCHANGED project (same JDK + same jars) keeps the SAME epoch
    /// across sessions, so the persisted cache survives every restart and the warm-up is genuinely
    /// incremental (it re-validates only files whose own bytes / project deps changed, not the whole
    /// project on every open). Deriving the epoch from the actually-resolved jar set — cached across
    /// sessions by pom mtime — instead of a `target/bennu-classpath.txt` that a build could rewrite
    /// (or that may not exist) is what makes it stable: a volatile/absent file used to flip the epoch
    /// on some opens, dropping the whole cache and forcing a full cold re-warm every startup.
    fn diag_epoch(&self, root: &str) -> u64 {
        let (jdk, mut jars) = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            match slots.get(&PathBuf::from(root)) {
                Some(s) => (
                    s.jdk_version.clone(),
                    s.dep_jars.read().unwrap_or_else(|p| p.into_inner()).clone(),
                ),
                None => (String::new(), Vec::new()),
            }
        };
        jars.sort_unstable(); // order-independent: the jar SET, not the resolve order, defines the epoch
        let mut seed = format!("{jdk}\0");
        for j in &jars {
            seed.push_str(j);
            seed.push('\0');
        }
        bennu_intel::prelude::source_hash(&seed)
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
            config_offset: target.config_offset,
            class_fqcn: target.class_fqcn,
            view_jsp: target.view_jsp,
        })
    }

    /// Resolve a Spring **bean id** (a struts `<action class="beanId">` / spring `<… ref>`)
    /// to its implementation class FQCN, over the owning project's config resolver — for
    /// go-to on a config XML. `None` when no project owns the file, its config isn't built,
    /// or the id names no known bean.
    pub fn bean_class(&self, file: &str, bean_id: &str) -> Option<String> {
        self.config_for_file(file)?.resolve_bean_class(bean_id)
    }

    /// Resolve a MyBatis mapper interface **method** to the `<select|insert|update|
    /// delete id=…>` statement with the matching id (go-to XML). `file` is any file inside
    /// the owning project (to pick the project's config); `interface_fqcn` is the mapper
    /// interface FQCN, `method` the invoked method name. Returns `None` when no project owns
    /// the file, its config isn't built yet, or the interface has no such statement.
    pub fn definition_mapper(
        &self,
        file: &str,
        interface_fqcn: &str,
        method: &str,
    ) -> Option<MapperDefinition> {
        let cfg = self.config_for_file(file)?;
        let target = cfg.statement_for_method(interface_fqcn, method)?;
        Some(MapperDefinition {
            config_file: target.file.to_string(),
            offset: target.offset,
            kind: target.kind.as_str().to_string(),
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

    /// Everything the "New validator" modal needs for a `<Action>-validation.xml`: the
    /// bound action class (by the file-name convention), that class's writable bean
    /// properties (the `<field name>` candidates), and the fields already validated in the
    /// file. Never errors — an unresolved action just yields empty lists.
    pub fn validation_context(&self, file: &str) -> ValidationContext {
        let (action_simple, _alias) =
            bennu_web::prelude::split_validation_filename(Path::new(file)).unwrap_or_default();

        // Fields already carrying a validator in this file (parsed off disk).
        let existing_fields = bennu_web::prelude::parse_validation(Path::new(file))
            .map(|r| r.fields.into_iter().map(|f| f.name).collect())
            .unwrap_or_default();

        // Resolve the action class via the owning project's class index (simple-name match).
        let entry = self.slot_for_file(file).and_then(|slot| {
            let root = slot.root.to_string_lossy().to_string();
            self.class_index(&root)?.into_iter().find(|c| c.simple == action_simple)
        });
        let (action_fqcn, action_file) = match entry {
            Some(c) => (Some(c.fqcn), Some(c.file)),
            None => (None, None),
        };

        // Writable bean properties = the `setXxx(` setters of the action source (a form
        // binds + validates writable properties). Name-only scan, so a lossy read is fine.
        let properties = action_file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|src| scan_setter_properties(&src))
            .unwrap_or_default();

        ValidationContext { action_simple, action_fqcn, properties, existing_fields }
    }

    /// The correlation context for ONE form action, for `bennu_form_analysis`: the resolved
    /// action class FQCN, the struts config fragment it's declared in, the class's **writable
    /// properties** (its `setXxx` setters — what a form binds), and the field names its
    /// validation ruleset covers. `file` is any file inside the owning project (to pick the
    /// project's config); `action` is the form's raw/normalized action reference.
    ///
    /// Every tuple element is best-effort: an unresolvable action yields
    /// `(None, None, [], [])` (the caller still lists the form, all fields unbound). Reuses
    /// the same private helpers as [`validation_context`](Self::validation_context)
    /// (`config_for_file` → `action_class_ref`, `class_index` + `scan_setter_properties`,
    /// `validations_for_class`) — no new parsing here.
    pub fn form_action_context(
        &self,
        file: &str,
        action: &str,
    ) -> (Option<String>, Option<String>, Vec<String>, Vec<String>) {
        let Some(cfg) = self.config_for_file(file) else {
            return (None, None, Vec::new(), Vec::new());
        };
        let Some(target) = cfg.action_class_ref(action) else {
            return (None, None, Vec::new(), Vec::new());
        };
        let class_fqcn = target.class_fqcn;
        let config_file = (!target.config_file.is_empty()).then(|| target.config_file.replace('\\', "/"));

        // Writable properties: find the action class source by simple name (from the FQCN)
        // via the class index, then scan its setters. Empty when the class isn't a project
        // source (dependency-jar action) or the index hasn't landed yet.
        let simple = class_fqcn.as_deref().and_then(|fqcn| fqcn.rsplit('.').next());
        let writable = simple
            .and_then(|simple| {
                let root = self.slot_for_file(file)?.root.to_string_lossy().to_string();
                let entry = self.class_index(&root)?.into_iter().find(|c| c.simple == simple)?;
                let src = std::fs::read_to_string(&entry.file).ok()?;
                Some(scan_setter_properties(&src))
            })
            .unwrap_or_default();

        // Validated fields: flatten the `<field name>`s across the action's validation
        // rulesets (bound to the class by its simple name).
        let validated = simple
            .map(|simple| {
                cfg.validations_for_class(simple)
                    .iter()
                    .flat_map(|v| v.fields.iter().map(|f| f.name.clone()))
                    .collect()
            })
            .unwrap_or_default();

        (class_fqcn, config_file, writable, validated)
    }

    /// Candidate Struts actions a JSP's OGNL binds to, for the action picker + linting. Two sources,
    /// unioned (deduped by action qname):
    ///   * **direct** — actions whose result view IS this JSP ([`ConfigResolver::actions_for_view`]);
    ///   * **inherited** — this JSP may be an included **fragment**; its effective action(s) are those
    ///     of the page(s) that include it (transitively), so we walk the include graph's REVERSE edges
    ///     and add each including page's actions. A child (`.jspf`) thus gets its parent view's action.
    /// Empty when no project/config owns `file`.
    pub fn jsp_action_candidates(&self, file: &str) -> Vec<(String, Option<String>)> {
        let Some(cfg) = self.config_for_file(file) else {
            return Vec::new();
        };
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        merge_candidates(&mut out, cfg.actions_for_view(file));
        if let Some(root) = self.root_for_file(file) {
            let graph = self.include_graph(&root, file, false);
            let related = bennu_web::prelude::related_files(&graph, &file.replace('\\', "/"), 200);
            for rf in related.files {
                if matches!(rf.relation, bennu_web::prelude::IncludeRelation::IncludedBy) {
                    merge_candidates(&mut out, cfg.actions_for_view(&rf.file));
                }
            }
        }
        out
    }

    /// Find-usages for a Struts action: every JSP `action="…"` reference to `action_qname`
    /// across the project. `action_qname` is the **raw** attribute value the editor sends
    /// verbatim (with a possible `.action`/`.do` suffix, `?query`, or namespace-less bare
    /// name), so it's normalized to the scanner's key before matching — otherwise the needle
    /// never equals the stored refs (the bug where find-usages silently returned nothing).
    /// `[]` when no project owns the file. Glue over `bennu-web`'s tested `parse_jsp`;
    /// computed (`${…}`/`%{…}`) refs are excluded.
    pub fn action_usages(&self, file: &str, action_qname: &str) -> Vec<UsageHit> {
        // Normalize the caret token the same way the scanner normalizes the refs it stores.
        let Some(needle) = bennu_web::prelude::normalize_action_ref(action_qname) else {
            return Vec::new(); // computed / empty → nothing to find
        };
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let sources: Vec<(String, String)> = crate::web_discovery::discover_jsp_files(&slot.root)
            .into_iter()
            .filter_map(|jsp| std::fs::read_to_string(&jsp).ok().map(|s| (norm_path(&jsp), s)))
            .collect();
        action_usage_hits(&sources, &needle)
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

    /// The inherited ("super") members of the type declared at `file`:(`type_name`,`line`),
    /// over the owning project's rename engine (which shares the whole-project resolver +
    /// source sets, built off-thread on open). `line` is the 1-based declaration line, to
    /// disambiguate a nested / same-simple-named type. Returns `[]` when no project owns the
    /// file, the engine is still building, or the type can't be resolved. Mirrors
    /// [`declaration`](Self::declaration).
    pub fn inherited_members(&self, file: &str, type_name: &str, line: i64) -> Vec<InheritedMember> {
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        };
        let Some(engine) = engine else {
            return Vec::new();
        };
        engine.inherited_members(file, type_name, line).into_iter().map(inherited_member_of).collect()
    }

    /// Resolve the symbol at `file`:`offset` to a hover card, over the owning project's
    /// rename engine (which shares the whole-project reference index + resolver, built
    /// off-thread on open). `source` is the current (possibly-unsaved) buffer. Returns
    /// `None` when no project owns the file, the engine is still building, or the caret
    /// isn't on a symbol we can classify. Mirrors [`find_usages`](Self::find_usages).
    pub fn hover(&self, file: &str, source: &str, offset: usize) -> Option<HoverInfo> {
        let slot = self.slot_for_file(file)?;
        // 1. The reference-index classifier: fields / methods / types (with Javadoc).
        let engine = {
            let g = slot.rename.read().unwrap_or_else(|p| p.into_inner());
            g.as_ref().map(Arc::clone)
        };
        if let Some(engine) = engine {
            if let Some(info) = engine.hover(file, source, offset) {
                return Some(hover_info_of(info));
            }
        }
        // 2. Fallback: a local variable / parameter isn't keyed in the reference index — resolve its
        //    TYPE via the provider's full (JDK-aware) resolver, so hovering a `var`/`val` (or any
        //    local) shows what it is. Runs on the provider, not the rename engine (which is
        //    project-only and can't type a JDK `var`).
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        provider.var_hover(source, offset).map(hover_info_of)
    }

    /// Resolve the library/JDK type `name` references in `source` to an on-disk **source view** for
    /// go-to, returning `(path, offset, can_download)`. The view is, best-first: the REAL `.java`
    /// from the JDK's `src.zip`, then a dependency's `-sources.jar` (if downloaded), else a
    /// signatures-only **decompiled stub** from the class's bytecode. Cached under the profile's data
    /// dir so a second go-to reuses it. `None` when `name` doesn't resolve, is a PROJECT type (real
    /// source exists), or its bytecode isn't decodable. `offset` is the top of the file (member-
    /// precise jumps are a follow-up). `can_download` is `true` when we served a STUB for a
    /// third-party dependency (the project has deps and it isn't a JDK type) — the FE then offers the
    /// "Download sources" banner.
    pub fn decompiled_stub(&self, file: &str, source: &str, name: &str) -> Option<DecompiledView> {
        let slot = self.slot_for_file(file)?;
        let root = norm_path(&slot.root);
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        let binary = provider.library_binary(source, name)?;
        let (text, file_binary, is_stub) = self.serve_source_view(&provider, &root, &binary)?;
        let path = write_view(&file_binary, &text)?;
        Some(DecompiledView {
            file: path,
            offset: 0,
            can_download: self.can_download_sources(&slot, &binary, is_stub),
        })
    }

    /// The on-disk **source view** for a library/JDK `binary`, best-first: the REAL `.java` from the
    /// JDK's `src.zip`, then a dependency's `-sources.jar` (if downloaded), else a signatures-only
    /// decompiled stub. Returns `(text, file_binary, is_stub)` — `file_binary` is the OUTER type for
    /// real source (an inner class lives in its enclosing compilation unit), so the cached file is
    /// named after the type it declares. Shared by [`decompiled_stub`](Self::decompiled_stub) (plain
    /// go-to) and [`library_declaration`](Self::library_declaration) (in-library nav).
    fn serve_source_view(
        &self,
        provider: &NativeJavaProvider,
        root: &str,
        binary: &str,
    ) -> Option<(String, String, bool)> {
        if let Some(t) = provider.jdk_source_text(binary) {
            Some((t, outer_binary(binary), false))
        } else if let Some((t, outer)) = self.dep_source_text(root, binary) {
            Some((t, outer, false))
        } else {
            Some((provider.stub_for(binary)?, binary.to_string(), true))
        }
    }

    /// Whether to offer the "Download sources" banner: only for a STUB of a third-party dependency (a
    /// JDK stub has no Maven artifact; a project with no resolved deps can't fetch anything).
    fn can_download_sources(&self, slot: &ProjectSlot, binary: &str, is_stub: bool) -> bool {
        let has_deps = !slot.dep_jars.read().unwrap_or_else(|p| p.into_inner()).is_empty();
        is_stub && has_deps && !crate::sources_download::is_jdk_package(binary)
    }

    /// Go-to-declaration from a caret INSIDE a library/JDK source view (`view_source` = the library
    /// tab's buffer, `offset` = the caret). Resolves the target against the ORIGIN project's classpath
    /// resolver (`origin_file` picks the project — a library file is under no project root, so its own
    /// path can't), serves the target type's source view, and lands **member-precise** on the method /
    /// field (or the type declaration for a plain type reference). Returns a [`DecompiledView`] the FE
    /// opens exactly like the initial go-to — so navigation chains library → library. `None` when the
    /// caret isn't a resolvable type / member access.
    pub fn library_declaration(
        &self,
        origin_file: &str,
        view_source: &str,
        offset: usize,
    ) -> Option<DecompiledView> {
        let slot = self.slot_for_file(origin_file)?;
        let root = norm_path(&slot.root);
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        let target = provider.library_target_at(view_source, offset)?;
        let (text, file_binary, is_stub) = self.serve_source_view(&provider, &root, &target.binary)?;
        let jump = member_jump_offset(&text, &file_binary, target.member.as_ref());
        let path = write_view(&file_binary, &text)?;
        Some(DecompiledView {
            file: path,
            offset: jump,
            can_download: self.can_download_sources(&slot, &target.binary, is_stub),
        })
    }

    /// Hover INSIDE a library/JDK source view — the local/`var`/parameter/expression type at the
    /// caret, via the ORIGIN project's full (JDK-aware) resolver on the library buffer. `None` when
    /// the caret isn't on a typeable local (type/member signature hover is a follow-up). Mirrors the
    /// project-file hover's `var_hover` fallback, routed through the origin project's provider.
    pub fn library_hover(&self, origin_file: &str, view_source: &str, offset: usize) -> Option<HoverInfo> {
        let slot = self.slot_for_file(origin_file)?;
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        provider.var_hover(view_source, offset).map(hover_info_of)
    }

    /// REAL library source for `binary` from an already-downloaded dependency `-sources.jar`, plus
    /// the OUTER binary name (for the cache filename). `None` when no dep sources jar on disk holds
    /// it. Lazily builds + caches the per-root pool of opened sources jars (only those present on
    /// disk); [`refresh_dep_sources`](Self::refresh_dep_sources) clears it after a download.
    fn dep_source_text(&self, root: &str, binary: &str) -> Option<(String, String)> {
        let pool = self.dep_sources_pool(root);
        for zip in pool.iter() {
            if let Some(text) = zip.source_text(binary) {
                return Some((text, outer_binary(binary)));
            }
        }
        None
    }

    /// The per-root pool of opened dependency `-sources.jar`s, built lazily from the slot's resolved
    /// dep jars (only the sources jars that already exist on disk).
    fn dep_sources_pool(
        &self,
        root: &str,
    ) -> Arc<Vec<bennu_classpath::prelude::JavaSourceZip>> {
        {
            let cache = self.dep_sources.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(pool) = cache.get(root) {
                return Arc::clone(pool);
            }
        }
        // Build outside the lock (opening zips does I/O). A racing second builder is harmless — the
        // last insert wins and both hand back an equivalent pool.
        let dep_jars = {
            let slot = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slot.get(&PathBuf::from(root))
                .map(|s| s.dep_jars.read().unwrap_or_else(|p| p.into_inner()).clone())
                .unwrap_or_default()
        };
        let pool = Arc::new(crate::sources_download::open_dep_source_zips(&dep_jars));
        self.dep_sources
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(root.to_string(), Arc::clone(&pool));
        pool
    }

    /// Drop the cached dependency-sources pool for `root`, so the next go-to rebuilds it and picks up
    /// a freshly-downloaded `-sources.jar`. Called after a successful "Download sources" fetch.
    fn refresh_dep_sources(&self, root: &str) {
        self.dep_sources.lock().unwrap_or_else(|p| p.into_inner()).remove(root);
    }

    /// Fetch the `-sources.jar` for the dependency that owns the library type `name` (resolved via
    /// `file`'s buffer `source`) and reload the decompiled tab at `view_path` with the real source.
    /// Runs `mvn dependency:get` in the project dir (so its configured repositories apply) as a
    /// **tracked job**, on a background thread — the potentially-slow owning-jar search + Maven run
    /// stay OFF the IPC dispatcher. On completion emits `arbor://bennu/sources-ready { path, ok }`;
    /// on success the tab reloads with the real source, on failure the FE just clears its spinner
    /// (a toast explains why). `Err` fast only when the type isn't a resolvable library type.
    pub fn download_sources(
        &'static self,
        file: &str,
        source: &str,
        name: &str,
        view_path: &str,
        host: Option<Arc<dyn HostCaller>>,
        sink: Arc<dyn EventSink>,
    ) -> Result<String, String> {
        let slot = self.slot_for_file(file).ok_or("no open project owns this file")?;
        let root = norm_path(&slot.root);
        let jdk_version = slot.jdk_version.clone();
        let root_path = slot.root.clone();
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        // Cheap (index lookup) — fast-fail here so the FE gets an immediate error.
        let binary = provider
            .library_binary(source, name)
            .ok_or("this type isn't a resolvable library type")?;
        let dep_jars = slot.dep_jars.read().unwrap_or_else(|p| p.into_inner()).clone();
        let host = host.ok_or("no reverse channel to register the download job")?;

        // Everything the background thread needs, owned. The owning-jar scan (opens each dep jar) and
        // the Maven run happen here, not on the dispatcher.
        let svc: &'static IndexService = self;
        let (file, source, name, view_path) =
            (file.to_string(), source.to_string(), name.to_string(), view_path.to_string());
        std::thread::spawn(move || {
            let fail = |sink: &Arc<dyn EventSink>, msg: &str| {
                sink.emit(EVT_SOURCES_READY, json!({ "path": &view_path, "ok": false }));
                notify(sink, "Download sources failed", msg, "error");
            };
            let Some(jar) = crate::sources_download::find_owning_jar(&dep_jars, &binary) else {
                return fail(&sink, "No resolved dependency jar contains this type.");
            };
            let Some(gav) = crate::sources_download::gav_from_m2_jar(&jar) else {
                return fail(&sink, "Couldn't determine the dependency's Maven coordinates.");
            };
            let job = register_bennu_job(
                &host,
                &sink,
                &format!("Download sources: {}", gav.label()),
                &gav.sources_artifact(),
                "Download",
                true,
            );
            let mvn = crate::dep_classpath::find_mvn_launcher();
            let jdk_home = bennu_classpath::prelude::find_jdk_home(&jdk_version);
            match crate::sources_download::run_mvn_get_sources(&root_path, &mvn, jdk_home.as_deref(), &gav)
            {
                Ok((true, _log)) => {
                    // Pick up the freshly-downloaded jar and rewrite the tab file as real source.
                    svc.refresh_dep_sources(&root);
                    let _ = svc.decompiled_stub(&file, &source, &name);
                    sink.emit(EVT_SOURCES_READY, json!({ "path": &view_path, "ok": true }));
                    finish_bennu_job(&sink, job, true, None);
                    notify(&sink, "Sources downloaded", &format!("{} sources attached", gav.label()), "success");
                }
                Ok((false, log)) => {
                    let msg = sources_failure_reason(&log);
                    sink.emit(EVT_SOURCES_READY, json!({ "path": &view_path, "ok": false }));
                    finish_bennu_job(&sink, job, false, Some(msg.clone()));
                    notify(&sink, "Download sources failed", &msg, "error");
                }
                Err(e) => {
                    sink.emit(EVT_SOURCES_READY, json!({ "path": &view_path, "ok": false }));
                    finish_bennu_job(&sink, job, false, Some(e.clone()));
                    notify(&sink, "Download sources failed", &e, "error");
                }
            }
        });
        Ok(String::new())
    }

    /// Importable FQNs (dotted, sorted) for a simple type `name`, from the owning project's class-name
    /// index (JDK + dependency + project types). Empty when no project owns `file` or its index isn't
    /// built. Powers the "Import class" intention's candidate list.
    pub fn import_candidates(&self, file: &str, name: &str) -> Vec<String> {
        let Some(slot) = self.slot_for_file(file) else {
            return Vec::new();
        };
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        provider.import_candidates(name).to_vec()
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
            // The dependency jars the INDEX resolver loaded (its own tier) — reflects what
            // completion/validation resolve against, independent of whether `mvn` re-ran this
            // session. Falls back to the Build's cached `target/bennu-classpath.txt` when the index
            // has no dep tier yet (still building, or dep resolution off/failed) — never a shell-out.
            jar_count: {
                let n = slot.dep_jars.read().unwrap_or_else(|p| p.into_inner()).len();
                if n > 0 { n } else { cached_jar_count(&slot.root) }
            },
            actions,
            beans,
            relations,
            ready: slot.ready.load(Ordering::Relaxed),
        }
    }

    /// The per-kind entry list for the index inspector, read off the already-built
    /// structures for the project rooted at `root` (no re-walk / re-parse). `kind` is one
    /// of `"members"` / `"jars"` / `"jdk"` / `"beans"` / `"actions"` / `"relations"`
    /// (`"types"` is served by `bennu_class_index`). Returns `[]` for an unknown root, an
    /// unrecognised kind, or a kind whose source isn't available yet (still building / no
    /// config) — the FE degrades gracefully.
    pub fn index_entries(&self, root: &str, kind: &str) -> Vec<IndexEntry> {
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots.get(&PathBuf::from(root)).map(Arc::clone)
        };
        let Some(slot) = slot else {
            return Vec::new();
        };
        match kind {
            "members" => self.member_entries(&slot),
            "jars" => {
                // The resolver's own dep jars when present, else the Build's `target/` classpath.
                let g = slot.dep_jars.read().unwrap_or_else(|p| p.into_inner());
                if g.is_empty() {
                    jar_entries(&slot.root)
                } else {
                    g.iter().filter_map(|e| jar_entry_of(e)).collect()
                }
            }
            "jdk" => jdk_entries(&slot),
            "beans" | "actions" | "relations" => config_entries(&slot, kind),
            _ => Vec::new(),
        }
    }

    /// The project's members (methods + fields) from the built index, one [`IndexEntry`]
    /// each: primary = simple name, secondary = owning FQCN (dotted) + signature, file =
    /// the declaring source, line = the declaring type's line (from the class cache — the
    /// index has no per-member line). `[]` while the index is still building.
    fn member_entries(&self, slot: &Arc<ProjectSlot>) -> Vec<IndexEntry> {
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        // fqcn (dotted) → declaring type's 1-based line, from the class navigator cache.
        let line_of: HashMap<String, i64> = {
            let classes = slot.classes.read().unwrap_or_else(|p| p.into_inner());
            classes.iter().map(|c| (c.fqcn.clone(), c.line as i64)).collect()
        };
        provider
            .project_members()
            .into_iter()
            .map(|m| {
                let owner_dotted = m.owner_binary.replace('/', ".");
                let secondary = if m.signature.is_empty() {
                    owner_dotted.clone()
                } else {
                    format!("{owner_dotted} · {}", m.signature)
                };
                let file = (!m.file.is_empty()).then(|| m.file.clone());
                // Line only when we have an openable file AND a known declaring-type line.
                let line = file.as_ref().and(line_of.get(&owner_dotted).copied());
                IndexEntry { primary: m.name, secondary, file, line }
            })
            .collect()
    }

    /// The root (forward-slashed) of the OPEN project owning `file` (longest-prefix match),
    /// or `None` when no project is open at a prefix of `file`. Lets a handler that needs the
    /// project's JSP set (e.g. the include-aware form tree) reuse the exact root the index was
    /// opened at, rather than re-deriving it from the filesystem.
    pub fn root_for_file(&self, file: &str) -> Option<String> {
        self.slot_for_file(file).map(|s| norm_path(&s.root))
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

        // Buffer-edit bookkeeping for the out-of-code-block cache: bump the global count + this file's
        // own count. A file A's resolver revision is `total − per_file[A]`, so editing A leaves A's own
        // cache valid (both counts rise together) while it invalidates every OTHER file's cache that
        // might depend on A's now-changed types.
        {
            let mut g = self.patch_counts.lock().unwrap_or_else(|p| p.into_inner());
            g.total = g.total.wrapping_add(1);
            *g.per_file.entry(file_path.clone()).or_insert(0) += 1;
        }

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
        let provider = {
            let g = slot.provider.read().unwrap_or_else(|p| p.into_inner());
            Arc::clone(&g)
        };
        // Resolve a wildcard-imported supertype/return/param to the exact package via the live
        // resolver's project view (the merged `simple` map is lossy on same-name collisions).
        let is_project = |b: &str| provider.is_project_type(b);
        let symbols: Vec<Symbol> = source
            .map(|src| {
                file_records_from_source(&file_path, src, &simple, u32::MAX / 2, &is_project)
                    .into_iter()
                    .map(|r| r.symbol)
                    .collect()
            })
            .unwrap_or_default();
        provider.apply_file_patch(file, &symbols);

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
        // Re-collect annotation-declared beans so a `.java` that added/removed a `@Service`
        // since the last full build is reflected on this config rebuild. Costs one source
        // walk (bounded), acceptable off the keystroke path (this is a config-XML edit).
        let ProjectSources { sources, .. } = read_java_sources(&slot.root, &slot.encoding_label);
        let annotation_beans = collect_annotation_beans(&sources);
        match ingest_config_graph(&graph, &index_dir, &annotation_beans) {
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

/// One file's result from [`IndexService::validate_project_batch`] — the diagnostics to report
/// plus the cache bookkeeping the caller folds back in.
pub struct BatchResult {
    /// The forward-slashed file key.
    pub file: String,
    /// The diagnostics (served from the cache, or freshly computed).
    pub diags: Vec<bennu_proto::prelude::Diagnostic>,
    /// The recorded dependencies to STORE for this file — `Some` only when it was (re)validated;
    /// `None` on a cache hit (nothing to store) or when there's no resolver (uncacheable).
    pub deps: Option<bennu_intel::prelude::FileDeps>,
    /// Whether this file was served from the cache (no re-validation).
    pub hit: bool,
    /// The file's own validation time in ms (0 on a cache hit) — for the "slowest files" table.
    pub ms: u64,
}

/// The folded outcome of a whole-project validation ([`IndexService::validate_project_collect`]).
/// Everything is UNcapped — each handler applies its own payload caps (slowest-N stats, first-N
/// diagnostic files). `Default` = the empty outcome returned when a run is cancelled.
#[derive(Default)]
pub struct RunOutcome {
    /// Files that have diagnostics, in walk order (uncapped).
    pub diagnostics: Vec<FileDiagnostics>,
    /// Per-file timing stats, in walk order (uncapped; the handler sorts + caps for the table).
    pub stats: Vec<FileValidationStat>,
    /// Real wall-clock of the (parallel) run, in ms.
    pub wall_ms: u64,
    /// Sum of per-file processing times, in ms (for the mean-per-file metric).
    pub sum_ms: u64,
    pub error_count: usize,
    pub warning_count: usize,
    pub validated: usize,
    pub cached_hits: usize,
    pub max_ms: u64,
    pub max_file: Option<String>,
}

/// Cooperative cancel flag for the whole-project validation sweep (warm-up + explicit "Validate").
/// Set via [`IndexService::request_cancel_validation`] (the FE's Cancel on the operation card); the
/// parallel worker loop checks it per file and drains the rest cheaply, and the caller then discards
/// the (partial) results instead of persisting them. Reset at the start of each run.
static CANCEL_VALIDATION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The worker cap for the whole-project validation sweep: the user's `validation_threads` setting when
/// set, else auto = roughly half the cores — so the sweep leaves ample headroom for the interactive
/// path (go-to / completion) and the UI shell and never pegs the machine. Distinct from the one-shot
/// index build, which keeps the faster `available_parallelism − 2` default.
fn validation_worker_cap() -> usize {
    let cfg = bennu_core::config::load().validation_threads;
    if cfg > 0 {
        return cfg;
    }
    let cores = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    (cores / 2).max(1)
}

/// Validate `files` in parallel against `provider`, consulting the read-only `cache`. Pure/
/// read-only over the cache + the `Arc`-shared resolver (recording is thread-local per worker), so
/// it parallelizes safely; the pool is capped by [`validation_worker_cap`] (gentle by default) so it
/// stays a background citizen, and honours the [`CANCEL_VALIDATION`] flag. `on_progress` is called
/// (from a worker) with the running count every so often.
fn validate_files_parallel(
    provider: &NativeJavaProvider,
    jdk_available: bool,
    classpath_complete: bool,
    requested_major: Option<u32>,
    files: &[(PathBuf, String)],
    cache: &bennu_intel::prelude::DiagCache,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> Vec<BatchResult> {
    let total = files.len();
    let counter = AtomicUsize::new(0);
    // Fresh run → clear any stale cancel request from a previous sweep.
    CANCEL_VALIDATION.store(false, Ordering::Release);
    bennu_intel::prelude::parallel_map_capped(files, validation_worker_cap(), |(path, source)| {
        // Cooperative cancel: once requested (FE Cancel on the operation card), workers stop doing
        // real work and drain the remaining files as cheap empty results; the caller then discards
        // the whole (partial) batch instead of persisting it.
        if CANCEL_VALIDATION.load(Ordering::Acquire) {
            return BatchResult { file: norm_path(path), diags: Vec::new(), deps: None, hit: false, ms: 0 };
        }
        let file = norm_path(path);
        let ctx = bennu_check::prelude::FileContext {
            file_stem: path.file_stem().and_then(|s| s.to_str()).map(str::to_string),
            expected_package: path.parent().and_then(bennu_java::prelude::infer_package),
            java_major: requested_major,
            classpath_complete,
        };
        let own = bennu_intel::prelude::source_hash(source);
        // A still-fresh cached entry (the `view` borrow ends when the diags are cloned out).
        let cached = provider
            .project_view()
            .and_then(|view| cache.get_fresh(&file, own, view).map(<[_]>::to_vec));
        let result = match cached {
            Some(diags) => BatchResult { file, diags, deps: None, hit: true, ms: 0 },
            None => {
                let t = std::time::Instant::now();
                let (diags, recorded) = provider.validate_recording(source, &ctx, jdk_available);
                let ms = t.elapsed().as_millis() as u64;
                // Store deps only when a resolver exists to check freshness against next time.
                let deps = provider
                    .project_view()
                    .is_some()
                    .then(|| bennu_intel::prelude::FileDeps::from_recorded(own, &recorded));
                BatchResult { file, diags, deps, hit: false, ms }
            }
        };
        let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
        if done % 64 == 0 || done == total {
            on_progress(done, total);
        }
        result
    })
}

/// Background warm-up of the whole-project validation cache after an index build (opt-in via
/// `validate_on_open`). Loads the persisted cache, validates every file in parallel to fill the
/// misses, prunes entries for deleted files, and persists it — so the first explicit "Validate
/// (no compile)" is a near-instant all-cache-hit. Skips silently when the project has no resolver
/// yet (nothing to cache against). Emits no FE events (a silent warm-up); logged for diagnostics.
/// Register a bennu background task as a tracked job in the shell registry and emit
/// `arbor://job-started`, so it appears in the bennu Jobs overlay. `None` when registration fails.
///
/// Routing: `target: "bennu"` — the FE `job-started` listener drops events not addressed to the
/// window (the bennu FeedbackHost accepts only `"bennu"`), so the target MUST be in the event
/// payload too, not just the registry spec. Category `"system"` marks a job the FE auto-dismisses on
/// successful completion (matching `is_system`, which purges the registry entry) — an ephemeral pass
/// visible only while it runs; any other category lingers as completed. `non_cancellable` because no
/// FE cancel is wired to these yet.
fn register_bennu_job(
    host: &Arc<dyn HostCaller>,
    sink: &Arc<dyn EventSink>,
    name: &str,
    command: &str,
    category: &str,
    is_system: bool,
) -> Option<JobHandle> {
    let job = JobHandle::register(
        Arc::clone(host),
        JobSpec {
            name: name.to_string(),
            plugin_name: "bennu".into(),
            command: command.to_string(),
            category: Some(category.to_string()),
            non_cancellable: true,
            hidden: false,
            is_system,
            target: Some("bennu".into()),
        },
    )
    .ok()?;
    sink.emit(
        "arbor://job-started",
        json!({
            "job_id": &job.id,
            "name": name,
            "plugin_name": "bennu",
            "command": command,
            "category": category,
            "target": "bennu",
        }),
    );
    Some(job)
}

/// Mark a bennu job done: set its terminal status in the registry and emit `arbor://job-done`. A
/// no-op when the job was never registered (no reverse channel).
fn finish_bennu_job(
    sink: &Arc<dyn EventSink>,
    job: Option<JobHandle>,
    success: bool,
    error: Option<String>,
) {
    let Some(job) = job else { return };
    let status = if success {
        JobStatus::Completed { exit_code: 0 }
    } else {
        JobStatus::Failed { error: error.clone().unwrap_or_default() }
    };
    job.set_status(status);
    sink.emit(
        "arbor://job-done",
        json!({
            "job_id": job.id,
            "success": success,
            "exit_code": if success { 0i32 } else { -1i32 },
            "cancelled": false,
            "error": error,
        }),
    );
}

/// Emit a toast notification to the bennu window (`plugin:notification`, re-emitted by the shell).
fn notify(sink: &Arc<dyn EventSink>, title: &str, message: &str, level: &str) {
    sink.emit(
        "plugin:notification",
        json!({ "plugin": "bennu", "title": title, "message": message, "level": level }),
    );
}

/// A concise, user-facing reason for a failed `mvn dependency:get -…:sources` from its output — the
/// common case is that no repository publishes a sources jar for the artifact.
fn sources_failure_reason(log: &str) -> String {
    if log.contains("Could not find artifact") || log.contains("Could not resolve") {
        "No sources jar is published for this dependency in the configured repositories.".to_string()
    } else {
        "Maven couldn't download the sources jar (see the job output).".to_string()
    }
}

/// The whole-project analysis warm-up job (visible while it runs, self-clears when done — like
/// IntelliJ's background analysis). Supersession by a rebuild stops the pass.
fn register_warmup_job(
    host: &Arc<dyn HostCaller>,
    sink: &Arc<dyn EventSink>,
    root: &str,
) -> Option<JobHandle> {
    let display = root.rsplit(['/', '\\']).next().unwrap_or(root);
    register_bennu_job(host, sink, "Analyzing project", &format!("→ {display}"), "system", true)
}

/// Finish the warm-up job (always success — the pass either completes or is superseded before this).
fn finish_warmup_job(sink: &Arc<dyn EventSink>, job: Option<JobHandle>) {
    finish_bennu_job(sink, job, true, None);
}

fn warm_up_validation_cache(root: &str, sources: &[(PathBuf, String)]) {
    let svc = IndexService::global();
    if !svc.has_resolver(root) {
        return; // pre-index / no JDK — validating pure-AST with nothing to cache would be waste
    }
    let mut cache = svc.diag_cache_load(root);
    let results = svc.validate_project_batch(root, sources, &cache, &|_done: usize, _total: usize| {});
    if svc.validation_cancelled() {
        return; // cancelled → don't stamp/persist a partial warm-up
    }
    let mut seen = HashSet::with_capacity(results.len());
    let mut filled = 0usize;
    for r in results {
        seen.insert(r.file.clone());
        if let Some(deps) = r.deps {
            cache.put(&r.file, deps, r.diags);
            filled += 1;
        }
    }
    cache.files.retain(|k, _| seen.contains(k));
    svc.diag_cache_save(root, &cache);
    eprintln!(
        "bennu-be: validation cache warmed for {root} ({filled} validated, {} total cached)",
        seen.len()
    );
}

/// Build + swap in the rename engine for `slot`: reuse the already-read `.java` `sources`
/// (shared with the symbol-index build — no second disk pass) and read the Spring `.xml`
/// fragments (the only XML that can carry `<bean class=>`), then build the whole-project
/// reference index + resolver. Non-fatal on failure (rename then just returns "still
/// building"). Runs on the index background thread.
#[allow(clippy::too_many_arguments)]
fn build_rename_engine(
    slot: &Arc<ProjectSlot>,
    root: &Path,
    index_dir: &Path,
    jdk_version: &str,
    simple_names: &[(String, String)],
    sources: &[(PathBuf, String)],
    sink: &Arc<dyn EventSink>,
    root_str: &str,
) {
    // Reuse the shared sources (path normalized to forward slashes to match FE file keys) —
    // this is the second consumer of the single disk read done in `open`.
    let java: Vec<(String, String)> =
        sources.iter().map(|(p, s)| (norm_path(p), s.clone())).collect();

    // Spring bean XML fragments (any `.xml` with a `<beans` root) — the class-rename
    // config-aware edit target set.
    eprintln!("bennu-be: rename engine — gathering Spring XML for {}", root.display());
    let xml: Vec<(String, String)> = discover_web_inputs(root)
        .spring_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (norm_path(p), s)))
        .collect();

    eprintln!(
        "bennu-be: rename engine — building ({} java files, {} spring xml)",
        java.len(),
        xml.len()
    );
    // Surface the O(N) reference walk as live progress in the "Indexing" operation card:
    // emit a `references` progress event (files done / total) as the walk advances.
    let on_progress = |done: usize, total: usize| {
        sink.emit(
            EVT_INDEX_PROGRESS,
            json!({ "root": root_str, "phase": "references", "state": "progress", "done": done, "total": total }),
        );
    };
    match RenameEngine::for_project(index_dir, jdk_version, simple_names, java, xml, &on_progress) {
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
    sources: &[(PathBuf, String)],
    sink: &Arc<dyn EventSink>,
    root_str: &str,
) {
    emit_progress(sink, root_str, "config", "start");
    let inputs = discover_web_inputs(root);
    if inputs.struts_roots.is_empty()
        && inputs.spring_files.is_empty()
        && inputs.mapper_files.is_empty()
    {
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
    // Annotation-declared Spring beans (`@Service`/`@Component`/…) from the already-read Java
    // sources — the Option-B C1 fallback map. Reuses `sources` (no third disk walk).
    let annotation_beans = collect_annotation_beans(sources);
    match ingest_config_graph(&graph, index_dir, &annotation_beans) {
        Ok(cfg) => {
            let (a, b, r) = (cfg.action_count(), cfg.bean_count(), cfg.relation_count());
            let (i, v) = (cfg.interceptor_count(), cfg.validation_count());
            let (m, s) = (cfg.mapper_count(), cfg.statement_count());
            let ab = cfg.annotation_bean_count();
            *slot.config.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(cfg));
            eprintln!(
                "bennu-be: config graph live for {} ({a} actions, {b} beans, {ab} annotation beans, {i} interceptors, {v} validations, {m} mappers, {s} statements, {r} edges)",
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

/// Map an intel [`IntelInheritedMember`] onto the wire [`InheritedMember`] (field-for-field,
/// lifting the optional project source). Kept in the be layer so the wire mapping lives at
/// the process boundary (like `references` / `declaration`).
fn inherited_member_of(m: IntelInheritedMember) -> InheritedMember {
    InheritedMember {
        kind: m.kind,
        name: m.name,
        detail: m.detail,
        visibility: m.visibility,
        declaring_type: m.declaring_type,
        source: m.source.map(|s| InheritedSource { file_path: s.file, line: s.line }),
    }
}

/// Map the classpath [`bennu_classpath::prelude::JdkStatus`] onto the wire [`JdkStatus`]
/// (home path lossily stringified with forward slashes).
fn jdk_status_of(s: bennu_classpath::prelude::JdkStatus) -> JdkStatus {
    JdkStatus {
        requested_major: s.requested_major,
        resolved_home: s.resolved_home.map(|p| p.to_string_lossy().replace('\\', "/")),
        resolved_major: s.resolved_major,
        exact: s.exact,
        any_installed: s.any_installed,
    }
}

/// Map an intel [`NonCompliantSource`] onto the wire [`EncodingIssue`] (forward-slash path,
/// declared vs. recovered encoding), for the encoding report.
fn encoding_issue_of(s: &NonCompliantSource) -> EncodingIssue {
    EncodingIssue {
        file: norm_path(&s.file),
        declared_encoding: s.declared_encoding.clone(),
        decoded_as: s.decoded_as.clone(),
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

/// The classpath jar entries for the index inspector, read cheaply from the cached
/// `target/bennu-classpath.txt` (the same file [`cached_jar_count`] counts). One
/// [`IndexEntry`] per existing `.jar`: primary = filename, secondary = its abs path
/// (forward slashes); no openable source site (a jar isn't a project file). `[]` when the
/// classpath file is absent (no build/run has resolved it yet).
fn jar_entries(root: &Path) -> Vec<IndexEntry> {
    let cp_file = root.join("target").join("bennu-classpath.txt");
    let Ok(raw) = std::fs::read_to_string(&cp_file) else {
        return Vec::new();
    };
    split_classpath_entries(&raw).into_iter().filter_map(|e| jar_entry_of(&e)).collect()
}

/// One inspector [`IndexEntry`] for a jar `path` — `Some` only for an existing `.jar` on disk.
/// Primary = filename, secondary = abs path (forward slashes); a jar has no openable project
/// source site. Shared by the resolver-jars list and the `target/`-classpath fallback.
fn jar_entry_of(path: &str) -> Option<IndexEntry> {
    let p = Path::new(path);
    if !path.to_ascii_lowercase().ends_with(".jar") || !p.is_file() {
        return None;
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string();
    Some(IndexEntry { primary: name, secondary: path.replace('\\', "/"), file: None, line: None })
}

/// The resolved-JDK summary for the index inspector: a single [`IndexEntry`] naming the
/// project's resolved Java language level. The index has no enumerable JDK module list
/// (the JDK is resolved live off rt.jar / jimage, never persisted), so this reports the
/// one cheap datum the slot holds — the language level the project was opened at. `[]`
/// when the slot carries no version.
fn jdk_entries(slot: &Arc<ProjectSlot>) -> Vec<IndexEntry> {
    if slot.jdk_version.is_empty() {
        return Vec::new();
    }
    vec![IndexEntry {
        primary: format!("Java {}", slot.jdk_version),
        secondary: "resolved language level".to_string(),
        file: None,
        line: None,
    }]
}

/// The config-graph entries for `kind` (`"beans"` / `"actions"` / `"relations"`), read off
/// the slot's live [`ConfigResolver`]. `[]` when the project has no config graph (not built
/// / no web config).
fn config_entries(slot: &Arc<ProjectSlot>, kind: &str) -> Vec<IndexEntry> {
    let cfg = {
        let g = slot.config.read().unwrap_or_else(|p| p.into_inner());
        match g.as_ref() {
            Some(cfg) => Arc::clone(cfg),
            None => return Vec::new(),
        }
    };
    config_entries_of(&cfg, kind)
}

/// Map a [`ConfigResolver`]'s parsed graph onto the inspector entries for `kind` — the pure
/// core of [`config_entries`], factored out so it's unit-testable off a fixture graph.
fn config_entries_of(cfg: &ConfigResolver, kind: &str) -> Vec<IndexEntry> {
    let graph = cfg.graph();
    match kind {
        // Spring beans: primary = bean id, secondary = impl class FQCN, file = the config
        // fragment (the bean's declaration site). No per-bean line is parsed.
        "beans" => graph
            .beans
            .iter()
            .map(|b| IndexEntry {
                primary: b.id.clone(),
                secondary: b.class.clone(),
                file: config_site(&b.source_file),
                line: None,
            })
            .collect(),
        // Struts actions: primary = qualified name, secondary = resolved class FQCN (the C1
        // chain), file = the `<action>` config fragment. No per-action line is parsed.
        "actions" => graph
            .actions
            .iter()
            .map(|a| IndexEntry {
                primary: a.qualified_name.clone(),
                secondary: cfg.resolve_action_class(&a.qualified_name).unwrap_or_default(),
                file: config_site(&a.source_file),
                line: None,
            })
            .collect(),
        // Config edges: primary = the edge label (`from → to`), secondary = the relation
        // kind. Edges carry no source site.
        "relations" => graph
            .relations
            .iter()
            .map(|r| IndexEntry {
                primary: format!("{} → {}", r.from, r.to),
                secondary: rel_kind_label(r.kind).to_string(),
                file: None,
                line: None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A config fragment path → an openable, forward-slash source site, or `None` when the
/// record carries no source file.
fn config_site(source_file: &str) -> Option<String> {
    (!source_file.is_empty()).then(|| source_file.replace('\\', "/"))
}

/// Best-effort writable bean-property names from a Java source: each `setXxx(` setter's
/// property (JavaBeans convention). Writable properties are what a Struts form binds and
/// validates, so these are the `<field name>` candidates. A name-only text scan (no parse):
/// finds every `set` that begins an identifier `set<Upper>…` immediately followed by `(`.
fn scan_setter_properties(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut seen = std::collections::HashSet::new();
    let mut props = Vec::new();
    for (i, _) in source.match_indices("set") {
        // `set` must START an identifier (not the tail of `reset`, `offset`, …).
        if i > 0 {
            let p = bytes[i - 1];
            if p.is_ascii_alphanumeric() || p == b'_' || p == b'$' {
                continue;
            }
        }
        let rest = &source[i + 3..];
        let mut it = rest.char_indices();
        let Some((_, first)) = it.next() else { continue };
        if !first.is_ascii_uppercase() {
            continue; // `setUp` ok, `settings` (lowercase) is not a setter
        }
        let mut end = first.len_utf8();
        for (off, c) in it {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                end = off + c.len_utf8();
            } else {
                break;
            }
        }
        // Must be `set<Name>(` — a method call, allowing whitespace before `(`.
        if !rest[end..].trim_start().starts_with('(') {
            continue;
        }
        let prop = bean_property_name(&rest[..end]);
        if seen.insert(prop.clone()) {
            props.push(prop);
        }
    }
    props.sort();
    props
}

/// Every JSP `action="…"` reference matching `needle` across `sources` (`(path, src)`
/// pairs), as [`UsageHit`]s. `needle` is already normalized (the scanner's key shape); each
/// scanned ref is compared with [`action_ref_matches`]. Pure over its inputs (no FS / slot),
/// so the matching is unit-testable off in-memory fixtures.
fn action_usage_hits(sources: &[(String, String)], needle: &str) -> Vec<UsageHit> {
    let mut out = Vec::new();
    for (path, src) in sources {
        for r in bennu_web::prelude::parse_jsp(src).action_refs {
            if r.computed || !action_ref_matches(&r.name, needle) {
                continue;
            }
            let (line, col, preview) = line_col_preview(src, r.start);
            out.push(UsageHit { file: path.clone(), start: r.start, end: r.end, line, col, preview });
        }
    }
    out
}

/// Does a scanned JSP action ref `ref_name` (already normalized by `parse_jsp`) refer to the
/// same action as the caret's normalized `needle`? An absolute needle must match exactly; a
/// bare needle (no namespace, e.g. `edit` — the caret sits on a namespace-less `action=`)
/// matches any ref whose trailing name segment is that name, so find-usages of a bare name
/// still surfaces the family rather than nothing.
fn action_ref_matches(ref_name: &str, needle: &str) -> bool {
    if ref_name == needle {
        return true;
    }
    if !needle.contains('/') {
        return ref_name.rsplit('/').next() == Some(needle);
    }
    false
}

/// 1-based line + column and the trimmed line text for a byte `off` in `src` (the
/// find-usages preview). Clamps `off` to a char boundary so a multi-byte source is safe.
/// `pub(crate)` so the JSP-nav handler ([`crate::jsp_nav`]) builds usage previews the same way.
pub(crate) fn line_col_preview(src: &str, off: usize) -> (usize, usize, String) {
    let mut off = off.min(src.len());
    while off > 0 && !src.is_char_boundary(off) {
        off -= 1;
    }
    let before = &src[..off];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = off - line_start + 1;
    let line_end = src[off..].find('\n').map(|i| off + i).unwrap_or(src.len());
    (line, col, src[line_start..line_end].trim().to_string())
}

/// Append every `(qname, fqcn)` from `more` into `out`, skipping actions already present (dedup by
/// qname) — so the direct + inherited (include-graph) candidate sets union cleanly, in discovery order.
fn merge_candidates(
    out: &mut Vec<(String, Option<String>)>,
    more: Vec<(String, Option<String>)>,
) {
    for c in more {
        if !out.iter().any(|(q, _)| q == &c.0) {
            out.push(c);
        }
    }
}

/// The JavaBeans property name for an accessor suffix: lowercase the first letter, UNLESS
/// the first two letters are both upper-case (`setURL` → `URL`, not `uRL`).
pub(crate) fn bean_property_name(suffix: &str) -> String {
    let mut chars = suffix.chars();
    let Some(first) = chars.next() else { return String::new() };
    if suffix.chars().nth(1).is_some_and(|c| c.is_uppercase()) {
        return suffix.to_string();
    }
    first.to_lowercase().chain(chars).collect()
}

/// A human label for a config-graph relation kind (the inspector's `secondary`).
fn rel_kind_label(kind: bennu_web::prelude::RelKind) -> &'static str {
    use bennu_web::prelude::RelKind;
    match kind {
        RelKind::ActionToClass => "action-class",
        RelKind::ActionToResult => "action-result",
        RelKind::ResultToView => "result-view",
        RelKind::BeanIdToImpl => "bean-impl",
        RelKind::InterceptorRefToDef => "interceptor-ref",
        RelKind::InterceptorToClass => "interceptor-class",
        RelKind::MethodToStatement => "method-statement",
    }
}

/// A resolved go-to-definition target for a JSP action reference (the be-layer view of
/// [`bennu_intel::prelude::ActionTarget`]).
#[derive(Debug, Clone)]
pub struct ActionDefinition {
    /// The struts config fragment the `<action>` is declared in.
    pub config_file: String,
    /// Byte offset of the `<action>` element in `config_file` (go-to lands on the declaration).
    pub config_offset: usize,
    /// The resolved implementation class FQCN (the C1 chain), if resolvable.
    pub class_fqcn: Option<String>,
    /// The resolved view JSP (the Tiles chain), if resolvable.
    pub view_jsp: Option<String>,
}

/// A resolved go-to-definition target for a MyBatis mapper method (the be-layer view of
/// [`bennu_intel::prelude::StatementTarget`]): the mapper XML the `<select|…>` statement
/// is declared in + the byte offset of its `id` attribute value + the statement kind.
#[derive(Debug, Clone)]
pub struct MapperDefinition {
    /// The mapper XML the statement is declared in.
    pub config_file: String,
    /// Byte offset of the `id` attribute value (the go-to target).
    pub offset: usize,
    /// The statement kind (`select` / `insert` / `update` / `delete`).
    pub kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_setter_properties_from_source() {
        let src = r#"
            package com.x;
            public class LoginAction extends BaseAction {
                private String username;
                public String getUsername() { return username; }
                public void setUsername(String u) { this.username = u; }
                public void setPassword(String p) {}
                public void setURL(String u) {}        // two-caps: property stays `URL`
                protected void setInternal(int i) {}
                public int reset() { return 0; }       // `reset` is NOT a setter
                public void notASetter() {}
            }
        "#;
        let props = scan_setter_properties(src);
        assert_eq!(props, vec!["URL", "internal", "password", "username"]);
    }

    #[test]
    fn bean_property_name_handles_caps() {
        assert_eq!(bean_property_name("Username"), "username");
        assert_eq!(bean_property_name("URL"), "URL");
        assert_eq!(bean_property_name("X"), "x");
    }

    #[test]
    fn decompiled_stub_paths_are_recognised() {
        // A path under the decompiled cache dir is a stub (skip validation); a project path is not.
        let stub = decompiled_cache_path("javax/crypto/Cipher");
        assert!(is_decompiled_stub(&stub.to_string_lossy()), "{}", stub.display());
        assert!(!is_decompiled_stub("C:/proj/src/main/java/com/acme/Foo.java"));
        assert!(!is_decompiled_stub("/home/u/proj/src/main/java/Foo.java"));
    }

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
    fn gc_old_gens_preserves_newer_concurrent_gen() {
        // A concurrent open holds a HIGHER gen (its build is in progress). GC'ing for the older
        // build must NOT delete it — else that build's persist hits `os error 3`.
        let base = std::env::temp_dir()
            .join(format!("bennu-gcnewer-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&base);
        for g in [3u64, 4, 5] {
            std::fs::create_dir_all(gen_dir(&base, g)).unwrap();
        }
        // The older build (gen 4) GCs: gen 3 goes, gen 4 (its own) stays, gen 5 (a newer
        // concurrent build) is preserved.
        gc_old_gens(&base, 4);
        assert!(!gen_dir(&base, 3).exists(), "strictly-older gen removed");
        assert!(gen_dir(&base, 4).exists(), "own gen kept");
        assert!(gen_dir(&base, 5).exists(), "newer concurrent gen preserved");
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

    /// Build a small Struts/Spring config graph, ingest it, and prove the beans / actions
    /// / relations inspector mappings (primary/secondary/file) over the resulting resolver.
    #[test]
    fn config_entries_map_beans_actions_relations() {
        use bennu_intel::prelude::ingest_config_graph;
        use bennu_web::prelude::{build_web_graph, WebInputs};

        let dir = std::env::temp_dir().join(format!(
            "bennu-cfgentries-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let struts = dir.join("s.xml");
        std::fs::write(
            &struts,
            r#"<struts><package name="p" namespace="/do/Cat" extends="japs-default">
                <action name="viewTree" class="categoryAction">
                  <result type="tiles">admin.Cat.viewTree</result>
                </action>
              </package></struts>"#,
        )
        .unwrap();
        let beans = dir.join("b.xml");
        std::fs::write(
            &beans,
            r#"<beans>
                <bean id="categoryAction" class="com.x.CategoryAction"/>
              </beans>"#,
        )
        .unwrap();

        let inputs = WebInputs {
            struts_roots: vec![struts.clone()],
            resource_roots: vec![],
            spring_files: vec![beans.clone()],
            tiles_files: vec![],
            validation_files: vec![],
            mapper_files: vec![],
        };
        let (graph, _report) = build_web_graph(&inputs);
        let cfg = ingest_config_graph(&graph, &dir, &[]).unwrap();

        // beans: primary = id, secondary = class FQCN, file = the config fragment.
        let beans_e = config_entries_of(&cfg, "beans");
        assert_eq!(beans_e.len(), 1);
        assert_eq!(beans_e[0].primary, "categoryAction");
        assert_eq!(beans_e[0].secondary, "com.x.CategoryAction");
        assert_eq!(beans_e[0].file.as_deref(), Some(beans.to_string_lossy().replace('\\', "/").as_str()));
        assert_eq!(beans_e[0].line, None);

        // actions: primary = qualified name, secondary = resolved class (C1 chain), file =
        // the <action> fragment.
        let actions_e = config_entries_of(&cfg, "actions");
        assert_eq!(actions_e.len(), 1);
        assert_eq!(actions_e[0].primary, "/do/Cat/viewTree");
        assert_eq!(actions_e[0].secondary, "com.x.CategoryAction");
        assert_eq!(
            actions_e[0].file.as_deref(),
            Some(struts.to_string_lossy().replace('\\', "/").as_str())
        );

        // relations: at least the action→class edge, labelled `from → to` + a kind.
        let rel_e = config_entries_of(&cfg, "relations");
        assert!(rel_e.iter().any(|r| r.primary == "/do/Cat/viewTree → categoryAction"
            && r.secondary == "action-class"
            && r.file.is_none()));

        // an unrecognised kind → empty.
        assert!(config_entries_of(&cfg, "nope").is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jar_entries_lists_existing_jars_by_filename() {
        let root = std::env::temp_dir().join(format!(
            "bennu-jarentries-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        // Two real jar files + one non-existent path — only the existing `.jar`s count.
        let a = target.join("struts2-core-2.5.30.jar");
        let b = target.join("spring-beans-5.3.jar");
        std::fs::write(&a, b"jar").unwrap();
        std::fs::write(&b, b"jar").unwrap();
        let missing = target.join("gone.jar");
        let raw = format!("{};{};{}", a.display(), b.display(), missing.display());
        std::fs::write(target.join("bennu-classpath.txt"), raw).unwrap();

        let entries = jar_entries(&root);
        assert_eq!(entries.len(), 2, "only the two existing jars are listed");
        assert!(entries.iter().any(|e| e.primary == "struts2-core-2.5.30.jar"
            && e.secondary == a.to_string_lossy().replace('\\', "/")
            && e.file.is_none()));
        assert!(entries.iter().any(|e| e.primary == "spring-beans-5.3.jar"));

        // No classpath file → empty.
        let _ = std::fs::remove_dir_all(&root);
        assert!(jar_entries(&root).is_empty());
    }

    #[test]
    fn action_ref_matches_absolute_and_bare() {
        // Absolute needle → exact only.
        assert!(action_ref_matches("/do/Cat/viewTree", "/do/Cat/viewTree"));
        assert!(!action_ref_matches("/do/Other/viewTree", "/do/Cat/viewTree"));
        // Bare needle → trailing-segment match across namespaces.
        assert!(action_ref_matches("/do/Cat/viewTree", "viewTree"));
        assert!(action_ref_matches("viewTree", "viewTree"));
        assert!(!action_ref_matches("/do/Cat/viewTreeX", "viewTree"));
    }

    #[test]
    fn action_usage_hits_finds_refs_across_sources() {
        // Two JSPs reference the same action (one absolute, one with a `.action` suffix); a
        // third references a different action; a computed ref is never a hit.
        let a = (
            "proj/a.jsp".to_string(),
            "<s:url action=\"/do/Cat/viewTree\"/>\n<s:a action=\"/do/Cat/viewTree.action\">x</s:a>"
                .to_string(),
        );
        let b = ("proj/b.jsp".to_string(), "<s:form action=\"/do/Cat/other\">y</s:form>".to_string());
        let c = ("proj/c.jsp".to_string(), "<s:url action=\"%{bean.url}\"/>".to_string());
        let sources = vec![a, b, c];

        let hits = action_usage_hits(&sources, "/do/Cat/viewTree");
        assert_eq!(hits.len(), 2, "both refs in a.jsp match (raw suffix included): {hits:?}");
        assert!(hits.iter().all(|h| h.file == "proj/a.jsp"));
        // Preview carries the source line; line/col are 1-based.
        assert!(hits[0].line >= 1 && hits[0].col >= 1);
        assert!(hits.iter().any(|h| h.preview.contains("viewTree")));

        // A bare needle surfaces the family (trailing-segment match).
        assert_eq!(action_usage_hits(&sources, "viewTree").len(), 2);
        // No match → empty (not an error).
        assert!(action_usage_hits(&sources, "/do/Cat/ghost").is_empty());
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
