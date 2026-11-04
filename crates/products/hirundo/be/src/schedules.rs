//! Schedules for the runner: "every night at 02:00 run the Smoke collection on staging".
//!
//! The recurrence, its validation, the next run and the missed-run policy are `arbor-scheduler`'s
//! `calendar` — the same data the Imber schedules use — so there is one idea of "every weekday at nine".
//! What is Hirundo's is only what to run: a collection or folder, in an environment.
//!
//! A schedule is *data*, evaluated by a tick: the window asks `schedules_due` every few seconds, runs
//! what it is given with the runner it already has, and reports back with `schedule_ran`. Nothing fires
//! while the Hirundo window is closed (the backend ends with it); a run that was missed meanwhile is
//! skipped or caught up once, per schedule.

use arbor_scheduler::prelude::{due, next_after, validate_recurrence, Due, MissedPolicy, Recurrence};
use hirundo_core::prelude::HirundoState;
use serde::{Deserialize, Serialize};

use crate::project_state::{load, save};

const FILE: &str = "schedules";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Empty on a new one: the backend gives it an id.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// The collection or folder to run (`bandi`, `bandi/auth`).
    pub target: String,
    /// The environment to run in; the active one when empty.
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

fn next_run(s: &Schedule) -> Option<i64> {
    next_after(&s.recurrence, s.last_run_ms.unwrap_or(s.created_ms)).ok().flatten()
}

/// What is due at `now_ms`: the schedules to run, and those whose missed run was dropped (marked
/// `skipped`, so their next run moves on).
fn collect_due(schedules: &mut [Schedule], now_ms: i64) -> Vec<Schedule> {
    let mut run = Vec::new();
    for s in schedules.iter_mut().filter(|s| s.enabled) {
        let Some(next) = next_run(s) else { continue };
        match due(next, now_ms, s.missed) {
            Due::Wait => {}
            Due::Run => run.push(s.clone()),
            Due::Skip => {
                s.last_run_ms = Some(now_ms);
                s.last_result = "skipped".into();
                s.last_summary = "Missed while Hirundo was closed.".into();
            }
        }
    }
    run
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[arbor_rpc::handler]
fn schedules_list(_state: &HirundoState, project: String) -> Result<Vec<ScheduleView>, String> {
    let schedules: Vec<Schedule> = load(&project, FILE);
    Ok(schedules.into_iter().map(|s| ScheduleView { next_run_ms: if s.enabled { next_run(&s) } else { None }, schedule: s }).collect())
}

#[arbor_rpc::handler]
fn schedule_save(_state: &HirundoState, project: String, schedule: Schedule) -> Result<(), String> {
    validate_recurrence(&schedule.recurrence)?;
    if schedule.name.trim().is_empty() {
        return Err("the schedule needs a name".into());
    }
    if schedule.target.trim().is_empty() {
        return Err("choose the collection to run".into());
    }
    let mut all: Vec<Schedule> = load(&project, FILE);
    let mut s = schedule;
    if s.id.is_empty() {
        s.id = format!("s{}", now_ms());
        s.created_ms = now_ms();
    }
    match all.iter_mut().find(|x| x.id == s.id) {
        Some(existing) => {
            // Editing the time of a schedule restarts its clock; the history of its last run stays.
            s.created_ms = if existing.recurrence == s.recurrence { existing.created_ms } else { now_ms() };
            s.last_run_ms = if existing.recurrence == s.recurrence { existing.last_run_ms } else { None };
            s.last_result = existing.last_result.clone();
            s.last_summary = existing.last_summary.clone();
            *existing = s;
        }
        None => all.push(s),
    }
    save(&project, FILE, &all)
}

#[arbor_rpc::handler]
fn schedule_delete(_state: &HirundoState, project: String, id: String) -> Result<(), String> {
    let mut all: Vec<Schedule> = load(&project, FILE);
    all.retain(|s| s.id != id);
    save(&project, FILE, &all)
}

/// The schedules to run now. Calling it again before `schedule_ran` returns the same ones, so the caller
/// must mark them as it starts them.
#[arbor_rpc::handler]
fn schedules_due(_state: &HirundoState, project: String) -> Result<Vec<Schedule>, String> {
    let mut all: Vec<Schedule> = load(&project, FILE);
    let run = collect_due(&mut all, now_ms());
    save(&project, FILE, &all)?;
    Ok(run)
}

#[arbor_rpc::handler]
fn schedule_ran(_state: &HirundoState, project: String, id: String, passed: bool, summary: String) -> Result<(), String> {
    let mut all: Vec<Schedule> = load(&project, FILE);
    if let Some(s) = all.iter_mut().find(|s| s.id == id) {
        s.last_run_ms = Some(now_ms());
        s.last_result = if passed { "ok" } else { "failed" }.into();
        s.last_summary = summary;
    }
    save(&project, FILE, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn every_ten_minutes(created: i64, missed: MissedPolicy) -> Schedule {
        Schedule {
            id: "s1".into(), name: "Smoke".into(), target: "bandi".into(), env_id: String::new(),
            recurrence: Recurrence::Interval { secs: 600, anchor_ms: created }, missed, enabled: true, created_ms: created,
            last_run_ms: None, last_result: String::new(), last_summary: String::new(),
        }
    }

    #[test]
    fn a_schedule_is_due_at_its_time_and_not_before() {
        let mut all = vec![every_ten_minutes(0, MissedPolicy::Skip)];
        assert!(collect_due(&mut all, 5 * MIN).is_empty(), "not yet");
        assert_eq!(collect_due(&mut all, 10 * MIN).len(), 1);
    }

    #[test]
    fn a_disabled_schedule_never_runs() {
        let mut s = every_ten_minutes(0, MissedPolicy::Skip);
        s.enabled = false;
        assert!(collect_due(&mut [s], 20 * MIN).is_empty());
    }

    #[test]
    fn a_missed_run_is_dropped_or_caught_up_once_by_policy() {
        let mut skip = vec![every_ten_minutes(0, MissedPolicy::Skip)];
        assert!(collect_due(&mut skip, 60 * MIN).is_empty());
        assert_eq!(skip[0].last_result, "skipped");
        assert_eq!(skip[0].last_run_ms, Some(60 * MIN), "the clock moved on");

        let mut once = vec![every_ten_minutes(0, MissedPolicy::RunOnce)];
        assert_eq!(collect_due(&mut once, 60 * MIN).len(), 1);
    }

    #[test]
    fn after_a_run_the_next_one_counts_from_it() {
        let mut s = every_ten_minutes(0, MissedPolicy::Skip);
        s.last_run_ms = Some(10 * MIN);
        assert_eq!(next_run(&s), Some(20 * MIN));
    }
}
