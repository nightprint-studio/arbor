//! `IfBlock` — the `StepDef`-carrying structural piece of an `if/elif/else`.
//!
//! The condition evaluation core (`Condition`, `CompareOp`, `evaluate`), the
//! free-form parser, and the run-variable engine now live in the
//! `corvus-pipeline-api` crate. They're re-exported here so existing
//! `crate::pipeline::condition::*` call sites keep resolving. `IfBlock` stays in
//! the shell because its branch bodies are `super::StepDef`s (the step model).

use serde::{Deserialize, Serialize};

pub use corvus_pipeline_api::prelude::{evaluate, CompareOp, Condition};
use corvus_pipeline_api::prelude::RunContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfBranch {
    pub condition: Condition,
    #[serde(default)]
    pub steps: Vec<super::StepDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfBlock {
    /// The first branch is the `if`; any subsequent entries are `elif`s.
    /// At least one branch is required; an empty list short-circuits to
    /// the `else_steps` body.
    pub branches: Vec<IfBranch>,
    /// Optional `else` body — runs when no branch's condition matches.
    #[serde(default)]
    pub else_steps: Vec<super::StepDef>,
}

impl IfBlock {
    /// Pick the first matching branch and return its steps, or the
    /// `else_steps` if none matched. Returns a `&[StepDef]` directly so the
    /// orchestrator's nested executor can iterate without cloning anything.
    pub fn select<'a>(&'a self, ctx: &RunContext) -> (BranchSelection, &'a [super::StepDef]) {
        for (i, br) in self.branches.iter().enumerate() {
            if evaluate(&br.condition, ctx) {
                return (BranchSelection::Branch(i), &br.steps);
            }
        }
        (BranchSelection::Else, &self.else_steps)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSelection {
    /// Index into `IfBlock.branches` — 0 means the `if`, 1+ means `elif`s.
    Branch(usize),
    /// No branch matched — the `else_steps` body was selected (which may
    /// itself be empty).
    Else,
}

impl BranchSelection {
    pub fn label(&self) -> String {
        match self {
            BranchSelection::Branch(0) => "if".into(),
            BranchSelection::Branch(i) => format!("elif #{}", i),
            BranchSelection::Else      => "else".into(),
        }
    }
}
