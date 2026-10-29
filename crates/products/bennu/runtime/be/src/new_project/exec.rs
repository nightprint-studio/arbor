//! Executing a [`Plan`]: the staged run behind Create.
//!
//! The destination is never written until the project is complete. Everything happens in a scratch
//! folder **beside** the destination (`.arbor-new-<id>`, same volume, so the last step is one
//! rename), and the scratch folder is removed by a drop guard on every exit path — success, error,
//! cancel, panic. A failed or cancelled run therefore leaves nothing behind, including the parent
//! folders the run had to create.
//!
//! Progress goes out as `arbor://bennu/new-project` events keyed by the caller's run id:
//! `stage` (a heading), `log` (a line of tool output), and exactly one of `done` / `error` /
//! `cancelled`. The wizard subscribes **before** it starts the run, so no event can be missed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use arbor_ipc::prelude::EventSink;
use serde_json::{json, Value};

use super::extract::extract_skeleton;
use super::http::{download_to, MAX_DOWNLOAD_BYTES};
use super::model::{GenFile, Plan, Source, Step};
use super::process::{self, Ctl, Ended};

pub const EVENT: &str = "arbor://bennu/new-project";

fn registry() -> &'static Mutex<HashMap<String, Arc<Ctl>>> {
    static R: OnceLock<Mutex<HashMap<String, Arc<Ctl>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Run ids come from the frontend and become part of a folder name, so they are restricted to
/// what cannot be a path.
pub fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What the run produced.
#[derive(Debug)]
pub struct Done {
    pub path: PathBuf,
    pub open: Option<PathBuf>,
    pub warnings: Vec<String>,
}

enum Failure {
    Cancelled,
    Failed(String),
}

impl From<&str> for Failure {
    fn from(m: &str) -> Self {
        Failure::Failed(m.to_string())
    }
}

impl From<String> for Failure {
    fn from(m: String) -> Self {
        Failure::Failed(m)
    }
}

struct Job {
    id: String,
    sink: Arc<dyn EventSink>,
    ctl: Arc<Ctl>,
}

impl Job {
    fn emit(&self, kind: &str, extra: Value) {
        let mut payload = json!({ "id": self.id, "kind": kind });
        if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        self.sink.emit(EVENT, payload);
    }
    fn stage(&self, text: &str) {
        self.emit("stage", json!({ "text": text }));
    }
    fn logger(&self) -> Arc<dyn Fn(&str) + Send + Sync> {
        let (sink, id) = (self.sink.clone(), self.id.clone());
        Arc::new(move |line: &str| sink.emit(EVENT, json!({ "id": id, "kind": "log", "text": line })))
    }
    fn check(&self) -> Result<(), Failure> {
        if self.ctl.is_cancelled() { Err(Failure::Cancelled) } else { Ok(()) }
    }
}

/// Start `plan` on its own thread and return at once. `git` asks for a `git init` after the move.
pub fn start(
    sink: Arc<dyn EventSink>,
    id: String,
    plan: Plan,
    target: PathBuf,
    git: bool,
) -> Result<(), String> {
    if !valid_run_id(&id) {
        return Err("invalid run id".into());
    }
    let ctl = Arc::new(Ctl::default());
    {
        let mut runs = registry().lock().map_err(|_| "run registry poisoned".to_string())?;
        if runs.contains_key(&id) {
            return Err("a run with this id is already going".into());
        }
        runs.insert(id.clone(), ctl.clone());
    }
    let job = Job { id: id.clone(), sink, ctl };
    std::thread::Builder::new()
        .name(format!("bennu-new-project-{id}"))
        .spawn(move || {
            match execute(&job, plan, &target, git) {
                Ok(done) => job.emit(
                    "done",
                    json!({
                        "path": fwd(&done.path),
                        "open": done.open.as_deref().map(fwd),
                        "warnings": done.warnings,
                    }),
                ),
                Err(Failure::Cancelled) => job.emit("cancelled", json!({})),
                Err(Failure::Failed(message)) => job.emit("error", json!({ "message": message })),
            }
            if let Ok(mut runs) = registry().lock() {
                runs.remove(&job.id);
            }
        })
        .map(|_| ())
        .map_err(|e| {
            if let Ok(mut runs) = registry().lock() {
                runs.remove(&id);
            }
            format!("could not start the run: {e}")
        })
}

/// Cancel a run. `false` when it is not (or no longer) running.
pub fn cancel(id: &str) -> bool {
    let ctl = registry().lock().ok().and_then(|r| r.get(id).cloned());
    ctl.map(|c| c.cancel()).is_some()
}

fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ── the scratch folder ───────────────────────────────────────────────────────────────────

/// Removes the scratch folder when dropped, and — unless the run committed — the parent folders it
/// had to create (deepest first, and only while empty).
struct Scratch {
    dir: PathBuf,
    created: Vec<PathBuf>,
    committed: bool,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        if !self.committed {
            for d in self.created.iter().rev() {
                let _ = std::fs::remove_dir(d);
            }
        }
    }
}

/// The folders in the chain down to `dir` that do not exist yet, outermost first.
fn missing_ancestors(dir: &Path) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = dir.ancestors().take_while(|a| !a.exists()).map(Path::to_path_buf).collect();
    missing.reverse();
    missing
}

// ── the run ──────────────────────────────────────────────────────────────────────────────

fn execute(job: &Job, plan: Plan, target: &Path, git: bool) -> Result<Done, Failure> {
    let name = target.file_name().and_then(|n| n.to_str()).ok_or("the project name is not valid text")?.to_string();
    let parent = target.parent().ok_or("the location has no parent folder")?;
    job.check()?;

    let created = missing_ancestors(parent);
    std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", fwd(parent)))?;
    let mut scratch = Scratch { dir: parent.join(format!(".arbor-new-{}", job.id)), created, committed: false };
    std::fs::create_dir_all(&scratch.dir).map_err(|e| format!("could not create a working folder: {e}"))?;

    let project = build_project(job, &plan.source, &scratch.dir, &name)?;
    job.check()?;
    job.stage("Moving the project into place");
    commit(&project, target)?;
    scratch.committed = true;
    drop(scratch);

    let warnings = post_steps(job, &plan, target, git);
    let open = plan.open.iter().map(|rel| target.join(rel)).find(|p| p.is_file());
    Ok(Done { path: target.to_path_buf(), open, warnings })
}

/// Produce the project inside `stage`; returns the folder that holds it.
fn build_project(job: &Job, source: &Source, stage: &Path, name: &str) -> Result<PathBuf, Failure> {
    match source {
        Source::Files(files) => {
            job.stage("Writing files");
            let root = stage.join(name);
            write_files(&root, files)?;
            Ok(root)
        }
        Source::Download { url } => {
            let host = url.split('/').nth(2).unwrap_or("the service");
            job.stage(&format!("Downloading from {host}"));
            let zip = stage.join("download.zip");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("could not start the network runtime: {e}"))?;
            let cancel: &AtomicBool = job.ctl.cancelled_flag();
            let bytes = rt.block_on(download_to(url, &zip, MAX_DOWNLOAD_BYTES, cancel));
            match bytes {
                Ok(_) => {}
                Err(m) if m == "cancelled" => return Err(Failure::Cancelled),
                Err(m) => return Err(Failure::Failed(m)),
            }
            job.check()?;
            job.stage("Unpacking");
            let root = stage.join(name);
            let report = extract_skeleton(&zip, &root)?;
            (job.logger())(&format!("{} files, {} KB", report.files, report.bytes / 1024));
            let _ = std::fs::remove_file(&zip);
            Ok(root)
        }
        Source::Command { argv, produces } => {
            job.stage(&format!("Running {}", argv.join(" ")));
            (job.logger())(&format!("$ {}", argv.join(" ")));
            match process::run(argv, stage, &job.ctl, job.logger()) {
                Ended::Success => {}
                Ended::Cancelled => return Err(Failure::Cancelled),
                Ended::NotStarted(m) => return Err(Failure::Failed(m)),
                Ended::Failed { code, tail } => {
                    let code = code.map_or(String::from("a failure"), |c| format!("exit code {c}"));
                    return Err(Failure::Failed(format!("The command ended with {code}.\n{tail}").trim().to_string()));
                }
            }
            let root = stage.join(produces);
            if !root.is_dir() {
                return Err(Failure::Failed(format!("The command finished but did not create “{produces}”")));
            }
            Ok(root)
        }
    }
}

/// Write `files` under `root`, refusing any path that escapes it.
pub fn write_files(root: &Path, files: &[GenFile]) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| format!("could not create the project folder: {e}"))?;
    for f in files {
        let comps = arbor_fs::unzip_limited::entry_components(&f.path)
            .map_err(|m| format!("generator produced an unsafe path: {m}"))?;
        if comps.is_empty() {
            return Err("generator produced an empty path".into());
        }
        let out = comps.iter().fold(root.to_path_buf(), |p, c| p.join(c));
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", fwd(dir)))?;
        }
        std::fs::write(&out, f.content.as_bytes()).map_err(|e| format!("could not write {}: {e}", f.path))?;
        #[cfg(unix)]
        if f.executable {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o755));
        }
    }
    Ok(())
}

/// Move the finished project to `target`. An empty folder already there is replaced; anything
/// else is an error — the "never overwrite" rule, re-checked at the last moment.
fn commit(project: &Path, target: &Path) -> Result<(), String> {
    match std::fs::metadata(target) {
        Ok(md) if md.is_dir() && std::fs::read_dir(target).map_or(false, |mut d| d.next().is_none()) => {
            std::fs::remove_dir(target).map_err(|e| format!("could not replace the empty folder: {e}"))?;
        }
        Ok(_) => {
            return Err("The destination appeared while the project was being created — nothing was overwritten".into())
        }
        Err(_) => {}
    }
    std::fs::rename(project, target).map_err(|e| format!("could not move the project into place: {e}"))
}

/// `git init` (when asked and not already done by a scaffolder) and the plan's own steps. Failures
/// are warnings: the project exists, and undoing it for a failed `npm install` would be worse.
fn post_steps(job: &Job, plan: &Plan, target: &Path, git: bool) -> Vec<String> {
    let mut steps: Vec<Step> = Vec::new();
    if git && !target.join(".git").exists() {
        steps.push(Step { label: "Initialising the Git repository".into(), argv: vec!["git".into(), "init".into()] });
    }
    steps.extend(plan.post.iter().cloned());
    let mut warnings = Vec::new();
    for step in steps {
        if job.ctl.is_cancelled() {
            warnings.push("Cancelled: the remaining steps were skipped".into());
            break;
        }
        job.stage(&step.label);
        (job.logger())(&format!("$ {}", step.argv.join(" ")));
        match process::run(&step.argv, target, &job.ctl, job.logger()) {
            Ended::Success => {}
            Ended::Cancelled => warnings.push(format!("{} was cancelled", step.label)),
            Ended::NotStarted(m) => warnings.push(format!("{}: {m}", step.label)),
            Ended::Failed { code, tail } => warnings.push(format!(
                "{} failed{}{}",
                step.label,
                code.map_or(String::new(), |c| format!(" (exit code {c})")),
                if tail.is_empty() { String::new() } else { format!(": {}", tail.lines().last().unwrap_or("")) }
            )),
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::Plan;

    #[derive(Default)]
    struct Collect(Mutex<Vec<Value>>);
    impl EventSink for Collect {
        fn emit(&self, _topic: &str, payload: Value) {
            self.0.lock().unwrap().push(payload);
        }
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-np-exec-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn job(sink: Arc<Collect>, id: &str) -> Job {
        Job { id: id.into(), sink, ctl: Arc::new(Ctl::default()) }
    }

    fn files_plan(files: Vec<GenFile>) -> Plan {
        Plan::files(files, &["README.md"], vec![])
    }

    #[test]
    fn run_ids_cannot_be_paths() {
        assert!(valid_run_id("run-1_a"));
        for bad in ["", "a/b", "..", "a b", &"x".repeat(41)] {
            assert!(!valid_run_id(bad), "{bad}");
        }
    }

    #[test]
    fn writes_a_project_atomically_and_reports_the_file_to_open() {
        let d = scratch_dir("ok");
        let target = d.join("loc").join("demo");
        let sink = Arc::new(Collect::default());
        let plan = files_plan(vec![GenFile::new("README.md", "hi"), GenFile::new("src/a/b.txt", "x")]);
        let done = execute(&job(sink, "t1"), plan, &target, false).ok().expect("run succeeds");
        assert_eq!(std::fs::read_to_string(target.join("src/a/b.txt")).unwrap(), "x");
        assert_eq!(done.open, Some(target.join("README.md")));
        assert!(!d.join("loc").join(".arbor-new-t1").exists(), "scratch folder is gone");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_unsafe_generated_path_fails_and_leaves_nothing_behind() {
        let d = scratch_dir("slip");
        let target = d.join("fresh").join("deep").join("demo");
        let plan = files_plan(vec![GenFile::new("ok.txt", "x"), GenFile::new("../escape.txt", "y")]);
        let r = execute(&job(Arc::new(Collect::default()), "t2"), plan, &target, false);
        assert!(matches!(r, Err(Failure::Failed(_))));
        assert!(!d.join("fresh").exists(), "folders the run created are removed again");
        assert!(!d.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_non_empty_destination_is_never_overwritten() {
        let d = scratch_dir("full");
        let target = d.join("demo");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("mine.txt"), "keep me").unwrap();
        let r = execute(&job(Arc::new(Collect::default()), "t3"), files_plan(vec![GenFile::new("a", "b")]), &target, false);
        assert!(matches!(r, Err(Failure::Failed(_))));
        assert_eq!(std::fs::read_to_string(target.join("mine.txt")).unwrap(), "keep me");
        assert!(!target.join("a").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_empty_destination_folder_is_used() {
        let d = scratch_dir("empty");
        let target = d.join("demo");
        std::fs::create_dir_all(&target).unwrap();
        let r = execute(&job(Arc::new(Collect::default()), "t4"), files_plan(vec![GenFile::new("a", "b")]), &target, false);
        assert!(r.is_ok() && target.join("a").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_cancelled_run_touches_nothing() {
        let d = scratch_dir("cancel");
        let target = d.join("new").join("demo");
        let j = job(Arc::new(Collect::default()), "t5");
        j.ctl.cancel();
        let r = execute(&j, files_plan(vec![GenFile::new("a", "b")]), &target, false);
        assert!(matches!(r, Err(Failure::Cancelled)));
        assert!(!d.join("new").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_ancestors_are_listed_outermost_first() {
        let d = scratch_dir("anc");
        let deep = d.join("a").join("b");
        assert_eq!(missing_ancestors(&deep), vec![d.join("a"), d.join("a").join("b")]);
        assert!(missing_ancestors(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
