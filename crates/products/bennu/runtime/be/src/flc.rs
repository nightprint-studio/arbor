//! `flc` domain — `bennu_flc_complete`, `bennu_flc_status`, `bennu_flc_prepare`.
//!
//! The rest of the line the caret is in, drawn as ghost text. Two sources, in this order:
//!
//! 1. **The project's log profile** — a log call is completed from the pattern the project uses most in that
//!    place (the entry line of a method, a `catch`). No model; the profile is counters per pattern, counted once
//!    in a background thread per project and kept current one file at a time (see `bennu_flc::logs`).
//! 2. **The model** — a small local GPT, constrained to the member names the index offers after a `.`. Loaded on
//!    first use in a background thread; until it is ready, or while it is busy with another request, it answers
//!    nothing rather than making the editor wait.
//!
//! Nothing here blocks the request that asked: the scan and the model load run on their own threads, the model is
//! only ever `try_lock`ed, and the settings are read from a hot copy that the config save pushes (the same
//! arrangement as `completion_tuning`).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock, RwLock};

use bennu_complete::prelude::MatchCase;
use bennu_core::config::BennuConfig;
use bennu_core::prelude::BennuState;
use bennu_flc::logs::context_at;
use bennu_flc::prelude::*;
use serde::{Deserialize, Serialize};

use crate::flc_chain::chain_constraint;
use crate::index_service::IndexService;

/// Directories the log scan never walks into, on top of the user's `excluded_dirs`.
const SKIP_DIRS: &[&str] = &[".git", "target", "build", "node_modules", ".idea", "out"];
/// A source file larger than this is not read for log statements.
const MAX_FILE_BYTES: u64 = 1_000_000;

// ---- settings ---------------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Settings {
    enabled: bool,
    model_dir: String,
    min_confidence: f32,
    logs: bool,
    alternatives: bool,
    check: bool,
    ignore_commented_code: bool,
    profile_project: bool,
    profile_global: bool,
    /// The small layers that can be switched off one by one; see the `flc_layer_*` settings.
    layer_names: bool,
    layer_ngram: bool,
    layer_creation: bool,
    layer_echo: bool,
    layer_habits: bool,
    excluded_dirs: Vec<String>,
}

fn settings_from(config: &BennuConfig) -> Settings {
    Settings {
        enabled: config.flc_enabled,
        model_dir: config.flc_model_dir.trim().to_string(),
        min_confidence: config.flc_min_confidence.min(100) as f32 / 100.0,
        logs: config.flc_logs,
        alternatives: config.flc_alternatives,
        check: config.flc_check,
        ignore_commented_code: config.flc_ignore_commented_code,
        profile_project: config.flc_profile_project,
        profile_global: config.flc_profile_global,
        layer_names: config.flc_layer_names,
        layer_ngram: config.flc_layer_ngram,
        layer_creation: config.flc_layer_creation,
        layer_echo: config.flc_layer_echo,
        layer_habits: config.flc_layer_habits,
        excluded_dirs: config.excluded_dirs.clone(),
    }
}

fn settings_cell() -> &'static RwLock<Settings> {
    static CELL: OnceLock<RwLock<Settings>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(settings_from(&bennu_core::config::load())))
}

/// Which of the two profile switches are on, for `flc_profile`.
pub(crate) fn profile_flags() -> crate::flc_profile::Flags {
    let s = settings();
    crate::flc_profile::Flags { project: s.profile_project, global: s.profile_global }
}

fn settings() -> Settings {
    settings_cell().read().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Hand the completion the settings the caller already holds (the config save). A model whose folder changed is
/// dropped by the next request; switching the model off drops it here, so its weights leave memory with the switch.
pub(crate) fn apply_flc_settings(config: &BennuConfig) {
    let next = settings_from(config);
    let off = !next.enabled || next.model_dir.is_empty();
    *settings_cell().write().unwrap_or_else(|p| p.into_inner()) = next;
    if off {
        release_model();
    }
}

/// Re-read the settings from disk — a project opening picks up a hand-edited file.
pub(crate) fn refresh_flc_settings() {
    apply_flc_settings(&bennu_core::config::load());
}

// ---- the model --------------------------------------------------------------------------------------------------

enum ModelState {
    Idle,
    Loading,
    Ready(Box<LineModel>),
    Failed(String),
}

struct ModelSlot {
    dir: String,
    state: ModelState,
}

fn model_slot() -> &'static Mutex<ModelSlot> {
    static SLOT: OnceLock<Mutex<ModelSlot>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(ModelSlot { dir: String::new(), state: ModelState::Idle }))
}

/// Empties the slot. A load still under way finds the folder gone and discards its result. Waits for an inference
/// in flight (tens of milliseconds), which is why the lock is taken blocking here and `try_lock`ed on the request path.
fn release_model() {
    let mut slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
    slot.dir.clear();
    slot.state = ModelState::Idle;
}

/// Starts loading the model of `dir` on its own thread, unless that is already done or under way.
fn ensure_model(dir: &str) {
    {
        let mut slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
        if slot.dir != dir {
            slot.dir = dir.to_string();
            slot.state = ModelState::Idle;
        }
        if !matches!(slot.state, ModelState::Idle) {
            return;
        }
        slot.state = ModelState::Loading;
    }
    let dir = dir.to_string();
    std::thread::Builder::new()
        .name("flc-model-load".into())
        .spawn(move || {
            let loaded = LineModel::load(&dir);
            let mut slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
            // the folder may have changed while this loaded; then this result is for nobody
            if slot.dir == dir {
                slot.state = match loaded {
                    Ok(m) => ModelState::Ready(Box::new(m)),
                    Err(e) => ModelState::Failed(e),
                };
            }
        })
        .ok();
}

// ---- the log profile --------------------------------------------------------------------------------------------

#[derive(Default)]
struct LogState {
    profile: LogProfile,
    /// The lines several files write the same way; text only for those, see `bennu_flc::freq`.
    lines: LineIndex,
    /// A hash of each file's contents as last counted: an unchanged file is not read into the counters twice.
    seen: HashMap<String, u64>,
    /// The same for the line index, which is rebuilt from the text on every start and so cannot be skipped by `seen`.
    fed: HashMap<String, u64>,
    /// Counted since the profile was last saved.
    dirty: bool,
    scanned: HashSet<String>,
    scanning: HashSet<String>,
}

fn log_state() -> &'static Mutex<LogState> {
    static STATE: OnceLock<Mutex<LogState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(LogState::default()))
}

pub(crate) fn content_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Counts `source` under `key` unless the same text was counted already.
fn count_file(key: &str, source: &str) {
    let hash = content_hash(source);
    let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
    if st.fed.get(key) != Some(&hash) {
        st.fed.insert(key.to_string(), hash);
        st.lines.add_file(key, source);
    }
    // a profile loaded from disk already holds this text's calls: only the line index needed it
    if st.seen.get(key) != Some(&hash) {
        st.seen.insert(key.to_string(), hash);
        st.profile.add_file(key, source);
        st.dirty = true;
    }
}

/// Makes every project be read again at its next suggestion, as after a restart: what *Forget what Bennu learned* empties
/// is only filled again by a pass over the files, and a project that was already passed over would otherwise stay empty
/// until the backend restarted.
pub(crate) fn forget_scans() {
    log_state().lock().unwrap_or_else(|p| p.into_inner()).scanned.clear();
}

/// Takes a file that is gone back out of everything it was counted into.
fn forget_file(st: &mut LogState, key: &str) {
    st.profile.remove_file(key);
    st.lines.remove_file(key);
    crate::flc_names::remove_file(key);
    crate::flc_similar::remove_file(key);
    crate::flc_next::remove_file(key);
    crate::flc_habits::remove_file(key);
    crate::flc_annotations::remove_file(key);
    crate::flc_format::remove_file(key);
    crate::flc_sequence::remove_file(key);
    crate::flc_member_args::remove_file(key);
    crate::flc_created::remove_file(key);
    crate::flc_ngram::remove_file(key);
    crate::flc_catch::remove_file(key);
    crate::flc_lambda::remove_file(key);
    crate::flc_assert::remove_file(key);
    st.seen.remove(key);
    st.fed.remove(key);
    st.dirty = true;
}

fn walk_java(dir: &Path, skip: &[String], out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) && !skip.iter().any(|s| *s == name) {
                walk_java(&path, skip, out);
            }
        } else if kind.is_file() && name.ends_with(".java") {
            out.push(path);
        }
    }
}

/// What the saved profiles are checked against: if either hash function changed, the copy is not trusted.
fn probes() -> (u64, u64) {
    (LogProfile::hash_probe(), content_hash("flc-text-probe"))
}

#[cfg(test)]
static TEST_STORE: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Where the saved profile of `root` lives.
fn store_path(root: &Path) -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_STORE.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        return dir.join("profile.json");
    }
    crate::flc_persist::profile_path(root)
}

/// Writes the profile of `root` to disk (best effort, atomically).
fn save_profile(root: &str) {
    let root = Path::new(root);
    let (snapshot, hashes) = {
        let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        st.dirty = false;
        let hashes: HashMap<String, u64> = st.seen.iter().filter(|(k, _)| Path::new(k).starts_with(root)).map(|(k, v)| (k.clone(), *v)).collect();
        (st.profile.snapshot(|k| Path::new(k).starts_with(root)), hashes)
    };
    let _ = crate::flc_persist::save_to(&store_path(root), root, &snapshot, &hashes, probes());
}

/// Counts the project's log statements, once per process, on a thread of its own.
fn prepare_logs(root: &str) {
    {
        let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        if st.scanned.contains(root) || st.scanning.contains(root) {
            return;
        }
        st.scanning.insert(root.to_string());
    }
    let _ = maintenance_sender();
    let root = root.to_string();
    let skip = settings().excluded_dirs;
    std::thread::Builder::new()
        .name("flc-log-scan".into())
        .spawn(move || {
            scan_root(&root, &skip);
            let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
            st.scanning.remove(&root);
            st.scanned.insert(root);
        })
        .ok();
}

/// One pass over a project: take the saved profile, read every `.java` file (the line index needs the text, and
/// a file whose hash matches the saved one is not counted again), forget the files that vanished, save.
fn scan_root(root: &str, skip: &[String]) {
    let root_path = Path::new(root);
    // what Bennu learned here before, put back first so suggestions work at once and unchanged files are not parsed again
    crate::flc_profile::load_project(root);
    if let Some(saved) = crate::flc_persist::load_from(&store_path(root_path), root_path, probes()) {
        let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        st.profile.restore(&saved.snapshot);
        for (key, hash) in saved.hashes {
            st.seen.entry(key).or_insert(hash);
        }
    }
    let mut files = Vec::new();
    walk_java(root_path, skip, &mut files);
    let mut walked = HashSet::new();
    for path in files {
        let key = path.to_string_lossy().into_owned();
        walked.insert(key.clone());
        if std::fs::metadata(&path).map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        // legacy sources are not always UTF-8; a lossy read keeps the structure, which is all this needs
        let text = String::from_utf8_lossy(&bytes);
        count_file(&key, &text);
        // the names the project gives its types: read in the background only, a parse per keystroke is not worth it
        crate::flc_names::add_file(&key, &text);
        crate::flc_similar::add_file(&key, &text);
        crate::flc_next::add_file(&key, &text);
        crate::flc_habits::add_file(&key, &text);
        crate::flc_annotations::add_file(&key, &text);
        crate::flc_format::add_file(&key, &text);
        crate::flc_sequence::add_file(&key, &text);
        crate::flc_member_args::add_file(&key, &text);
        crate::flc_created::add_file(&key, &text);
        crate::flc_ngram::add_file(&key, &text);
        crate::flc_catch::add_file(&key, &text);
        crate::flc_lambda::add_file(&key, &text);
        crate::flc_assert::add_file(&key, &text);
    }
    {
        let mut st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        let known: HashMap<String, u64> = st.seen.iter().filter(|(k, _)| Path::new(k).starts_with(root_path)).map(|(k, v)| (k.clone(), *v)).collect();
        for gone in crate::flc_persist::vanished(&known, &walked) {
            forget_file(&mut st, &gone);
        }
    }
    crate::flc_profile::finish_scan(root, &walked);
    save_profile(root);
}

// ---- keeping up with the disk -------------------------------------------------------------------------------------

/// What the project tree's watcher heard, for one project.
enum Change {
    /// These files (root-relative, forward slashes) were created, written, renamed or removed.
    Files { root: String, rels: Vec<String> },
    /// Directories came or went, or events were lost: look at the whole project again.
    Rescan { root: String },
}

/// How long changes are gathered before they are applied, and how often a changed profile is written.
const GATHER: std::time::Duration = std::time::Duration::from_millis(300);
const SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

static MAINTENANCE: OnceLock<std::sync::mpsc::Sender<Change>> = OnceLock::new();

/// The thread that applies file changes and writes the profile, started with the first scan.
fn maintenance_sender() -> &'static std::sync::mpsc::Sender<Change> {
    MAINTENANCE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Change>();
        std::thread::Builder::new()
            .name("flc-maintenance".into())
            .spawn(move || {
                let mut last_save = std::time::Instant::now();
                loop {
                    match rx.recv_timeout(SAVE_EVERY) {
                        Ok(first) => {
                            std::thread::sleep(GATHER);
                            let mut batch = vec![first];
                            while let Ok(more) = rx.try_recv() {
                                batch.push(more);
                            }
                            apply_changes(batch);
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if last_save.elapsed() >= SAVE_EVERY {
                        last_save = std::time::Instant::now();
                        save_dirty_profiles();
                        crate::flc_profile::save_dirty();
                    }
                }
            })
            .ok();
        tx
    })
}

/// The scanned root that is the same directory as `root` (the watcher and the index may spell it differently).
fn scanned_root(root: &str) -> Option<String> {
    let st = log_state().lock().unwrap_or_else(|p| p.into_inner());
    st.scanned.iter().find(|r| Path::new(r.as_str()) == Path::new(root)).cloned()
}

fn save_dirty_profiles() {
    let roots: Vec<String> = {
        let st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        if !st.dirty {
            return;
        }
        st.scanned.iter().cloned().collect()
    };
    for root in roots {
        save_profile(&root);
    }
}

fn apply_changes(batch: Vec<Change>) {
    let settings = settings();
    if !(settings.logs || settings.enabled) {
        return;
    }
    let mut files: HashMap<String, HashSet<String>> = HashMap::new();
    let mut rescan: HashSet<String> = HashSet::new();
    for change in batch {
        match change {
            Change::Files { root, rels } => files.entry(root).or_default().extend(rels),
            Change::Rescan { root } => {
                rescan.insert(root);
            }
        }
    }
    for root in rescan {
        let Some(root) = scanned_root(&root) else { continue };
        files.remove(&root);
        log_state().lock().unwrap_or_else(|p| p.into_inner()).scanning.insert(root.clone());
        scan_root(&root, &settings.excluded_dirs);
        log_state().lock().unwrap_or_else(|p| p.into_inner()).scanning.remove(&root);
    }
    for (root, rels) in files {
        let Some(root) = scanned_root(&root) else { continue };
        log_state().lock().unwrap_or_else(|p| p.into_inner()).scanning.insert(root.clone());
        for rel in rels.iter().filter(|r| r.ends_with(".java")) {
            let path = Path::new(&root).join(rel);
            let key = path.to_string_lossy().into_owned();
            let readable = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE_BYTES);
            match std::fs::read(&path) {
                Ok(bytes) if readable => {
                    let text = String::from_utf8_lossy(&bytes);
                    count_file(&key, &text);
                    crate::flc_names::add_file(&key, &text);
                    crate::flc_similar::add_file(&key, &text);
                    crate::flc_next::add_file(&key, &text);
                    crate::flc_habits::add_file(&key, &text);
                    crate::flc_annotations::add_file(&key, &text);
                    crate::flc_format::add_file(&key, &text);
                    crate::flc_sequence::add_file(&key, &text);
                    crate::flc_member_args::add_file(&key, &text);
                    crate::flc_created::add_file(&key, &text);
                    crate::flc_ngram::add_file(&key, &text);
                    crate::flc_catch::add_file(&key, &text);
                    crate::flc_lambda::add_file(&key, &text);
                    crate::flc_assert::add_file(&key, &text);
                }
                _ => forget_file(&mut log_state().lock().unwrap_or_else(|p| p.into_inner()), &key),
            }
        }
        log_state().lock().unwrap_or_else(|p| p.into_inner()).scanning.remove(&root);
    }
}

/// The project tree's watcher heard `.java` files change on disk (a checkout, another editor): count them again, or
/// take them back out when they are gone. Cheap to call; the work happens on the maintenance thread. A no-op until a
/// project has been scanned.
pub(crate) fn java_files_changed(root: &Path, rels: Vec<String>) {
    if let Some(tx) = MAINTENANCE.get() {
        let _ = tx.send(Change::Files { root: root.to_string_lossy().into_owned(), rels });
    }
}

/// Directories came or went under `root`, or the watcher lost events: the whole project is read again.
pub(crate) fn project_changed(root: &Path) {
    if let Some(tx) = MAINTENANCE.get() {
        let _ = tx.send(Change::Rescan { root: root.to_string_lossy().into_owned() });
    }
}

// ---- the request ------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FlcArgs {
    /// Absolute path of the Java file the caret is in.
    pub file: String,
    /// Byte offset of the caret in `source`.
    pub offset: usize,
    /// The live buffer text; the file is read from disk when absent.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Serialize)]
pub struct FlcReply {
    /// The rest of the line after the caret.
    pub text: String,
    /// 0..1. For the log profile, the share of the place's calls the pattern holds.
    pub confidence: f32,
    /// `"log"`, `"project"` (a line several files write alike), `"name"`, `"mapping"`, `"arguments"`, `"habit"` (the usual argument at a place), `"annotation"`, `"similar"` (the body of a twin method), `"derived"` (a block written out from the class, such as a constructor) or `"model"`.
    pub origin: String,
}

/// What the local model and the project profiles hold, for the process monitor. The model is reported as its own group
/// because it is the one part of this backend that is a different kind of thing — a runtime with weights loaded, resident
/// whether or not a suggestion is being asked for — and it is the part that can be switched off.
pub(crate) fn memory_items() -> Vec<arbor_be::prelude::MemoryItem> {
    use arbor_be::prelude::MemoryItem;
    let mut items = Vec::new();
    {
        let slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
        let weights: u64 = ["flc.onnx", "flc_prefill.onnx"]
            .iter()
            .filter_map(|f| std::fs::metadata(std::path::Path::new(&slot.dir).join(f)).ok())
            .map(|m| m.len())
            .sum();
        match &slot.state {
            ModelState::Ready(_) => items.push(MemoryItem::exact("model", "Weights loaded (ONNX Runtime), resident while idle", 1, weights as usize)),
            ModelState::Loading => items.push(MemoryItem::counted("model", "Loading the weights…", 1)),
            ModelState::Failed(_) | ModelState::Idle => items.push(MemoryItem::counted("model", "Not loaded", 0)),
        }
    }
    // What was learned, by project (what its files contributed) and for the process (the totals, what other projects
    // taught, the counters of what you did with suggestions, the rows you stopped offering).
    let tables = [
        ("usual arguments and variables", crate::flc_habits::sizes()),
        ("variable names by type", crate::flc_names::sizes()),
        ("annotations", crate::flc_annotations::sizes()),
        ("what follows what", crate::flc_sequence::sizes()),
        ("calls on a variable", crate::flc_member_args::sizes()),
        ("layout", crate::flc_format::sizes()),
        ("types created under a name", crate::flc_created::sizes()),
        ("lines copied from the project", crate::flc_ngram::sizes()),
        ("how exceptions are handled", crate::flc_catch::sizes()),
        ("functions passed to calls", crate::flc_lambda::sizes()),
        ("assertions in tests", crate::flc_assert::sizes()),
        ("picks in the completion list", crate::flc_choices::sizes()),
    ];
    let extra = vec![crate::learn::memory_item(), crate::learned_rules::memory_item()];
    let roots = crate::index_service::IndexService::global().open_roots();
    items.extend(crate::flc_memory::items(&tables, extra, &roots));
    items
}

/// A reply as the editor receives it: the suggestion, and what it was for, which the editor hands back untouched when it
/// reports what became of the suggestion (see `learn`).
#[derive(Serialize)]
pub struct FlcAnswer {
    #[serde(flatten)]
    pub reply: FlcReply,
    pub context: String,
}

#[derive(Serialize)]
pub struct FlcStatus {
    pub enabled: bool,
    pub model_dir: String,
    /// `"off"`, `"no folder"`, `"idle"`, `"loading"`, `"ready"`, or `"failed: …"`.
    pub model: String,
    pub log_calls: u32,
    pub log_patterns: usize,
    pub log_scanning: bool,
}

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Where the receiver expression before the dot at `dot` begins: back over names, dots and balanced brackets.
fn receiver_start(text: &str, dot: usize) -> usize {
    let b = text.as_bytes();
    let mut i = dot;
    while i > 0 {
        match b[i - 1] {
            b')' | b']' => {
                let (mut depth, mut k) = (0i32, i);
                while k > 0 {
                    depth += matches!(b[k - 1], b')' | b']') as i32 - matches!(b[k - 1], b'(' | b'[') as i32;
                    k -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                i = k;
            }
            c if ident_byte(c) || c == b'.' => i -= 1,
            _ => break,
        }
    }
    i
}

/// What the index says about the types the header names: resolved against the file's own imports, members read from
/// the index tables (fields, then methods, own type first and then its supertypes).
struct IndexTypes<'a> {
    file: &'a str,
    imports: String,
}

impl IndexTypes<'_> {
    /// The `import` lines of the buffer: the only part of it a name is resolved against.
    fn for_buffer<'a>(file: &'a str, source: &str) -> IndexTypes<'a> {
        let imports = source.lines().map(str::trim_start).filter(|l| l.starts_with("import ") || l.starts_with("package ")).collect::<Vec<_>>().join("\n");
        IndexTypes { file, imports }
    }
}

fn simple_type(t: &bennu_java::prelude::TypeRef) -> String {
    let name = t.binary_name.rsplit(['/', '$']).next().unwrap_or(&t.binary_name);
    format!("{name}{}", "[]".repeat(t.dims as usize))
}

impl TypeLookup for IndexTypes<'_> {
    fn resolve(&self, name: &str) -> Option<TypeInfo> {
        let (binary, tables) = IndexService::global().header_type(self.file, &self.imports, name)?;
        let mut members = Vec::new();
        for table in &tables {
            let listed = |m: &&bennu_java::prelude::Member| !m.name.starts_with('<') && !m.name.contains('$');
            members.extend(table.fields.iter().filter(listed).map(|m| HeaderMember { name: m.name.clone(), is_method: false, params: Vec::new() }));
            members.extend(table.methods.iter().filter(listed).map(|m| HeaderMember { name: m.name.clone(), is_method: true, params: m.params.iter().map(simple_type).collect() }));
        }
        Some(TypeInfo { key: binary, members })
    }
}

/// The member names the index offers after the dot at `offset`.
pub(crate) fn member_names(file: &str, source: &str, offset: usize) -> Vec<String> {
    let items = IndexService::global().completion(file, offset, Some(source), MatchCase::default());
    let mut names: Vec<String> = items
        .iter()
        .filter(|i| !matches!(i.kind.as_str(), "keyword" | "snippet" | "class" | "interface" | "package"))
        .map(|i| i.label.split('(').next().unwrap_or(&i.label).trim().to_string())
        .filter(|n| !n.is_empty() && n.bytes().all(ident_byte))
        .collect();
    names.dedup();
    names
}

/// Where in `text` (what the model wrote after the caret) the first `.member` is one its receiver does not have, when
/// the index can say. A receiver it cannot type answers with nothing or only `Object`'s methods, and then it has no opinion.
fn invalid_member_at(file: &str, source: &str, offset: usize, text: &str) -> Option<usize> {
    const OBJECT: [&str; 9] = ["equals", "hashCode", "toString", "getClass", "notify", "notifyAll", "wait", "clone", "finalize"];
    let bytes = text.as_bytes();
    let (mut i, mut in_str, mut checked) = (0, false, 0);
    while i < bytes.len() && checked < 4 {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'.' if !in_str && bytes.get(i + 1).is_some_and(|&c| ident_byte(c) && !c.is_ascii_digit()) => {
                let before = if i == 0 { source.as_bytes().get(offset.saturating_sub(1)).copied() } else { Some(bytes[i - 1]) };
                if before.is_some_and(|c| ident_byte(c) || matches!(c, b')' | b']' | b'\n' | b' ' | b'\t')) {
                    let name: String = text[i + 1..].bytes().take_while(|&c| ident_byte(c)).map(|c| c as char).collect();
                    let virt = format!("{}{}", &source[..offset], &text[..=i]);
                    let names = member_names(file, &virt, virt.len());
                    checked += 1;
                    if names.iter().any(|n| !OBJECT.contains(&n.as_str())) && !names.contains(&name) {
                        return Some(i);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The first `.member` the suggestion chains onto a `)` that closes a parenthesis opened before the caret, when the
/// index cannot show that the closed call has that member: where the receiver is not typed there is nothing to
/// support a `get`, and a small model fills the gap with the commonest word.
fn unproven_chain_after_close(file: &str, source: &str, offset: usize, text: &str) -> Option<usize> {
    let dot = *bennu_flc::prelude::dots_after_prefix_close(text).first()?;
    let name: String = text[dot + 1..].bytes().take_while(|&c| ident_byte(c)).map(|c| c as char).collect();
    let virt = format!("{}{}", &source[..offset], &text[..=dot]);
    let names = member_names(file, &virt, virt.len());
    (!names.contains(&name)).then_some(dot)
}

/// `text` cut before a chained member the index cannot show on the call it follows (see [`unproven_chain_after_close`]);
/// `None` when nothing but closers would be left.
fn without_unproven_chain(file: &str, source: &str, offset: usize, text: String) -> Option<String> {
    let Some(at) = unproven_chain_after_close(file, source, offset, &text) else { return Some(text) };
    let kept = text[..at].trim_end();
    (!kept.chars().all(|c| c == ')' || c.is_whitespace())).then(|| kept.to_string())
}

/// The rest of the suggestion up to the last `&&`/`||` before the invented member, or nothing when there is none.
fn cut_before_invention(text: &str, at: usize) -> Option<String> {
    let head = &text[..at];
    let cut = [" && ", " || "].iter().filter_map(|k| head.rfind(k)).max()?;
    let kept = text[..cut].trim_end();
    (kept.len() >= 8).then(|| kept.to_string())
}

/// The member names the index offers at the caret and the header that lists them; empty unless the caret follows a dot.
fn member_context(file: &str, source: &str, offset: usize, line_prefix: &str) -> (Vec<String>, Option<String>, String) {
    let partial_len = line_prefix.bytes().rev().take_while(|&c| ident_byte(c)).count();
    let dot = line_prefix.len() - partial_len;
    if dot == 0 || line_prefix.as_bytes()[dot - 1] != b'.' {
        return (Vec::new(), None, String::new());
    }
    let partial = line_prefix[dot..].to_string();
    let names = member_names(file, source, offset);
    // the receiver's type, for the header; without it the model still has the constraint
    let text_dot = offset - partial.len() - 1;
    let start = receiver_start(source, text_dot);
    let receiver = IndexService::global().expression_type(file, source, start, text_dot);
    (names, receiver, partial)
}

fn model_suggestion(settings: &Settings, args: &FlcArgs, source: &str, line_prefix: &str, line_end: usize) -> Option<FlcReply> {
    if !settings.enabled || settings.model_dir.is_empty() {
        return None;
    }
    // where the model has nothing useful to say, and where a variable name is all that can follow
    #[cfg(test)]
    let bypass = crate::flc_quality::BYPASS_GATE.with(|b| b.get());
    #[cfg(not(test))]
    let bypass = false;
    match if line_prefix.trim().is_empty() || bypass { Gate::Model } else { gate(line_prefix) } {
        Gate::Silent => return None,
        // a type name inside the parentheses of an unfinished call is an argument (`String` of `String.class`), not a
        // declaration waiting for its variable: the line alone cannot tell, the lines above it can
        Gate::Name(_) if !settings.layer_names || open_parens_before(source, args.offset).is_some_and(|depth| depth > 0) => {}
        Gate::Name(derived) => {
            // the name this project gives that type, when it has a habit; else the one the type suggests
            let habit = bennu_flc::prelude::bare_type(line_prefix).and_then(|ty| crate::flc_names::name_for(&args.file, source, args.offset, &ty));
            let text = match habit {
                Some(n) if line_prefix.ends_with(char::is_whitespace) => n,
                Some(n) => format!(" {n}"),
                None if crate::flc_names::is_taken(source, args.offset, derived.trim()) => return None,
                None => derived,
            };
            return Some(FlcReply { text, confidence: 0.6, origin: "name".into() });
        }
        Gate::Model => {}
    }
    ensure_model(&settings.model_dir);
    // gather what the index knows BEFORE taking the model: the lock is held only for the inference
    // a new line under an open chain starts with the next call, so the model is asked as if the dot were typed:
    // it knows that position best, and the receiver's members come with it
    let chain_start = line_prefix.trim().is_empty();
    let dotted = format!("{}.", &source[..args.offset]);
    let (prompt_src, ctx_offset, ctx_prefix) = if chain_start { (dotted.as_str(), dotted.len(), ".") } else { (source, args.offset, line_prefix) };
    let (names, receiver, partial) = member_context(&args.file, prompt_src, ctx_offset, ctx_prefix);
    let mut slot = model_slot().try_lock().ok()?;
    let ModelState::Ready(model) = &mut slot.state else { return None };
    // the word being typed is not shown to the model but forced as the start of its answer (token healing)
    let typed = typed_word(ctx_prefix);
    // old code in `//` lines, when the user asked for it not to be read: the model copies it into whatever is typed below
    let stripped = settings.ignore_commented_code.then(|| bennu_flc::prelude::strip_commented_code(&prompt_src[..ctx_offset - typed.len()]));
    let prompt_prefix = stripped.as_deref().unwrap_or(&prompt_src[..ctx_offset - typed.len()]);
    // the header names the types of the part of the file the prompt will carry; a handful of index lookups, so it is
    // built with the model in hand (the window depends on its tokenizer) and not before
    let types = IndexTypes::for_buffer(&args.file, prompt_src);
    let focus = receiver.as_deref().and_then(|t| types.resolve(t));
    let window = bennu_flc::prompt::code_window(&model.tk, prompt_prefix);
    // a model trained with the `vars` line also reads the declarations above its window; an older one never saw it
    let outside = if model.header_version >= 2 { &prompt_prefix[..prompt_prefix.len() - window.len()] } else { "" };
    let header = bennu_flc::prelude::header_in(&types, outside, window, focus.as_ref(), &partial);
    let suffix = if settings.ignore_commented_code { bennu_flc::prelude::strip_commented_code(&source[line_end..]) } else { source[line_end..].to_string() };
    let mut prompt = bennu_flc::prompt::build(&model.tk, &header, prompt_prefix, &suffix);
    prompt.fit(model.ctx.saturating_sub(MAX_NEW_TOKENS));
    let members = names_constraint(&names, if typed.is_empty() { &partial } else { "" });
    // the first dot is constrained from the caret's names; every later dot, from what the chain so far really returns
    let first: Option<Constraint> = if names.is_empty() { None } else { Some(&members) };
    let chain = chain_constraint(prompt_prefix, |virt| member_names(&args.file, virt, virt.len()), first);
    let constraint = forced_prefix(typed, Some(&chain));
    // three candidates when the setting asks for them: the first that passes every check wins
    let width = if settings.alternatives { 3 } else { 1 };
    let candidates = model.complete(&prompt.ids, width, Some(&constraint)).ok()?;
    #[cfg(test)]
    crate::flc_quality::record(&prompt.ids, typed, chain_start, &candidates);
    drop(slot);
    let words = words_of(source);
    // a class the file has not imported yet is still a real class: it needs an import, not a different suggestion
    let known = |w: &str| {
        words.contains(w)
            || (w.starts_with(|c: char| c.is_ascii_uppercase())
                && (IndexService::global().type_resolves(&args.file, source, w)
                    || IndexService::global().type_name_matches(&args.file, w, 30).iter().any(|n| n == w)))
    };
    candidates.into_iter().take(width).find_map(|candidate| {
        let mut text = candidate.text.get(typed.len()..)?.to_string();
        if chain_start {
            text.insert(0, '.');
        }
        if candidate.confidence < settings.min_confidence {
            return None;
        }
        // a line that names something this file and project do not have is the model inventing
        let mut text = vet(line_prefix, &text, &known)?;
        // a member the receiver does not have (`.build().addPin(..)`, `&& field.nothing()`): keep what precedes it
        if let Some(at) = invalid_member_at(&args.file, source, args.offset, &text) {
            text = cut_before_invention(&text, at)?;
        }
        // a member chained onto the result of an outer call the caret is still inside (`foo(x.build()` + `).get()`):
        // kept only when the index can say what that call returns and that it has the member
        if let Some(at) = unproven_chain_after_close(&args.file, source, args.offset, &text) {
            let kept = text[..at].trim_end();
            if kept.chars().all(|c| c == ')' || c.is_whitespace()) {
                return None;
            }
            text = kept.to_string();
        }
        // a `)` that closes nothing (`.build()` + `).get(…)`): the suggestion ends before it
        if let Some(open) = open_parens_before(source, args.offset) {
            text = bennu_flc::prelude::cut_unbalanced(&text, open);
            if text.trim().is_empty() {
                return None;
            }
        }
        // what the code checker calls wrong rather than missing: the next candidate gets its turn
        if settings.check && crate::flc_check::invents(&args.file, source, args.offset, &text) {
            return None;
        }
        Some(FlcReply { text, confidence: candidate.confidence, origin: "model".into() })
    })
}

/// How many `(` are still open at the caret, read from the lines of the statement the caret is in (back to the
/// previous `;`, `{` or `}` line). `None` when the statement has braces or comments in it or does not add up: a lambda
/// block or a stray closer makes the count unreliable, and the caller then leaves the suggestion alone.
fn open_parens_before(source: &str, offset: usize) -> Option<i32> {
    let head = &source[..offset];
    let mut lines: Vec<&str> = head.rsplit('\n').take(12).collect();
    // lines[0] is the caret's own line; go back while the line above does not end the previous statement
    let mut keep = 1;
    while keep < lines.len() {
        let t = lines[keep].trim_end();
        if t.is_empty() || t.ends_with(';') || t.ends_with('{') || t.ends_with('}') {
            break;
        }
        keep += 1;
    }
    lines.truncate(keep);
    lines.reverse();
    let text = lines.join("\n");
    if text.contains(['{', '}']) || text.contains("//") || text.contains("/*") {
        return None;
    }
    let b = text.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    (depth >= 0).then_some(depth)
}

/// What the model alone says at `args` — no layer before it, and, when the harness asks, no gate: the quality harness
/// uses it to see what the model would write where the editor stays silent.
#[cfg(test)]
pub(crate) fn model_only(args: &FlcArgs) -> Option<FlcReply> {
    let source = args.source.clone()?;
    let line_start = source[..args.offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[args.offset..].find('\n').map_or(source.len(), |p| args.offset + p);
    model_suggestion(&settings(), args, &source, &source[line_start..args.offset], line_end)
}

/// The rest of the line at the caret, from the project's log profile or the model; `None` when neither has
/// something it is sure of.
#[arbor_rpc::handler]
fn bennu_flc_complete(_ctx: &BennuState, args: FlcArgs) -> Result<Option<FlcAnswer>, String> {
    let (file, offset, given) = (args.file.clone(), args.offset, args.source.clone());
    let Some(reply) = suggest(args)? else { return Ok(None) };
    let context = learning_context(&file, given.as_deref(), offset);
    Ok(Some(FlcAnswer { reply, context }))
}

pub(crate) fn suggest(args: FlcArgs) -> Result<Option<FlcReply>, String> {
    let min = settings().min_confidence;
    // what the suggestion is for, read only when there is a learner to ask
    let context = if crate::learn::enabled() { learning_context(&args.file, args.source.as_deref(), args.offset) } else { String::new() };
    let lay_out = settings().profile_project;
    let (file, offset, given) = (args.file.clone(), args.offset, args.source.clone());
    Ok(suggest_from_layers(args)?
        .filter(|reply| !drawn_where_it_cannot_fit(&file, given.as_deref(), offset, &reply.text))
        .and_then(|reply| calibrated(reply, min, &context))
        .map(|reply| laid_out(reply, lay_out)))
}

/// A lambda or an anonymous class where the expected type is a plain class — see `flc_fit`.
fn drawn_where_it_cannot_fit(file: &str, given: Option<&str>, offset: usize, text: &str) -> bool {
    let read;
    let source = match given {
        Some(s) => s,
        None => match std::fs::read_to_string(file) {
            Ok(text) => {
                read = text;
                &read
            }
            Err(_) => return false,
        },
    };
    crate::flc_fit::impossible_here(file, source, offset, text)
}

/// A suggestion the layers WROTE (the model, an argument list built here) in the way the project lays out its code; one
/// they copied from the project already has it.
fn laid_out(reply: FlcReply, lay_out: bool) -> FlcReply {
    if !lay_out || !matches!(reply.origin.as_str(), "model" | "arguments" | "habit" | "name") {
        return reply;
    }
    FlcReply { text: crate::flc_format::conformed(&reply.text), ..reply }
}

/// What a suggestion at `offset` is for (see `learn::context_of`), read off the text the layers read.
fn learning_context(file: &str, given: Option<&str>, offset: usize) -> String {
    let read;
    let source = match given {
        Some(s) => s,
        None => match std::fs::read_to_string(file) {
            Ok(text) => {
                read = text;
                &read
            }
            Err(_) => return "-".to_string(),
        },
    };
    let at = offset.min(source.len());
    if source.is_char_boundary(at) { crate::learn::context_of(source, at) } else { "-".to_string() }
}

/// What the user's own corrections say about the kind of suggestion this is — see [`calibrate`]. A no-op while *Learn
/// from corrections* is off, and for a kind that has too few corrections to say anything.
fn calibrated(reply: FlcReply, min: f32, context: &str) -> Option<FlcReply> {
    let Some(useful) = crate::learn::useful_rate(&reply.origin, context) else { return Some(reply) };
    let confidence = calibrate(reply.confidence, useful, min)?;
    Some(FlcReply { confidence, ..reply })
}

/// `confidence` scaled by how useful this kind of suggestion has been to the user: from 70% of it (never) to all of it
/// (always). Bounded on purpose: a handful of bad days must not silence a layer for good. `None` when the scaling is what
/// takes a suggestion that was drawn under the threshold — one that was already under it is left as it was.
fn calibrate(confidence: f32, useful: f32, min: f32) -> Option<f32> {
    let scaled = confidence * (0.7 + 0.3 * useful.clamp(0.0, 1.0));
    (confidence < min || scaled >= min).then_some(scaled)
}

fn suggest_from_layers(args: FlcArgs) -> Result<Option<FlcReply>, String> {
    let settings = settings();
    if !(settings.enabled || settings.logs) || !args.file.to_ascii_lowercase().ends_with(".java") {
        return Ok(None);
    }
    let source = match &args.source {
        Some(s) => s.clone(),
        None => std::fs::read_to_string(&args.file).map_err(|e| e.to_string())?,
    };
    if args.offset > source.len() || !source.is_char_boundary(args.offset) {
        return Ok(None);
    }
    let line_start = source[..args.offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[args.offset..].find('\n').map_or(source.len(), |p| args.offset + p);
    // the body of a twin method, right after a new signature: both caret shapes (`{|}` and the empty line below)
    // would be turned away by the checks that follow
    // the body of a getter or a setter, written from its field
    if let Some(b) = crate::flc_body::suggest(&source, args.offset) {
        return Ok(Some(FlcReply { text: b.text, confidence: b.confidence, origin: "derived".into() }));
    }
    if let Some(b) = crate::flc_similar::suggest(&args.file, &source, args.offset) {
        return Ok(Some(FlcReply { text: b.text, confidence: b.confidence, origin: "similar".into() }));
    }
    // the annotations this project always writes: the arguments after `@Name(`, or the one that usually follows
    if let Some(a) = crate::flc_annotations::suggest(&args.file, &source, args.offset) {
        let text = if source[args.offset..line_end].trim() == ")" { a.text.strip_suffix(')').unwrap_or(&a.text).to_string() } else { a.text };
        return Ok(Some(FlcReply { text, confidence: a.confidence, origin: "annotation".into() }));
    }
    // a block that follows from the class alone — the constructor of `public Order(`, over several lines. The editor's own closers
    // after the caret are overwritten by it, so it is asked before the rule below that turns text after the caret away
    if let Some(d) = crate::flc_derived::suggest(&source, args.offset) {
        return Ok(Some(FlcReply { text: d.text, confidence: d.confidence, origin: "derived".into() }));
    }
    // a method the class owes its supertypes, started by name — written out from the index, like the blocks above
    if let Some(d) = crate::flc_stubs::suggest(&args.file, &source, args.offset) {
        return Ok(Some(FlcReply { text: d.text, confidence: d.confidence, origin: "derived".into() }));
    }
    // the test of a method of the class under test, in its `…Test` file
    if let Some(d) = crate::flc_tests::suggest(&args.file, &source, args.offset) {
        return Ok(Some(FlcReply { text: d.text, confidence: d.confidence, origin: "derived".into() }));
    }
    let line_prefix = &source[line_start..args.offset];
    // Only at the end of the line: text after the caret would be written twice. The one exception is the closer the
    // editor adds with the `(` — `transfer(|)` — where the arguments go in between and the closer stays where it is.
    let rest = source[args.offset..line_end].trim();
    if !rest.is_empty() {
        // `foo(a.bar(|), b);` — the call was written first and its argument is being filled in: the call's own closer
        // follows the caret, and whatever comes after it is the rest of the statement
        if line_prefix.trim_end().ends_with('(') && rest.starts_with(')') {
            let without = format!("{}{}", &source[..args.offset], &source[line_end..]);
            if let Some(a) = crate::flc_arglist::suggest(&args.file, &without, args.offset) {
                let inner = a.text.strip_suffix(");").or_else(|| a.text.strip_suffix(')')).unwrap_or(&a.text).to_string();
                return Ok(Some(FlcReply { text: inner, confidence: a.confidence, origin: "arguments".into() }));
            }
            // the call takes a function: the one the project passes there, for the type the index knows it receives
            if let Some(f) = crate::flc_lambda::suggest(&args.file, &without, args.offset) {
                return Ok(Some(FlcReply { text: f.text, confidence: f.confidence, origin: "lambda".into() }));
            }
        }
        // any other closer the editor added with its opener (`@PostMapping(value = |)`, `foo(a, |)`): the layers below
        // see the line without it, and an answer that ends with the same closer leaves it where it is
        if rest.chars().all(|c| matches!(c, ')' | ';')) && line_prefix.contains('(') {
            let without = format!("{}{}", &source[..args.offset], &source[line_end..]);
            let again = FlcArgs { file: args.file.clone(), offset: args.offset, source: Some(without.clone()) };
            let reply = crate::flc_echo::suggest(&without, args.offset)
                .filter(|_| settings.layer_echo)
                .map(|e| FlcReply { text: e.text, confidence: e.confidence, origin: "echo".into() })
                .or_else(|| crate::flc_arglist::creation(&args.file, &without, args.offset).filter(|_| settings.layer_creation).map(|a| FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }))
                .or_else(|| crate::flc_arglist::array_constructor(&args.file, &without, args.offset)
                .map(|a| FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }))
                .or_else(|| crate::flc_arglist::class_literal(&args.file, &without, args.offset)
                .map(|a| FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }))
                .or_else(|| crate::flc_member_args::suggest(&args.file, &without, args.offset)
                .filter(|m| !(settings.check && crate::flc_check::invents(&args.file, &without, args.offset, &m.text)))
                .map(|m| FlcReply { text: m.text, confidence: m.confidence, origin: "habit".into() }))
                .or_else(|| crate::flc_habits::suggest(&args.file, &without, args.offset)
                .filter(|_| settings.layer_habits)
                .filter(|h| !(settings.check && crate::flc_check::invents(&args.file, &without, args.offset, &h.text)))
                .map(|h| FlcReply { text: h.text, confidence: h.confidence, origin: "habit".into() }))
                .or_else(|| model_suggestion(&settings, &again, &without, line_prefix, line_end - rest.len()));
            return Ok(reply.and_then(|mut r| {
                // a model line may end with the closer that is already there; an expression built here never does
                if matches!(r.origin.as_str(), "habit" | "model") {
                    r.text = r.text.trim_end().strip_suffix(rest).unwrap_or(r.text.trim_end()).trim_end().to_string();
                }
                (!r.text.is_empty()).then_some(r)
            }));
        }
        return Ok(None);
    }
    // the next element of a list that repeats itself: the call before the comma, again
    if let Some(e) = crate::flc_echo::suggest(&source, args.offset).filter(|_| settings.layer_echo) {
        return Ok(Some(FlcReply { text: e.text, confidence: e.confidence, origin: "echo".into() }));
    }
    // the next field of a conversion (setter or builder step), in the style of the lines above it
    if line_prefix.trim().is_empty() {
        if let Some(m) = crate::flc_mapper::suggest(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: m.text, confidence: m.confidence, origin: "mapping".into() }));
        }
    }
    // the assignment a constructor is missing, once its parameter is there
    if line_prefix.trim().is_empty() {
        if let Some(m) = crate::flc_missing::suggest(&source, args.offset) {
            return Ok(Some(FlcReply { text: m.text, confidence: m.confidence, origin: "derived".into() }));
        }
    }
    // what a test checks under the variable it just computed, in the way the project's tests write it
    if line_prefix.trim().is_empty() {
        if let Some(a) = crate::flc_assert::suggest(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "assert".into() }));
        }
    }
    // what this project usually does first when it catches this exception, on the empty line after the `catch (…) {`
    if line_prefix.trim().is_empty() {
        if let Some(c) = crate::flc_catch::suggest(&source, args.offset) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &c.text)) {
                return Ok(Some(FlcReply { text: c.text, confidence: c.confidence, origin: "catch".into() }));
            }
        }
    }
    // the next statement of a method, from the structure already written and the methods of this project that began alike
    if line_prefix.trim().is_empty() {
        if let Some(n) = crate::flc_next::suggest(&args.file, &source, args.offset) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &n.text)) {
                return Ok(Some(FlcReply { text: n.text, confidence: n.confidence, origin: "next".into() }));
            }
        }
    }
    // what this project usually writes after the statement above, on an empty line
    if line_prefix.trim().is_empty() {
        if let Some(q) = crate::flc_sequence::suggest(&source, args.offset) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &q.text)) {
                return Ok(Some(FlcReply { text: q.text, confidence: q.confidence, origin: "sequence".into() }));
            }
        }
    }
    // an empty line is only worth a suggestion under an open call chain (a builder's next step)
    let chain_start = line_prefix.trim().is_empty();
    if chain_start && !continues_chain(&source[..line_start]) {
        return Ok(None);
    }

    // the log profile: counted once per project in the background, then one file at a time
    if let Some(root) = IndexService::global().root_for_file(&args.file) {
        prepare_logs(&root);
    }
    if !chain_start {
        // every layer from here to the closing brace — the argument, habit and lambda layers (each types the call under
        // the caret) — reported as one so the gap to the model's own time shows what they cost together
        let _layers = crate::perf::phase("ghost", "argument/habit/log layers");
        // the type this project always creates under the name being declared (`val bao = new B|`)
        if let Some(c) = crate::flc_created::suggest(line_prefix) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &c.text)) {
                return Ok(Some(FlcReply { text: c.text, confidence: c.confidence, origin: "habit".into() }));
            }
        }
        // Re-counts the WHOLE edited buffer into the line index and the log profile on every request (the hash
        // changes with each keystroke), under the global log-state lock — timed to see what that costs.
        let counted = crate::perf::phase("ghost", "count_file (re-index edited buffer)");
        count_file(&args.file, &source);
        drop(counted);
        let st = log_state().lock().unwrap_or_else(|p| p.into_inner());
        if settings.logs {
            let ctx = context_at(&source, args.offset);
            if let Some(s) = st.profile.suggest(&ctx, line_prefix) {
                return Ok(Some(FlcReply { text: s.text, confidence: s.confidence, origin: "log".into() }));
            }
        }
        // what the project already writes the same way in several files comes before any guess
        if let Some(s) = st.lines.suggest(&args.file, line_prefix) {
            // a repeated line written under another receiver may chain onto the `)` of a call here that does not have the
            // member (`foo(x.build()` + `).get();`): kept only as far as the index can follow it
            if let Some(text) = without_unproven_chain(&args.file, &source, args.offset, s.text) {
                return Ok(Some(FlcReply { text, confidence: s.share, origin: "project".into() }));
            }
        }
        drop(st);
        // the argument the project always passes at this place (`.filter(Boolean.FALSE::equals)`)
        // `list.toArray(` wants `Item[]::new`: the collection knows its element type
        if let Some(a) = crate::flc_arglist::array_constructor(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }));
        }
        // a type name where a `Class<?>` is passed: the literal
        if let Some(a) = crate::flc_arglist::class_literal(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }));
        }
        // the position wants a class: how this code makes one, before any guess
        if let Some(a) = crate::flc_arglist::creation(&args.file, &source, args.offset).filter(|_| settings.layer_creation) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &a.text)) {
                return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }));
            }
        }
        // (a habit written for another method may name a variable this one does not have: the code checks decide)
        if let Some(h) = crate::flc_habits::suggest(&args.file, &source, args.offset).filter(|_| settings.layer_habits) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &h.text)) {
                return Ok(Some(FlcReply { text: h.text, confidence: h.confidence, origin: "habit".into() }));
            }
        }
        // the usual call on a variable (`bao.toByteArray()`), with the nearest variable that has the method
        if let Some(m) = crate::flc_member_args::suggest(&args.file, &source, args.offset) {
            if !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &m.text)) {
                return Ok(Some(FlcReply { text: m.text, confidence: m.confidence, origin: "habit".into() }));
            }
        }
        // the arguments after the first, where the caret follows a comma: each as certain as its own type makes it
        if let Some(a) = crate::flc_arglist::rest(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }));
        }
        // the function a call takes (`.map(▌`), as the project writes it, for the type the index knows it receives
        if let Some(f) = crate::flc_lambda::suggest(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: f.text, confidence: f.confidence, origin: "lambda".into() }));
        }
        // the whole argument list of the call just opened, when every parameter has a clear match in scope
        if let Some(a) = crate::flc_arglist::suggest(&args.file, &source, args.offset) {
            return Ok(Some(FlcReply { text: a.text, confidence: a.confidence, origin: "arguments".into() }));
        }
    }
    // what the project wrote after the same words, copied: a strong share goes before the model, a weak one only fills its silence.
    // The bench first read this layer as stealing lines from the model; judged as far as it writes, it is right more often than the
    // model on the lines it takes (75% against 41%), and its confidence tracks its precision.
    let copied = if settings.enabled && settings.layer_ngram {
        crate::flc_ngram::suggest(line_prefix)
            .filter(|g| !(settings.check && crate::flc_check::invents(&args.file, &source, args.offset, &g.text)))
    } else {
        None
    };
    let reply = |g: crate::flc_ngram::Copied| FlcReply { text: g.text, confidence: g.confidence, origin: "ngram".into() };
    if copied.as_ref().is_some_and(|g| g.confidence >= crate::flc_ngram::STRONG_SHARE) {
        return Ok(copied.map(reply));
    }
    let _model = crate::perf::phase("ghost", "ngram+model");
    Ok(model_suggestion(&settings, &args, &source, line_prefix, line_end).or_else(|| copied.map(reply)))
}

#[derive(Deserialize)]
pub struct PrepareArgs {
    /// The project root to count the log statements of.
    pub root: String,
}

/// Start the background work for a project: count its log statements, and load the model if it is switched on.
/// Safe to call as often as you like; each part runs once.
#[arbor_rpc::handler]
fn bennu_flc_prepare(_ctx: &BennuState, args: PrepareArgs) -> Result<(), String> {
    prepare_now(&args.root);
    Ok(())
}

/// [`bennu_flc_prepare`] without the request around it.
pub(crate) fn prepare_now(root: &str) {
    let settings = settings();
    if settings.logs || settings.enabled {
        prepare_logs(root);
    }
    if settings.enabled && !settings.model_dir.is_empty() {
        ensure_model(&settings.model_dir);
    }
}

/// What is running, for the settings page.
#[arbor_rpc::handler]
fn bennu_flc_status(_ctx: &BennuState) -> Result<FlcStatus, String> {
    Ok(status_now())
}

/// [`bennu_flc_status`] without the request around it — what the quality harness waits on.
pub(crate) fn status_now() -> FlcStatus {
    let settings = settings();
    let model = if !settings.enabled {
        "off".to_string()
    } else if settings.model_dir.is_empty() {
        "no folder".to_string()
    } else {
        match model_slot().lock().unwrap_or_else(|p| p.into_inner()).state {
            ModelState::Idle => "idle".to_string(),
            ModelState::Loading => "loading".to_string(),
            ModelState::Ready(_) => "ready".to_string(),
            ModelState::Failed(ref e) => format!("failed: {e}"),
        }
    };
    let st = log_state().lock().unwrap_or_else(|p| p.into_inner());
    FlcStatus { enabled: settings.enabled, model_dir: settings.model_dir, model, log_calls: st.profile.call_count(), log_patterns: st.profile.pattern_count(), log_scanning: !st.scanning.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings and the profile are process-wide, so the tests that change them take turns.
    static TURN: Mutex<()> = Mutex::new(());

    const SERVICE: &str = "class A {\n    private static final Logger LOG = LoggerFactory.getLogger(A.class);\n\n    void load(String id) {\n        LOG.debug(\"START - load({})\", id);\n    }\n\n    void save(String name) {\n        LOG.debug(\"START - save({})\", name);\n    }\n}\n";

    #[test]
    fn the_receiver_starts_where_the_chain_does() {
        let text = "x = repo.find(id).items().";
        assert_eq!(&text[receiver_start(text, text.len() - 1)..text.len() - 1], "repo.find(id).items()");
        assert_eq!(receiver_start("items.", 5), 0);
    }

    fn reset_log_state() {
        *log_state().lock().unwrap_or_else(|p| p.into_inner()) = LogState::default();
    }

    fn calls() -> u32 {
        log_state().lock().unwrap_or_else(|p| p.into_inner()).profile.call_count()
    }

    /// The profile is saved, comes back after a restart without the files being counted again, loses the files that
    /// vanished while the app was closed, and follows changes the watcher reports.
    #[test]
    fn the_profile_survives_a_restart_and_follows_the_disk() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!("arbor-flc-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("proj");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/A.java"), SERVICE).unwrap();
        std::fs::write(root.join("src/B.java"), SERVICE).unwrap();
        *TEST_STORE.lock().unwrap_or_else(|p| p.into_inner()) = Some(dir.join("store"));
        let mut config = BennuConfig::default();
        config.flc_logs = true;
        apply_flc_settings(&config);
        let root_s = root.to_string_lossy().into_owned();

        reset_log_state();
        scan_root(&root_s, &[]);
        assert_eq!(calls(), 4);
        assert!(dir.join("store/profile.json").is_file());

        // a restart: nothing in memory, the saved copy answers before any counting
        reset_log_state();
        scan_root(&root_s, &[]);
        assert_eq!(calls(), 4);

        // a file removed while the app was closed
        std::fs::remove_file(root.join("src/B.java")).unwrap();
        reset_log_state();
        scan_root(&root_s, &[]);
        assert_eq!(calls(), 2);

        // the watcher reports a removal and a new file
        log_state().lock().unwrap_or_else(|p| p.into_inner()).scanned.insert(root_s.clone());
        std::fs::remove_file(root.join("src/A.java")).unwrap();
        std::fs::write(root.join("src/C.java"), SERVICE).unwrap();
        apply_changes(vec![Change::Files { root: root_s.clone(), rels: vec!["src/A.java".into(), "src/C.java".into(), "src/notes.txt".into()] }]);
        assert_eq!(calls(), 2, "A left, C came");
        std::fs::remove_file(root.join("src/C.java")).unwrap();
        apply_changes(vec![Change::Rescan { root: root_s }]);
        assert_eq!(calls(), 0);

        *TEST_STORE.lock().unwrap_or_else(|p| p.into_inner()) = None;
        reset_log_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole log path through the request: counted once, then a new method gets the project's entry line.
    #[test]
    fn a_new_method_gets_the_projects_entry_line() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let mut config = BennuConfig::default();
        config.flc_logs = true;
        config.flc_enabled = false;
        apply_flc_settings(&config);
        count_file("/tmp/flc-test/A.java", SERVICE);
        count_file("/tmp/flc-test/A.java", SERVICE); // the same text twice counts once
        let src = "class B {\n    void rename(String from, String to) {\n        LOG";
        let reply = suggest(FlcArgs { file: "/tmp/flc-test/B.java".into(), offset: src.len(), source: Some(src.into()) })
            .expect("no error")
            .expect("a suggestion");
        assert_eq!(reply.origin, "log");
        assert_eq!(reply.text, ".debug(\"START - rename({}, {})\", from, to);");
    }

    /// A start that several files finish the same way is completed from them, ahead of the model and of the log profile.
    #[test]
    fn a_line_several_files_write_alike_is_completed_from_them() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let mut config = BennuConfig::default();
        config.flc_logs = true;
        apply_flc_settings(&config);
        let guard = "class G {\n    Order find(long id) {\n        return repository.findOrderById(id).orElseThrow(OrderNotFoundException::new);\n    }\n}\n";
        for name in ["G1", "G2", "G3"] {
            count_file(&format!("/tmp/flc-test/{name}.java"), guard);
        }
        let src = "class H {\n    Order get(long id) {\n        return repository.findOr";
        let reply = suggest(FlcArgs { file: "/tmp/flc-test/H.java".into(), offset: src.len(), source: Some(src.into()) })
            .expect("no error")
            .expect("a suggestion");
        assert_eq!(reply.origin, "project");
        assert_eq!(reply.text, "derById(id).orElseThrow(OrderNotFoundException::new);");
    }

    #[test]
    fn nothing_is_said_mid_line_or_when_the_switches_are_off() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let mut config = BennuConfig::default();
        config.flc_logs = true;
        apply_flc_settings(&config);
        let src = "class B {\n    void f() {\n        foo(bar)\n";
        let at = src.find("foo(").unwrap() + 4;
        assert!(suggest(FlcArgs { file: "/tmp/flc-test/B.java".into(), offset: at, source: Some(src.into()) }).unwrap().is_none());
        config.flc_logs = false;
        apply_flc_settings(&config);
        assert!(suggest(FlcArgs { file: "/tmp/flc-test/B.java".into(), offset: src.len(), source: Some(src.into()) }).unwrap().is_none());
        config.flc_logs = true;
        apply_flc_settings(&config);
    }

    #[test]
    fn switching_the_model_off_empties_its_slot() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
            slot.dir = "/tmp/flc-test/model".into();
            slot.state = ModelState::Failed("stand-in for a loaded model".into());
        }
        let mut config = BennuConfig::default();
        config.flc_enabled = false;
        config.flc_model_dir = "/tmp/flc-test/model".into();
        apply_flc_settings(&config);
        let slot = model_slot().lock().unwrap_or_else(|p| p.into_inner());
        assert!(slot.dir.is_empty() && matches!(slot.state, ModelState::Idle));
    }

    /// The model path through the request, with real weights: loads in the background, then answers. Skipped
    /// unless `BENNU_FLC_MODEL` names the folder: `BENNU_FLC_MODEL=/path cargo test -p bennu-be flc::tests::model`.
    #[test]
    fn model_loads_in_the_background_and_answers() {
        let Ok(dir) = std::env::var("BENNU_FLC_MODEL") else {
            eprintln!("BENNU_FLC_MODEL not set: skipping");
            return;
        };
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let mut config = BennuConfig::default();
        config.flc_enabled = true;
        config.flc_logs = false;
        config.flc_model_dir = dir;
        config.flc_min_confidence = 30;
        apply_flc_settings(&config);
        let src = "package p;\n\nimport java.util.List;\n\nclass A {\n    int count(List<String> items) {\n        return items.";
        let args = || FlcArgs { file: "/tmp/flc-test/A.java".into(), offset: src.len(), source: Some(src.into()) };
        // the first request starts the load and must not wait for it
        let started = std::time::Instant::now();
        let first = suggest(args()).expect("no error");
        assert!(started.elapsed().as_millis() < 500, "the request waited for the model");
        let mut reply = first;
        for _ in 0..100 {
            if reply.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            reply = suggest(args()).expect("no error");
        }
        let reply = reply.expect("the model answered once it was loaded");
        eprintln!("model said {:?} ({:.2})", reply.text, reply.confidence);
        assert_eq!(reply.origin, "model");
        assert!(!reply.text.contains('\n'));
        // a new line under an open builder chain is asked about too
        let chain = "package p;\n\nclass A {\n    Object make() {\n        return java.util.stream.Stream.builder()\n            .add(1)\n            ".to_string();
        let args = FlcArgs { file: "/tmp/flc-test/A.java".into(), offset: chain.len(), source: Some(chain) };
        eprintln!("chain line, model said {:?}", suggest(args).expect("no error").map(|r| r.text));
        // mid-word: what is typed stays out of the answer, which continues it
        let typed = format!("{src}si");
        let args = FlcArgs { file: "/tmp/flc-test/A.java".into(), offset: typed.len(), source: Some(typed) };
        if let Some(r) = suggest(args).expect("no error") {
            eprintln!("mid-word, model said {:?}", r.text);
            assert!(r.text.starts_with("ze"), "{:?}", r.text);
        }
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::calibrate;

    #[test]
    fn a_kind_the_user_always_keeps_is_untouched_and_one_never_kept_loses_a_third() {
        assert_eq!(calibrate(0.8, 1.0, 0.7), Some(0.8));
        assert_eq!(calibrate(0.95, 0.0, 0.7), None, "0.95 x 0.7 is under the threshold");
        assert!((calibrate(1.0, 0.0, 0.5).unwrap() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn scaling_that_takes_a_suggestion_under_the_threshold_silences_it() {
        assert_eq!(calibrate(0.8, 0.0, 0.7), None);
    }

    #[test]
    fn a_suggestion_already_under_the_threshold_is_left_as_it_was() {
        assert_eq!(calibrate(0.6, 0.0, 0.7), Some(0.6 * 0.7));
    }
}
