//! Conditions for pipeline `if/elif/else` blocks.
//!
//! A condition is a small structured value tree (no parser, no DSL): leaf
//! nodes compare two operands or check a single value, combinators wrap
//! arbitrary children. Operands are arbitrary strings — typically `${var}`
//! references — and are resolved via `vars::resolve_vars` before the leaf
//! evaluates. That keeps the editor's job dead simple (3 dropdowns +
//! 1–2 text inputs per leaf) while still covering the realistic use cases.
//!
//! `IfBlock` lives on a `StepDef` — see `pipeline::mod` for how the
//! orchestrator dispatches it.

use serde::{Deserialize, Serialize};
use super::vars::{RunContext, VarValue, resolve_vars};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    /// String equality (case-sensitive).
    Eq,
    /// String inequality.
    Ne,
    /// Numeric `>`. Both sides must coerce to a number; otherwise false.
    Gt,
    /// Numeric `<`.
    Lt,
    /// Numeric `>=`.
    Gte,
    /// Numeric `<=`.
    Lte,
    /// `left.contains(right)` substring check.
    Contains,
    /// `right` is interpreted as a regex matched against `left`.
    Matches,
    /// Case-insensitive equality.
    IEq,
    /// `left.starts_with(right)`.
    StartsWith,
    /// `left.ends_with(right)`.
    EndsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    /// Two operands compared with `op`. Strings are resolved through
    /// `${var}` substitution before the comparison.
    Compare { left: String, op: CompareOp, right: String },
    /// Truthiness check on a single operand (after var resolution +
    /// fallback to the literal string).
    Truthy { value: String },
    /// `defined(var)` — true when the variable is present AND not Null.
    Defined { var: String },
    /// True when `value` (after resolution) is empty.
    Empty { value: String },
    /// Logical AND over an arbitrary set of children.
    AllOf { conditions: Vec<Condition> },
    /// Logical OR.
    AnyOf { conditions: Vec<Condition> },
    /// Logical NOT.
    Not { condition: Box<Condition> },
    /// Constant for skeleton/disabled branches in the editor.
    Always,
    Never,
    /// Free-form expression string parsed at evaluation time. Cheap parse
    /// (~µs) so we re-parse on every evaluation rather than carry mutable
    /// state through the Serialize/Deserialize boundary; the orchestrator
    /// only fires conditions a handful of times per run, so this is fine.
    /// Syntax + grammar live in `condition_parser.rs`. A parse failure
    /// degrades silently to `false` and a warning line in the run log.
    Expr { expr: String },
}

pub fn evaluate(c: &Condition, ctx: &RunContext) -> bool {
    match c {
        Condition::Always => true,
        Condition::Never  => false,
        Condition::Compare { left, op, right } => {
            let l = resolve_vars(left, ctx);
            let r = resolve_vars(right, ctx);
            evaluate_compare(&l, *op, &r)
        }
        Condition::Truthy { value } => {
            let resolved = resolve_vars(value, ctx);
            // If `value` was a single `${var}` reference, prefer the typed
            // variable's truthiness over the resolved string. That makes
            // `bool` and `number` captures behave intuitively.
            if let Some(name) = single_var_ref(value) {
                if let Some(v) = ctx.get(name) { return v.truthy(); }
            }
            VarValue::String(resolved).truthy()
        }
        Condition::Defined { var } => {
            ctx.get(var).map(|v| !matches!(v, VarValue::Null)).unwrap_or(false)
        }
        Condition::Empty { value } => {
            let s = resolve_vars(value, ctx);
            s.is_empty()
        }
        Condition::AllOf { conditions } => conditions.iter().all(|c| evaluate(c, ctx)),
        Condition::AnyOf { conditions } => conditions.iter().any(|c| evaluate(c, ctx)),
        Condition::Not   { condition }  => !evaluate(condition, ctx),
        Condition::Expr  { expr }       => match super::condition_parser::parse(expr) {
            Ok(parsed) => evaluate(&parsed, ctx),
            Err(e) => {
                tracing::warn!("if-block expression parse failed ({e}): {expr:?} — defaulting to false");
                false
            }
        },
    }
}

fn evaluate_compare(l: &str, op: CompareOp, r: &str) -> bool {
    match op {
        CompareOp::Eq         => l == r,
        CompareOp::Ne         => l != r,
        CompareOp::IEq        => l.eq_ignore_ascii_case(r),
        CompareOp::Contains   => l.contains(r),
        CompareOp::StartsWith => l.starts_with(r),
        CompareOp::EndsWith   => l.ends_with(r),
        CompareOp::Matches    => regex::Regex::new(r).map(|re| re.is_match(l)).unwrap_or(false),
        CompareOp::Gt | CompareOp::Lt | CompareOp::Gte | CompareOp::Lte => {
            let lf = l.trim().parse::<f64>().ok();
            let rf = r.trim().parse::<f64>().ok();
            match (lf, rf) {
                (Some(a), Some(b)) => match op {
                    CompareOp::Gt  => a >  b,
                    CompareOp::Lt  => a <  b,
                    CompareOp::Gte => a >= b,
                    CompareOp::Lte => a <= b,
                    _ => false,
                },
                _ => false,
            }
        }
    }
}

/// When `s` is exactly `"${name}"` (no surrounding text, no fallback),
/// return `name`. Used by `Condition::Truthy` to honor the variable's
/// underlying type instead of comparing the rendered string.
fn single_var_ref(s: &str) -> Option<&str> {
    let s = s.trim();
    if !s.starts_with("${") || !s.ends_with('}') { return None; }
    let inner = &s[2..s.len()-1];
    if inner.contains(['$', '{', '}', ':']) { return None; }
    Some(inner)
}

// ---------------------------------------------------------------------------
// IfBlock — the structural piece carried on a StepDef
// ---------------------------------------------------------------------------

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
    /// `else_steps` if none matched. Caller-friendly: returns a `&[StepDef]`
    /// directly so the orchestrator's nested executor can iterate without
    /// cloning anything.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RunContext {
        let mut c = RunContext::new();
        c.set("name", VarValue::String("arbor".into()));
        c.set("count", VarValue::Number(3.0));
        c.set("flag", VarValue::Bool(true));
        c
    }

    #[test]
    fn compare_eq() {
        let c = Condition::Compare {
            left: "${name}".into(), op: CompareOp::Eq, right: "arbor".into(),
        };
        assert!(evaluate(&c, &ctx()));
    }

    #[test]
    fn compare_gt_numeric() {
        let c = Condition::Compare {
            left: "${count}".into(), op: CompareOp::Gt, right: "1".into(),
        };
        assert!(evaluate(&c, &ctx()));
    }

    #[test]
    fn truthy_typed() {
        let c = Condition::Truthy { value: "${flag}".into() };
        assert!(evaluate(&c, &ctx()));
    }

    #[test]
    fn defined_excludes_null() {
        let mut x = ctx();
        x.set("nothing", VarValue::Null);
        assert!(!evaluate(&Condition::Defined { var: "nothing".into() }, &x));
        assert!(!evaluate(&Condition::Defined { var: "missing".into() }, &x));
        assert!( evaluate(&Condition::Defined { var: "name".into()    }, &x));
    }

    #[test]
    fn all_of_short_circuit() {
        let c = Condition::AllOf { conditions: vec![
            Condition::Always,
            Condition::Compare { left: "1".into(), op: CompareOp::Eq, right: "1".into() },
        ]};
        assert!(evaluate(&c, &ctx()));
    }
}
