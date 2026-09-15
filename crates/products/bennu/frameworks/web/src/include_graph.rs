//! JSP **include graph** — the directed graph of `<%@ include %>` / `<jsp:include>` /
//! `<s:include>` / `<c:import>` edges across a project's JSP files, plus a cycle-safe
//! transitive walk from a start file.
//!
//! Companion to [`crate::jsp_includes`] (the per-file include scan + path resolution): that
//! module answers *which references a single JSP holds* and *where each resolves*; this one
//! stitches those per-file edges into a whole-project graph so a consumer can ask the two
//! questions the include-aware Forms tool window needs:
//!
//!   - **forward**: which fragments does this page (transitively) include? — a page that
//!     `<jsp:include>`s a form body should surface that body's `<form>`s;
//!   - **reverse**: which pages (transitively) include *this* fragment? — sitting on an
//!     included `form_body.jspf`, the user should still see the form it participates in.
//!
//! Both directions are transitive and **cycle-safe**: a JSP that includes itself, or a
//! mutual `A ↔ B` include, terminates (a per-direction `visited` set emits each file once)
//! and a hard `max_nodes` backstop caps a pathological graph (reported as truncation, never
//! a silent drop). The graph is keyed by **forward-slashed** path strings (the FE's file
//! key convention) so a Windows `\` never splits an edge from its reverse.
//!
//! This module is PURE over its `&[PathBuf]` input (the caller passes the project's JSP set,
//! e.g. from a filesystem discovery walk) + `std` + [`crate::jsp_includes`] — no `bennu-be`,
//! no live index — so the loop-safety lives here and is unit-tested off temp fixtures.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::jsp_includes::{parse_jsp_includes_file, resolve_include_target};

/// Forward-slash a path into the graph's node key (matches the FE's file-key convention, so
/// a `\`-separated Windows path and a `/`-separated one address the same node).
///
/// `pub(crate)` so the include-aware form aggregation ([`crate::form_expand`]) tags each
/// spliced field with the SAME forward-slashed key the graph + FE use.
pub(crate) fn key_of(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The directed include graph of a project's JSPs. `forward[a]` lists the files `a` includes;
/// `reverse[b]` lists the files that include `b`. Both are keyed by forward-slashed absolute
/// path strings and each adjacency list is deduped (a page including the same fragment twice
/// yields one edge).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IncludeGraph {
    /// `a → [b, …]`: `a` includes each `b` (the resolved, on-disk include targets).
    pub forward: HashMap<String, Vec<String>>,
    /// `b → [a, …]`: each `a` includes `b` (the inverse of `forward`).
    pub reverse: HashMap<String, Vec<String>>,
}

impl IncludeGraph {
    /// Add a `from → to` edge to `forward` and its inverse to `reverse`, deduping both
    /// adjacency lists (a duplicate include reference contributes a single edge).
    ///
    /// `pub(crate)` so the incremental [`crate::include_cache`] can re-assemble a graph from
    /// its cached per-file edge lists without re-parsing.
    pub(crate) fn add_edge(&mut self, from: &str, to: &str) {
        push_unique(self.forward.entry(from.to_string()).or_default(), to);
        push_unique(self.reverse.entry(to.to_string()).or_default(), from);
    }

    /// The files `file` directly includes (forward neighbours), or an empty slice.
    fn includes(&self, file: &str) -> &[String] {
        self.forward.get(file).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The files that directly include `file` (reverse neighbours), or an empty slice.
    fn included_by(&self, file: &str) -> &[String] {
        self.reverse.get(file).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Push `value` onto `list` only if absent (keeps adjacency lists deduped + order-stable).
fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_string());
    }
}

/// Build the [`IncludeGraph`] over `jsps` (the project's JSP-family files). For each file, its
/// non-computed include references are resolved with [`resolve_include_target`]; every
/// reference that resolves to an existing on-disk file becomes a `jsp → target` edge (and its
/// reverse). Unresolved / computed (`${…}` / `%{…}`) / external references are skipped — the
/// graph holds only real, navigable edges.
///
/// Pure over `jsps`: the caller supplies the file set (a discovery walk), so this is
/// unit-testable off temp fixtures with no live project.
pub fn build_include_graph(jsps: &[PathBuf]) -> IncludeGraph {
    let mut graph = IncludeGraph::default();
    for jsp in jsps {
        let from = key_of(jsp);
        for inc in parse_jsp_includes_file(jsp) {
            if inc.computed {
                continue; // runtime expression → not a static edge
            }
            if let Some(target) = resolve_include_target(jsp, &inc.raw) {
                graph.add_edge(&from, &key_of(&target));
            }
        }
    }
    graph
}

/// How a related file sits relative to the start file in [`related_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeRelation {
    /// The start file itself (depth 0).
    SelfPage,
    /// A file the start file (transitively) includes.
    Includes,
    /// A file that (transitively) includes the start file.
    IncludedBy,
}

impl IncludeRelation {
    /// A stable string tag (`"self"` / `"includes"` / `"included_by"`) for logs/diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            IncludeRelation::SelfPage => "self",
            IncludeRelation::Includes => "includes",
            IncludeRelation::IncludedBy => "included_by",
        }
    }
}

/// One file reached from the start file, with how it relates + its distance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedFile {
    /// Forward-slashed absolute path of the related file.
    pub file: String,
    /// Its relation to the start file.
    pub relation: IncludeRelation,
    /// BFS distance from the start file (0 for the start file itself).
    pub depth: usize,
}

/// The result of a [`related_files`] walk: the reached files (closest first) plus whether the
/// `max_nodes` backstop truncated the walk (a huge include graph never silently drops
/// coverage — the caller surfaces a "…more" hint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedFiles {
    /// The related files, BFS-ordered (`SelfPage` first, then nearest relations outward).
    pub files: Vec<RelatedFile>,
    /// True when the node cap was hit and further files were left unvisited.
    pub truncated: bool,
}

/// Collect the files related to `start` in `graph`, walking transitively FORWARD (what `start`
/// includes) and REVERSE (who includes `start`). Returns the start file (relation
/// [`IncludeRelation::SelfPage`], depth 0) plus each reachable file, BFS-ordered so the closest
/// relations come first.
///
/// **Cycle safety** (non-negotiable): a single `visited` set (seeded with `start`) means each
/// file is emitted at most once, so a self-include (`a → a`) or a mutual include (`a ↔ b`)
/// terminates instead of looping. A hard `max_nodes` backstop stops the walk once `visited`
/// reaches the cap and sets [`RelatedFiles::truncated`], so even a pathological graph is
/// bounded and the truncation is visible (never a silent drop). A `max_nodes` of 0 is treated
/// as 1 (the start file always appears).
pub fn related_files(graph: &IncludeGraph, start: &str, max_nodes: usize) -> RelatedFiles {
    let cap = max_nodes.max(1);
    let mut visited: HashSet<String> = HashSet::new();
    let mut out: Vec<RelatedFile> = Vec::new();
    let mut truncated = false;

    // BFS queue of (file, relation, depth). The start file seeds both directions; every file
    // dequeued is already in `visited` (marked at enqueue time) so it is processed once.
    let mut queue: VecDeque<(String, IncludeRelation, usize)> = VecDeque::new();
    visited.insert(start.to_string());
    queue.push_back((start.to_string(), IncludeRelation::SelfPage, 0));

    while let Some((file, relation, depth)) = queue.pop_front() {
        out.push(RelatedFile { file: file.clone(), relation, depth });

        // Expand outward in the SAME direction the node was reached in — a fragment reached as
        // "included_by" keeps walking reverse (its includers' includers), the start file
        // expands BOTH ways. This keeps each branch a pure ancestry/descendancy chain.
        let forward = matches!(relation, IncludeRelation::SelfPage | IncludeRelation::Includes);
        let backward = matches!(relation, IncludeRelation::SelfPage | IncludeRelation::IncludedBy);

        if forward {
            enqueue_neighbours(
                graph.includes(&file),
                IncludeRelation::Includes,
                depth + 1,
                cap,
                &mut visited,
                &mut queue,
                &mut truncated,
            );
        }
        if backward {
            enqueue_neighbours(
                graph.included_by(&file),
                IncludeRelation::IncludedBy,
                depth + 1,
                cap,
                &mut visited,
                &mut queue,
                &mut truncated,
            );
        }
    }

    RelatedFiles { files: out, truncated }
}

/// Enqueue each not-yet-visited neighbour with `relation`/`depth`, marking it visited so it is
/// never re-enqueued (the cycle guard). Once `visited` reaches `cap`, stops and flags
/// `truncated` — the hard backstop against a pathological graph.
#[allow(clippy::too_many_arguments)]
fn enqueue_neighbours(
    neighbours: &[String],
    relation: IncludeRelation,
    depth: usize,
    cap: usize,
    visited: &mut HashSet<String>,
    queue: &mut VecDeque<(String, IncludeRelation, usize)>,
    truncated: &mut bool,
) {
    for next in neighbours {
        if visited.contains(next) {
            continue; // already emitted (or queued) — the loop/diamond guard
        }
        if visited.len() >= cap {
            *truncated = true;
            return; // node cap hit — stop expanding, report truncation
        }
        visited.insert(next.clone());
        queue.push_back((next.clone(), relation, depth));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tmp_dir;

    /// Write `name` with `body` under `dir`, returning its path.
    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    /// `page.jsp` includes `a.jspf` + `b.jspf`; assert the forward + reverse edges.
    #[test]
    fn builds_forward_and_reverse_edges() {
        let dir = tmp_dir("graph");
        let a = write(&dir, "a.jspf", "<div/>");
        let b = write(&dir, "b.jspf", "<div/>");
        let page = write(
            &dir,
            "page.jsp",
            r#"<jsp:include page="a.jspf"/><jsp:include page="b.jspf"/>"#,
        );

        let graph = build_include_graph(&[page.clone(), a.clone(), b.clone()]);
        let pk = key_of(&page);
        let ak = key_of(&a);
        let bk = key_of(&b);

        let mut fwd = graph.includes(&pk).to_vec();
        fwd.sort();
        assert_eq!(fwd, vec![ak.clone(), bk.clone()]);
        assert_eq!(graph.included_by(&ak), &[pk.clone()]);
        assert_eq!(graph.included_by(&bk), &[pk.clone()]);
    }

    /// THE KEY SCENARIO: sitting on the included fragment, its includer surfaces as
    /// `IncludedBy` (reverse walk).
    #[test]
    fn reverse_walk_surfaces_includer_of_a_fragment() {
        let dir = tmp_dir("graph");
        let frag = write(&dir, "a.jspf", "<s:textfield name='x'/>");
        let page = write(&dir, "page.jsp", r#"<jsp:include page="a.jspf"/>"#);

        let graph = build_include_graph(&[page.clone(), frag.clone()]);
        let related = related_files(&graph, &key_of(&frag), 200);

        assert!(!related.truncated);
        // The fragment itself is SelfPage at depth 0.
        assert_eq!(related.files[0].file, key_of(&frag));
        assert_eq!(related.files[0].relation, IncludeRelation::SelfPage);
        // The page that includes it is surfaced as IncludedBy at depth 1.
        let inc = related.files.iter().find(|r| r.file == key_of(&page)).expect("includer surfaced");
        assert_eq!(inc.relation, IncludeRelation::IncludedBy);
        assert_eq!(inc.depth, 1);
    }

    /// Forward walk: a page reaches the fragments it includes, transitively.
    #[test]
    fn forward_walk_is_transitive() {
        let dir = tmp_dir("graph");
        let leaf = write(&dir, "leaf.jspf", "<div/>");
        let mid = write(&dir, "mid.jspf", r#"<jsp:include page="leaf.jspf"/>"#);
        let page = write(&dir, "page.jsp", r#"<jsp:include page="mid.jspf"/>"#);

        let graph = build_include_graph(&[page.clone(), mid.clone(), leaf.clone()]);
        let related = related_files(&graph, &key_of(&page), 200);

        let by = |p: &PathBuf| related.files.iter().find(|r| r.file == key_of(p)).cloned();
        assert_eq!(by(&mid).unwrap().relation, IncludeRelation::Includes);
        assert_eq!(by(&mid).unwrap().depth, 1);
        assert_eq!(by(&leaf).unwrap().relation, IncludeRelation::Includes);
        assert_eq!(by(&leaf).unwrap().depth, 2, "transitive include is depth 2");
    }

    /// A self-include (`loop.jsp` includes itself) terminates; the file is emitted once.
    #[test]
    fn self_include_terminates() {
        let dir = tmp_dir("graph");
        let selfinc = write(&dir, "loop.jsp", r#"<jsp:include page="loop.jsp"/>"#);

        let graph = build_include_graph(&[selfinc.clone()]);
        let related = related_files(&graph, &key_of(&selfinc), 200);

        assert!(!related.truncated);
        let hits = related.files.iter().filter(|r| r.file == key_of(&selfinc)).count();
        assert_eq!(hits, 1, "self-include emitted exactly once");
    }

    /// A mutual include (`x ↔ y`) terminates; each file is emitted once.
    #[test]
    fn mutual_include_terminates() {
        let dir = tmp_dir("graph");
        let x = write(&dir, "x.jsp", r#"<jsp:include page="y.jsp"/>"#);
        let y = write(&dir, "y.jsp", r#"<jsp:include page="x.jsp"/>"#);

        let graph = build_include_graph(&[x.clone(), y.clone()]);
        let related = related_files(&graph, &key_of(&x), 200);

        assert!(!related.truncated);
        assert_eq!(related.files.iter().filter(|r| r.file == key_of(&x)).count(), 1);
        assert_eq!(related.files.iter().filter(|r| r.file == key_of(&y)).count(), 1);
    }

    /// A diamond (`top → mid1 + mid2`, both `→ leaf`) reaches every node, `leaf` once, no loop.
    #[test]
    fn diamond_reaches_leaf_once() {
        let dir = tmp_dir("graph");
        let leaf = write(&dir, "leaf.jspf", "<div/>");
        let mid1 = write(&dir, "mid1.jspf", r#"<jsp:include page="leaf.jspf"/>"#);
        let mid2 = write(&dir, "mid2.jspf", r#"<jsp:include page="leaf.jspf"/>"#);
        let top = write(
            &dir,
            "top.jsp",
            r#"<jsp:include page="mid1.jspf"/><jsp:include page="mid2.jspf"/>"#,
        );

        let graph = build_include_graph(&[top.clone(), mid1.clone(), mid2.clone(), leaf.clone()]);
        let related = related_files(&graph, &key_of(&top), 200);

        assert!(!related.truncated);
        // 4 distinct nodes, leaf emitted exactly once despite two paths to it.
        assert_eq!(related.files.len(), 4, "top + mid1 + mid2 + leaf");
        assert_eq!(related.files.iter().filter(|r| r.file == key_of(&leaf)).count(), 1);
    }

    /// A long chain with a small `max_nodes` stops at the cap and reports truncation.
    #[test]
    fn node_cap_truncates_a_long_chain() {
        let dir = tmp_dir("graph");
        // Build a 5-deep forward chain: c0 → c1 → c2 → c3 → c4.
        let mut files = Vec::new();
        for i in 0..5 {
            let body = if i < 4 {
                format!(r#"<jsp:include page="c{}.jsp"/>"#, i + 1)
            } else {
                "<div/>".to_string()
            };
            files.push(write(&dir, &format!("c{i}.jsp"), &body));
        }

        let graph = build_include_graph(&files);
        // Cap at 3 nodes → the walk stops before reaching all 5.
        let related = related_files(&graph, &key_of(&files[0]), 3);

        assert!(related.truncated, "small cap must report truncation");
        assert!(related.files.len() <= 3, "no more than the cap emitted: {}", related.files.len());
    }

    /// A computed (`${…}`) include is not an edge; an external URL / unresolved ref is skipped.
    #[test]
    fn computed_and_unresolved_includes_are_not_edges() {
        let dir = tmp_dir("graph");
        let page = write(
            &dir,
            "page.jsp",
            r#"<jsp:include page="${dyn}"/><c:import url="https://x/y"/><jsp:include page="ghost.jsp"/>"#,
        );

        let graph = build_include_graph(&[page.clone()]);
        // No target exists on disk / all refs are computed|external|missing → no edges.
        assert!(graph.forward.get(&key_of(&page)).map(Vec::is_empty).unwrap_or(true));
        let related = related_files(&graph, &key_of(&page), 200);
        assert_eq!(related.files.len(), 1, "only the page itself");
    }
}
