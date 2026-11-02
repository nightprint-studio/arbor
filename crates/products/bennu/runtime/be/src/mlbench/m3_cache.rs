//! The ranker's raw material, collected once per project and kept: the instances (one popup per site, a row per candidate with the
//! measures `flc_rank` reads and which one was right) and the habit tables the project contributes. Fitting the regression is then
//! a read of the chosen projects' files and a few seconds of arithmetic; collecting — opening the project, asking the popup at every
//! sampled site — is the part that takes minutes, and it is independent per project.
//!
//! Where: `work/cache/collect/<unit>-<key>.json`, `<unit>` being `train-<project>` or `eval-<project>`. The key is the corpus' content
//! (training) or the held-out files and their dependencies' (evaluation), the sampling (`BENNU_MLB_N`, `BENNU_MLB_HARVEST`), the
//! crate's version and [`COLLECT_VERSION`]. **Bump `COLLECT_VERSION` whenever the features, the popup or the indexer change** — what it
//! cannot see is a change in the provider's code, which only `BENNU_MLB_REBUILD=1` (collect everything again) covers.
//!
//! Missing units are collected in parallel, one *process* each — `IndexService` is a global, so threads would share one index —
//! at most `BENNU_MLB_JOBS` at a time (default: cores − 2, never more than the units to collect; 1 collects in this process, in order).
//! A child runs `compute_one` of this file with a home of its own: the JDK memo copied (it is written while it works), the folder
//! of per-project indexes shared (each child writes only its project's).

use std::path::{Path, PathBuf};

use super::common::*;
use super::m3_rank::{collect, java_files, assemble_training, Collected, Instance, Kind, Tables};
use crate::index_service::IndexService;

/// Bumped when the measures, the popup or the indexer change.
const COLLECT_VERSION: u32 = 3;

type Counts = Vec<(String, u32)>;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct ProjectData {
    pub instances: Vec<Instance>,
    member: Vec<(String, Counts)>,
    chain: Vec<(String, Counts)>,
    arg: Vec<((String, usize), Counts)>,
    asked: Vec<(Kind, usize, usize)>,
}

fn sorted<K: Clone + Ord>(m: &std::collections::HashMap<K, std::collections::HashMap<String, u32>>) -> Vec<(K, Counts)> {
    let mut out: Vec<(K, Counts)> = m.iter().map(|(k, v)| { let mut c: Counts = v.iter().map(|(n, c)| (n.clone(), *c)).collect(); c.sort(); (k.clone(), c) }).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn unsorted<K: std::hash::Hash + Eq>(v: &[(K, Counts)], key: impl Fn(&K) -> K) -> std::collections::HashMap<K, std::collections::HashMap<String, u32>> {
    v.iter().map(|(k, c)| (key(k), c.iter().cloned().collect())).collect()
}

impl ProjectData {
    fn new(c: Collected, t: &Tables) -> Self {
        let mut asked: Vec<(Kind, usize, usize)> = c.asked.iter().map(|(&(k, typed), &n)| (k, typed, n)).collect();
        asked.sort();
        ProjectData { instances: c.instances, member: sorted(&t.member), chain: sorted(&t.chain), arg: sorted(&t.arg), asked }
    }

    /// The habit tables this project contributed.
    pub(super) fn tables(&self) -> Tables {
        Tables { member: unsorted(&self.member, |k| k.clone()), chain: unsorted(&self.chain, |k| k.clone()), arg: unsorted(&self.arg, |k| k.clone()) }
    }

    pub(super) fn asked(&self) -> impl Iterator<Item = ((Kind, usize), usize)> + '_ {
        self.asked.iter().map(|&(k, typed, n)| ((k, typed), n))
    }
}

/// Collects one unit in this process: the code that used to run inline in `m3_learned_ranking`.
fn compute(unit: &str, wanted: usize) -> ProjectData {
    // the popup is read as the provider leaves it, in a child process as in the parent: the production ranker must not reorder it
    crate::flc_rank::OFF.store(true, std::sync::atomic::Ordering::Relaxed);
    let svc = IndexService::global();
    if let Some(name) = unit.strip_prefix("train-") {
        let root = assemble_training(name);
        open_and_wait(&root);
        let mut paths = Vec::new();
        java_files(&root, &mut paths);
        paths.sort();
        let files: Vec<(String, String)> = paths.iter().map(|p| (p.to_string_lossy().into_owned(), std::fs::read_to_string(p).unwrap_or_default())).collect();
        let mut tables = Tables::default();
        let c = collect(svc, &files, wanted, Some(&mut tables));
        eprintln!("train {name}: {} files, {} instances", files.len(), c.instances.len());
        svc.forget(&root.to_string_lossy());
        ProjectData::new(c, &tables)
    } else {
        let name = unit.strip_prefix("eval-").unwrap_or(unit);
        let spec = PROJECTS.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no project {name}"));
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let files: Vec<(String, String)> = held.iter().map(|h| (h.path.to_string_lossy().into_owned(), h.text.clone())).collect();
        let c = collect(svc, &files, wanted, None);
        svc.forget(&root.to_string_lossy());
        ProjectData::new(c, &Tables::default())
    }
}

struct Job {
    unit: String,
    file: PathBuf,
}

fn cache_dir() -> PathBuf {
    let dir = work_dir().join("cache/collect");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn job(unit: String, key: String) -> Job {
    let file = cache_dir().join(format!("{unit}-{key}.json"));
    Job { unit, file }
}

fn sampling(h: &mut u64, wanted: usize) {
    fnv(h, &COLLECT_VERSION.to_le_bytes());
    fnv(h, &wanted.to_le_bytes());
    fnv(h, env("BENNU_MLB_HARVEST").unwrap_or_default().as_bytes());
}

fn train_job(name: &str, wanted: usize) -> Job {
    job(format!("train-{name}"), cache_key(|h| { tree_fingerprint(&corpus().join(name), h); sampling(h, wanted) }))
}

fn eval_job(spec: &ProjectSpec, wanted: usize) -> Job {
    let (_, held) = assemble(spec);
    job(format!("eval-{}", spec.name), cache_key(|h| {
        for f in &held { fnv(h, f.text.as_bytes()) }
        for dep in spec.deps { tree_fingerprint(&corpus().join(dep), h) }
        sampling(h, wanted)
    }))
}

fn save(job: &Job, data: &ProjectData) {
    let prefix = format!("{}-", job.unit);
    for e in std::fs::read_dir(cache_dir()).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(&prefix) && e.path() != job.file { let _ = std::fs::remove_file(e.path()); }
    }
    let tmp = job.file.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(data).expect("serialize")).expect("write the cache");
    std::fs::rename(&tmp, &job.file).expect("move the cache into place");
}

fn load(job: &Job) -> ProjectData {
    serde_json::from_slice(&std::fs::read(&job.file).expect("read the cache")).expect("parse the cache")
}

fn copy_dir(from: &Path, to: &Path, skip: &Path) {
    let _ = std::fs::create_dir_all(to);
    for e in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src == skip { continue }
        if src.is_dir() { copy_dir(&src, &dst, skip) } else { let _ = std::fs::copy(&src, &dst); }
    }
}

/// A home for a child: the parent's, with the JDK memo copied and the per-project indexes shared through a link.
fn child_home(unit: &str) -> PathBuf {
    let data = arbor_core::prelude::bennu_data_dir();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let rel = data.strip_prefix(&home).expect("the data folder is under HOME").to_path_buf();
    let arbor = data.parent().and_then(Path::parent).expect("arbor data folder").to_path_buf();
    let shared_index = data.join("index");
    let _ = std::fs::create_dir_all(&shared_index);
    let child = work_dir().join("cache/home-par").join(unit);
    let _ = std::fs::remove_dir_all(&child);
    let arbor_rel = arbor.strip_prefix(&home).expect("under HOME").to_path_buf();
    copy_dir(&arbor, &child.join(&arbor_rel), &shared_index);
    let _ = std::fs::create_dir_all(child.join(&rel));
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&shared_index, child.join(&rel).join("index"));
    child
}

fn run_child(job: &Job, wanted: usize) {
    let logs = cache_dir().join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let log = std::fs::File::create(logs.join(format!("{}.log", job.unit))).expect("log file");
    let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["mlbench::m3_cache::compute_one", "--ignored", "--exact", "--nocapture", "--test-threads=1"])
        .env("BENNU_MLB_UNIT", &job.unit)
        .env("BENNU_MLB_CACHE_FILE", &job.file)
        .env("BENNU_MLB_N", wanted.to_string())
        .env("BENNU_MLB_HOME_OVERRIDE", child_home(&job.unit))
        .stdout(log.try_clone().expect("log"))
        .stderr(log)
        .status()
        .expect("start the collection child");
    assert!(status.success(), "collecting {} failed: see {}", job.unit, logs.join(format!("{}.log", job.unit)).display());
}

/// Every job's data, collecting the missing ones first (in parallel when `BENNU_MLB_JOBS` allows).
fn ensure(jobs: &[Job], wanted: usize) -> Vec<ProjectData> {
    let missing: Vec<&Job> = jobs.iter().filter(|j| rebuild() || !j.file.exists()).collect();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let workers = env_num("BENNU_MLB_JOBS", cores.saturating_sub(2).max(1)).min(missing.len());
    let started = std::time::Instant::now();
    if workers <= 1 {
        for j in &missing { save(j, &compute(&j.unit, wanted)) }
    } else {
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(j) = missing.get(i) else { break };
                    run_child(j, wanted);
                });
            }
        });
    }
    if !missing.is_empty() {
        record_timing("collect", &format!("{} units, {} workers", missing.len(), workers.max(1)), started.elapsed(), if rebuild() { "rebuilt" } else { "missing" });
    }
    jobs.iter().map(load).collect()
}

pub(super) fn train_data(names: &[String], wanted: usize) -> Vec<ProjectData> {
    let jobs: Vec<Job> = names.iter().map(|n| train_job(n, wanted)).collect();
    ensure(&jobs, wanted)
}

pub(super) fn eval_data(specs: &[&ProjectSpec], wanted: usize) -> Vec<ProjectData> {
    let jobs: Vec<Job> = specs.iter().map(|s| eval_job(s, wanted)).collect();
    ensure(&jobs, wanted)
}

/// The child's entry point: collects the unit `BENNU_MLB_UNIT` into `BENNU_MLB_CACHE_FILE`. Not a test of anything.
#[test]
#[ignore = "run by m3_learned_ranking as a child process"]
fn compute_one() {
    let (Some(unit), Some(file)) = (env("BENNU_MLB_UNIT"), env("BENNU_MLB_CACHE_FILE")) else { return };
    let data = compute(&unit, env_num("BENNU_MLB_N", 500));
    let file = PathBuf::from(file);
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(&data).expect("serialize")).expect("write");
    std::fs::rename(&tmp, &file).expect("move");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn counts_survive_the_round_trip_and_serialize_the_same_every_time() {
        let mut t = Tables::default();
        t.member.entry("List".into()).or_default().insert("size".into(), 3);
        t.member.entry("List".into()).or_default().insert("add".into(), 1);
        t.arg.entry(("foo".into(), 1)).or_default().insert("x".into(), 2);
        let d = ProjectData::new(Collected { instances: Vec::new(), asked: HashMap::from([((Kind::Argument, 0), 5), ((Kind::Member, 1), 2)]) }, &t);
        let again = ProjectData::new(Collected { instances: Vec::new(), asked: HashMap::from([((Kind::Member, 1), 2), ((Kind::Argument, 0), 5)]) }, &t);
        assert_eq!(serde_json::to_vec(&d).unwrap(), serde_json::to_vec(&again).unwrap());
        let back: ProjectData = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
        assert_eq!(back.tables().member["List"]["size"], 3);
        assert_eq!(back.tables().arg[&("foo".to_string(), 1)]["x"], 2);
        assert_eq!(back.asked().count(), 2);
    }

    #[test]
    fn merging_tables_adds_counts() {
        let mut a = Tables::default();
        a.member.entry("List".into()).or_default().insert("size".into(), 3);
        let mut b = Tables::default();
        b.member.entry("List".into()).or_default().insert("size".into(), 4);
        b.chain.entry("get".into()).or_default().insert("x".into(), 1);
        a.merge(b);
        assert_eq!(a.member["List"]["size"], 7);
        assert_eq!(a.chain["get"]["x"], 1);
    }
}
