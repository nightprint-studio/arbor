//! Totality check: grove forbids recursion so evaluation always terminates
//! (`design/grove/semantics.md`). We build the `fn` call graph and reject any
//! cycle (direct or mutual) before evaluating.

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, ExprKind, Item, Program};
use crate::error::{LangError, LangErrorKind, Result};

/// Reject the program if its `fn` definitions form a call cycle.
pub fn check(program: &Program) -> Result<()> {
    let names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Fn(f) => Some(f.name.name.clone()),
            _ => None,
        })
        .collect();

    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for item in &program.items {
        if let Item::Fn(f) = item {
            let mut refs = Vec::new();
            collect_refs(&f.body, &mut refs);
            let edges = refs.into_iter().filter(|r| names.contains(r)).collect();
            graph.insert(f.name.name.clone(), edges);
        }
    }

    let mut state: HashMap<String, u8> = HashMap::new(); // 0 = open, 1 = on-stack, 2 = done
    let mut path: Vec<String> = Vec::new();
    for name in graph.keys() {
        dfs(name, &graph, &mut state, &mut path)?;
    }
    Ok(())
}

fn dfs(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    state: &mut HashMap<String, u8>,
    path: &mut Vec<String>,
) -> Result<()> {
    match state.get(node).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => {
            let mut chain = path.clone();
            chain.push(node.to_string());
            return Err(LangError::unlocated(LangErrorKind::Recursion(trim_to_cycle(
                chain, node,
            ))));
        }
        _ => {}
    }
    state.insert(node.to_string(), 1);
    path.push(node.to_string());
    if let Some(edges) = graph.get(node) {
        for e in edges {
            dfs(e, graph, state, path)?;
        }
    }
    path.pop();
    state.insert(node.to_string(), 2);
    Ok(())
}

/// Keep only the part of the chain from the cycle's start onward.
fn trim_to_cycle(chain: Vec<String>, node: &str) -> Vec<String> {
    match chain.iter().position(|n| n == node) {
        Some(pos) => chain[pos..].to_vec(),
        None => chain,
    }
}

/// Collect identifier and call names referenced by an expression (for graph
/// edges). Method names are stdlib transforms, never user functions, so they're
/// skipped; islands don't call functions.
fn collect_refs(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Var(name) => out.push(name.clone()),
        ExprKind::Call { name, args } => {
            out.push(name.name.clone());
            for a in args {
                collect_refs(a, out);
            }
        }
        ExprKind::Method { recv, args, .. } => {
            collect_refs(recv, out);
            for a in args {
                collect_refs(a, out);
            }
        }
        ExprKind::Unary { rhs, .. } => collect_refs(rhs, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_refs(lhs, out);
            collect_refs(rhs, out);
        }
        ExprKind::Range { lo, hi, .. } => {
            collect_refs(lo, out);
            collect_refs(hi, out);
        }
        ExprKind::Lambda { body, .. } => collect_refs(body, out),
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Island(_) => {}
    }
}
