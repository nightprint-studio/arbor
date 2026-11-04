//! `bennu_dependency_conflicts` — libraries resolved to a version somebody did not ask for, read off
//! the resolved dependency tree (Maven's own mediation, Gradle's `a -> b`, cargo's duplicate crates).

use std::collections::BTreeMap;

use bennu_deps::prelude::{ModuleTree, TreeNode};
use serde::Serialize;

use super::has;

/// Hops shown in a "who asked for it" chain.
const MAX_CHAIN: usize = 6;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Request {
    pub version: String,
    /// The chain of dependencies that asked for it, from the module's direct dependency down.
    pub via: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ConflictRow {
    /// `group:artifact`.
    pub dependency: String,
    pub module: String,
    /// The version that won.
    pub resolved: String,
    /// The other versions somebody asked for, and who.
    pub requested: Vec<Request>,
}

#[derive(Debug, Default)]
pub struct ConflictFilter {
    pub dependency: Option<String>,
}

fn chain(nodes: &[TreeNode], mut at: usize) -> Vec<String> {
    let mut out = vec![nodes[at].coord()];
    // `parent` indexes the same list and points backwards, so this ends; the bound is for a malformed tree.
    while let Some(p) = nodes[at].parent {
        if out.len() >= MAX_CHAIN || p >= at {
            break;
        }
        out.push(nodes[p].coord());
        at = p;
    }
    out.reverse();
    out
}

/// Conflicts inside each module, from the nodes Maven or Gradle dropped (`omitted == "conflict"`), grouped
/// by dependency. A tree resolved without the verbose flag has no dropped nodes and reports none.
pub fn conflicts(modules: &[ModuleTree], filter: &ConflictFilter) -> Vec<ConflictRow> {
    let mut out = Vec::new();
    for m in modules {
        let mut by_dep: BTreeMap<String, ConflictRow> = BTreeMap::new();
        for (i, n) in m.nodes.iter().enumerate().filter(|(_, n)| n.omitted == "conflict") {
            let coord = n.coord();
            if !has(&coord, &filter.dependency) {
                continue;
            }
            let row = by_dep.entry(coord.clone()).or_insert_with(|| ConflictRow {
                dependency: coord,
                module: m.module.clone(),
                resolved: n.kept_version.clone(),
                requested: Vec::new(),
            });
            if !row.requested.iter().any(|r| r.version == n.version) {
                row.requested.push(Request { version: n.version.clone(), via: chain(&m.nodes, i) });
            }
        }
        out.extend(by_dep.into_values());
    }
    out
}

/// Dependencies a multi-module build resolves to different versions in different modules — invisible
/// inside any one tree, and the usual reason "it works in module A but not in B".
pub fn skew(modules: &[ModuleTree], filter: &ConflictFilter) -> Vec<ConflictRow> {
    let mut seen: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for m in modules {
        for n in m.nodes.iter().filter(|n| n.omitted.is_empty()) {
            let coord = n.coord();
            if !has(&coord, &filter.dependency) {
                continue;
            }
            let modules_at = seen.entry(coord).or_default().entry(n.version.clone()).or_default();
            if !modules_at.contains(&m.module) {
                modules_at.push(m.module.clone());
            }
        }
    }
    seen.into_iter()
        .filter(|(_, versions)| versions.len() > 1)
        .map(|(dependency, versions)| {
            // The most widely used version is the reference; the others are what differs from it.
            let resolved = versions.iter().max_by_key(|(_, mods)| mods.len()).map(|(v, _)| v.clone()).unwrap_or_default();
            let requested = versions
                .into_iter()
                .filter(|(v, _)| *v != resolved)
                .map(|(version, mods)| Request { version, via: mods })
                .collect();
            ConflictRow { dependency, module: "(across modules)".into(), resolved, requested }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, version: &str, parent: Option<usize>, omitted: &str, kept: &str) -> TreeNode {
        TreeNode {
            group: "org.x".into(),
            name: name.into(),
            version: version.into(),
            depth: parent.map_or(1, |_| 2),
            parent,
            omitted: omitted.into(),
            kept_version: kept.into(),
            ..TreeNode::default()
        }
    }

    fn module(name: &str, nodes: Vec<TreeNode>) -> ModuleTree {
        ModuleTree { module: name.into(), version: "1".into(), nodes }
    }

    #[test]
    fn a_dropped_version_is_reported_with_who_asked_for_it() {
        let m = module("app", vec![node("web", "5", None, "", ""), node("log", "1.2", Some(0), "conflict", "1.3"), node("log", "1.3", None, "", "")]);
        let rows = conflicts(&[m], &ConflictFilter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].dependency.as_str(), rows[0].resolved.as_str()), ("org.x:log", "1.3"));
        assert_eq!(rows[0].requested, vec![Request { version: "1.2".into(), via: vec!["org.x:web".into(), "org.x:log".into()] }]);
    }

    #[test]
    fn duplicates_are_not_conflicts_and_the_same_version_is_listed_once() {
        let m = module(
            "app",
            vec![node("a", "1", None, "duplicate", ""), node("b", "1", None, "conflict", "2"), node("b", "1", None, "conflict", "2")],
        );
        let rows = conflicts(&[m], &ConflictFilter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requested.len(), 1);
    }

    #[test]
    fn the_filter_narrows_by_dependency() {
        let m = module("app", vec![node("b", "1", None, "conflict", "2")]);
        assert!(conflicts(&[m], &ConflictFilter { dependency: Some("zzz".into()) }).is_empty());
    }

    #[test]
    fn skew_names_the_modules_that_differ_from_the_majority() {
        let a = module("a", vec![node("json", "2", None, "", "")]);
        let b = module("b", vec![node("json", "2", None, "", "")]);
        let c = module("c", vec![node("json", "1", None, "", "")]);
        let rows = skew(&[a, b, c], &ConflictFilter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].resolved, "2");
        assert_eq!(rows[0].requested, vec![Request { version: "1".into(), via: vec!["c".into()] }]);
    }

    #[test]
    fn a_dependency_on_one_version_everywhere_is_not_skew() {
        let a = module("a", vec![node("json", "2", None, "", "")]);
        let b = module("b", vec![node("json", "2", None, "", "")]);
        assert!(skew(&[a, b], &ConflictFilter::default()).is_empty());
    }
}
