//! The plans this backend holds: in memory, mirrored to `<corvus dir>/plans.json` after every
//! change so a plan survives a restart. Every mutation goes through [`mutate`], which applies it
//! under the lock, then — lock released — writes the file and tells the UI.
//!
//! A plan that was `Running` when the process went away cannot still be running: on load its
//! in-flight steps are failed ("interrupted") and it is paused, so it resumes from there.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use corvus_core::prelude::CorvusState;

use super::model::{Plan, PlanStatus, StepStatus};

static PLANS: Mutex<Vec<Plan>> = Mutex::new(Vec::new());
static LOAD: Once = Once::new();
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Egress topic: the whole changed plan as payload.
pub const PLAN_UPDATED: &str = "arbor://plan-updated";
pub const PLAN_REMOVED: &str = "arbor://plan-removed";

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A new id with the given prefix, unique within the process and across restarts (clock + counter).
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{:x}-{}", now_ms(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn file_path(state: &CorvusState) -> Result<PathBuf, String> {
    crate::corvus_config::corvus_config_dir(state).map(|d| PathBuf::from(d).join("plans.json"))
}

fn lock() -> std::sync::MutexGuard<'static, Vec<Plan>> {
    PLANS.lock().unwrap_or_else(|e| e.into_inner())
}

fn load_once(state: &CorvusState) {
    LOAD.call_once(|| {
        let Ok(path) = file_path(state) else { return };
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let mut plans: Vec<Plan> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("corvus-be: plans.json is unreadable, starting empty: {e}");
                return;
            }
        };
        for p in plans.iter_mut().filter(|p| p.status == PlanStatus::Running) {
            fail_interrupted(p);
        }
        *lock() = plans;
    });
}

fn fail_interrupted(plan: &mut Plan) {
    let steps = plan.phases.iter_mut().flat_map(|p| p.steps.iter_mut()).chain(plan.finally.iter_mut());
    for s in steps.filter(|s| s.status == StepStatus::Running) {
        s.status = StepStatus::Failed;
        s.error = Some("interrupted: the app was closed while this step ran".into());
    }
    plan.status = PlanStatus::Paused;
    plan.pause_reason = Some("Interrupted by a restart".into());
}

fn save(state: &CorvusState, plans: &[Plan]) {
    let Ok(path) = file_path(state) else { return };
    let write = || -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(plans).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    };
    if let Err(e) = write() {
        eprintln!("corvus-be: could not save plans.json: {e}");
    }
}

pub fn list(state: &CorvusState) -> Vec<Plan> {
    load_once(state);
    lock().clone()
}

pub fn get(state: &CorvusState, id: &str) -> Result<Plan, String> {
    load_once(state);
    lock().iter().find(|p| p.id == id).cloned().ok_or_else(|| format!("plan '{id}' not found"))
}

/// Read a plan without cloning it all: `f` runs under the lock, so keep it short.
pub fn read<R>(state: &CorvusState, id: &str, f: impl FnOnce(&Plan) -> R) -> Result<R, String> {
    load_once(state);
    let plans = lock();
    plans.iter().find(|p| p.id == id).map(f).ok_or_else(|| format!("plan '{id}' not found"))
}

/// Apply `f` to the plan, then persist and announce the result. `f` may refuse with an error, in
/// which case nothing is written.
pub fn mutate<R>(state: &CorvusState, id: &str, f: impl FnOnce(&mut Plan) -> Result<R, String>) -> Result<R, String> {
    load_once(state);
    let (out, snapshot, all) = {
        let mut plans = lock();
        let plan = plans.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("plan '{id}' not found"))?;
        let out = f(plan)?;
        plan.updated_ms = now_ms();
        let snapshot = plan.clone();
        (out, snapshot, plans.clone())
    };
    save(state, &all);
    announce(state, &snapshot);
    Ok(out)
}

/// Add a plan that does not exist yet.
pub fn insert(state: &CorvusState, plan: Plan) -> Plan {
    load_once(state);
    let all = {
        let mut plans = lock();
        plans.push(plan.clone());
        plans.clone()
    };
    save(state, &all);
    announce(state, &plan);
    plan
}

pub fn remove(state: &CorvusState, id: &str) -> Result<(), String> {
    load_once(state);
    let all = {
        let mut plans = lock();
        let before = plans.len();
        plans.retain(|p| p.id != id);
        if plans.len() == before {
            return Err(format!("plan '{id}' not found"));
        }
        plans.clone()
    };
    save(state, &all);
    state.emit(PLAN_REMOVED, serde_json::json!({ "id": id }));
    Ok(())
}

fn announce(state: &CorvusState, plan: &Plan) {
    if let Ok(v) = serde_json::to_value(plan) {
        state.emit(PLAN_UPDATED, v);
    }
}

/// A fresh, empty plan named `name`.
pub fn blank(name: &str) -> Plan {
    let now = now_ms();
    Plan {
        id: new_id("plan"),
        name: name.to_string(),
        phases: Vec::new(),
        finally: Vec::new(),
        status: PlanStatus::Draft,
        pause_reason: None,
        template: false,
        created_ms: now,
        updated_ms: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::model::{Gate, Phase, RepoRef, Step, StepSpec};

    #[test]
    fn ids_are_unique_in_a_burst() {
        let a = new_id("p");
        let b = new_id("p");
        assert_ne!(a, b);
        assert!(a.starts_with("p-"));
    }

    #[test]
    fn an_interrupted_plan_pauses_with_its_running_step_failed() {
        let running = Step {
            id: "s".into(),
            spec: StepSpec::Pull {},
            repo: RepoRef { path: "/r".into(), name: "r".into(), workspace: None },
            status: StepStatus::Running,
            error: None,
            started_ms: Some(1),
            finished_ms: None,
            result: None,
        };
        let mut plan = blank("x");
        plan.phases.push(Phase { id: "ph".into(), name: "ph".into(), gate: Gate::default(), steps: vec![running] });
        plan.status = PlanStatus::Running;
        fail_interrupted(&mut plan);
        assert_eq!(plan.status, PlanStatus::Paused);
        assert_eq!(plan.phases[0].steps[0].status, StepStatus::Failed);
        assert!(plan.phases[0].steps[0].error.as_deref().unwrap().contains("interrupted"));
    }
}
