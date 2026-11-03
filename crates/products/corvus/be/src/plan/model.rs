//! The plan model: a sequence of **phases**, each a set of remote git steps that start together,
//! joined by **gates** that say when the next phase may begin. Pure data + the rules that read it
//! (which phase is next, which steps share a repo, whether the plan is coherent) — no I/O, so the
//! scheduling rules are unit-testable without a repo or a provider.
//!
//! A phase never starts while an earlier one has an unsettled step, whatever its gate says: the
//! gate is an *additional* condition (an MR merged, a delay, a human go-ahead), never a way to
//! skip the previous phase. The `finally` steps sit outside that chain and run when it ends.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A repository a step acts on. The path is what the executor opens; name and workspace are only
/// carried so the UI can draw the card without a registry lookup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoRef {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// What a step does. Only remote, ordered operations belong here: a local action that captures
/// the working tree *now* (a commit) cannot be deferred without changing its meaning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepSpec {
    Push {
        branch: String,
    },
    CreateMr {
        source: String,
        target: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        auto_merge: bool,
        #[serde(default)]
        squash: bool,
        #[serde(default)]
        delete_branch: bool,
    },
    /// Merges an MR: either one created by an earlier `CreateMr` step (`from_step`) or an
    /// existing one by `number`.
    MergeMr {
        #[serde(default)]
        from_step: Option<String>,
        #[serde(default)]
        number: Option<u64>,
        #[serde(default)]
        squash: bool,
        #[serde(default)]
        delete_branch: bool,
    },
    RunPipeline {
        branch: String,
        #[serde(default)]
        workflow_id: Option<String>,
        #[serde(default)]
        variables: Vec<(String, String)>,
    },
    /// Creates a tag on `target` (a branch or commit; the checked-out commit when empty) and pushes it.
    Tag {
        name: String,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        target: Option<String>,
    },
    Pull {},
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    #[default]
    Queued,
    /// Not reached yet because an earlier phase stopped the plan.
    Held,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub id: String,
    #[serde(flatten)]
    pub spec: StepSpec,
    pub repo: RepoRef,
    #[serde(default)]
    pub status: StepStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub started_ms: Option<u64>,
    #[serde(default)]
    pub finished_ms: Option<u64>,
    /// What the step produced that later steps may need (an MR's `number` and `url`).
    #[serde(default)]
    pub result: Option<Value>,
}

impl Step {
    /// Done or skipped: nothing left to run, and a later phase may rely on it.
    pub fn is_settled(&self) -> bool {
        matches!(self.status, StepStatus::Done | StepStatus::Skipped)
    }

    /// One log line's worth of "what this step does", e.g. `push feature/x (api-gateway)`.
    pub fn label(&self) -> String {
        let what = match &self.spec {
            StepSpec::Push { branch } => format!("push {branch}"),
            StepSpec::CreateMr { source, target, .. } => format!("create MR {source} → {target}"),
            StepSpec::MergeMr { .. } => "merge MR".to_string(),
            StepSpec::RunPipeline { branch, .. } => format!("run pipeline on {branch}"),
            StepSpec::Tag { name, .. } => format!("tag {name}"),
            StepSpec::Pull {} => "pull".to_string(),
        };
        format!("{what} ({})", self.repo.name)
    }
}

fn default_poll_secs() -> u64 {
    30
}
fn default_timeout_secs() -> u64 {
    2 * 60 * 60
}

/// The extra condition for a phase to start, on top of the previous phase having settled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateSpec {
    PhaseDone,
    /// Waits for the MR a `CreateMr` step of an earlier phase opened to be merged (by anyone).
    MrMerged {
        step_id: String,
        #[serde(default = "default_poll_secs")]
        poll_secs: u64,
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
    },
    Delay {
        secs: u64,
    },
    /// Stops the plan until the user releases the gate.
    Manual,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    #[default]
    Idle,
    Waiting,
    Open,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gate {
    #[serde(flatten)]
    pub spec: GateSpec,
    #[serde(default)]
    pub state: GateState,
    /// When the gate started waiting, for "waiting for 3m 12s".
    #[serde(default)]
    pub since_ms: Option<u64>,
}

impl Default for Gate {
    fn default() -> Self {
        Gate { spec: GateSpec::PhaseDone, state: GateState::Idle, since_ms: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Phase {
    pub id: String,
    pub name: String,
    /// Ignored on the first phase (nothing precedes it).
    #[serde(default)]
    pub gate: Gate,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    Draft,
    Running,
    /// Stopped on its own because it needs the user: a failed step or a manual gate.
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl PlanStatus {
    /// Whether the plan's steps may be edited (a running plan owns its own statuses).
    pub fn is_editable(self) -> bool {
        !matches!(self, PlanStatus::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub phases: Vec<Phase>,
    /// Steps that run once the phases have ended — also after the user stops a paused plan.
    #[serde(default)]
    pub finally: Vec<Step>,
    #[serde(default)]
    pub status: PlanStatus,
    #[serde(default)]
    pub pause_reason: Option<String>,
    /// A template is never run in place: it is the shape a new plan is copied from.
    #[serde(default)]
    pub template: bool,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub updated_ms: u64,
}

impl Plan {
    pub fn find_step(&self, id: &str) -> Option<&Step> {
        self.phases.iter().flat_map(|p| p.steps.iter()).chain(self.finally.iter()).find(|s| s.id == id)
    }

    pub fn find_step_mut(&mut self, id: &str) -> Option<&mut Step> {
        self.phases.iter_mut().flat_map(|p| p.steps.iter_mut()).chain(self.finally.iter_mut()).find(|s| s.id == id)
    }

    fn steps_mut(&mut self) -> impl Iterator<Item = &mut Step> {
        self.phases.iter_mut().flat_map(|p| p.steps.iter_mut()).chain(self.finally.iter_mut())
    }

    /// Back to a plan nobody has run: every step queued, every gate idle.
    pub fn reset(&mut self) {
        for s in self.steps_mut() {
            s.status = StepStatus::Queued;
            s.error = None;
            s.started_ms = None;
            s.finished_ms = None;
            s.result = None;
        }
        for p in &mut self.phases {
            p.gate.state = GateState::Idle;
            p.gate.since_ms = None;
        }
        self.pause_reason = None;
        self.status = PlanStatus::Draft;
    }

    /// Get ready to run (or resume): what is settled stays settled, everything else is queued
    /// again — a failed step is retried, a held one released — and gates that were not opened wait
    /// afresh.
    pub fn prepare_run(&mut self) {
        for s in self.steps_mut() {
            if !s.is_settled() {
                s.status = StepStatus::Queued;
                s.error = None;
            }
        }
        for p in &mut self.phases {
            if p.gate.state != GateState::Open {
                p.gate.state = GateState::Idle;
                p.gate.since_ms = None;
            }
        }
        self.pause_reason = None;
        self.status = PlanStatus::Running;
    }

    /// A phase is complete when all its steps are settled and, past the first, its gate is open.
    pub fn phase_complete(&self, index: usize) -> bool {
        let Some(p) = self.phases.get(index) else { return true };
        p.steps.iter().all(Step::is_settled) && (index == 0 || p.gate.state == GateState::Open)
    }

    /// The first phase that is not complete — the one the executor works on next.
    pub fn first_open_phase(&self) -> Option<usize> {
        (0..self.phases.len()).find(|&i| !self.phase_complete(i))
    }

    /// After phase `index` stopped the plan, the steps of every later phase are not going to run
    /// until it resumes: show them as held rather than queued.
    pub fn hold_after(&mut self, index: usize) {
        for p in self.phases.iter_mut().skip(index + 1) {
            for s in &mut p.steps {
                if s.status == StepStatus::Queued {
                    s.status = StepStatus::Held;
                }
            }
        }
    }

    /// The number of the MR a `CreateMr` step opened, once it has run.
    pub fn mr_number_of(&self, step_id: &str) -> Option<u64> {
        self.find_step(step_id)?.result.as_ref()?.get("number")?.as_u64()
    }

    /// Whether the plan is coherent enough to run: ids are unique and every reference to an
    /// earlier step points at a `CreateMr` of the same repo in a strictly earlier phase.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for id in self.phases.iter().map(|p| &p.id).chain(self.phases.iter().flat_map(|p| p.steps.iter()).chain(self.finally.iter()).map(|s| &s.id)) {
            if !seen.insert(id) {
                return Err(format!("duplicate id '{id}' in plan '{}'", self.name));
            }
        }
        for (pi, phase) in self.phases.iter().enumerate() {
            if pi > 0 {
                if let GateSpec::MrMerged { step_id, .. } = &phase.gate.spec {
                    self.check_mr_source(step_id, pi, None)?;
                }
            }
            for step in &phase.steps {
                self.check_step(step, pi)?;
            }
        }
        // `finally` sits after every phase, so every phase is "earlier" for it.
        for step in &self.finally {
            self.check_step(step, self.phases.len())?;
        }
        Ok(())
    }

    fn check_step(&self, step: &Step, phase_index: usize) -> Result<(), String> {
        if let StepSpec::MergeMr { from_step, number, .. } = &step.spec {
            match (from_step, number) {
                (Some(src), _) => self.check_mr_source(src, phase_index, Some(&step.repo))?,
                (None, Some(_)) => {}
                (None, None) => return Err(format!("{}: pick the MR to merge", step.label())),
            }
        }
        Ok(())
    }

    fn check_mr_source(&self, step_id: &str, before_phase: usize, repo: Option<&RepoRef>) -> Result<(), String> {
        let found = self
            .phases
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.steps.iter().find(|s| s.id == step_id).map(|s| (i, s)));
        match found {
            Some((i, s)) if i < before_phase && matches!(s.spec, StepSpec::CreateMr { .. }) => {
                match repo {
                    Some(r) if r.path != s.repo.path => Err(format!("{}: the MR belongs to another repository", s.label())),
                    _ => Ok(()),
                }
            }
            Some(_) => Err("a step can only depend on an MR created in an earlier phase".to_string()),
            None => Err(format!("unknown step '{step_id}'")),
        }
    }
}

/// Indices of `steps` grouped by repository, in order. Steps of one repo run one after the other
/// (two operations on the same index would race); different repos run side by side.
pub fn group_by_repo(steps: &[Step]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        match groups.iter_mut().find(|(path, _)| *path == s.repo.path) {
            Some((_, g)) => g.push(i),
            None => groups.push((&s.repo.path, vec![i])),
        }
    }
    groups.into_iter().map(|(_, g)| g).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> RepoRef {
        RepoRef { path: format!("/w/{name}"), name: name.into(), workspace: None }
    }

    fn step(id: &str, repo_name: &str, spec: StepSpec) -> Step {
        Step { id: id.into(), spec, repo: repo(repo_name), status: StepStatus::Queued, error: None, started_ms: None, finished_ms: None, result: None }
    }

    fn push(id: &str, repo_name: &str) -> Step {
        step(id, repo_name, StepSpec::Push { branch: "main".into() })
    }

    fn mr(id: &str, repo_name: &str) -> Step {
        step(id, repo_name, StepSpec::CreateMr {
            source: "f".into(), target: "main".into(), title: "t".into(), description: String::new(),
            labels: vec![], draft: false, auto_merge: false, squash: false, delete_branch: false,
        })
    }

    fn phase(id: &str, steps: Vec<Step>) -> Phase {
        Phase { id: id.into(), name: id.into(), gate: Gate::default(), steps }
    }

    fn plan(phases: Vec<Phase>) -> Plan {
        Plan { id: "p".into(), name: "p".into(), phases, finally: vec![], status: PlanStatus::Draft, pause_reason: None, template: false, created_ms: 0, updated_ms: 0 }
    }

    #[test]
    fn first_open_phase_follows_settled_steps_and_gates() {
        let mut p = plan(vec![phase("a", vec![push("s1", "x")]), phase("b", vec![push("s2", "x")])]);
        assert_eq!(p.first_open_phase(), Some(0));
        p.phases[0].steps[0].status = StepStatus::Done;
        // Phase b's steps are queued and its gate is not open.
        assert_eq!(p.first_open_phase(), Some(1));
        p.phases[1].gate.state = GateState::Open;
        p.phases[1].steps[0].status = StepStatus::Skipped;
        assert_eq!(p.first_open_phase(), None);
    }

    #[test]
    fn a_closed_gate_keeps_an_empty_phase_open() {
        let p = plan(vec![phase("a", vec![]), phase("b", vec![])]);
        assert_eq!(p.first_open_phase(), Some(1));
    }

    #[test]
    fn prepare_run_keeps_settled_steps_and_retries_failed_ones() {
        let mut p = plan(vec![phase("a", vec![push("s1", "x"), push("s2", "y")])]);
        p.phases[0].steps[0].status = StepStatus::Done;
        p.phases[0].steps[1].status = StepStatus::Failed;
        p.phases[0].steps[1].error = Some("boom".into());
        p.prepare_run();
        assert_eq!(p.phases[0].steps[0].status, StepStatus::Done);
        assert_eq!(p.phases[0].steps[1].status, StepStatus::Queued);
        assert_eq!(p.phases[0].steps[1].error, None);
        assert_eq!(p.status, PlanStatus::Running);
    }

    #[test]
    fn hold_after_only_touches_queued_steps_of_later_phases() {
        let mut p = plan(vec![phase("a", vec![push("s1", "x")]), phase("b", vec![push("s2", "x")])]);
        p.hold_after(0);
        assert_eq!(p.phases[0].steps[0].status, StepStatus::Queued);
        assert_eq!(p.phases[1].steps[0].status, StepStatus::Held);
    }

    #[test]
    fn steps_group_by_repo_in_order() {
        let steps = vec![push("1", "a"), push("2", "b"), push("3", "a")];
        assert_eq!(group_by_repo(&steps), vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let p = plan(vec![phase("a", vec![push("s", "x"), push("s", "y")])]);
        assert!(p.validate().unwrap_err().contains("duplicate"));
    }

    #[test]
    fn merge_mr_must_follow_the_create_mr_of_the_same_repo() {
        let merge = |from: &str, repo_name: &str| step("m", repo_name, StepSpec::MergeMr { from_step: Some(from.into()), number: None, squash: false, delete_branch: false });
        let ok = plan(vec![phase("a", vec![mr("c", "x")]), phase("b", vec![merge("c", "x")])]);
        assert!(ok.validate().is_ok());
        let same_phase = plan(vec![phase("a", vec![mr("c", "x"), merge("c", "x")])]);
        assert!(same_phase.validate().is_err());
        let other_repo = plan(vec![phase("a", vec![mr("c", "x")]), phase("b", vec![merge("c", "y")])]);
        assert!(other_repo.validate().unwrap_err().contains("another repository"));
        let neither = plan(vec![phase("a", vec![step("m", "x", StepSpec::MergeMr { from_step: None, number: None, squash: false, delete_branch: false })])]);
        assert!(neither.validate().is_err());
    }

    #[test]
    fn an_mr_merged_gate_needs_an_earlier_create_mr() {
        let mut p = plan(vec![phase("a", vec![mr("c", "x")]), phase("b", vec![push("s", "x")])]);
        p.phases[1].gate.spec = GateSpec::MrMerged { step_id: "c".into(), poll_secs: 30, timeout_secs: 60 };
        assert!(p.validate().is_ok());
        p.phases[1].gate.spec = GateSpec::MrMerged { step_id: "nope".into(), poll_secs: 30, timeout_secs: 60 };
        assert!(p.validate().is_err());
    }

    #[test]
    fn json_round_trip_keeps_the_flattened_kinds() {
        let mut p = plan(vec![phase("a", vec![mr("c", "x")])]);
        p.phases[0].gate = Gate { spec: GateSpec::Delay { secs: 5 }, state: GateState::Waiting, since_ms: Some(1) };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["phases"][0]["steps"][0]["kind"], "create_mr");
        assert_eq!(json["phases"][0]["gate"]["kind"], "delay");
        let back: Plan = serde_json::from_value(json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn mr_number_is_read_from_the_step_result() {
        let mut p = plan(vec![phase("a", vec![mr("c", "x")])]);
        assert_eq!(p.mr_number_of("c"), None);
        p.phases[0].steps[0].result = Some(serde_json::json!({ "number": 482 }));
        assert_eq!(p.mr_number_of("c"), Some(482));
    }
}
