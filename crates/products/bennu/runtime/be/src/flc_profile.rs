//! What Bennu learned about how you write — the arguments a project always passes, the annotations it always writes,
//! the names it gives its variables — kept on disk, per project and across projects, so the next start has it before
//! the first file is read.
//!
//! **What is written.** Short pieces of source: argument texts (`Boolean.FALSE::equals`), the shape of an annotation
//! with its strings blanked, variable names with their types. Counters and hashes, no whole line and no method body
//! (the repeated-lines index and the sibling-method bodies stay in memory, always). Everything lives in the user's
//! Arbor data folder, never in a repository, and is never sent anywhere.
//!
//! **Per project** (`<root hash>.profile.json`): for every file, its path relative to the root, the hash of the text
//! that was counted and what it contributed. A start loads it first; the scan then skips the parse of every file whose
//! hash still matches, and takes back what files that vanished had contributed.
//!
//! **Across projects** (`global.profile.json`): per project, its total counters. A lookup that finds too little in the
//! current project asks what the *other* projects saw. A project's entry is replaced whenever it is saved, so scanning
//! it again never counts it twice.
//!
//! Two settings decide: `flc_profile_project` and `flc_profile_global`. With both off nothing is read or written.
//! Writes are atomic (temp file, rename), on the background threads only, and never on the keystroke path.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::flc_annotations::{AnnTotals, Part};
use crate::flc_habits::{HabitPart, HabitTotals};
use crate::flc_names::NameTotals;

const VERSION: u32 = 1;

/// Which of the two settings are on: `(per project, across projects)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Flags {
    pub project: bool,
    pub global: bool,
}

// ---- hashing and the dirty flag ---------------------------------------------------------------------------------------

/// FNV-1a over the text: the same on every run and every Rust release, which a hash kept on disk has to be.
pub(crate) fn text_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// A copy written by a build that hashes differently is not trusted.
fn probe() -> u64 {
    text_hash("flc-profile-probe")
}

static DIRTY: AtomicBool = AtomicBool::new(false);

/// What the tables hold changed since the last write.
pub(crate) fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

// ---- where it lives -----------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct Store {
    dir: PathBuf,
}

impl Store {
    fn new(dir: PathBuf) -> Self {
        Store { dir }
    }

    #[cfg(test)]
    pub(crate) fn at(dir: &Path) -> Self {
        Store::new(dir.to_path_buf())
    }

    fn standard() -> Self {
        Store::new(arbor_core::prelude::bennu_data_dir().join("flc"))
    }

    fn project(&self, root: &Path) -> PathBuf {
        self.dir.join(format!("{}.profile.json", root_key(root)))
    }

    fn global(&self) -> PathBuf {
        self.dir.join("global.profile.json")
    }
}

fn root_key(root: &Path) -> String {
    format!("{:016x}", text_hash(&root.to_string_lossy()))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

// ---- the formats ---------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct ProjectDoc {
    version: u32,
    probe: u64,
    files: Vec<FileDoc>,
}

#[derive(Serialize, Deserialize, Default)]
struct FileDoc {
    /// Relative to the project root, forward slashes: the copy survives the project being moved.
    path: String,
    /// Hash of the text whose contribution this is.
    hash: u64,
    habits: HabitPart,
    names: Vec<(String, String)>,
    anns: Vec<Part>,
}

/// A project's counters, the three tables summed over its files.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub(crate) struct Totals {
    habits: HabitTotals,
    names: NameTotals,
    anns: AnnTotals,
}

#[derive(Serialize, Deserialize)]
struct GlobalDoc {
    version: u32,
    probe: u64,
    /// project root hash → its totals
    projects: BTreeMap<String, Totals>,
}

fn relative(root: &Path, key: &str) -> Option<String> {
    Some(Path::new(key).strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"))
}

fn under(root: &Path) -> impl Fn(&str) -> bool + '_ {
    move |key| Path::new(key).starts_with(root)
}

// ---- the process state ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct State {
    /// Roots that were loaded or scanned: the ones the timer writes.
    roots: Vec<PathBuf>,
    /// What the global file says, by root hash, as last read or written.
    global: BTreeMap<String, Totals>,
    /// Root hashes whose files are in the tables now: their totals are not "other projects'".
    live: HashSet<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|p| p.into_inner())
}

fn read_project(store: &Store, root: &Path) -> Option<ProjectDoc> {
    let doc: ProjectDoc = serde_json::from_slice(&std::fs::read(store.project(root)).ok()?).ok()?;
    (doc.version == VERSION && doc.probe == probe()).then_some(doc)
}

fn read_global(store: &Store) -> BTreeMap<String, Totals> {
    let Ok(bytes) = std::fs::read(store.global()) else { return BTreeMap::new() };
    match serde_json::from_slice::<GlobalDoc>(&bytes) {
        Ok(doc) if doc.version == VERSION && doc.probe == probe() => doc.projects,
        _ => BTreeMap::new(),
    }
}

/// Hands the tables what the projects that are not loaded saw.
fn refresh_prior(st: &State) {
    let mut sum = Totals::default();
    for (key, totals) in st.global.iter().filter(|(k, _)| !st.live.contains(*k)) {
        let _ = key;
        sum.habits.texts.extend(totals.habits.texts.iter().cloned());
        sum.habits.names.extend(totals.habits.names.iter().cloned());
        sum.names.pairs.extend(totals.names.pairs.iter().cloned());
        sum.anns.forms.extend(totals.anns.forms.iter().cloned());
        sum.anns.next.extend(totals.anns.next.iter().cloned());
    }
    crate::flc_habits::set_prior(&sum.habits);
    crate::flc_names::set_prior(&sum.names);
    crate::flc_annotations::set_prior(&sum.anns);
}

// ---- loading -------------------------------------------------------------------------------------------------------

/// Before the scan of `root`: put back what was saved, so suggestions work at once and the scan skips unchanged files.
pub(crate) fn load_project(root: &str) {
    load_in(&Store::standard(), Path::new(root), crate::flc::profile_flags());
}

pub(crate) fn load_in(store: &Store, root: &Path, flags: Flags) {
    if !flags.project && !flags.global {
        return;
    }
    if flags.project {
        if let Some(doc) = read_project(store, root) {
            for f in doc.files {
                let key = root.join(&f.path).to_string_lossy().into_owned();
                crate::flc_habits::restore_file(&key, f.hash, f.habits);
                crate::flc_names::restore_file(&key, f.hash, f.names);
                crate::flc_annotations::restore_file(&key, f.hash, f.anns);
            }
        }
    }
    let mut st = lock();
    if !st.roots.iter().any(|r| r == root) {
        st.roots.push(root.to_path_buf());
    }
    st.live.insert(root_key(root));
    if flags.global {
        st.global = read_global(store);
        refresh_prior(&st);
    }
}

// ---- saving --------------------------------------------------------------------------------------------------------

/// What the three tables hold for `root`, as the per-file documents.
fn project_doc(root: &Path) -> ProjectDoc {
    let include = under(root);
    let mut files: BTreeMap<String, FileDoc> = BTreeMap::new();
    for (key, hash, habits) in crate::flc_habits::snapshot(&include) {
        if let Some(path) = relative(root, &key) {
            files.insert(key.clone(), FileDoc { path, hash, habits, ..Default::default() });
        }
    }
    for (key, hash, names) in crate::flc_names::snapshot(&include) {
        if let Some(f) = files.get_mut(&key) {
            f.names = names;
        } else if let Some(path) = relative(root, &key) {
            files.insert(key, FileDoc { path, hash, names, ..Default::default() });
        }
    }
    for (key, hash, anns) in crate::flc_annotations::snapshot(&include) {
        if let Some(f) = files.get_mut(&key) {
            f.anns = anns;
        } else if let Some(path) = relative(root, &key) {
            files.insert(key, FileDoc { path, hash, anns, ..Default::default() });
        }
    }
    let mut files: Vec<FileDoc> = files.into_values().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    ProjectDoc { version: VERSION, probe: probe(), files }
}

fn totals_of(root: &Path) -> Totals {
    let include = under(root);
    Totals { habits: crate::flc_habits::totals(&include), names: crate::flc_names::totals(&include), anns: crate::flc_annotations::totals(&include) }
}

pub(crate) fn save_in(store: &Store, root: &Path, flags: Flags) {
    if flags.project {
        if let Ok(json) = serde_json::to_vec(&project_doc(root)) {
            let _ = write_atomically(&store.project(root), &json);
        }
    }
    if flags.global {
        let mine = totals_of(root);
        let mut st = lock();
        // the project being saved is the one in the tables: its own totals are not "other projects'"
        st.live.insert(root_key(root));
        // another run may have saved other projects since: keep them, replace only this one
        let mut projects = read_global(store);
        projects.insert(root_key(root), mine);
        let doc = GlobalDoc { version: VERSION, probe: probe(), projects: projects.clone() };
        if let Ok(json) = serde_json::to_vec(&doc) {
            let _ = write_atomically(&store.global(), &json);
        }
        st.global = projects;
        refresh_prior(&st);
    }
}

/// After the scan of `root`: the files that vanished give back what they contributed, then the profile is written.
pub(crate) fn finish_scan(root: &str, walked: &HashSet<String>) {
    let root_path = Path::new(root);
    let flags = crate::flc::profile_flags();
    {
        let mut st = lock();
        if !st.roots.iter().any(|r| r == root_path) {
            st.roots.push(root_path.to_path_buf());
        }
        st.live.insert(root_key(root_path));
    }
    let known: HashSet<String> = crate::flc_habits::files().into_iter().chain(crate::flc_names::files()).chain(crate::flc_annotations::files()).collect();
    for gone in known.iter().filter(|k| Path::new(k).starts_with(root_path) && !walked.contains(*k)) {
        crate::flc_habits::remove_file(gone);
        crate::flc_names::remove_file(gone);
        crate::flc_annotations::remove_file(gone);
    }
    if flags.project || flags.global {
        save_in(&Store::standard(), root_path, flags);
    }
    DIRTY.store(false, Ordering::Relaxed);
}

/// On the maintenance thread's timer: write what changed, for every project that was scanned.
pub(crate) fn save_dirty() {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let flags = crate::flc::profile_flags();
    if !flags.project && !flags.global {
        return;
    }
    let roots: Vec<PathBuf> = lock().roots.clone();
    let store = Store::standard();
    for root in roots {
        save_in(&store, &root, flags);
    }
}

// ---- forgetting ----------------------------------------------------------------------------------------------------

/// Deletes every saved profile, project and global. Returns how many files went.
pub(crate) fn forget_files(store: &Store) -> usize {
    let mut gone = 0;
    for entry in std::fs::read_dir(&store.dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".profile.json") && std::fs::remove_file(entry.path()).is_ok() {
            gone += 1;
        }
    }
    gone
}

/// Forget what Bennu learned: the files, and what the tables hold now.
#[arbor_rpc::handler]
fn bennu_flc_forget(_ctx: &BennuState) -> Result<(), String> {
    forget_files(&Store::standard());
    crate::flc_habits::clear();
    crate::flc_names::clear();
    crate::flc_annotations::clear();
    crate::flc_choices::clear();
    crate::flc_sequence::clear();
    crate::flc_member_args::clear();
    crate::flc_created::clear();
    crate::flc_ngram::clear();
    crate::flc_catch::clear();
    crate::flc_lambda::clear();
    crate::flc_assert::clear();
    crate::flc_next::clear();
    crate::flc_format::clear();
    crate::flc::forget_scans();
    let mut st = lock();
    st.global.clear();
    DIRTY.store(false, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Flags = Flags { project: true, global: true };
    const OFF: Flags = Flags { project: false, global: false };

    /// The tables are process-wide: these tests take turns and only touch keys under their own temp root.
    static TURN: Mutex<()> = Mutex::new(());

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arbor-flc-profile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A test starts with no prior: what an earlier test carried over would answer for it.
    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        crate::flc_habits::set_prior(&HabitTotals::default());
        crate::flc_names::set_prior(&NameTotals::default());
        crate::flc_annotations::set_prior(&AnnTotals::default());
        *lock() = State::default();
        turn
    }

    fn controller(path: &str) -> String {
        format!(
            "class C {{\n    @Operation(summary = \"x\")\n    @PostMapping(\"{path}\")\n    public Object go(OrderService orderSvc) {{\n        return repo.find(id).filter(Boolean.FALSE::equals).orElseThrow(NotFoundException::new).map(request);\n    }}\n}}\n"
        )
    }

    fn learn(root: &Path, files: &[&str]) -> Vec<String> {
        files
            .iter()
            .map(|f| {
                let key = root.join(f).to_string_lossy().into_owned();
                let text = controller(f);
                crate::flc_habits::add_file(&key, &text);
                crate::flc_names::add_file(&key, &text);
                crate::flc_annotations::add_file(&key, &text);
                key
            })
            .collect()
    }

    fn forget_keys(keys: &[String]) {
        for k in keys {
            crate::flc_habits::remove_file(k);
            crate::flc_names::remove_file(k);
            crate::flc_annotations::remove_file(k);
        }
    }

    #[test]
    fn a_saved_project_comes_back_without_reading_its_files() {
        let _t = fresh();
        let dir = scratch("roundtrip");
        let root = dir.join("proj");
        let store = Store::at(&dir.join("store"));
        let keys = learn(&root, &["A.java", "B.java", "C.java"]);
        save_in(&store, &root, ON);
        // the file holds counters, not code
        let text = std::fs::read_to_string(store.project(&root)).unwrap();
        assert!(text.contains("Boolean.FALSE::equals") && !text.contains("public Object go"), "{text}");
        // a fresh process: the tables are empty until the profile is loaded
        forget_keys(&keys);
        assert!(crate::flc_habits::files().iter().all(|k| !keys.contains(k)));
        load_in(&store, &root, ON);
        for k in &keys {
            assert!(crate::flc_habits::files().contains(k) && crate::flc_names::files().contains(k) && crate::flc_annotations::files().contains(k), "{k}");
        }
        // three files said `request` at `map`/0: the habit is back
        assert_eq!(crate::flc_habits::habit_name_in("Other.java", "map", 0).map(|h| h.0).as_deref(), Some("request"));
        // the same text again is not parsed again: the contribution is still counted once
        let before = crate::flc_habits::totals(&under(&root));
        crate::flc_habits::add_file(&keys[0], &controller("A.java"));
        assert_eq!(crate::flc_habits::totals(&under(&root)), before);
        forget_keys(&keys);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_copy_written_under_other_rules_or_damaged_is_not_loaded() {
        let dir = scratch("rules");
        let root = dir.join("proj");
        let store = Store::at(&dir);
        std::fs::write(store.project(&root), b"{ not json").unwrap();
        assert!(read_project(&store, &root).is_none());
        let other = ProjectDoc { version: VERSION + 1, probe: probe(), files: Vec::new() };
        std::fs::write(store.project(&root), serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(read_project(&store, &root).is_none(), "another version means a rescan");
        let other = ProjectDoc { version: VERSION, probe: probe() + 1, files: Vec::new() };
        std::fs::write(store.project(&root), serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(read_project(&store, &root).is_none(), "another hash function means a rescan");
        assert!(read_global(&Store::at(&dir.join("missing"))).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_vanished_file_gives_back_what_it_contributed() {
        let _t = fresh();
        let dir = scratch("vanished");
        let root = dir.join("proj");
        let keys = learn(&root, &["A.java", "B.java", "C.java"]);
        assert_eq!(crate::flc_habits::habit_name_in("Other.java", "map", 0).map(|h| h.0).as_deref(), Some("request"));
        // the walk found only two of them
        let walked: HashSet<String> = keys[..2].iter().cloned().collect();
        let known: Vec<String> = crate::flc_habits::files().into_iter().filter(|k| Path::new(k).starts_with(&root)).collect();
        assert_eq!(known.len(), 3);
        for gone in known.iter().filter(|k| !walked.contains(*k)) {
            crate::flc_habits::remove_file(gone);
            crate::flc_names::remove_file(gone);
            crate::flc_annotations::remove_file(gone);
        }
        assert!(crate::flc_habits::habit_name_in("Other.java", "map", 0).is_none(), "two sightings are not a habit");
        forget_keys(&keys);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_a_project_again_replaces_its_entry_in_the_global_file() {
        let _t = fresh();
        let dir = scratch("global");
        let root = dir.join("proj");
        let store = Store::at(&dir.join("store"));
        let keys = learn(&root, &["A.java", "B.java", "C.java"]);
        save_in(&store, &root, ON);
        let once = read_global(&store)[&root_key(&root)].clone();
        save_in(&store, &root, ON);
        save_in(&store, &root, ON);
        let thrice = read_global(&store);
        assert_eq!(thrice.len(), 1);
        assert_eq!(thrice[&root_key(&root)], once, "scanned again, not counted again");
        // another project's entry is kept when this one is saved
        let mut projects = read_global(&store);
        projects.insert("other".into(), Totals::default());
        let json = serde_json::to_vec(&GlobalDoc { version: VERSION, probe: probe(), projects }).unwrap();
        std::fs::write(store.global(), json).unwrap();
        save_in(&store, &root, ON);
        assert!(read_global(&store).contains_key("other"));
        forget_keys(&keys);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_other_projects_saw_is_a_prior_only_where_this_one_has_too_little() {
        let _t = fresh();
        let dir = scratch("prior");
        let store = Store::at(&dir.join("store"));
        // project one learned a habit and was saved
        let one = dir.join("one");
        let keys = learn(&one, &["A.java", "B.java", "C.java"]);
        save_in(&store, &one, ON);
        forget_keys(&keys);
        // a new process: nothing is loaded yet
        *lock() = State::default();
        // project two has seen nothing: it loads, and the habit of project one answers
        let two = dir.join("two");
        load_in(&store, &two, ON);
        assert_eq!(crate::flc_habits::habit_name_in("X.java", "map", 0).map(|h| h.0).as_deref(), Some("request"));
        assert_eq!(crate::flc_names::name_for("X.java", "class Z {\n    void f() {\n        OrderService\n    }\n}\n", 38, "OrderService").as_deref(), Some("orderSvc"));
        // with the global switch off, nothing is carried over
        crate::flc_habits::set_prior(&HabitTotals::default());
        crate::flc_names::set_prior(&NameTotals::default());
        crate::flc_annotations::set_prior(&AnnTotals::default());
        load_in(&store, &two, Flags { project: true, global: false });
        assert!(crate::flc_habits::habit_name_in("X.java", "map", 0).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_both_switches_off_nothing_is_written_or_loaded() {
        let _t = fresh();
        let dir = scratch("off");
        let root = dir.join("proj");
        let store = Store::at(&dir.join("store"));
        let keys = learn(&root, &["A.java"]);
        save_in(&store, &root, OFF);
        assert!(!store.dir.exists(), "no folder, no file");
        std::fs::create_dir_all(&store.dir).unwrap();
        save_in(&store, &root, ON);
        forget_keys(&keys);
        load_in(&store, &root, OFF);
        assert!(crate::flc_habits::files().iter().all(|k| !keys.contains(k)), "nothing was loaded");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn forgetting_deletes_the_profiles() {
        let dir = scratch("forget");
        let store = Store::at(&dir);
        std::fs::write(store.project(Path::new("/a")), b"{}").unwrap();
        std::fs::write(store.project(Path::new("/b")), b"{}").unwrap();
        std::fs::write(store.global(), b"{}").unwrap();
        std::fs::write(dir.join("keep.json"), b"{}").unwrap();
        assert_eq!(forget_files(&store), 3);
        assert!(dir.join("keep.json").exists() && !store.global().exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_text_hash_is_fixed() {
        assert_eq!(text_hash(""), 0xcbf29ce484222325);
        assert_ne!(text_hash("a"), text_hash("b"));
    }
}
