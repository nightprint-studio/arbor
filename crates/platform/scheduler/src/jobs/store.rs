//! The jobs in memory, mirrored to one JSON file on every change.
//!
//! One list, one lock, loaded on first use. A file that cannot be read starts an empty list rather than
//! failing the window — and is left alone until the next save, so a hand-edit mistake is not erased by merely
//! opening the app. Writes go to a temp file and are renamed over the real one, so a crash cannot leave half.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use super::job::{Job, JobAction, Tick};

/// How a file's text becomes jobs. The default reads the current format; a product with an older format of
/// its own passes a reader that understands both.
pub type Reader<A> = Box<dyn Fn(&str) -> Vec<Job<A>> + Send + Sync>;

struct State<A> {
    loaded: bool,
    jobs: Vec<Job<A>>,
}

pub struct Store<A> {
    path: PathBuf,
    read: Reader<A>,
    state: Mutex<State<A>>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// An id for a new job, unique enough for a list a person edits by hand.
pub fn new_id() -> String {
    format!("sch-{:x}", now_ms())
}

/// The jobs as the file holds them; none when the text is not readable.
pub fn parse<A: JobAction>(text: &str) -> Vec<Job<A>> {
    serde_json::from_str(text).unwrap_or_default()
}

/// Write `jobs` to `path` through a temp file.
pub fn write_atomic<A: JobAction>(path: &Path, jobs: &[Job<A>]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(jobs).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Soonest first; jobs with nothing coming (disabled, finished) after, by name.
fn by_next_run<A>(a: &Job<A>, b: &Job<A>) -> Ordering {
    match (a.next_run_ms, b.next_run_ms) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

impl<A: JobAction + Send + 'static> Store<A> {
    pub fn new(path: PathBuf) -> Self {
        Self::with_reader(path, Box::new(parse::<A>))
    }

    pub fn with_reader(path: PathBuf, read: Reader<A>) -> Self {
        Self { path, read, state: Mutex::new(State { loaded: false, jobs: Vec::new() }) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock(&self) -> MutexGuard<'_, State<A>> {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !s.loaded {
            s.loaded = true;
            if let Some(text) = std::fs::read_to_string(&self.path).ok() {
                s.jobs = (self.read)(&text);
            }
        }
        s
    }

    fn persist(&self, s: &State<A>) -> Result<(), String> {
        write_atomic(&self.path, &s.jobs)
    }

    pub fn list(&self) -> Vec<Job<A>> {
        let mut jobs = self.lock().jobs.clone();
        jobs.sort_by(by_next_run);
        jobs
    }

    pub fn get(&self, id: &str) -> Option<Job<A>> {
        self.lock().jobs.iter().find(|j| j.id == id).cloned()
    }

    /// Insert a new job or replace the one with the same id. The caller has validated and scheduled it.
    pub fn upsert(&self, job: Job<A>) -> Result<Job<A>, String> {
        let mut s = self.lock();
        match s.jobs.iter_mut().find(|j| j.id == job.id) {
            Some(slot) => *slot = job.clone(),
            None => s.jobs.push(job.clone()),
        }
        self.persist(&s)?;
        Ok(job)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut s = self.lock();
        let before = s.jobs.len();
        s.jobs.retain(|j| j.id != id);
        if s.jobs.len() == before {
            return Err(format!("schedule '{id}' not found"));
        }
        self.persist(&s)
    }

    /// Change one job in place and save; `f` sees the stored copy. Returns what it left.
    pub fn mutate<R>(&self, id: &str, f: impl FnOnce(&mut Job<A>) -> Result<R, String>) -> Result<R, String> {
        let mut s = self.lock();
        let job = s.jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| format!("schedule '{id}' not found"))?;
        let out = f(job)?;
        self.persist(&s)?;
        Ok(out)
    }

    /// Every job's tick at `now_ms`: the jobs to run now (already marked started), saved.
    pub fn due_now(&self, now_ms: i64) -> Vec<Job<A>> {
        let mut s = self.lock();
        let mut to_run = Vec::new();
        let mut changed = false;
        for job in s.jobs.iter_mut() {
            let before = job.clone();
            if job.tick(now_ms) == Tick::Run {
                job.started(now_ms);
                to_run.push(job.clone());
            }
            changed |= *job != before;
        }
        if changed {
            let _ = self.persist(&s);
        }
        to_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{MissedPolicy, Recurrence};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Act {
        what: String,
    }
    impl JobAction for Act {
        fn word(&self) -> &'static str {
            "act"
        }
    }

    fn job(id: &str) -> Job<Act> {
        Job {
            id: id.into(),
            name: id.into(),
            enabled: true,
            recurrence: Recurrence::Daily { hour: 9, minute: 0 },
            missed: MissedPolicy::Skip,
            action: Act { what: "x".into() },
            root: "/r".into(),
            mode: "manual".into(),
            created_ms: 0,
            next_run_ms: None,
            last_run: None,
            runs: 0,
        }
    }

    fn temp_file(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arbor-jobs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("schedules.json")
    }

    #[test]
    fn a_saved_file_reads_back_and_a_broken_one_reads_as_empty() {
        let path = temp_file("rw");
        write_atomic(&path, &[job("a"), job("b")]).unwrap();
        let back = parse::<Act>(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(back.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(!path.with_extension("json.tmp").exists(), "the temp file was renamed away");
        assert!(parse::<Act>("{ not json").is_empty());
        assert!(parse::<Act>("").is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_damaged_file_is_not_overwritten_until_the_next_save() {
        let path = temp_file("damaged");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ hand edit gone wrong").unwrap();
        let store = Store::<Act>::new(path.clone());
        assert!(store.list().is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ hand edit gone wrong", "reading did not touch it");
        store.upsert(job("a")).unwrap();
        assert_eq!(parse::<Act>(&std::fs::read_to_string(&path).unwrap()).len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn upsert_replaces_by_id_and_remove_says_when_there_is_nothing_to_remove() {
        let path = temp_file("crud");
        let store = Store::<Act>::new(path.clone());
        store.upsert(job("a")).unwrap();
        let mut renamed = job("a");
        renamed.name = "Renamed".into();
        store.upsert(renamed).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get("a").unwrap().name, "Renamed");
        store.remove("a").unwrap();
        assert!(store.remove("a").unwrap_err().contains("not found"));
        // A new store over the same file sees what was saved.
        assert!(Store::<Act>::new(path.clone()).list().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_list_is_soonest_first_then_the_idle_ones_by_name() {
        let path = temp_file("order");
        let store = Store::<Act>::new(path.clone());
        let mut late = job("late");
        late.next_run_ms = Some(200);
        let mut soon = job("soon");
        soon.next_run_ms = Some(100);
        let mut idle_b = job("Bravo");
        idle_b.enabled = false;
        let mut idle_a = job("alpha");
        idle_a.enabled = false;
        for j in [idle_b, late, idle_a, soon] {
            store.upsert(j).unwrap();
        }
        let names: Vec<String> = store.list().into_iter().map(|j| j.name).collect();
        assert_eq!(names, ["soon", "late", "alpha", "Bravo"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn mutate_changes_the_stored_copy_and_a_refusal_changes_nothing() {
        let path = temp_file("mutate");
        let store = Store::<Act>::new(path.clone());
        store.upsert(job("a")).unwrap();
        store.mutate("a", |j| { j.runs = 7; Ok(()) }).unwrap();
        assert_eq!(store.get("a").unwrap().runs, 7);
        let refused: Result<(), String> = store.mutate("a", |_| Err("no".into()));
        assert!(refused.is_err());
        assert!(store.mutate("zzz", |_| Ok(())).unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn due_now_starts_each_due_job_once_and_saves() {
        let path = temp_file("due");
        let store = Store::<Act>::new(path.clone());
        let mut j = job("a");
        j.recurrence = Recurrence::Interval { secs: 3600, anchor_ms: 0 };
        j.schedule_from(0).unwrap();
        store.upsert(j).unwrap();
        assert!(store.due_now(1_000).is_empty());
        let ran = store.due_now(3_600_000 + 5_000);
        assert_eq!(ran.len(), 1);
        assert_eq!(ran[0].runs, 1);
        assert!(store.due_now(3_600_000 + 25_000).is_empty(), "not started twice");
        let reread = Store::<Act>::new(path.clone());
        assert_eq!(reread.get("a").unwrap().runs, 1, "the start was saved");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_reader_of_an_older_format_is_used_on_load() {
        let path = temp_file("legacy");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "legacy").unwrap();
        let reader: Reader<Act> = Box::new(|text| if text == "legacy" { vec![job("from-old-format")] } else { Vec::new() });
        let store = Store::with_reader(path.clone(), reader);
        assert_eq!(store.list()[0].id, "from-old-format");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
