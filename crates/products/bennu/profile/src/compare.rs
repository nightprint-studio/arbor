//! "Compare with the previous recording": per-method change in share of the run.
//!
//! Percentages rather than raw counts, because two recordings of the same scenario are rarely the
//! same length; a method that went from 10% to 25% of the run is the finding, whatever the
//! absolute sample counts were.

use serde::Serialize;
use std::collections::HashMap;

use crate::model::*;

#[derive(Debug, Clone, Serialize)]
pub struct MethodDelta {
    pub class: String,
    pub method: String,
    pub old_self_pct: f64,
    pub new_self_pct: f64,
    /// `new - old`, in percentage points of the run.
    pub delta_pp: f64,
    pub old_self_ms: f64,
    pub new_self_ms: f64,
}

fn pct(w: u64, total: u64) -> f64 {
    if total == 0 { 0.0 } else { w as f64 * 100.0 / total as f64 }
}

/// Every method present in either profile, biggest absolute change first.
pub fn compare(old: &Profile, new: &Profile, limit: usize) -> Vec<MethodDelta> {
    let key = |m: &MethodStat| format!("{}#{}/{}", m.class, m.method, m.arity.map_or(-1, |a| a as i64));
    let mut map: HashMap<String, MethodDelta> = HashMap::new();
    for m in &old.methods {
        let e = map.entry(key(m)).or_insert_with(|| blank(m));
        e.old_self_pct += pct(m.self_w, old.tree.total);
        e.old_self_ms += old.ms(m.self_w);
    }
    for m in &new.methods {
        let e = map.entry(key(m)).or_insert_with(|| blank(m));
        e.new_self_pct += pct(m.self_w, new.tree.total);
        e.new_self_ms += new.ms(m.self_w);
    }
    let mut v: Vec<MethodDelta> = map
        .into_values()
        .map(|mut d| {
            d.delta_pp = d.new_self_pct - d.old_self_pct;
            d
        })
        .collect();
    v.sort_by(|a, b| b.delta_pp.abs().total_cmp(&a.delta_pp.abs()));
    v.truncate(limit);
    v
}

fn blank(m: &MethodStat) -> MethodDelta {
    MethodDelta {
        class: m.class.clone(),
        method: m.method.clone(),
        old_self_pct: 0.0,
        new_self_pct: 0.0,
        delta_pp: 0.0,
        old_self_ms: 0.0,
        new_self_ms: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{RawFrame, TreeBuilder};

    fn profile(stacks: &[(&str, u64)]) -> Profile {
        let mut b = TreeBuilder::default();
        for (m, w) in stacks {
            b.add_stack(None, &[RawFrame::method("A", m)], *w);
        }
        let tree = b.finish();
        Profile {
            id: "p".into(),
            lang: Lang::Java,
            label: "p".into(),
            created_ms: 0,
            duration_ms: None,
            ms_per_unit: 10.0,
            methods: tree.method_stats(100),
            tree,
            alloc_sites: vec![],
            alloc_tree: Default::default(),
            wait_tree: Default::default(),
            waits: vec![],
            alloc_types: vec![],
            locks: vec![],
            gc: vec![],
            threads: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn share_of_run_is_compared_not_raw_counts() {
        let old = profile(&[("a", 50), ("b", 50)]);
        let new = profile(&[("a", 400), ("b", 100)]);
        let d = compare(&old, &new, 10);
        let a = d.iter().find(|x| x.method == "a").unwrap();
        let b = d.iter().find(|x| x.method == "b").unwrap();
        assert!((a.delta_pp - 30.0).abs() < 1e-9);
        assert!((b.delta_pp + 30.0).abs() < 1e-9);
    }
}
