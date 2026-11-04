//! Imber's tools for Plans — the feature that orders remote git steps (push, create / merge a request, pipeline,
//! tag, pull) in phases with gates between them, across repositories.
//!
//! The tools call the plan handlers directly (`plan::plan_add_step`, `plan::plan_run`, …), as the git tools call
//! the git handlers: the same validation, the same store, the same executor, and the same
//! `arbor://plan-updated` events — so the plan card in the chat and the Plan window show one plan.
//!
//! Permissions are the table's (`permission.rs`): building or editing a plan is a *write* (a draft changes
//! nothing outside Corvus); running it is a *write* too — it pushes, opens and merges requests — so in manual
//! mode the run is proposed and waits for the user (the panel opens the plan for review), and in auto it runs.
//! A scheduled run (nobody there) may not run a plan that merges requests.

use corvus_core::prelude::CorvusState;
use imber_core::prelude::ToolSpec;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::plan::model::{GateSpec, Plan, PlanStatus, RepoRef, StepSpec, StepStatus};
use crate::plan::{self, PhaseIn};

/// The tools that answer with a plan, so the chat can show it as a card.
pub const PLAN_TOOLS: [&str; 10] =
    ["plan_create", "plan_show", "plan_add_step", "plan_remove_step", "plan_set_gate", "plan_run", "plan_stop", "plan_retry_step", "plan_skip_step", "plan_release_gate"];

pub fn is_plan_tool(name: &str) -> bool {
    name == "plan_list" || PLAN_TOOLS.contains(&name)
}

fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": properties, "required": required }) }
}

fn s(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

const STEP_KINDS: &str = "Step kinds: push{branch}, create_mr{source,target,title,description?,draft?}, merge_mr{from_step? | number?, squash?, delete_branch?}, run_pipeline{branch}, tag{name,message?,target?}, pull{}. `repo` is {path,name}; omit it for this repository.";
const GATES: &str = "Gates start a phase after the first: {\"kind\":\"phase_done\"} (default), {\"kind\":\"mr_merged\",\"step_id\":\"<an earlier create_mr step>\"}, {\"kind\":\"delay\",\"secs\":N}, {\"kind\":\"manual\"}.";

pub fn specs() -> Vec<ToolSpec> {
    vec![
        spec("plan_list", "List the plans (name, status, how many phases and steps).", json!({}), &[]),
        spec("plan_show", "One plan in full: phases, gates, and each step's status and error. Use it to follow a plan that is running.", json!({ "plan_id": s("The plan's id.") }), &["plan_id"]),
        spec(
            "plan_create",
            &format!("Create a DRAFT plan of ordered remote steps, possibly across repositories — for chains that must wait on something remote, e.g. 'open an MR, and when it is merged start one on another repo'. Phases run in order; the steps of a phase start together. {GATES} {STEP_KINDS} Give a step an `id` when a gate or merge_mr must refer to it. A commit cannot be planned (it captures the working tree now): do it first. Nothing runs until plan_run."),
            json!({
                "name": s("Plan name."),
                "phases": json!({ "type": "array", "items": { "type": "object", "properties": {
                    "name": s("Phase name."),
                    "gate": json!({ "type": "object", "description": "Condition to start this phase (not the first)." }),
                    "steps": json!({ "type": "array", "items": { "type": "object", "description": "A step: `kind` plus its fields, optional `id`, optional `repo`." } }),
                } }, "description": "Phases in order." }),
            }),
            &["phases"],
        ),
        spec(
            "plan_add_step",
            &format!("Add one step to a plan: to its last phase, to a given phase, in a new phase after the last (`new_phase_after`: it starts only once the rest has finished), or to `finally` (runs when the plan ends). {STEP_KINDS}"),
            json!({
                "plan_id": s("The plan's id."),
                "kind": json!({ "type": "string", "enum": ["push", "create_mr", "merge_mr", "run_pipeline", "tag", "pull"] }),
                "id": s("Optional id other steps can refer to."),
                "phase_id": s("Add to this phase."),
                "new_phase_after": json!({ "type": "boolean" }),
                "to_finally": json!({ "type": "boolean" }),
                "repo": json!({ "type": "object", "description": "{path,name} of another repository." }),
            }),
            &["plan_id", "kind"],
        ),
        spec("plan_remove_step", "Take a step out of a plan that is not running.", json!({ "plan_id": s("The plan's id."), "step_id": s("The step's id.") }), &["plan_id", "step_id"]),
        spec("plan_set_gate", &format!("Set the condition that starts a phase. {GATES}"), json!({ "plan_id": s("The plan's id."), "phase_id": s("The phase (not the first)."), "gate": json!({ "type": "object" }) }), &["plan_id", "phase_id", "gate"]),
        spec(
            "plan_run",
            "Run a plan (or resume a paused one). It runs in the background — pushes, opens and merges requests, starts pipelines, tags — so in manual mode the user approves first. Follow it with plan_show.",
            json!({ "plan_id": s("The plan's id.") }),
            &["plan_id"],
        ),
        spec("plan_stop", "Stop a running plan: the steps in flight finish, then `finally` runs. A paused plan goes straight to `finally`.", json!({ "plan_id": s("The plan's id.") }), &["plan_id"]),
        spec("plan_retry_step", "Put a failed or skipped step back in the queue; the plan picks it up when resumed with plan_run.", json!({ "plan_id": s("The plan's id."), "step_id": s("The step's id.") }), &["plan_id", "step_id"]),
        spec("plan_skip_step", "Give up on a step: later phases stop waiting for it.", json!({ "plan_id": s("The plan's id."), "step_id": s("The step's id.") }), &["plan_id", "step_id"]),
        spec(
            "plan_release_gate",
            "Open the gate before a phase: the go-ahead for a manual gate, 'skip the wait' for the others. A paused plan resumes.",
            json!({ "plan_id": s("The plan's id."), "phase_id": s("The phase whose gate to open.") }),
            &["plan_id", "phase_id"],
        ),
    ]
}

// ── What the model reads ─────────────────────────────────────────────────────────────────────────

/// A gate in a few words.
pub fn gate_text(g: &GateSpec) -> String {
    match g {
        GateSpec::PhaseDone => "when the previous phase is done".into(),
        GateSpec::MrMerged { step_id, .. } => format!("when the MR of step {step_id} is merged"),
        GateSpec::Delay { secs } => format!("after {secs}s"),
        GateSpec::Manual => "when you say so".into(),
    }
}

fn status_word(s: StepStatus) -> &'static str {
    match s {
        StepStatus::Queued => "queued",
        StepStatus::Held => "held",
        StepStatus::Running => "running",
        StepStatus::Done => "done",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}

fn plan_status_word(s: PlanStatus) -> &'static str {
    match s {
        PlanStatus::Draft => "draft",
        PlanStatus::Running => "running",
        PlanStatus::Paused => "paused",
        PlanStatus::Done => "done",
        PlanStatus::Failed => "failed",
        PlanStatus::Cancelled => "cancelled",
    }
}

/// The plan as the model reads it: compact, with the ids it needs to refer to things.
pub fn summary(p: &Plan) -> Value {
    let step = |s: &crate::plan::model::Step| {
        let mut v = json!({ "id": s.id, "what": s.label(), "status": status_word(s.status) });
        if let Some(e) = &s.error {
            v["error"] = json!(e);
        }
        if let Some(r) = s.result.as_ref().filter(|r| !r.is_null()) {
            v["result"] = r.clone();
        }
        v
    };
    json!({
        "plan_id": p.id,
        "name": p.name,
        "status": plan_status_word(p.status),
        "pause_reason": p.pause_reason,
        "phases": p.phases.iter().enumerate().map(|(i, ph)| json!({
            "id": ph.id, "name": ph.name,
            "starts": if i == 0 { "immediately".to_string() } else { gate_text(&ph.gate.spec) },
            "steps": ph.steps.iter().map(step).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "finally": p.finally.iter().map(step).collect::<Vec<_>>(),
    })
}

/// Whether the plan contains a step that merges a request — the one a scheduled run must not start.
pub fn merges_requests(p: &Plan) -> bool {
    p.phases.iter().flat_map(|ph| ph.steps.iter()).chain(p.finally.iter()).any(|s| matches!(s.spec, StepSpec::MergeMr { .. }))
}

// ── Running a tool ───────────────────────────────────────────────────────────────────────────────

pub struct Ctx<'a> {
    pub state: &'a CorvusState,
    pub handle: &'a tokio::runtime::Handle,
    pub root: &'a str,
    pub unattended: bool,
}

fn text(input: &Value, key: &str) -> Result<String, String> {
    input.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty()).ok_or_else(|| format!("`{key}` is required"))
}

fn here(root: &str) -> RepoRef {
    RepoRef { path: root.to_string(), name: std::path::Path::new(root).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), workspace: None }
}

fn answer(p: &Plan, note: &str) -> Result<String, String> {
    let mut v = summary(p);
    if !note.is_empty() {
        v["note"] = json!(note);
    }
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

pub fn run(ctx: &Ctx<'_>, name: &str, input: &Value) -> Result<String, String> {
    let state = ctx.state;
    match name {
        "plan_list" => {
            let rows: Vec<Value> = plan::plan_list(state)?
                .iter()
                .filter(|p| !p.template)
                .map(|p| json!({ "plan_id": p.id, "name": p.name, "status": plan_status_word(p.status), "phases": p.phases.len(), "steps": p.phases.iter().map(|ph| ph.steps.len()).sum::<usize>() + p.finally.len() }))
                .collect();
            serde_json::to_string(&rows).map_err(|e| e.to_string())
        }
        "plan_show" => answer(&plan::plan_get(state, text(input, "plan_id")?)?, ""),
        "plan_create" => {
            #[derive(Deserialize)]
            struct A {
                name: Option<String>,
                phases: Vec<PhaseIn>,
            }
            let a: A = serde_json::from_value(input.clone()).map_err(|e| format!("the arguments were not valid: {e}"))?;
            let name = a.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Imber plan".into());
            let made = plan::create_draft(state, &name, a.phases, &here(ctx.root))?;
            answer(&made, "A draft: nothing has run. plan_run starts it.")
        }
        "plan_add_step" => {
            let spec: StepSpec = serde_json::from_value(input.clone()).map_err(|e| format!("not a valid step: {e}"))?;
            let repo = match input.get("repo") {
                Some(r) if !r.is_null() => serde_json::from_value::<RepoRef>(r.clone()).map_err(|e| format!("`repo` must be {{path, name}}: {e}"))?,
                _ => here(ctx.root),
            };
            let flag = |k: &str| input.get(k).and_then(Value::as_bool);
            let phase = input.get("phase_id").and_then(Value::as_str).map(str::to_string);
            let plan_id = text(input, "plan_id")?;
            let updated = plan::plan_add_step(state, Some(plan_id), phase, flag("new_phase_after"), flag("to_finally"), repo, spec)?;
            answer(&updated, "")
        }
        "plan_remove_step" => answer(&plan::remove_step(state, &text(input, "plan_id")?, &text(input, "step_id")?)?, ""),
        "plan_set_gate" => {
            let gate: GateSpec = serde_json::from_value(input.get("gate").cloned().ok_or("`gate` is required")?).map_err(|e| format!("not a valid gate: {e}"))?;
            answer(&plan::set_gate(state, &text(input, "plan_id")?, &text(input, "phase_id")?, gate)?, "")
        }
        "plan_run" => {
            let id = text(input, "plan_id")?;
            if ctx.unattended && merges_requests(&plan::plan_get(state, id.clone())?) {
                return Err("A scheduled run does not start a plan that merges requests. Report that a person has to run it.".into());
            }
            let started = ctx.handle.block_on(plan::plan_run(state, id))?;
            answer(&started, "Running in the background. plan_show reports each step as it finishes.")
        }
        "plan_stop" => answer(&ctx.handle.block_on(plan::plan_stop(state, text(input, "plan_id")?))?, ""),
        "plan_retry_step" => answer(&plan::plan_retry_step(state, text(input, "plan_id")?, text(input, "step_id")?)?, "Queued again; plan_run resumes the plan."),
        "plan_skip_step" => answer(&plan::plan_skip_step(state, text(input, "plan_id")?, text(input, "step_id")?)?, ""),
        "plan_release_gate" => answer(&ctx.handle.block_on(plan::plan_release_gate(state, text(input, "plan_id")?, text(input, "phase_id")?))?, ""),
        other => Err(format!("'{other}' is not a plan tool.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_with(steps: Value, finally: Value) -> Plan {
        serde_json::from_value(json!({
            "id": "plan-1", "name": "Release", "status": "paused", "pause_reason": "step failed",
            "phases": [
                { "id": "ph1", "name": "Phase 1", "steps": steps },
                { "id": "ph2", "name": "Phase 2", "gate": { "kind": "mr_merged", "step_id": "mr1", "state": "waiting" }, "steps": [] },
            ],
            "finally": finally,
        }))
        .unwrap()
    }

    fn step(id: &str, spec: Value, status: &str) -> Value {
        let mut v = json!({ "id": id, "repo": { "path": "/p/api", "name": "api" }, "status": status });
        v.as_object_mut().unwrap().extend(spec.as_object().unwrap().clone());
        v
    }

    #[test]
    fn the_model_reads_each_step_with_its_id_status_and_error() {
        let mut failed = step("s2", json!({ "kind": "push", "branch": "feature/x" }), "failed");
        failed["error"] = json!("rejected: behind");
        let p = plan_with(json!([step("mr1", json!({ "kind": "create_mr", "source": "feature/x", "target": "main", "title": "T" }), "done"), failed]), json!([]));
        let v = summary(&p);
        assert_eq!(v["plan_id"], "plan-1");
        assert_eq!(v["status"], "paused");
        assert_eq!(v["phases"][0]["starts"], "immediately");
        assert_eq!(v["phases"][1]["starts"], "when the MR of step mr1 is merged");
        assert_eq!(v["phases"][0]["steps"][0]["what"], "create MR feature/x \u{2192} main (api)");
        assert_eq!(v["phases"][0]["steps"][1]["status"], "failed");
        assert_eq!(v["phases"][0]["steps"][1]["error"], "rejected: behind");
        assert!(v["phases"][0]["steps"][0].get("error").is_none());
    }

    #[test]
    fn gates_are_described_in_words() {
        assert_eq!(gate_text(&GateSpec::PhaseDone), "when the previous phase is done");
        assert_eq!(gate_text(&GateSpec::Delay { secs: 90 }), "after 90s");
        assert_eq!(gate_text(&GateSpec::Manual), "when you say so");
    }

    #[test]
    fn a_plan_that_merges_requests_is_recognised_in_its_phases_and_in_finally() {
        let plain = plan_with(json!([step("a", json!({ "kind": "push", "branch": "x" }), "queued")]), json!([]));
        assert!(!merges_requests(&plain));
        let in_phase = plan_with(json!([step("m", json!({ "kind": "merge_mr", "number": 4 }), "queued")]), json!([]));
        assert!(merges_requests(&in_phase));
        let in_finally = plan_with(json!([]), json!([step("m", json!({ "kind": "merge_mr", "number": 4 }), "queued")]));
        assert!(merges_requests(&in_finally));
    }

    #[test]
    fn every_plan_tool_is_offered_and_named_for_the_card() {
        let names: Vec<String> = specs().into_iter().map(|t| t.name).collect();
        for t in PLAN_TOOLS {
            assert!(names.iter().any(|n| n == t), "{t} has no spec");
        }
        assert!(names.iter().any(|n| n == "plan_list") && is_plan_tool("plan_list") && !is_plan_tool("git_push"));
        for t in specs() {
            for r in t.input_schema["required"].as_array().unwrap() {
                assert!(t.input_schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
            }
        }
    }

    #[test]
    fn a_missing_argument_is_named() {
        assert_eq!(text(&json!({}), "plan_id").unwrap_err(), "`plan_id` is required");
        assert_eq!(text(&json!({"plan_id": "  "}), "plan_id").unwrap_err(), "`plan_id` is required");
        assert_eq!(text(&json!({"plan_id": "p"}), "plan_id").unwrap(), "p");
    }
}
