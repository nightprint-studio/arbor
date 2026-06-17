//! `IfBlock` — the `StepDef`-carrying structural piece of an `if/elif/else`.
//!
//! The condition evaluation core ([`crate::condition`]) and the free-form
//! parser ([`crate::condition_parser`]) are siblings; this module adds the
//! branch structure whose bodies are [`crate::model::StepDef`]s. The
//! orchestrator (host-side) walks the selected branch's steps.

use serde::{Deserialize, Serialize};

use crate::condition::{evaluate, Condition};
use crate::model::StepDef;
use crate::vars::RunContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfBranch {
    pub condition: Condition,
    #[serde(default)]
    pub steps: Vec<StepDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfBlock {
    /// The first branch is the `if`; any subsequent entries are `elif`s.
    /// At least one branch is required; an empty list short-circuits to
    /// the `else_steps` body.
    pub branches: Vec<IfBranch>,
    /// Optional `else` body — runs when no branch's condition matches.
    #[serde(default)]
    pub else_steps: Vec<StepDef>,
}

impl IfBlock {
    /// Pick the first matching branch and return its steps, or the
    /// `else_steps` if none matched. Returns a `&[StepDef]` directly so the
    /// orchestrator's nested executor can iterate without cloning anything.
    pub fn select<'a>(&'a self, ctx: &RunContext) -> (BranchSelection, &'a [StepDef]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition_parser::parse;
    use crate::vars::VarValue;

    fn step(id: &str) -> StepDef {
        StepDef {
            id: id.into(), name: id.into(), command: "true".into(),
            lua_op: None, builtin: None, if_block: None, cwd: None,
            allow_failure: false, env: Default::default(), capture: None,
        }
    }

    fn block() -> IfBlock {
        IfBlock {
            branches: vec![
                IfBranch { condition: parse("${a}").unwrap(), steps: vec![step("if-a")] },
                IfBranch { condition: parse("${b}").unwrap(), steps: vec![step("elif-b")] },
            ],
            else_steps: vec![step("else-c")],
        }
    }

    #[test]
    fn selects_first_matching_branch() {
        let mut ctx = RunContext::new();
        ctx.vars.insert("a".into(), VarValue::Bool(true));
        let b = block();
        let (sel, steps) = b.select(&ctx);
        assert_eq!(sel, BranchSelection::Branch(0));
        assert_eq!(steps[0].id, "if-a");
        assert_eq!(sel.label(), "if");
    }

    #[test]
    fn falls_through_to_elif() {
        let mut ctx = RunContext::new();
        ctx.vars.insert("b".into(), VarValue::Bool(true));
        let b = block();
        let (sel, steps) = b.select(&ctx);
        assert_eq!(sel, BranchSelection::Branch(1));
        assert_eq!(steps[0].id, "elif-b");
        assert_eq!(sel.label(), "elif #1");
    }

    #[test]
    fn falls_through_to_else_when_nothing_matches() {
        let ctx = RunContext::new();
        let b = block();
        let (sel, steps) = b.select(&ctx);
        assert_eq!(sel, BranchSelection::Else);
        assert_eq!(steps[0].id, "else-c");
        assert_eq!(sel.label(), "else");
    }
}
