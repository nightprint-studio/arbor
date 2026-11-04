//! The schedules in memory, mirrored to `<corvus config dir>/imber-schedules.json` on every change.
//!
//! One list, one lock, loaded on first use. A file that cannot be read starts an empty list rather than
//! failing the panel — and is left alone until the next save, so a hand-edit mistake is not erased by merely
//! opening Corvus. Writes go to a temp file and are renamed over the real one, so a crash cannot leave half.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corvus_core::prelude::CorvusState;

use super::model::Job;

#[derive(Default)]
struct Store {
    loaded: bool,
    jobs: Vec<Job>,
}

fn store() -> MutexGuard<'static, Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn file(state: &CorvusState) -> Result<PathBuf, String> {
    Ok(PathBuf::from(crate::corvus_config::corvus_config_dir(state)?).join("imber-schedules.json"))
}

/// The jobs as the file holds them; none when there is no file or it is not readable.
pub fn parse(text: &str) -> Vec<Job> {
    serde_json::from_str(text).unwrap_or_default()
}

fn write_atomic(path: &Path, jobs: &[Job]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(jobs).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn ensure_loaded(state: &CorvusState, s: &mut Store) {
    if s.loaded {
        return;
    }
    s.loaded = true;
    if let Some(text) = file(state).ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        s.jobs = parse(&text);
    }
}

fn persist(state: &CorvusState, s: &Store) -> Result<(), String> {
    write_atomic(&file(state)?, &s.jobs)
}

pub fn list(state: &CorvusState) -> Vec<Job> {
    let mut s = store();
    ensure_loaded(state, &mut s);
    let mut jobs = s.jobs.clone();
    // Soonest first; jobs with nothing coming (disabled, finished) after, by name.
    jobs.sort_by(|a, b| match (a.next_run_ms, b.next_run_ms) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    jobs
}

pub fn get(state: &CorvusState, id: &str) -> Option<Job> {
    list(state).into_iter().find(|j| j.id == id)
}

/// Insert a new job or replace the one with the same id. The caller has validated and scheduled it.
pub fn upsert(state: &CorvusState, job: Job) -> Result<Job, String> {
    let mut s = store();
    ensure_loaded(state, &mut s);
    match s.jobs.iter_mut().find(|j| j.id == job.id) {
        Some(slot) => *slot = job.clone(),
        None => s.jobs.push(job.clone()),
    }
    persist(state, &s)?;
    Ok(job)
}

pub fn remove(state: &CorvusState, id: &str) -> Result<(), String> {
    let mut s = store();
    ensure_loaded(state, &mut s);
    let before = s.jobs.len();
    s.jobs.retain(|j| j.id != id);
    if s.jobs.len() == before {
        return Err(format!("schedule '{id}' not found"));
    }
    persist(state, &s)
}

/// Change one job in place and save; `f` sees the stored copy. Returns what it left.
pub fn mutate<R>(state: &CorvusState, id: &str, f: impl FnOnce(&mut Job) -> Result<R, String>) -> Result<R, String> {
    let mut s = store();
    ensure_loaded(state, &mut s);
    let job = s.jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| format!("schedule '{id}' not found"))?;
    let out = f(job)?;
    persist(state, &s)?;
    Ok(out)
}

/// Every job's tick at `now_ms`: the jobs to run now (already marked started), saved.
pub fn due_now(state: &CorvusState, now_ms: i64) -> Vec<Job> {
    use super::model::Tick;
    let mut s = store();
    ensure_loaded(state, &mut s);
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
        let _ = persist(state, &s);
    }
    to_run
}

pub fn new_id() -> String {
    format!("sch-{:x}", now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::{MissedPolicy, Recurrence};

    use crate::assistant::schedule::model::Action;

    fn job(id: &str) -> Job {
        Job {
            id: id.into(),
            name: id.into(),
            enabled: true,
            recurrence: Recurrence::Daily { hour: 9, minute: 0 },
            missed: MissedPolicy::Skip,
            action: Action::Fetch { remote: None },
            root: "/r".into(),
            mode: "manual".into(),
            created_ms: 0,
            next_run_ms: None,
            last_run: None,
            runs: 0,
        }
    }

    #[test]
    fn a_saved_file_reads_back_and_a_broken_one_reads_as_empty() {
        let dir = std::env::temp_dir().join(format!("corvus-imber-sched-{}", std::process::id()));
        let path = dir.join("imber-schedules.json");
        write_atomic(&path, &[job("a"), job("b")]).unwrap();
        let back = parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(back.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(!path.with_extension("json.tmp").exists(), "the temp file was renamed away");
        assert!(parse("{ not json").is_empty());
        assert!(parse("").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
