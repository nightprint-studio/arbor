//! "Is `target/classes` still the program the sources describe?" — the model a launch consults
//! before deciding whether to invoke Maven, and what to wipe first when Maven alone would get it
//! wrong.
//!
//! ## Why Maven's own up-to-date check is not enough
//!
//! `mvn compile` is incremental, and its incrementality has three holes that every one of them
//! ends in "run `mvn clean` by hand":
//!
//! 1. **Across invocations it cannot see an upstream change.** The compiler plugin decides a module's
//!    dependencies changed by looking for files written *during the current Maven session*. Launch
//!    `web` (compiling `core` + `web`), edit `core`, launch `batch` (compiling `core` + `batch`), then
//!    launch `web` again: `core` is already compiled, so nothing in that session is new, `web` is
//!    left as it was, and the JVM dies on `NoSuchMethodError` against classes that look current.
//! 2. **A deleted or renamed source leaves its `.class` behind.** Spring then scans a `@Component`
//!    that no longer exists, a `ServiceLoader` finds a provider nobody wrote.
//! 3. **Annotation-processor output outlives its input.** MapStruct's `FooMapperImpl.java` in
//!    `target/generated-sources/annotations` is a source root of the next compile, so a mapper renamed
//!    or a DTO deleted breaks the build with errors in a file nobody wrote.
//!
//! So this keeps, per module, what its inputs were when it last compiled successfully — its own
//! `src/main` and poms, and a hash of its upstream modules' code — and turns the difference into a
//! [`Plan`]: whether to compile at all, which modules to rebuild from nothing (their classes,
//! generated sources and compiler status wiped first), and which copied resources to remove.
//!
//! **Stats, not reads**: sizes and modification times, so assessing a large reactor costs
//! milliseconds against Maven's seconds. The record is kept between sessions, and checked before it
//! is believed — see the persistence section; [`forget`] drops it, disk included.
//!
//! [`plan`] is pure and unit-tested; the rest is filesystem glue around it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use bennu_proto::prelude::BuildDiagnostic;
use serde::{Deserialize, Serialize};

use crate::reactor::{self, ReactorModule};

/// A file as the model sees it: size and modification time (nanoseconds since the epoch).
type FileStamp = (u64, u128);

/// What a module compiles from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModuleInputs {
    /// The module's own `pom.xml` and its aggregators' — a managed version or a compiler flag edited
    /// in a parent changes how the module builds.
    pub(crate) poms: u64,
    /// Every file under `src/main` except `webapp/` (served, not compiled), by its path relative to
    /// `src/main` with `/` separators — `java/com/acme/App.java`, `resources/application.yml`.
    pub(crate) sources: BTreeMap<String, FileStamp>,
}

impl ModuleInputs {
    /// Whether the module compiles anything at all — a reactor root or a `pom` aggregator does not,
    /// and must never be "missing its output".
    pub(crate) fn has_code(&self) -> bool {
        self.sources.keys().any(|k| is_code(k))
    }

    /// A hash of what DOWNSTREAM modules compile against: the poms and every non-resource source.
    /// A resource edited upstream changes no signature, so it is left out.
    fn code_hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.poms.hash(&mut h);
        for (k, v) in self.sources.iter().filter(|(k, _)| is_code(k)) {
            k.hash(&mut h);
            v.hash(&mut h);
        }
        h.finish()
    }

    /// Whether a compiled source present in `self` is gone from `now` — deleted, or renamed.
    fn lost_code_in(&self, now: &ModuleInputs) -> bool {
        self.sources.keys().any(|k| is_code(k) && !now.sources.contains_key(k))
    }

    /// Resources present in `self` and gone from `now`, relative to `resources/`.
    fn lost_resources_in(&self, now: &ModuleInputs) -> Vec<String> {
        self.sources
            .keys()
            .filter(|k| !now.sources.contains_key(*k))
            .filter_map(|k| k.strip_prefix("resources/"))
            .map(str::to_string)
            .collect()
    }
}

/// Anything under `src/main` other than resources is compiled (Java, Kotlin, protobuf, ANTLR…).
fn is_code(rel: &str) -> bool {
    !rel.starts_with("resources/")
}

/// A module as it was when it last compiled successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledState {
    pub(crate) inputs: ModuleInputs,
    /// [`ModuleInputs::code_hash`] of its upstream modules, in build order, at that moment.
    pub(crate) upstream: u64,
}

/// Why a module is rebuilt from nothing — said in the build log, because a compile that takes
/// longer than the last one deserves a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RebuildReason {
    /// A module it is built from changed since it was compiled.
    UpstreamChanged,
    /// A source was deleted or renamed.
    SourceRemoved,
    /// Its pom, or a parent's, changed.
    PomChanged,
    /// Its classes are older than an upstream module's, and this session has no record of it.
    OlderThanUpstream,
    /// A module it is built from is itself being rebuilt.
    UpstreamRebuilt,
    /// The compile failed inside generated sources, or on a duplicate class.
    StaleGeneratedSources,
}

impl RebuildReason {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::UpstreamChanged => "a module it is built from has changed",
            Self::SourceRemoved => "a source file was deleted or renamed",
            Self::PomChanged => "its pom or a parent pom has changed",
            Self::OlderThanUpstream => "its classes are older than those of a module it is built from",
            Self::UpstreamRebuilt => "a module it is built from is being rebuilt",
            Self::StaleGeneratedSources => "generated sources left by an earlier compile no longer match",
        }
    }
}

/// What to do before (and whether to) compile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The modules the compile covers, upstream first.
    pub(crate) scope: Vec<String>,
    /// `false` = nothing changed: no Maven at all.
    pub(crate) needs_compile: bool,
    /// Modules whose compiled output is wiped so the compile starts from nothing.
    pub(crate) rebuild: Vec<(String, RebuildReason)>,
    /// Copied resources whose source is gone: (module, path relative to `target/classes`).
    pub(crate) removed_resources: Vec<(String, String)>,
}

/// Decide what a compile of `scope` needs.
///
/// - `upstream[m]`: the modules `m` is built from, upstream first (see [`reactor::upstream_of`]).
/// - `current[m]` / `previous[m]`: its inputs now, and its state at its last successful compile.
/// - `has_output(m)`: whether `m/target/classes` exists.
/// - `newest_output(m)`: the newest `.class` in it — asked only for a module with no record.
///
/// An empty `scope` is a project shape this model does not understand, and always compiles.
pub(crate) fn plan(
    scope: &[String],
    upstream: &HashMap<String, Vec<String>>,
    current: &HashMap<String, ModuleInputs>,
    previous: &HashMap<String, CompiledState>,
    has_output: &dyn Fn(&str) -> bool,
    newest_output: &dyn Fn(&str) -> Option<u128>,
) -> Plan {
    let mut out = Plan { scope: scope.to_vec(), needs_compile: scope.is_empty(), ..Plan::default() };
    let empty = ModuleInputs::default();
    let no_upstream = Vec::new();

    for m in scope {
        let cur = current.get(m).unwrap_or(&empty);
        let ups = upstream.get(m).unwrap_or(&no_upstream);
        let mut reason: Option<RebuildReason> = None;

        match previous.get(m) {
            None => {
                out.needs_compile = true;
                if cur.has_code() && has_output(m.as_str()) {
                    if let Some(mine) = newest_output(m.as_str()) {
                        if ups.iter().any(|u| newest_output(u.as_str()).is_some_and(|t| t > mine)) {
                            reason = Some(RebuildReason::OlderThanUpstream);
                        }
                    }
                }
            }
            Some(prev) => {
                let upstream_changed = prev.upstream != upstream_hash(ups, current);
                if prev.inputs != *cur || upstream_changed || (cur.has_code() && !has_output(m.as_str())) {
                    out.needs_compile = true;
                }
                if cur.has_code() {
                    reason = if prev.inputs.lost_code_in(cur) {
                        Some(RebuildReason::SourceRemoved)
                    } else if prev.inputs.poms != cur.poms {
                        Some(RebuildReason::PomChanged)
                    } else if upstream_changed {
                        Some(RebuildReason::UpstreamChanged)
                    } else {
                        None
                    };
                }
                for res in prev.inputs.lost_resources_in(cur) {
                    out.removed_resources.push((m.clone(), res));
                }
            }
        }

        // A module built from one that starts over starts over too: whatever made the upstream
        // rebuild (a pom, a deleted type) can have changed what this one compiled against.
        if reason.is_none() && cur.has_code() && ups.iter().any(|u| out.rebuild.iter().any(|(r, _)| r == u)) {
            reason = Some(RebuildReason::UpstreamRebuilt);
        }
        if let Some(r) = reason {
            out.needs_compile = true;
            out.rebuild.push((m.clone(), r));
        }
    }
    out
}

/// The hash of `ups`' code, in order — what [`CompiledState::upstream`] records.
fn upstream_hash(ups: &[String], current: &HashMap<String, ModuleInputs>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for u in ups {
        u.hash(&mut h);
        current.get(u).map(ModuleInputs::code_hash).hash(&mut h);
    }
    h.finish()
}

/// Whether a FAILED compile is the stale-generated-sources failure — an error inside
/// `target/generated-sources`, or javac's `duplicate class` — which a rebuild from nothing fixes.
pub(crate) fn is_stale_generated_failure(diagnostics: &[BuildDiagnostic], raw: &str) -> bool {
    let in_generated = |s: &str| s.replace('\\', "/").contains("/target/generated-sources/");
    diagnostics.iter().any(|d| {
        d.severity == "error"
            && (d.file.as_deref().is_some_and(in_generated) || d.message.contains("duplicate class"))
    }) || raw.lines().any(|l| l.contains("duplicate class:"))
}

// ── the glue: one assessment of a project on disk ───────────────────────────────

/// A [`Plan`] plus what is needed to record it as compiled afterwards.
pub(crate) struct Assessment {
    root: String,
    pub(crate) plan: Plan,
    current: HashMap<String, ModuleInputs>,
    upstream: HashMap<String, Vec<String>>,
}

/// Assess a compile of `module` (`None` = the whole reactor) under `root`.
///
/// The inputs are read BEFORE the compile, and that is what gets recorded on success: a build
/// writes into `target/`, and reading afterwards could record an edit made while Maven ran as
/// already compiled.
pub(crate) fn assess(root: &Path, module: Option<&str>) -> Assessment {
    let modules = reactor::load(root);
    let scope = scope_of(&modules, module);
    // What an earlier session recorded about these modules, where `target/classes` is still
    // exactly what it left — see the persistence section.
    adopt_disk_records(root, &root_key(root), &scope);
    let upstream: HashMap<String, Vec<String>> = scope
        .iter()
        .map(|m| (m.clone(), reactor::upstream_of(&modules, m)))
        .collect();
    let current: HashMap<String, ModuleInputs> =
        scope.iter().map(|m| (m.clone(), inputs_of(root, m))).collect();

    let key = root_key(root);
    let previous = state()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key)
        .map(|p| p.modules.clone())
        .unwrap_or_default();

    let newest: RefCell<HashMap<String, Option<u128>>> = RefCell::new(HashMap::new());
    let newest_output = |m: &str| -> Option<u128> {
        if let Some(hit) = newest.borrow().get(m) {
            return *hit;
        }
        let value = newest_class_mtime(&classes_dir(root, m));
        newest.borrow_mut().insert(m.to_string(), value);
        value
    };
    let has_output = |m: &str| classes_dir(root, m).is_dir();

    let plan = plan(&scope, &upstream, &current, &previous, &has_output, &newest_output);
    Assessment { root: key, plan, current, upstream }
}

/// The modules a compile of `module` covers, upstream first. A module the reactor does not list
/// (its own pom, outside `<modules>`) is compiled alone.
fn scope_of(modules: &[ReactorModule], module: Option<&str>) -> Vec<String> {
    match module.map(reactor::normalize_rel) {
        Some(m) => {
            let mut scope = reactor::upstream_of(modules, &m);
            scope.push(m);
            scope
        }
        None => {
            let mut scope: Vec<String> = Vec::new();
            for m in modules {
                for u in reactor::upstream_of(modules, &m.rel).into_iter().chain([m.rel.clone()]) {
                    if !scope.contains(&u) {
                        scope.push(u);
                    }
                }
            }
            scope
        }
    }
}

/// Carry out the wiping half of a plan. Returns one build-log line per thing done, so the log says
/// why this compile is slower than the last one. Best-effort: a file that cannot be deleted (held by
/// another process) is reported and the compile goes ahead.
pub(crate) fn apply(root: &Path, plan: &Plan) -> Vec<String> {
    let mut log = Vec::new();
    for (m, reason) in &plan.rebuild {
        let target = module_dir(root, m).join("target");
        let removed = remove_class_files(&target.join("classes"));
        let _ = std::fs::remove_dir_all(target.join("generated-sources").join("annotations"));
        let _ = std::fs::remove_dir_all(target.join("maven-status"));
        let name = if m.is_empty() { "the root module" } else { m.as_str() };
        log.push(format!(
            "Rebuilding {name} from scratch — {}{}.",
            reason.describe(),
            match removed {
                Ok(0) => String::new(),
                Ok(n) => format!(" (removed {n} compiled class file(s))"),
                Err(e) => format!(" (could not remove every old class file: {e})"),
            }
        ));
    }
    for (m, res) in &plan.removed_resources {
        let copied = classes_dir(root, m).join(res);
        if copied.is_file() && std::fs::remove_file(&copied).is_ok() {
            log.push(format!("Removed {res} from {}: its source is gone.", display_module(m)));
        }
    }
    log
}

/// Force every module in the scope to rebuild — the retry after [`is_stale_generated_failure`].
pub(crate) fn rebuild_all(assessment: &mut Assessment) {
    let scope = assessment.plan.scope.clone();
    assessment.plan.rebuild = scope
        .into_iter()
        .filter(|m| assessment.current.get(m).is_some_and(ModuleInputs::has_code))
        .map(|m| (m, RebuildReason::StaleGeneratedSources))
        .collect();
    assessment.plan.removed_resources.clear();
    assessment.plan.needs_compile = true;
}

/// Record a successful compile: every module in its scope is now what its inputs were.
pub(crate) fn record_success(assessment: &Assessment, compiled: bool) {
    let root = Path::new(&assessment.root);
    let on_disk = {
        let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
        let project = guard.entry(assessment.root.clone()).or_default();
        for m in &assessment.plan.scope {
            let Some(inputs) = assessment.current.get(m) else { continue };
            let ups = assessment.upstream.get(m).map(Vec::as_slice).unwrap_or(&[]);
            project.modules.insert(
                m.clone(),
                CompiledState { inputs: inputs.clone(), upstream: upstream_hash(ups, &assessment.current) },
            );
            // The output as the compile left it: what a later session compares `target/classes` to.
            project.outputs.insert(m.clone(), newest_class_mtime(&classes_dir(root, m)));
        }
        if compiled {
            project.generation += 1;
        }
        to_disk(project)
    };
    save_disk_state(&assessment.root, &on_disk);
}

/// How many successful compiles that actually ran a compiler this project has had this session —
/// a token that changes whenever `target/classes` may have.
pub(crate) fn generation(root: &Path) -> u64 {
    state()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&root_key(root))
        .map(|p| p.generation)
        .unwrap_or(0)
}

/// Forget everything about a project, so its next compile runs Maven. Called on reindex — the
/// moment the user has said not to trust what we remember.
pub(crate) fn forget(root: &Path) {
    let key = root_key(root);
    state().lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
    // Also what an earlier session wrote: "do not trust what you remember" includes the disk. The
    // key is marked as read so the file is not picked up again after it is deleted.
    pending().lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
    read_keys().lock().unwrap_or_else(|p| p.into_inner()).insert(key.clone());
    let _ = std::fs::remove_file(disk_path(&key));
}

#[derive(Default)]
struct ProjectState {
    modules: HashMap<String, CompiledState>,
    generation: u64,
    /// The newest `.class` of each module when it was recorded — persisted with it, so a later
    /// session can tell that `target/classes` is still what that compile produced.
    outputs: HashMap<String, Option<u128>>,
}

// ── persistence across sessions ─────────────────────────────────────────────────
//
// The record used to live in memory only, on the reasoning that a restart is the moment nothing
// should be trusted. The price was that the first launch of every project in every session ran
// Maven even with nothing changed — seconds of JVM start-up and pom resolution to conclude "up to
// date". What makes it safe to remember is that the record is never taken on faith:
//
// * the sources and poms are stat'ed again and compared, exactly as within a session — an edit, a
//   branch switch or a deletion made while the app was closed all show up as a difference;
// * a module's record is adopted only if the newest `.class` under its `target/classes` is the one
//   the compile recorded — so a `mvn clean`, a build from a terminal or an IDE writing other
//   classes into `target/` all make the record void, and the launch compiles.
//
// A manual re-index forgets the disk record too (`forget`).

const DISK_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
struct DiskState {
    version: u32,
    generation: u64,
    modules: BTreeMap<String, DiskModule>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DiskModule {
    poms: u64,
    sources: BTreeMap<String, (u64, u64)>,
    upstream: u64,
    /// The newest `.class` (ns since the epoch) when it was recorded; `None` for a module with none.
    output: Option<u64>,
}

/// Records read from disk and not yet adopted: a module is adopted when a compile that covers it is
/// assessed, after its output has been checked.
fn pending() -> &'static Mutex<HashMap<String, DiskState>> {
    static PENDING: OnceLock<Mutex<HashMap<String, DiskState>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The projects whose file has been read (or deliberately skipped) this session.
fn read_keys() -> &'static Mutex<HashSet<String>> {
    static READ: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    READ.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Where a project's record is kept: out of the repository, beside the other bennu data.
fn disk_path(key: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("build-state")
        .join(format!("{:016x}.json", fnv1a(key)))
}

/// FNV-1a — a file name has to mean the same thing in every session, and `DefaultHasher` makes no
/// such promise across toolchains.
pub(crate) fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn to_disk(project: &ProjectState) -> DiskState {
    DiskState {
        version: DISK_VERSION,
        generation: project.generation,
        modules: project
            .modules
            .iter()
            .map(|(name, compiled)| {
                let disk = DiskModule {
                    poms: compiled.inputs.poms,
                    sources: compiled
                        .inputs
                        .sources
                        .iter()
                        .map(|(k, (len, mtime))| (k.clone(), (*len, u64::try_from(*mtime).unwrap_or(u64::MAX))))
                        .collect(),
                    upstream: compiled.upstream,
                    output: project
                        .outputs
                        .get(name)
                        .copied()
                        .flatten()
                        .map(|t| u64::try_from(t).unwrap_or(u64::MAX)),
                };
                (name.clone(), disk)
            })
            .collect(),
    }
}

fn from_disk(module: &DiskModule) -> CompiledState {
    CompiledState {
        inputs: ModuleInputs {
            poms: module.poms,
            sources: module
                .sources
                .iter()
                .map(|(k, (len, mtime))| (k.clone(), (*len, u128::from(*mtime))))
                .collect(),
        },
        upstream: module.upstream,
    }
}

/// Whether a recorded output stamp is still what `target/classes` holds.
fn output_still_matches(recorded: Option<u64>, actual: Option<u128>) -> bool {
    recorded.map(u128::from) == actual
}

/// Best-effort write: a record that cannot be saved costs the next session one Maven run, never a
/// failed build. Written to a sibling file and renamed, so a crash cannot leave half a record.
fn save_disk_state(key: &str, disk: &DiskState) {
    let path = disk_path(key);
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(json) = serde_json::to_vec(disk) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn load_disk_state(key: &str) -> Option<DiskState> {
    let bytes = std::fs::read(disk_path(key)).ok()?;
    let disk: DiskState = serde_json::from_slice(&bytes).ok()?;
    (disk.version == DISK_VERSION).then_some(disk)
}

/// Bring the modules of `scope` that an earlier session recorded into the live state, for the ones
/// whose `target/classes` still holds what that compile produced. A module already known this
/// session is left alone: the live record is newer.
fn adopt_disk_records(root: &Path, key: &str, scope: &[String]) {
    {
        let mut read = read_keys().lock().unwrap_or_else(|p| p.into_inner());
        if read.insert(key.to_string()) {
            if let Some(disk) = load_disk_state(key) {
                pending().lock().unwrap_or_else(|p| p.into_inner()).insert(key.to_string(), disk);
            }
        }
    }
    let mut waiting = pending().lock().unwrap_or_else(|p| p.into_inner());
    let Some(disk) = waiting.get_mut(key) else { return };
    let mut live = state().lock().unwrap_or_else(|p| p.into_inner());
    let project = live.entry(key.to_string()).or_default();
    project.generation = project.generation.max(disk.generation);
    for module in scope {
        let Some(recorded) = disk.modules.remove(module) else { continue };
        if project.modules.contains_key(module) {
            continue;
        }
        let actual = newest_class_mtime(&classes_dir(root, module));
        if output_still_matches(recorded.output, actual) {
            project.outputs.insert(module.clone(), actual);
            project.modules.insert(module.clone(), from_disk(&recorded));
        }
    }
    if disk.modules.is_empty() {
        waiting.remove(key);
    }
}

fn state() -> &'static Mutex<HashMap<String, ProjectState>> {
    static STATE: OnceLock<Mutex<HashMap<String, ProjectState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn root_key(root: &Path) -> String {
    root.display().to_string()
}

fn module_dir(root: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() { root.to_path_buf() } else { root.join(rel) }
}

/// `<module>/target/classes`.
pub(crate) fn classes_dir(root: &Path, rel: &str) -> PathBuf {
    module_dir(root, rel).join("target").join("classes")
}

fn display_module(rel: &str) -> &str {
    if rel.is_empty() { "the root module" } else { rel }
}

/// Read a module's [`ModuleInputs`] off disk.
fn inputs_of(root: &Path, rel: &str) -> ModuleInputs {
    let mut poms = std::collections::hash_map::DefaultHasher::new();
    for dir in reactor::ancestors_of(rel).iter().chain(std::iter::once(&rel.to_string())) {
        let pom = module_dir(root, dir).join("pom.xml");
        dir.hash(&mut poms);
        stamp_of(&pom).hash(&mut poms);
    }
    let main = module_dir(root, rel).join("src").join("main");
    let mut sources = BTreeMap::new();
    collect_sources(&main, &main, &mut sources);
    ModuleInputs { poms: poms.finish(), sources }
}

fn stamp_of(path: &Path) -> Option<FileStamp> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some((md.len(), mtime))
}

/// Every file under `dir`, keyed relative to `base`. Skips dot-directories, `node_modules` and the
/// top-level `webapp` (served as it is, never compiled).
fn collect_sources(base: &Path, dir: &Path, out: &mut BTreeMap<String, FileStamp>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if name.starts_with('.') || name == "node_modules" || (dir == base && name == "webapp") {
                continue;
            }
            collect_sources(base, &p, out);
        } else if let Some(stamp) = stamp_of(&p) {
            if let Ok(rel) = p.strip_prefix(base) {
                out.insert(rel.to_string_lossy().replace('\\', "/"), stamp);
            }
        }
    }
}

/// The newest `.class` under `dir`, as nanoseconds since the epoch.
fn newest_class_mtime(dir: &Path) -> Option<u128> {
    let mut newest: Option<u128> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("class") {
                if let Some((_, t)) = stamp_of(&p) {
                    newest = Some(newest.map_or(t, |n| n.max(t)));
                }
            }
        }
    }
    newest
}

/// Delete every `.class` under `dir` — resources stay, Maven copies them again anyway. Returns how
/// many were removed; the first failure is reported after trying the rest.
fn remove_class_files(dir: &Path) -> Result<usize, String> {
    let mut removed = 0usize;
    let mut first_error: Option<String> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("class") {
                match std::fs::remove_file(&p) {
                    Ok(()) => removed += 1,
                    Err(err) => {
                        first_error.get_or_insert_with(|| format!("{}: {err}", p.display()));
                    }
                }
            }
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(removed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── persistence ─────────────────────────────────────────────────────────────

    fn project_with(module: &str, files: &[(&str, u128)], output: Option<u128>) -> ProjectState {
        let mut project = ProjectState::default();
        project.modules.insert(
            module.to_string(),
            CompiledState { inputs: inputs(7, files), upstream: 42 },
        );
        project.outputs.insert(module.to_string(), output);
        project.generation = 3;
        project
    }

    #[test]
    fn a_record_survives_the_trip_to_disk_and_back() {
        let project = project_with("web", &[("java/A.java", 1_700_000_000_123_456_789), ("resources/a.yml", 5)], Some(99));
        let json = serde_json::to_string(&to_disk(&project)).unwrap();
        let disk: DiskState = serde_json::from_str(&json).unwrap();
        assert_eq!(disk.generation, 3);
        let back = from_disk(&disk.modules["web"]);
        assert_eq!(back, project.modules["web"]);
        assert_eq!(disk.modules["web"].output, Some(99));
    }

    #[test]
    fn a_module_without_classes_records_no_output() {
        let project = project_with("parent", &[], None);
        assert_eq!(to_disk(&project).modules["parent"].output, None);
    }

    /// A recorded output is trusted only while `target/classes` still holds it.
    #[test]
    fn a_changed_or_missing_output_voids_the_record() {
        assert!(output_still_matches(Some(10), Some(10)));
        assert!(!output_still_matches(Some(10), Some(11)), "classes written by someone else");
        assert!(!output_still_matches(Some(10), None), "target/classes was cleaned");
        assert!(!output_still_matches(None, Some(10)), "classes appeared that the record does not know");
        assert!(output_still_matches(None, None), "an aggregator with no classes");
    }

    #[test]
    fn a_file_name_means_the_same_thing_every_session() {
        // Pinned values: if the hash ever changes, every persisted record silently stops being found.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv1a("/p/one"), fnv1a("/p/two"));
    }

    #[test]
    fn a_record_from_another_format_version_is_ignored() {
        let mut disk = to_disk(&project_with("web", &[], None));
        disk.version = DISK_VERSION + 1;
        let json = serde_json::to_vec(&disk).unwrap();
        let parsed: DiskState = serde_json::from_slice(&json).unwrap();
        assert_ne!(parsed.version, DISK_VERSION);
    }

    #[test]
    fn a_timestamp_too_big_for_the_file_is_never_mistaken_for_a_match() {
        let mut project = project_with("web", &[], Some(u128::MAX));
        project.generation = 0;
        let disk = to_disk(&project);
        // Saturated on the way out; the real output can never equal it, so the record is void.
        assert!(!output_still_matches(disk.modules["web"].output, Some(u128::MAX)));
    }

    fn inputs(poms: u64, files: &[(&str, u128)]) -> ModuleInputs {
        ModuleInputs {
            poms,
            sources: files.iter().map(|(k, t)| (k.to_string(), (10, *t))).collect(),
        }
    }

    /// core ← web; both compiled, nothing since.
    struct Fixture {
        scope: Vec<String>,
        upstream: HashMap<String, Vec<String>>,
        current: HashMap<String, ModuleInputs>,
        previous: HashMap<String, CompiledState>,
    }

    fn fixture() -> Fixture {
        let scope = vec!["core".to_string(), "web".to_string()];
        let upstream: HashMap<String, Vec<String>> =
            [("core".to_string(), vec![]), ("web".to_string(), vec!["core".to_string()])].into();
        let current: HashMap<String, ModuleInputs> = [
            ("core".to_string(), inputs(1, &[("java/c/Core.java", 1), ("resources/core.properties", 1)])),
            ("web".to_string(), inputs(2, &[("java/w/Web.java", 1), ("resources/application.yml", 1)])),
        ]
        .into();
        let mut f = Fixture { scope, upstream, current, previous: HashMap::new() };
        f.previous = compiled_now(&f);
        f
    }

    /// The states a successful compile of the fixture records right now.
    fn compiled_now(f: &Fixture) -> HashMap<String, CompiledState> {
        f.scope
            .iter()
            .map(|m| {
                (
                    m.clone(),
                    CompiledState {
                        inputs: f.current[m].clone(),
                        upstream: upstream_hash(&f.upstream[m], &f.current),
                    },
                )
            })
            .collect()
    }

    fn run(f: &Fixture) -> Plan {
        plan(&f.scope, &f.upstream, &f.current, &f.previous, &|_| true, &|_| Some(0))
    }

    fn rebuilt(p: &Plan) -> Vec<(&str, RebuildReason)> {
        p.rebuild.iter().map(|(m, r)| (m.as_str(), *r)).collect()
    }

    #[test]
    fn nothing_changed_means_no_maven() {
        let p = run(&fixture());
        assert!(!p.needs_compile);
        assert!(p.rebuild.is_empty());
    }

    /// The cross-invocation hole: `core` was recompiled by another launch, so Maven would find it
    /// up to date and leave `web` compiled against the old one.
    #[test]
    fn an_upstream_edit_rebuilds_the_downstream_module() {
        let mut f = fixture();
        f.current.get_mut("core").unwrap().sources.insert("java/c/Core.java".into(), (12, 2));
        let p = run(&f);
        assert!(p.needs_compile);
        assert_eq!(rebuilt(&p), vec![("web", RebuildReason::UpstreamChanged)]);
    }

    #[test]
    fn an_upstream_resource_edit_does_not_rebuild_downstream() {
        let mut f = fixture();
        f.current.get_mut("core").unwrap().sources.insert("resources/core.properties".into(), (99, 5));
        let p = run(&f);
        assert!(p.needs_compile, "the resource still has to be copied");
        assert!(p.rebuild.is_empty(), "{:?}", p.rebuild);
    }

    /// A deleted source leaves its class behind; the module starts over, and so does what is built
    /// from it — the type may have been what `web` compiled against.
    #[test]
    fn a_deleted_source_rebuilds_its_module_and_its_dependents() {
        let mut f = fixture();
        let core = f.current.get_mut("core").unwrap();
        core.sources.remove("java/c/Core.java");
        core.sources.insert("java/c/Renamed.java".into(), (10, 3));
        let p = run(&f);
        assert_eq!(
            rebuilt(&p),
            vec![("core", RebuildReason::SourceRemoved), ("web", RebuildReason::UpstreamChanged)]
        );
    }

    #[test]
    fn a_pom_change_rebuilds_the_module() {
        let mut f = fixture();
        f.current.get_mut("web").unwrap().poms = 77;
        let p = run(&f);
        assert_eq!(rebuilt(&p), vec![("web", RebuildReason::PomChanged)]);
    }

    #[test]
    fn a_deleted_resource_is_removed_from_the_output() {
        let mut f = fixture();
        f.current.get_mut("web").unwrap().sources.remove("resources/application.yml");
        let p = run(&f);
        assert!(p.needs_compile);
        assert!(p.rebuild.is_empty());
        assert_eq!(p.removed_resources, vec![("web".to_string(), "application.yml".to_string())]);
    }

    /// `mvn clean` in a terminal: the record says compiled, the disk says otherwise.
    #[test]
    fn missing_output_compiles_even_when_nothing_changed() {
        let f = fixture();
        let p = plan(&f.scope, &f.upstream, &f.current, &f.previous, &|m| m != "core", &|_| Some(0));
        assert!(p.needs_compile);
    }

    /// No record this session: compile, and rebuild a module whose classes predate its upstream's —
    /// the only evidence left that it was compiled against something older.
    #[test]
    fn without_a_record_older_classes_than_upstream_rebuild() {
        let mut f = fixture();
        f.previous.clear();
        let newest = |m: &str| Some(if m == "core" { 200 } else { 100 });
        let p = plan(&f.scope, &f.upstream, &f.current, &f.previous, &|_| true, &newest);
        assert!(p.needs_compile);
        assert_eq!(rebuilt(&p), vec![("web", RebuildReason::OlderThanUpstream)]);

        let fresh = |m: &str| Some(if m == "core" { 100 } else { 200 });
        let p = plan(&f.scope, &f.upstream, &f.current, &f.previous, &|_| true, &fresh);
        assert!(p.needs_compile);
        assert!(p.rebuild.is_empty());
    }

    /// A module that compiles nothing (a `pom` aggregator) is never rebuilt and never "missing".
    #[test]
    fn a_module_without_code_is_left_alone() {
        let scope = vec!["".to_string()];
        let upstream: HashMap<String, Vec<String>> = [("".to_string(), vec![])].into();
        let current: HashMap<String, ModuleInputs> = [("".to_string(), inputs(1, &[]))].into();
        let previous: HashMap<String, CompiledState> = [(
            "".to_string(),
            CompiledState { inputs: inputs(0, &[]), upstream: upstream_hash(&[], &current) },
        )]
        .into();
        let p = plan(&scope, &upstream, &current, &previous, &|_| false, &|_| None);
        assert!(p.needs_compile, "the pom changed");
        assert!(p.rebuild.is_empty());
    }

    #[test]
    fn an_empty_scope_always_compiles() {
        let p = plan(&[], &HashMap::new(), &HashMap::new(), &HashMap::new(), &|_| true, &|_| None);
        assert!(p.needs_compile);
    }

    #[test]
    fn stale_generated_failures_are_recognised() {
        let d = |file: &str, message: &str| BuildDiagnostic {
            file: Some(file.into()),
            line: Some(1),
            col: None,
            severity: "error".into(),
            message: message.into(),
        };
        assert!(is_stale_generated_failure(
            &[d(r"C:\p\core\target\generated-sources\annotations\it\FooMapperImpl.java", "cannot find symbol")],
            "",
        ));
        assert!(is_stale_generated_failure(&[d("/p/Foo.java", "duplicate class: it.Foo")], ""));
        assert!(!is_stale_generated_failure(&[d("/p/src/main/java/Foo.java", "cannot find symbol")], ""));
    }

    /// The scope of a whole-reactor build lists every module once, dependencies first.
    #[test]
    fn a_whole_reactor_scope_is_topological() {
        let m = |rel: &str, a: &str, deps: &[&str]| ReactorModule {
            rel: rel.into(),
            artifact_id: a.into(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
        };
        let modules = vec![m("", "parent", &[]), m("web", "web", &["core"]), m("core", "core", &[])];
        assert_eq!(scope_of(&modules, None), vec!["", "core", "web"]);
        assert_eq!(scope_of(&modules, Some(r"web\")), vec!["core", "web"]);
        assert_eq!(scope_of(&modules, Some("elsewhere")), vec!["elsewhere"]);
    }

    /// End to end on disk: a record, an edit upstream, and the downstream classes wiped.
    #[test]
    fn assess_and_apply_on_a_real_tree() {
        let dir = std::env::temp_dir().join(format!(
            "bennu-freshness-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |rel: &str, text: &str| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
        };
        write("pom.xml", "<project><artifactId>parent</artifactId><modules><module>core</module><module>web</module></modules></project>");
        write("core/pom.xml", "<project><artifactId>core</artifactId></project>");
        write("web/pom.xml", "<project><artifactId>web</artifactId><dependencies><dependency><groupId>it</groupId><artifactId>core</artifactId></dependency></dependencies></project>");
        write("core/src/main/java/c/Core.java", "class Core {}\n");
        write("web/src/main/java/w/Web.java", "class Web {}\n");
        write("web/src/main/resources/gone.yml", "a: 1\n");
        write("core/target/classes/c/Core.class", "x");
        write("web/target/classes/w/Web.class", "x");
        write("web/target/classes/gone.yml", "a: 1\n");
        write("web/target/generated-sources/annotations/w/WebMapperImpl.java", "class WebMapperImpl {}\n");

        forget(&dir);
        let first = assess(&dir, Some("web"));
        assert_eq!(first.plan.scope, vec!["core", "web"]);
        assert!(first.plan.needs_compile, "no record this session");
        record_success(&first, true);
        assert_eq!(generation(&dir), 1);
        assert!(!assess(&dir, Some("web")).plan.needs_compile, "recorded and unchanged");

        write("core/src/main/java/c/Core.java", "class Core { void added() {} }\n");
        std::fs::remove_file(dir.join("web/src/main/resources/gone.yml")).unwrap();
        let second = assess(&dir, Some("web"));
        assert_eq!(rebuilt(&second.plan), vec![("web", RebuildReason::UpstreamChanged)]);
        let log = apply(&dir, &second.plan);
        assert!(!dir.join("web/target/classes/w/Web.class").exists(), "{log:?}");
        assert!(!dir.join("web/target/generated-sources/annotations").exists());
        assert!(!dir.join("web/target/classes/gone.yml").exists(), "a removed resource is removed");
        assert!(dir.join("core/target/classes/c/Core.class").exists(), "core itself is not wiped");

        forget(&dir);
        assert_eq!(generation(&dir), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
