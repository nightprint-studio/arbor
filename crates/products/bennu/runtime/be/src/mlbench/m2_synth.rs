//! M2 — how much of what people write at `=`, `return` and call arguments is reachable by composing known conversions.
//!
//! A type-driven synthesiser would enumerate expressions made of variables/fields in scope, constructors, static factories
//! and instance methods the index knows, up to some depth. This does not write it: it asks, for the expression that is
//! really there, whether it is *in that space* — every call/constructor/field in it typed by the index, every leaf a name
//! in scope or a literal, and its tree no deeper than N. That is the ceiling of what an enumerator of depth N could
//! return (the truth is type-correct by construction, so the type filter cannot lose it).
//!
//! `cargo test -p bennu-be mlbench::m2_synth -- --ignored --nocapture`
//! (`BENNU_MLB_N` sites per position and project, default 600; `BENNU_MLB_ONLY=<project>`)

use std::collections::BTreeMap;

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use super::common::*;
use super::sites::{shape_of, sites, Position, Shape};
use crate::index_service::IndexService;

/// What reaching one expression costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reach {
    /// Depth of the composition: a variable is 0, `v.m()` is 1, `f(v.m())` is 2.
    pub steps: usize,
    /// A literal other than the handful that need no choosing (`null`, `true`, `false`, `0`, `1`, `-1`, `""`).
    pub free_literal: bool,
}

/// Why an expression is outside the space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Miss {
    /// A piece the index cannot type: a library without sources, a generic it cannot bind.
    Unresolved,
    /// Operators, ternaries, arrays, anonymous classes: not a composition of conversions.
    NotComposition,
    /// A lambda or method reference: the body is the thing being written.
    Lambda,
}

const EASY_LITERALS: &[&str] = &["null", "true", "false", "0", "1", "-1", "\"\"", "0L", "1L", "0.0", "0.0d", "1.0"];

fn is_type_name(n: &Node, w: &str) -> bool {
    n.kind() == "identifier" && w[n.start_byte()..n.end_byte()].starts_with(|c: char| c.is_ascii_uppercase())
}

fn walk(n: &Node, w: &str, shift: usize, base: usize, typed: &dyn Fn(usize, usize) -> bool) -> Result<Reach, Miss> {
    let text = &w[n.start_byte()..n.end_byte()];
    let whole_typed = || typed(base + n.start_byte() - shift, base + n.end_byte() - shift);
    let own = |parts: Vec<Result<Reach, Miss>>| -> Result<Reach, Miss> {
        let mut r = Reach { steps: 1, free_literal: false };
        for p in parts {
            let p = p?;
            r.steps = r.steps.max(1 + p.steps);
            r.free_literal |= p.free_literal;
        }
        Ok(r)
    };
    let mut cursor = n.walk();
    let kids: Vec<Node> = n.children(&mut cursor).collect();
    match n.kind() {
        "this" | "super" => Ok(Reach { steps: 0, free_literal: false }),
        "identifier" => if whole_typed() { Ok(Reach { steps: 0, free_literal: false }) } else { Err(Miss::Unresolved) },
        "decimal_integer_literal" | "hex_integer_literal" | "decimal_floating_point_literal" | "string_literal" | "character_literal" | "true" | "false" | "null_literal" | "binary_integer_literal" | "octal_integer_literal" | "text_block" => {
            Ok(Reach { steps: 0, free_literal: !EASY_LITERALS.contains(&text) })
        }
        "unary_expression" if text.starts_with('-') && kids.len() == 2 && kids[1].kind().ends_with("literal") => Ok(Reach { steps: 0, free_literal: !EASY_LITERALS.contains(&text) }),
        "parenthesized_expression" => kids.iter().find(|c| c.is_named()).map_or(Err(Miss::NotComposition), |c| walk(c, w, shift, base, typed)),
        "cast_expression" => n.child_by_field_name("value").map_or(Err(Miss::NotComposition), |c| walk(&c, w, shift, base, typed)),
        "field_access" => {
            if !whole_typed() {
                return Err(Miss::Unresolved);
            }
            let o = n.child_by_field_name("object").ok_or(Miss::NotComposition)?;
            // `this.x` is a name in scope; `Type.CONST` and `a.b` are one step
            if o.kind() == "this" {
                Ok(Reach { steps: 0, free_literal: false })
            } else if is_type_name(&o, w) {
                Ok(Reach { steps: 1, free_literal: false })
            } else {
                own(vec![walk(&o, w, shift, base, typed)])
            }
        }
        "method_invocation" | "object_creation_expression" => {
            if !whole_typed() {
                return Err(Miss::Unresolved);
            }
            if kids.iter().any(|c| c.kind() == "class_body") {
                return Err(Miss::NotComposition);
            }
            let mut parts = Vec::new();
            if let Some(o) = n.child_by_field_name("object") {
                if !(is_type_name(&o, w) || matches!(o.kind(), "this" | "super")) {
                    parts.push(walk(&o, w, shift, base, typed));
                }
            }
            if let Some(args) = n.child_by_field_name("arguments") {
                let mut c2 = args.walk();
                let list: Vec<Node> = args.children(&mut c2).filter(|a| a.is_named() && !a.kind().ends_with("comment")).collect();
                for a in list {
                    if matches!(a.kind(), "lambda_expression" | "method_reference") {
                        return Err(Miss::Lambda);
                    }
                    parts.push(walk(&a, w, shift, base, typed));
                }
            }
            own(parts)
        }
        "lambda_expression" | "method_reference" => Err(Miss::Lambda),
        _ => Err(Miss::NotComposition),
    }
}

/// Prices `expr`, the text found at `base` in the buffer that `typed(start, end)` can type pieces of.
pub(super) fn reach(expr: &str, base: usize, typed: &dyn Fn(usize, usize) -> bool) -> Result<Reach, Miss> {
    let prefix = "class C { Object o = ";
    let wrapped = format!("{prefix}{expr}; }}");
    let Some(tree) = parse_java(&wrapped) else { return Err(Miss::NotComposition) };
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_declarator" {
            let value = n.child_by_field_name("value").ok_or(Miss::NotComposition)?;
            return walk(&value, &wrapped, prefix.len(), base, typed);
        }
        let mut c = n.walk();
        stack.extend(n.children(&mut c).collect::<Vec<_>>());
    }
    Err(Miss::NotComposition)
}

#[derive(Default, Clone)]
struct Cell {
    n: usize,
    within: [usize; 4],       // strict (no free literal) at depth <= 0,1,2,3
    lenient: [usize; 4],      // literals allowed as holes
    misses: BTreeMap<Miss, usize>,
}

impl Cell {
    fn add(&mut self, r: Result<Reach, Miss>) {
        self.n += 1;
        match r {
            Ok(r) => (0..4).filter(|&d| r.steps <= d).for_each(|d| {
                self.lenient[d] += 1;
                self.within[d] += usize::from(!r.free_literal);
            }),
            Err(m) => *self.misses.entry(m).or_default() += 1,
        }
    }
}

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn m2_synthesis_coverage() {
    let wanted = env_num("BENNU_MLB_N", 600);
    let only = env("BENNU_MLB_ONLY");
    let mut table: BTreeMap<(Position, Shape), Cell> = BTreeMap::new();
    let mut by_position: BTreeMap<Position, Cell> = BTreeMap::new();
    let mut ambiguity: Vec<usize> = Vec::new();

    for spec in PROJECTS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        let (root, held) = assemble(spec);
        open_and_wait(&root);
        let svc = IndexService::global();
        let mut per_position: BTreeMap<Position, Vec<(usize, super::sites::Site)>> = BTreeMap::new();
        for (hi, h) in held.iter().enumerate() {
            for s in sites(&h.text) {
                if s.position != Position::MemberAfterDot && s.end > s.start {
                    per_position.entry(s.position).or_default().push((hi, s));
                }
            }
        }
        for (pos, all) in per_position {
            let stride = (all.len() / wanted.max(1)).max(1);
            for (hi, s) in all.iter().step_by(stride).take(wanted) {
                let h = &held[*hi];
                let file = h.path.to_string_lossy().into_owned();
                let expr = &h.text[s.start..s.end];
                let shape = shape_of(expr);
                let typed = |a: usize, b: usize| svc.expression_type(&file, &h.text, a, b).is_some();
                let r = reach(expr, s.start, &typed);
                table.entry((pos, shape)).or_default().add(r);
                by_position.entry(pos).or_default().add(r);
                // how many variables of the declared type stand next to a variable-shaped answer: the ranking left to do
                if shape == Shape::Variable {
                    if let Some(ty) = s.declared.as_deref().map(|t| t.split('<').next().unwrap_or(t).trim().to_string()) {
                        if let Some(vars) = crate::flc_member_args::scope_candidates(&file, &h.text, s.start) {
                            ambiguity.push(vars.iter().filter(|v| v.types.iter().any(|t| *t == ty)).count());
                        }
                    }
                }
            }
        }
        svc.forget(&root.to_string_lossy());
    }
    let report = report(&table, &by_position, &ambiguity);
    let md = write_result("m2_report.md", &report);
    eprintln!("{report}\nreport: {}", md.display());
}

fn row(label: &str, c: &Cell, of: usize) -> String {
    let p = |x: usize| format!("{:.1}%", pct(x, c.n));
    format!(
        "| {label} | {} | {:.1}% | {} / {} | {} / {} | {} / {} | {} | {} | {} |\n",
        c.n,
        pct(c.n, of),
        p(c.within[0]), p(c.lenient[0]),
        p(c.within[1]), p(c.lenient[1]),
        p(c.within[2]), p(c.lenient[2]),
        p(c.within[3]),
        pct(c.misses.get(&Miss::Unresolved).copied().unwrap_or(0), c.n).round(),
        pct(c.misses.get(&Miss::NotComposition).copied().unwrap_or(0) + c.misses.get(&Miss::Lambda).copied().unwrap_or(0), c.n).round(),
    )
}

fn report(table: &BTreeMap<(Position, Shape), Cell>, by_position: &BTreeMap<Position, Cell>, ambiguity: &[usize]) -> String {
    let head = "| | sites | share | depth 0 strict / +literals | depth ≤1 | depth ≤2 | depth ≤3 strict | unresolved % | not composition % |\n|---|---|---|---|---|---|---|---|---|\n";
    let mut out = String::from("## M2 — coverage of a type-driven enumerator\n\n");
    out.push_str("`strict` = every leaf is a name in scope or a trivial literal; `+literals` = any literal counts as a hole the enumerator fills.\n\n");
    out.push_str(head);
    for (pos, c) in by_position {
        out.push_str(&row(&format!("**{}**", pos.label()), c, c.n));
        let mut shapes: Vec<_> = table.iter().filter(|((p, _), _)| p == pos).collect();
        shapes.sort_by(|a, b| b.1.n.cmp(&a.1.n));
        for ((_, shape), cell) in shapes {
            out.push_str(&row(&format!("&nbsp;&nbsp;{}", shape.label()), cell, c.n));
        }
    }
    let mut total = Cell::default();
    let mut shares: BTreeMap<Shape, usize> = BTreeMap::new();
    for ((_, shape), c) in table {
        total.n += c.n;
        for d in 0..4 {
            total.within[d] += c.within[d];
            total.lenient[d] += c.lenient[d];
        }
        for (m, k) in &c.misses {
            *total.misses.entry(*m).or_default() += k;
        }
        *shares.entry(*shape).or_default() += c.n;
    }
    out.push_str(&row("**all three positions**", &total, total.n));
    out.push_str("\n### Shape of the right-hand side (all positions)\n\n| shape | share |\n|---|---|\n");
    let mut sh: Vec<_> = shares.into_iter().collect();
    sh.sort_by(|a, b| b.1.cmp(&a.1));
    for (s, n) in sh {
        out.push_str(&format!("| {} | {:.1}% |\n", s.label(), pct(n, total.n)));
    }
    if !ambiguity.is_empty() {
        let mut a = ambiguity.to_vec();
        a.sort();
        let one = a.iter().filter(|&&x| x == 1).count();
        out.push_str(&format!(
            "\nVariable-shaped answers with a declared type ({} sites): local variables and parameters of that type in scope (fields not counted) — median {}, p90 {}, exactly one in {:.1}% of them.\n",
            a.len(), a[a.len() / 2], a[a.len() * 9 / 10], pct(one, a.len())
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_typed(_: usize, _: usize) -> bool {
        true
    }

    #[test]
    fn a_name_is_free_a_call_costs_a_step_and_nesting_adds() {
        assert_eq!(reach("name", 0, &all_typed), Ok(Reach { steps: 0, free_literal: false }));
        assert_eq!(reach("list.size()", 0, &all_typed).unwrap().steps, 1);
        assert_eq!(reach("new Foo(a)", 0, &all_typed).unwrap().steps, 1);
        assert_eq!(reach("Objects.requireNonNull(a.b())", 0, &all_typed).unwrap().steps, 2);
    }

    #[test]
    fn a_free_literal_is_flagged_but_null_is_not() {
        assert!(reach("foo(\"text\")", 0, &all_typed).unwrap().free_literal);
        assert!(!reach("foo(null, 0)", 0, &all_typed).unwrap().free_literal);
    }

    #[test]
    fn operators_lambdas_and_untyped_pieces_are_misses() {
        assert_eq!(reach("a + b", 0, &all_typed), Err(Miss::NotComposition));
        assert_eq!(reach("x -> x", 0, &all_typed), Err(Miss::Lambda));
        assert_eq!(reach("foo(x -> x)", 0, &all_typed), Err(Miss::Lambda));
        assert_eq!(reach("missing", 0, &|_, _| false), Err(Miss::Unresolved));
    }
}
