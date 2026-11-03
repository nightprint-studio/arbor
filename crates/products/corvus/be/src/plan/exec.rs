//! The executor: walks a plan's phases, one detached task per run.
//!
//! The run ends — it does not sleep — whenever it needs the user: a failed step or a manual gate
//! pauses the plan, and *resume* starts a fresh run from the first phase that is not complete.
//! Only waits that resolve on their own (a delay, an MR being merged) keep the task alive.
//!
//! Within a phase, steps on different repos run side by side; steps on the same repo run in order.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::MrState;

use super::job::PlanJob;
use super::model::{group_by_repo, GateSpec, GateState, PlanStatus, RepoRef, Step, StepSpec, StepStatus};
use super::steps::{execute, StepCtx};
use super::store::{self, now_ms};

/// The process-wide state, handed in at startup: a detached run outlives the handler that
/// started it, so it cannot borrow the handler's `&CorvusState`.
static STATE: OnceLock<Arc<CorvusState>> = OnceLock::new();
/// Plans being driven right now, each with the flag that asks its run to stop.
static RUNS: Mutex<Vec<(String, Arc<AtomicBool>)>> = Mutex::new(Vec::new());

/// How often a waiting gate looks at the plan (to notice a release or a cancel).
const TICK: Duration = Duration::from_secs(1);
/// Consecutive failed polls of the provider before a gate gives up.
const MAX_POLL_ERRORS: u32 = 5;

pub fn init(state: Arc<CorvusState>) {
    let _ = STATE.set(state);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// From the first incomplete phase to the end, then `finally`.
    Full,
    /// Only the `finally` steps — what stopping a paused plan does.
    FinallyOnly,
}

enum Outcome {
    Done,
    Paused(String),
    Cancelled,
}

enum GateOutcome {
    Open,
    Paused(String),
    Cancelled,
}

#[derive(Clone, Copy)]
enum StepSet {
    Phase(usize),
    Finally,
}

fn runs() -> std::sync::MutexGuard<'static, Vec<(String, Arc<AtomicBool>)>> {
    RUNS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_running(plan_id: &str) -> bool {
    runs().iter().any(|(id, _)| id == plan_id)
}

/// Ask a running plan to stop after the steps now in flight. `false` when it is not running.
pub fn request_cancel(plan_id: &str) -> bool {
    match runs().iter().find(|(id, _)| id == plan_id) {
        Some((_, flag)) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Start driving `plan_id` on its own task. Must be called from inside the runtime.
pub fn start(plan_id: &str, run: Run) -> Result<(), String> {
    let state = STATE.get().cloned().ok_or("the plan executor is not initialised")?;
    let cancel = {
        let mut active = runs();
        if active.iter().any(|(id, _)| id == plan_id) {
            return Err("this plan is already running".into());
        }
        let flag = Arc::new(AtomicBool::new(false));
        active.push((plan_id.to_string(), Arc::clone(&flag)));
        flag
    };
    let id = plan_id.to_string();
    tokio::spawn(async move {
        drive(&state, &id, &cancel, run).await;
        runs().retain(|(i, _)| i != &id);
    });
    Ok(())
}

async fn drive(state: &Arc<CorvusState>, id: &str, cancel: &Arc<AtomicBool>, run: Run) {
    let name = store::read(state, id, |p| p.name.clone()).unwrap_or_default();
    let job = PlanJob::start(state, &name).await;
    job.line(format!("Plan “{name}” started"));

    let outcome = run_plan(state, id, cancel, &job, run).await;
    let (status, reason, result) = match outcome {
        Outcome::Done => (PlanStatus::Done, None, Ok(())),
        Outcome::Paused(why) => (PlanStatus::Paused, Some(why.clone()), Err(why)),
        Outcome::Cancelled => (PlanStatus::Cancelled, None, Err("stopped".to_string())),
    };
    let _ = store::mutate(state, id, |p| {
        p.status = status;
        p.pause_reason = reason;
        Ok(())
    });
    job.line(match &result {
        Ok(()) => format!("Plan “{name}” finished"),
        Err(why) => format!("Plan “{name}” stopped: {why}"),
    });
    job.finish(result).await;
}

async fn run_plan(state: &Arc<CorvusState>, id: &str, cancel: &Arc<AtomicBool>, job: &PlanJob, run: Run) -> Outcome {
    if run == Run::Full {
        loop {
            if cancel.load(Ordering::Relaxed) {
                run_finally(state, id, job).await;
                return Outcome::Cancelled;
            }
            let next = store::read(state, id, |p| {
                p.first_open_phase().map(|i| (i, p.phases[i].name.clone(), p.phases[i].gate.state))
            });
            let Ok(Some((index, name, gate_state))) = next else { break };

            if index > 0 && gate_state != GateState::Open {
                match wait_gate(state, id, index, cancel, job).await {
                    GateOutcome::Open => {}
                    GateOutcome::Paused(why) => return Outcome::Paused(why),
                    GateOutcome::Cancelled => {
                        run_finally(state, id, job).await;
                        return Outcome::Cancelled;
                    }
                }
            }

            job.line(format!("Phase {} · {name}", index + 1));
            let failed = run_steps(state, id, StepSet::Phase(index), cancel, job).await;
            if failed > 0 {
                let _ = store::mutate(state, id, |p| {
                    p.hold_after(index);
                    Ok(())
                });
                return Outcome::Paused(format!("{failed} step(s) failed in “{name}”"));
            }
        }
    }
    if run == Run::Full && cancel.load(Ordering::Relaxed) {
        run_finally(state, id, job).await;
        return Outcome::Cancelled;
    }
    run_finally(state, id, job).await;
    if run == Run::FinallyOnly { Outcome::Cancelled } else { Outcome::Done }
}

/// The `finally` steps run to completion even when the plan was stopped, so they ignore cancel.
async fn run_finally(state: &Arc<CorvusState>, id: &str, job: &PlanJob) {
    job.line("Finally");
    let ignore = Arc::new(AtomicBool::new(false));
    run_steps(state, id, StepSet::Finally, &ignore, job).await;
}

/// Run the queued steps of one phase (or of `finally`); returns how many failed.
async fn run_steps(state: &Arc<CorvusState>, id: &str, set: StepSet, cancel: &Arc<AtomicBool>, job: &PlanJob) -> usize {
    let groups = store::read(state, id, |p| {
        let all: &[Step] = match set {
            StepSet::Phase(i) => &p.phases[i].steps,
            StepSet::Finally => &p.finally,
        };
        let queued: Vec<&Step> = all.iter().filter(|s| s.status == StepStatus::Queued).collect();
        let owned: Vec<Step> = queued.into_iter().cloned().collect();
        group_by_repo(&owned).into_iter().map(|g| g.into_iter().map(|i| owned[i].id.clone()).collect::<Vec<String>>()).collect::<Vec<_>>()
    });
    let Ok(groups) = groups else { return 0 };

    let tasks: Vec<_> = groups
        .into_iter()
        .map(|ids| {
            let (state, id, cancel, job) = (Arc::clone(state), id.to_string(), Arc::clone(cancel), job.clone());
            tokio::spawn(async move { run_group(&state, &id, ids, &cancel, &job).await })
        })
        .collect();
    let mut failed = 0;
    for t in tasks {
        failed += t.await.unwrap_or(1);
    }
    failed
}

/// The steps of one repo, in order.
async fn run_group(state: &Arc<CorvusState>, id: &str, step_ids: Vec<String>, cancel: &Arc<AtomicBool>, job: &PlanJob) -> usize {
    let mut failed = 0;
    for step_id in step_ids {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if !run_one(state, id, &step_id, job).await {
            failed += 1;
        }
    }
    failed
}

async fn run_one(state: &Arc<CorvusState>, id: &str, step_id: &str, job: &PlanJob) -> bool {
    let started = store::mutate(state, id, |p| {
        let (spec, repo, label) = {
            let s = p.find_step_mut(step_id).ok_or_else(|| "step vanished".to_string())?;
            s.status = StepStatus::Running;
            s.started_ms = Some(now_ms());
            s.error = None;
            (s.spec.clone(), s.repo.clone(), s.label())
        };
        let from = match &spec {
            StepSpec::MergeMr { from_step: Some(f), .. } => Some(f.clone()),
            _ => None,
        };
        let (mr_number, mr_source) = match from {
            Some(f) => {
                let source = match p.find_step(&f).map(|s| &s.spec) {
                    Some(StepSpec::CreateMr { source, .. }) => Some(source.clone()),
                    _ => None,
                };
                (p.mr_number_of(&f), source)
            }
            None => (None, None),
        };
        Ok((StepCtx { spec, repo, mr_number, mr_source }, label))
    });
    let Ok((ctx, label)) = started else { return false };

    job.line(format!("▶ {label}"));
    let result = execute(state, ctx).await;
    let ok = result.is_ok();
    let line = match &result {
        Ok(_) => format!("✓ {label}"),
        Err(e) => format!("✗ {label}: {e}"),
    };
    let _ = store::mutate(state, id, |p| {
        if let Some(s) = p.find_step_mut(step_id) {
            s.finished_ms = Some(now_ms());
            match result {
                Ok(res) => {
                    s.status = StepStatus::Done;
                    s.result = res;
                }
                Err(e) => {
                    s.status = StepStatus::Failed;
                    s.error = Some(e);
                }
            }
        }
        Ok(())
    });
    job.line(line);
    ok
}

// ── gates ───────────────────────────────────────────────────────────────────

fn set_gate(state: &Arc<CorvusState>, id: &str, index: usize, to: GateState) {
    let _ = store::mutate(state, id, |p| {
        let gate = &mut p.phases[index].gate;
        gate.state = to;
        gate.since_ms = if to == GateState::Waiting { Some(now_ms()) } else { gate.since_ms };
        Ok(())
    });
}

/// Block until phase `index` may start. A gate the user releases (the UI sets it open) is noticed
/// on the next tick whatever it was waiting for.
async fn wait_gate(state: &Arc<CorvusState>, id: &str, index: usize, cancel: &Arc<AtomicBool>, job: &PlanJob) -> GateOutcome {
    let Ok((spec, name, mr)) = store::read(state, id, |p| {
        let phase = &p.phases[index];
        let mr = match &phase.gate.spec {
            GateSpec::MrMerged { step_id, .. } => p.find_step(step_id).map(|s| (s.repo.clone(), p.mr_number_of(step_id))),
            _ => None,
        };
        (phase.gate.spec.clone(), phase.name.clone(), mr)
    }) else {
        return GateOutcome::Cancelled;
    };

    match &spec {
        GateSpec::PhaseDone => {
            set_gate(state, id, index, GateState::Open);
            return GateOutcome::Open;
        }
        GateSpec::Manual => {
            set_gate(state, id, index, GateState::Waiting);
            let why = format!("Waiting for your go-ahead before “{name}”");
            job.line(why.clone());
            return GateOutcome::Paused(why);
        }
        _ => {}
    }

    set_gate(state, id, index, GateState::Waiting);
    let mr_ref: Option<(RepoRef, u64)> = mr.and_then(|(repo, n)| n.map(|n| (repo, n)));
    if matches!(spec, GateSpec::MrMerged { .. }) && mr_ref.is_none() {
        set_gate(state, id, index, GateState::Blocked);
        return GateOutcome::Paused("the MR this phase waits for was never created".into());
    }
    match &spec {
        GateSpec::Delay { secs } => job.line(format!("Waiting {secs}s before “{name}”")),
        _ => job.line(format!("Waiting for the MR to be merged before “{name}”")),
    }

    let started = Instant::now();
    let mut last_poll: Option<Instant> = None;
    let mut errors = 0u32;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return GateOutcome::Cancelled;
        }
        let released = store::read(state, id, |p| p.phases[index].gate.state == GateState::Open).unwrap_or(false);
        if released {
            job.line("Gate released");
            return GateOutcome::Open;
        }
        match &spec {
            GateSpec::Delay { secs } if started.elapsed() >= Duration::from_secs(*secs) => {
                set_gate(state, id, index, GateState::Open);
                return GateOutcome::Open;
            }
            GateSpec::MrMerged { poll_secs, timeout_secs, .. } => {
                if started.elapsed() >= Duration::from_secs(*timeout_secs) {
                    set_gate(state, id, index, GateState::Blocked);
                    return GateOutcome::Paused(format!("gave up waiting for the MR after {timeout_secs}s"));
                }
                let due = last_poll.map_or(true, |t| t.elapsed() >= Duration::from_secs((*poll_secs).max(5)));
                if due {
                    last_poll = Some(Instant::now());
                    let (repo, number) = mr_ref.clone().expect("checked above");
                    match poll_mr(state, &repo, number).await {
                        Ok(MrState::Merged) => {
                            job.line(format!("MR !{number} merged"));
                            set_gate(state, id, index, GateState::Open);
                            return GateOutcome::Open;
                        }
                        Ok(MrState::Closed) => {
                            set_gate(state, id, index, GateState::Blocked);
                            return GateOutcome::Paused(format!("MR !{number} was closed without being merged"));
                        }
                        Ok(_) => errors = 0,
                        Err(e) => {
                            errors += 1;
                            job.line(format!("could not check MR !{number}: {e}"));
                            if errors >= MAX_POLL_ERRORS {
                                set_gate(state, id, index, GateState::Blocked);
                                return GateOutcome::Paused(format!("could not check MR !{number}: {e}"));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        tokio::time::sleep(TICK).await;
    }
}

async fn poll_mr(state: &Arc<CorvusState>, repo: &RepoRef, number: u64) -> Result<MrState, String> {
    let tab = crate::ephemeral::tab_for(state, &repo.path)?;
    Ok(crate::mr::get_mr_detail(state, tab, number).await?.mr.state)
}
