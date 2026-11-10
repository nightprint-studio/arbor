//! Schedules for the runner: "every night at 02:00 run the Smoke collection on staging".
//!
//! The job — its recurrence, the next run, the missed-run policy, the history of the last run, the file it is
//! kept in — is `arbor-scheduler`'s `jobs`, the same data the Imber schedules use, so there is one idea of
//! "every weekday at nine". What is Hirundo's is only what to run ([`Run`]: a collection or folder, in an
//! environment) and the shape the window already speaks ([`Schedule`], converted at the edge).
//!
//! A schedule is *data*, evaluated by a tick: the window asks `schedules_due` every few seconds, runs what it is
//! given with the runner it already has, and reports back with `schedule_ran`. Nothing fires while the Hirundo
//! window is closed (the backend ends with it); a run that was missed meanwhile is skipped or caught up once,
//! per schedule.
//!
//! A project's list used to be a bare array of [`Schedule`]; it is read either way and written in the job format.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use arbor_scheduler::prelude::{next_after, now_ms, Job, JobAction, JobInput, LastRun, MissedPolicy, Recurrence, Store};
use hirundo_core::prelude::HirundoState;
use serde::{Deserialize, Serialize};

use crate::project_state::path_of;

const FILE: &str = "schedules";

/// What a schedule runs: a collection or folder, in an environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The collection or folder to run (`bandi`, `bandi/auth`).
    pub target: String,
    /// The environment to run in; the active one when empty.
    #[serde(default)]
    pub env_id: String,
}

impl JobAction for Run {
    fn word(&self) -> &'static str {
        "run"
    }
}

/// A schedule as the window sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Empty on a new one: the backend gives it an id.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub env_id: String,
    pub recurrence: Recurrence,
    #[serde(default)]
    pub missed: MissedPolicy,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// When it was created: a schedule never runs "for" a time before it existed.
    #[serde(default)]
    pub created_ms: i64,
    #[serde(default)]
    pub last_run_ms: Option<i64>,
    /// `ok`, `failed`, or `skipped` (a missed run that was dropped).
    #[serde(default)]
    pub last_result: String,
    #[serde(default)]
    pub last_summary: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ScheduleView {
    #[serde(flatten)]
    pub schedule: Schedule,
    pub next_run_ms: Option<i64>,
}

// ── The two shapes ───────────────────────────────────────────────────────────────────────────────

fn to_wire(job: &Job<Run>) -> Schedule {
    let last = job.last_run.as_ref();
    Schedule {
        id: job.id.clone(),
        name: job.name.clone(),
        target: job.action.target.clone(),
        env_id: job.action.env_id.clone(),
        recurrence: job.recurrence.clone(),
        missed: job.missed,
        enabled: job.enabled,
        created_ms: job.created_ms,
        last_run_ms: last.map(|l| l.at_ms),
        last_result: last.map(|l| (if l.skipped { "skipped" } else if l.ok { "ok" } else { "failed" }).to_string()).unwrap_or_default(),
        last_summary: last.map(|l| l.detail.clone()).unwrap_or_default(),
    }
}

fn view_of(job: &Job<Run>) -> ScheduleView {
    ScheduleView { schedule: to_wire(job), next_run_ms: if job.enabled { job.next_run_ms } else { None } }
}

/// A schedule saved before the job format: its next run counted from its last run (or its creation).
fn from_legacy(s: Schedule, project: &str) -> Job<Run> {
    let next_run_ms = if s.enabled { next_after(&s.recurrence, s.last_run_ms.unwrap_or(s.created_ms)).ok().flatten() } else { None };
    Job {
        id: s.id,
        name: s.name,
        enabled: s.enabled,
        recurrence: s.recurrence,
        missed: s.missed,
        action: Run { target: s.target, env_id: s.env_id },
        root: project.to_string(),
        mode: "manual".into(),
        created_ms: s.created_ms,
        next_run_ms,
        last_run: s.last_run_ms.map(|at_ms| LastRun { at_ms, ok: s.last_result == "ok", detail: s.last_summary, skipped: s.last_result == "skipped" }),
        runs: 0,
    }
}

/// A project's file: the job format, or the bare array of schedules it held before.
fn read_either(text: &str, project: &str) -> Vec<Job<Run>> {
    serde_json::from_str::<Vec<Job<Run>>>(text)
        .or_else(|_| serde_json::from_str::<Vec<Schedule>>(text).map(|old| old.into_iter().map(|s| from_legacy(s, project)).collect()))
        .unwrap_or_default()
}

/// The project's list, loaded on first use and then kept: the same store serves every call.
fn store_of(project: &str) -> Arc<Store<Run>> {
    static STORES: OnceLock<Mutex<HashMap<String, Arc<Store<Run>>>>> = OnceLock::new();
    let mut all = STORES.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    Arc::clone(all.entry(project.to_string()).or_insert_with(|| {
        let owner = project.to_string();
        Arc::new(Store::with_reader(path_of(project, FILE), Box::new(move |text| read_either(text, &owner))))
    }))
}

// ── Handlers ─────────────────────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn schedules_list(_state: &HirundoState, project: String) -> Result<Vec<ScheduleView>, String> {
    Ok(store_of(&project).list().iter().map(view_of).collect())
}

/// The job a window's save asks for. A schedule edited keeps its history; one whose time changed restarts its clock.
fn input_of(schedule: Schedule, project: &str) -> Result<JobInput<Run>, String> {
    if schedule.target.trim().is_empty() {
        return Err("choose the collection to run".into());
    }
    Ok(JobInput {
        id: Some(schedule.id).filter(|i| !i.is_empty()),
        name: schedule.name,
        enabled: schedule.enabled,
        recurrence: schedule.recurrence,
        missed: schedule.missed,
        action: Run { target: schedule.target.trim().to_string(), env_id: schedule.env_id },
        root: project.to_string(),
        mode: String::new(),
    })
}

#[arbor_rpc::handler]
fn schedule_save(_state: &HirundoState, project: String, schedule: Schedule) -> Result<(), String> {
    let store = store_of(&project);
    let input = input_of(schedule, &project)?;
    let existing = input.id.as_deref().and_then(|id| store.get(id));
    let now = now_ms();
    let job = Job::from_input(existing, input, now, || format!("s{now}"), |_| "manual".into())?;
    store.upsert(job).map(|_| ())
}

#[arbor_rpc::handler]
fn schedule_delete(_state: &HirundoState, project: String, id: String) -> Result<(), String> {
    // Deleting what is not there is not worth an error: the window wanted it gone.
    let _ = store_of(&project).remove(&id);
    Ok(())
}

/// The schedules to run now. They are marked started as they are returned, so asking again does not give them
/// twice; `schedule_ran` records how each ended. A missed run the policy drops is moved past here too.
#[arbor_rpc::handler]
fn schedules_due(_state: &HirundoState, project: String) -> Result<Vec<Schedule>, String> {
    Ok(store_of(&project).due_now(now_ms()).iter().map(to_wire).collect())
}

#[arbor_rpc::handler]
fn schedule_ran(_state: &HirundoState, project: String, id: String, passed: bool, summary: String) -> Result<(), String> {
    let _ = store_of(&project).mutate(&id, |j| {
        j.finished(now_ms(), passed, &summary);
        Ok(())
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::{Tick, GRACE_MS};

    const MIN: i64 = 60_000;

    fn every_ten_minutes(created: i64, missed: MissedPolicy) -> Job<Run> {
        let mut job = Job {
            id: "s1".into(),
            name: "Smoke".into(),
            enabled: true,
            recurrence: Recurrence::Interval { secs: 600, anchor_ms: created },
            missed,
            action: Run { target: "bandi".into(), env_id: String::new() },
            root: "/api".into(),
            mode: "manual".into(),
            created_ms: created,
            next_run_ms: None,
            last_run: None,
            runs: 0,
        };
        job.schedule_from(created).unwrap();
        job
    }

    fn legacy(last_run_ms: Option<i64>, result: &str) -> Schedule {
        Schedule {
            id: "s9".into(), name: "Nightly".into(), target: "bandi/auth".into(), env_id: "stg".into(),
            recurrence: Recurrence::Interval { secs: 600, anchor_ms: 0 }, missed: MissedPolicy::RunOnce, enabled: true, created_ms: 0,
            last_run_ms, last_result: result.into(), last_summary: "3 of 3 passed".into(),
        }
    }

    #[test]
    fn a_schedule_is_due_at_its_time_and_not_before() {
        let mut job = every_ten_minutes(0, MissedPolicy::Skip);
        assert_eq!(job.tick(5 * MIN), Tick::Nothing, "not yet");
        assert_eq!(job.tick(10 * MIN), Tick::Run);
    }

    #[test]
    fn a_missed_run_is_dropped_or_caught_up_once_by_policy() {
        let mut skip = every_ten_minutes(0, MissedPolicy::Skip);
        assert_eq!(skip.tick(10 * MIN + GRACE_MS + 1), Tick::Nothing);
        assert!(skip.next_run_ms.unwrap() > 10 * MIN + GRACE_MS, "the clock moved on");
        assert_eq!(every_ten_minutes(0, MissedPolicy::RunOnce).tick(60 * MIN), Tick::Run);
    }

    #[test]
    fn what_the_window_sends_becomes_a_job_and_back() {
        let input = input_of(legacy(None, ""), "/api").unwrap();
        let job = Job::from_input(None, input, 5 * MIN, || "unused".into(), |_| "manual".into()).unwrap();
        assert_eq!((job.id.as_str(), job.root.as_str()), ("s9", "/api"));
        let wire = to_wire(&job);
        assert_eq!((wire.target.as_str(), wire.env_id.as_str(), wire.last_result.as_str()), ("bandi/auth", "stg", ""));
        assert!(input_of(Schedule { target: "  ".into(), ..legacy(None, "") }, "/api").unwrap_err().contains("collection"));
        assert_eq!(view_of(&job).next_run_ms, job.next_run_ms);
        let mut off = job.clone();
        off.enabled = false;
        assert_eq!(view_of(&off).next_run_ms, None);
    }

    #[test]
    fn a_run_is_reported_ok_failed_or_skipped() {
        let mut job = every_ten_minutes(0, MissedPolicy::Skip);
        job.finished(1, true, "all green");
        assert_eq!(to_wire(&job).last_result, "ok");
        job.finished(2, false, "2 failed");
        assert_eq!((to_wire(&job).last_result.as_str(), to_wire(&job).last_summary.as_str()), ("failed", "2 failed"));
        let mut once = every_ten_minutes(0, MissedPolicy::Skip);
        once.recurrence = Recurrence::Once { at_ms: 10 * MIN };
        once.schedule_from(0).unwrap();
        assert_eq!(once.tick(60 * MIN), Tick::Nothing);
        assert_eq!(to_wire(&once).last_result, "skipped");
    }

    #[test]
    fn a_file_in_the_old_format_is_read_without_losing_anything() {
        let old = serde_json::to_string(&vec![legacy(Some(10 * MIN), "failed")]).unwrap();
        let jobs = read_either(&old, "/api");
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!((job.action.target.as_str(), job.action.env_id.as_str(), job.root.as_str()), ("bandi/auth", "stg", "/api"));
        assert_eq!(job.next_run_ms, Some(20 * MIN), "counted from the last run, as before");
        let wire = to_wire(job);
        assert_eq!((wire.last_run_ms, wire.last_result.as_str(), wire.last_summary.as_str()), (Some(10 * MIN), "failed", "3 of 3 passed"));
        assert_eq!(job.missed, MissedPolicy::RunOnce);
    }

    #[test]
    fn a_file_in_the_job_format_and_a_broken_one_are_read_too() {
        let now = every_ten_minutes(0, MissedPolicy::Skip);
        let text = serde_json::to_string(&vec![now.clone()]).unwrap();
        assert_eq!(read_either(&text, "/api"), vec![now]);
        assert!(read_either("{ not json", "/api").is_empty());
        assert!(read_either("", "/api").is_empty());
    }
}
