//! `plan` domain — a *plan* orders remote git steps (push, create/merge MR, pipeline, tag, pull)
//! across repositories and workspaces: phases whose steps run together, gates between phases
//! (an MR merged, a delay, a manual go-ahead), a `finally` that always runs.
//!
//! - [`model`] — the data and the scheduling rules (pure, unit-tested).
//! - [`store`] — the plans in memory, mirrored to `<corvus dir>/plans.json`, announced to the UI.
//! - [`exec`]  — one detached task per run; pauses (instead of sleeping) when it needs the user.
//! - [`steps`] — running a single step through the same handlers the UI's buttons reach.
//! - [`job`]   — a run shown as one Job in the Jobs panel.
//!
//! The handlers below are the whole RPC surface. Editing a plan is refused while it runs: a
//! running plan owns its step statuses, and the UI's copy would overwrite them.

mod exec;
mod job;
pub mod model;
mod steps;
mod store;

use std::sync::Arc;

use corvus_core::prelude::CorvusState;

use model::{Phase, Plan, PlanStatus, RepoRef, Step, StepSpec, StepStatus};

/// Wire the executor to the process-wide state. Called once at startup.
pub(crate) fn init(state: Arc<CorvusState>) {
    exec::init(state);
}

fn editable(plan: &Plan) -> Result<(), String> {
    if plan.status.is_editable() {
        Ok(())
    } else {
        Err("this plan is running — stop it or wait for it to pause before editing".into())
    }
}

/// The plan an entry point adds to when it does not name one: the most recently touched plan that
/// is still open for editing, or a new one if there is none.
fn default_target(state: &CorvusState) -> String {
    let open = store::list(state)
        .into_iter()
        .filter(|p| !p.template && matches!(p.status, PlanStatus::Draft | PlanStatus::Paused))
        .max_by_key(|p| p.updated_ms);
    match open {
        Some(p) => p.id,
        None => {
            let n = store::list(state).iter().filter(|p| !p.template).count() + 1;
            store::insert(state, store::blank(&format!("Plan {n}"))).id
        }
    }
}

fn new_phase(plan: &Plan) -> Phase {
    Phase { id: store::new_id("ph"), name: format!("Phase {}", plan.phases.len() + 1), gate: Default::default(), steps: Vec::new() }
}

#[arbor_rpc::handler]
pub(crate) fn plan_list(state: &CorvusState) -> Result<Vec<Plan>, String> {
    Ok(store::list(state))
}

#[arbor_rpc::handler]
pub(crate) fn plan_get(state: &CorvusState, plan_id: String) -> Result<Plan, String> {
    store::get(state, &plan_id)
}

#[arbor_rpc::handler]
pub(crate) fn plan_create(state: &CorvusState, name: Option<String>) -> Result<Plan, String> {
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Plan {}", store::list(state).len() + 1));
    Ok(store::insert(state, store::blank(&name)))
}

/// Replace a plan's name, phases, gates and `finally` with the editor's copy. A plan that already
/// ran starts over as a draft; one that is running cannot be saved.
#[arbor_rpc::handler]
fn plan_save(state: &CorvusState, plan: Plan) -> Result<Plan, String> {
    plan.validate()?;
    store::mutate(state, &plan.id.clone(), move |p| {
        editable(p)?;
        p.name = plan.name;
        p.phases = plan.phases;
        p.finally = plan.finally;
        p.template = plan.template;
        if matches!(p.status, PlanStatus::Done | PlanStatus::Failed | PlanStatus::Cancelled) {
            p.reset();
        }
        Ok(p.clone())
    })
}

#[arbor_rpc::handler]
fn plan_delete(state: &CorvusState, plan_id: String) -> Result<(), String> {
    if exec::is_running(&plan_id) {
        return Err("this plan is running — stop it first".into());
    }
    store::remove(state, &plan_id)
}

/// A copy with fresh statuses — used to run a template, or to keep a plan as one.
#[arbor_rpc::handler]
fn plan_copy(state: &CorvusState, plan_id: String, name: Option<String>, template: Option<bool>) -> Result<Plan, String> {
    let mut copy = store::get(state, &plan_id)?;
    let now = store::now_ms();
    copy.id = store::new_id("plan");
    copy.name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| copy.name.clone());
    copy.template = template.unwrap_or(false);
    copy.created_ms = now;
    copy.updated_ms = now;
    copy.reset();
    Ok(store::insert(state, copy))
}

/// Add one step. With no `plan_id` it lands in the plan the user is composing (see
/// [`default_target`]); with no `phase_id` it joins the last phase, or opens one. `new_phase_after`
/// puts it in a phase of its own after the last — "start this only once the rest has finished" —
/// and `to_finally` puts it in the `finally` steps instead.
#[arbor_rpc::handler]
pub(crate) fn plan_add_step(
    state: &CorvusState,
    plan_id: Option<String>,
    phase_id: Option<String>,
    new_phase_after: Option<bool>,
    to_finally: Option<bool>,
    repo: RepoRef,
    spec: StepSpec,
) -> Result<Plan, String> {
    let id = plan_id.unwrap_or_else(|| default_target(state));
    store::mutate(state, &id, move |p| {
        editable(p)?;
        let step = Step {
            id: store::new_id("st"),
            spec,
            repo,
            status: StepStatus::Queued,
            error: None,
            started_ms: None,
            finished_ms: None,
            result: None,
        };
        if to_finally.unwrap_or(false) {
            p.finally.push(step);
            if matches!(p.status, PlanStatus::Done | PlanStatus::Failed | PlanStatus::Cancelled) {
                p.reset();
            }
            p.validate()?;
            return Ok(p.clone());
        }
        let target = match (phase_id.as_deref(), new_phase_after.unwrap_or(false)) {
            (_, true) => None,
            (Some(pid), false) => Some(p.phases.iter().position(|ph| ph.id == pid).ok_or_else(|| format!("phase '{pid}' not found"))?),
            (None, false) => p.phases.len().checked_sub(1),
        };
        match target {
            Some(i) => p.phases[i].steps.push(step),
            None => {
                let mut phase = new_phase(p);
                phase.steps.push(step);
                p.phases.push(phase);
            }
        }
        if matches!(p.status, PlanStatus::Done | PlanStatus::Failed | PlanStatus::Cancelled) {
            p.reset();
        }
        p.validate()?;
        Ok(p.clone())
    })
}

/// Run a plan — from the start when it is new or finished, from where it stopped when paused.
#[arbor_rpc::handler]
pub(crate) async fn plan_run(state: &CorvusState, plan_id: String) -> Result<Plan, String> {
    let plan = store::mutate(state, &plan_id, |p| {
        if p.template {
            return Err("a template is copied, not run".into());
        }
        editable(p)?;
        p.validate()?;
        if p.phases.iter().all(|ph| ph.steps.is_empty()) {
            return Err("the plan has no steps yet".into());
        }
        if matches!(p.status, PlanStatus::Done | PlanStatus::Failed | PlanStatus::Cancelled) {
            p.reset();
        }
        p.prepare_run();
        Ok(p.clone())
    })?;
    exec::start(&plan_id, exec::Run::Full)?;
    Ok(plan)
}

/// Stop a plan: a running one finishes the steps in flight, a paused one goes straight on to its
/// `finally` steps; both end `Cancelled`.
#[arbor_rpc::handler]
pub(crate) async fn plan_stop(state: &CorvusState, plan_id: String) -> Result<Plan, String> {
    if !exec::request_cancel(&plan_id) {
        let paused = store::read(state, &plan_id, |p| p.status == PlanStatus::Paused)?;
        if !paused {
            return Err("this plan is not running".into());
        }
        store::mutate(state, &plan_id, |p| {
            p.status = PlanStatus::Running;
            Ok(())
        })?;
        exec::start(&plan_id, exec::Run::FinallyOnly)?;
    }
    store::get(state, &plan_id)
}

/// Put a failed or skipped step back in the queue; the plan picks it up when resumed.
#[arbor_rpc::handler]
pub(crate) fn plan_retry_step(state: &CorvusState, plan_id: String, step_id: String) -> Result<Plan, String> {
    store::mutate(state, &plan_id, |p| {
        editable(p)?;
        let s = p.find_step_mut(&step_id).ok_or_else(|| format!("step '{step_id}' not found"))?;
        if matches!(s.status, StepStatus::Failed | StepStatus::Skipped) {
            s.status = StepStatus::Queued;
            s.error = None;
        }
        Ok(p.clone())
    })
}

/// Give up on a step: later phases stop waiting for it.
#[arbor_rpc::handler]
pub(crate) fn plan_skip_step(state: &CorvusState, plan_id: String, step_id: String) -> Result<Plan, String> {
    store::mutate(state, &plan_id, |p| {
        editable(p)?;
        let s = p.find_step_mut(&step_id).ok_or_else(|| format!("step '{step_id}' not found"))?;
        if matches!(s.status, StepStatus::Failed | StepStatus::Queued | StepStatus::Held) {
            s.status = StepStatus::Skipped;
            s.error = None;
        }
        Ok(p.clone())
    })
}

/// Open the gate before phase `phase_id` — the go-ahead for a manual gate, "skip the wait" for the
/// others. A paused plan resumes; a running one notices within a second.
#[arbor_rpc::handler]
pub(crate) async fn plan_release_gate(state: &CorvusState, plan_id: String, phase_id: String) -> Result<Plan, String> {
    let (plan, resume) = store::mutate(state, &plan_id, |p| {
        let phase = p.phases.iter_mut().find(|ph| ph.id == phase_id).ok_or_else(|| format!("phase '{phase_id}' not found"))?;
        phase.gate.state = model::GateState::Open;
        let resume = p.status == PlanStatus::Paused;
        if resume {
            p.prepare_run();
        }
        Ok((p.clone(), resume))
    })?;
    if resume {
        exec::start(&plan_id, exec::Run::Full)?;
    }
    Ok(plan)
}

/// What a plan is built from by something that is not the editor (Imber): phases of steps, each step
/// optionally carrying an id other steps' gates can name.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct StepIn {
    pub id: Option<String>,
    #[serde(flatten)]
    pub spec: StepSpec,
    pub repo: Option<RepoRef>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PhaseIn {
    pub name: Option<String>,
    pub gate: Option<model::GateSpec>,
    #[serde(default)]
    pub steps: Vec<StepIn>,
}

/// A draft plan from phases, validated. Nothing runs: the user reviews it in the Plan window and runs it.
/// `default_repo` stands for steps that name no repository.
pub(crate) fn create_draft(state: &CorvusState, name: &str, phases: Vec<PhaseIn>, default_repo: &RepoRef) -> Result<Plan, String> {
    let mut plan = store::blank(name);
    for (i, p) in phases.into_iter().enumerate() {
        let steps = p
            .steps
            .into_iter()
            .map(|s| Step {
                id: s.id.filter(|x| !x.is_empty()).unwrap_or_else(|| store::new_id("st")),
                spec: s.spec,
                repo: s.repo.unwrap_or_else(|| default_repo.clone()),
                status: StepStatus::Queued,
                error: None,
                started_ms: None,
                finished_ms: None,
                result: None,
            })
            .collect();
        plan.phases.push(Phase {
            id: store::new_id("ph"),
            name: p.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Phase {}", i + 1)),
            gate: p.gate.map(|spec| model::Gate { spec, ..Default::default() }).unwrap_or_default(),
            steps,
        });
    }
    plan.validate()?;
    Ok(store::insert(state, plan))
}

/// Set the condition that starts phase `phase_id` (the first phase has none: nothing precedes it). The change
/// is validated on a copy: a gate that would make the plan incoherent leaves the plan as it was.
pub(crate) fn set_gate(state: &CorvusState, plan_id: &str, phase_id: &str, spec: model::GateSpec) -> Result<Plan, String> {
    store::mutate(state, plan_id, |p| {
        editable(p)?;
        let at = p.phases.iter().position(|ph| ph.id == phase_id).ok_or_else(|| format!("phase '{phase_id}' not found"))?;
        if at == 0 {
            return Err("the first phase has no gate: nothing comes before it".into());
        }
        let mut next = p.clone();
        next.phases[at].gate = model::Gate { spec, ..Default::default() };
        next.validate()?;
        *p = next;
        Ok(p.clone())
    })
}

/// Take a step out of a plan. An empty phase is left in place (the user may fill it again). Validated on a
/// copy, so removing a step a gate or a merge refers to is refused rather than half done.
pub(crate) fn remove_step(state: &CorvusState, plan_id: &str, step_id: &str) -> Result<Plan, String> {
    store::mutate(state, plan_id, |p| {
        editable(p)?;
        let mut next = p.clone();
        let mut found = false;
        for ph in &mut next.phases {
            let before = ph.steps.len();
            ph.steps.retain(|s| s.id != step_id);
            found |= ph.steps.len() != before;
        }
        let before = next.finally.len();
        next.finally.retain(|s| s.id != step_id);
        found |= next.finally.len() != before;
        if !found {
            return Err(format!("step '{step_id}' not found"));
        }
        next.validate()?;
        *p = next;
        Ok(p.clone())
    })
}
