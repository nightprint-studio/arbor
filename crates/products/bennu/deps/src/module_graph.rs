//! Who depends on whom **inside** the project — the graph behind the Module Graph window.
//!
//! ## Why this is not the dependency list again
//!
//! [`crate::model::Report`] answers "what does this module need". It cannot answer the questions a
//! twenty-crate workspace actually generates:
//!
//! * *who uses this crate* — and therefore whether anything still does;
//! * *what breaks if I touch it* — how many crates rebuild, transitively;
//! * *what is foundational* — which crates sit at the bottom and are worth being careful with;
//! * *is there a cycle* — which Cargo refuses to build and Maven refuses to order, and whose error
//!   message names two crates out of the eight actually in the ring.
//!
//! All four are properties of the *edges between the project's own modules*, and every one of them
//! is a graph walk rather than a row in a list. So this module keeps only the internal edges and
//! computes on them; third-party dependencies are counted per module and otherwise left where they
//! are, because a graph that draws four hundred crates.io nodes answers nothing.
//!
//! ## One shape, two ecosystems
//!
//! Built from the [`Report`], so a Maven reactor and a Cargo workspace produce the same graph — the
//! questions above are the same questions, and Java projects big enough to need them exist. The two
//! places the ecosystems genuinely differ are named and handled rather than smoothed over: how a
//! dependency's crate/artifact identity is read (see [`internal_name`]), and which edges can legally
//! close a cycle (see [`structural`]).
//!
//! ## Nothing is resolved, and nothing is executed
//!
//! Same contract as the rest of the crate: this reads the report the manifests produced. Feature
//! unification, `cfg`-gating and which profile is active are Cargo's and Maven's answers, so an
//! optional or target-specific edge is *on the graph and labelled* rather than silently included or
//! silently dropped. Whether the graph shows it is the panel's choice, made visibly.

use serde::{Deserialize, Serialize};

use crate::model::Report;

/// Ceiling on the modules a graph is built for.
///
/// Above this the layout stops being readable long before the walks stop being cheap, and the
/// transitive closures below are `O(V·E)`. Far above any real reactor; a generated tree of thousands
/// would otherwise spend a second computing a picture nobody can use.
const MAX_MODULES: usize = 400;

/// One module of the project, with what the graph knows about its position in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphNode {
    /// The build tool's identifier — the artifactId, or the crate name. Matches
    /// [`crate::model::Module::id`], and what an edge's endpoints are resolved against.
    pub id: String,
    /// Display name — the pom's `<name>`, else the artifactId; the crate name for Cargo.
    pub name: String,
    /// Absolute path of the module's manifest, forward-slashed. What a row opens.
    pub manifest: String,
    /// What it builds: Maven's `<packaging>`, or Cargo's target kinds (`lib` · `bin` · `lib+bin` ·
    /// `proc-macro`).
    pub kind: String,
    /// How far above the foundation it sits: `0` when it depends on no other module of the project,
    /// otherwise one more than the deepest module it depends on.
    ///
    /// Computed on the condensation (see [`ModuleGraph::cycles`]), so a cycle does not make this
    /// undefined — every module of one strongly-connected group shares its layer.
    pub layer: usize,
    /// Modules of this project that depend on it, directly.
    pub dependents: usize,
    /// Modules of this project it depends on, directly.
    pub dependencies: usize,
    /// Third-party dependencies it declares, counted by distinct coordinate. The number that says
    /// "this crate is small but it pulls in half of crates.io".
    pub external: usize,
    /// Modules it depends on **transitively** — the part of the project it is built on.
    pub reach: usize,
    /// Modules that depend on it transitively: *change this and this many rebuild*. The number worth
    /// knowing before touching something, and the reason a leaf with a high one is not a leaf.
    pub impact: usize,
    /// Whether it is in a dependency cycle. See [`ModuleGraph::cycles`].
    pub in_cycle: bool,
}

impl GraphNode {
    /// Whether nothing in the project depends on it.
    ///
    /// Deliberately not called "unused": a library crate published to a registry, and a `war` that
    /// is the thing being deployed, both legitimately have no internal dependents. It is a question
    /// worth filtering by, not a verdict — the panel says "nothing here depends on it" and lets the
    /// reader decide, because for a `lib` in a private workspace that usually means dead code and
    /// for the top-level binary it never does.
    pub fn is_root(&self) -> bool {
        self.dependents == 0
    }

    /// Whether it depends on nothing else in the project — the bottom of the graph.
    pub fn is_leaf(&self) -> bool {
        self.dependencies == 0
    }
}

/// One dependency edge between two modules of the project.
///
/// A pair can carry several: the same two crates related by a normal dependency *and* a dev one is
/// two different facts (see [`structural`]), and collapsing them would lose the one that matters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphEdge {
    /// Index into [`ModuleGraph::nodes`] — the module that declares the dependency.
    pub from: usize,
    /// Index into [`ModuleGraph::nodes`] — the module it depends on.
    pub to: usize,
    /// Maven's scope (`compile` · `test` · `provided` …) or Cargo's kind (`normal` · `dev` ·
    /// `build`).
    pub scope: String,
    /// Whether the edge only exists when a feature turns it on (`optional = true`).
    pub optional: bool,
    /// What has to be true for it to be on the graph at all — the profile, or the `cfg(…)` of a
    /// target table. Empty for the ordinary case. Neither can be evaluated here.
    pub condition: String,
    /// Whether this edge participates in a cycle — so the panel can draw the ring rather than
    /// merely name its members.
    pub in_cycle: bool,
    /// Whether the ecosystem would refuse a cycle closed by this edge. See [`structural`].
    pub structural: bool,
}

/// The project's internal dependency graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleGraph {
    /// `maven` or `cargo`, carried through from the report — the panel needs it for the words, not
    /// for the shape.
    pub ecosystem: String,
    /// The modules, in the report's order (which is the manifests' declaration order).
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Each entry is a set of modules that all reach each other — a **dependency cycle**, by node
    /// index, ascending.
    ///
    /// Reported as a group and not as a ring because a strongly-connected component is what is
    /// actually true: five crates that all reach each other usually contain several rings, and
    /// picking one to print would suggest the others are fine. What the build tool says when it
    /// refuses is a single pair out of the group, which is precisely why this is worth showing.
    ///
    /// Computed on [`GraphEdge::structural`] edges only: a Cargo cycle through `dev-dependencies`
    /// is **legal** and common (a crate whose tests use something that depends on it), so counting
    /// those would report a cycle on half the workspaces in existence.
    pub cycles: Vec<Vec<usize>>,
    /// The longest chain of modules, in modules — the depth of the deepest rebuild a change can
    /// trigger, and the number of layers a layout needs.
    pub depth: usize,
    /// Distinct third-party dependencies across the whole project, by coordinate.
    pub external_total: usize,
    /// Set when the project has more modules than the graph is built for ([`MAX_MODULES`]); the
    /// nodes present are the first that fit. Said out loud rather than shown as a smaller project.
    pub truncated: bool,
}

impl ModuleGraph {
    /// The node called `id`, and its index.
    pub fn node(&self, id: &str) -> Option<(usize, &GraphNode)> {
        self.nodes.iter().enumerate().find(|(_, n)| n.id == id)
    }

    /// Modules nothing in the project depends on. See [`GraphNode::is_root`] for why that is a
    /// question and not an accusation.
    pub fn roots(&self) -> Vec<usize> {
        (0..self.nodes.len()).filter(|&i| self.nodes[i].is_root()).collect()
    }
}

/// Whether the ecosystem would **refuse** a cycle closed by an edge of this scope.
///
/// The one place the two build tools genuinely disagree, and getting it wrong makes the cycle
/// report useless in opposite directions:
///
/// * **Cargo** allows a cycle through `dev-dependencies` — it compiles the library and its test
///   harness as separate units, so `a` depending on `b` while `b`'s tests depend on `a` is a normal,
///   deliberate arrangement. It refuses one through `dependencies` or `build-dependencies`.
/// * **Maven** orders the whole reactor as one graph and refuses *any* cycle in it, `test` scope
///   included: the modules are single units and there is no order that builds them.
///
/// Non-structural edges are still on the graph and still drawn — they are real dependencies — they
/// just do not make a cycle, and do not constrain the layering.
fn structural(ecosystem: &str, scope: &str) -> bool {
    if ecosystem == "cargo" {
        return scope != "dev";
    }
    true
}

/// The name that identifies a dependency as one of *this project's* modules.
///
/// Ecosystem-specific because [`crate::model::Dependency::variant`] carries two different things:
/// for Cargo it is the real crate name behind a rename (`json = { package = "serde_json" }`), which
/// is exactly what has to be matched; for Maven it is the `<classifier>`, and matching on it would
/// resolve `spring-core:tests` to a module called `tests`.
///
/// Maven is matched on the artifactId alone, since [`crate::model::Module`] carries no groupId. A
/// third-party artifact whose artifactId collides with one of your modules would produce a false
/// edge — rare, and the alternative (dropping the whole feature for Maven) is worse.
fn internal_name<'a>(ecosystem: &str, dep: &'a crate::model::Dependency) -> &'a str {
    if ecosystem == "cargo" && !dep.variant.is_empty() {
        return &dep.variant;
    }
    &dep.name
}

/// Build the internal module graph from a dependency report.
///
/// Never fails and never panics: a report with no modules yields an empty graph, which is what a
/// project that is neither Maven nor Cargo should show.
pub fn module_graph(report: &Report) -> ModuleGraph {
    let truncated = report.modules.len() > MAX_MODULES;
    let modules = &report.modules[..report.modules.len().min(MAX_MODULES)];
    let eco = report.ecosystem.as_str();

    let mut graph = ModuleGraph {
        ecosystem: report.ecosystem.clone(),
        truncated,
        nodes: modules
            .iter()
            .map(|m| GraphNode {
                id: m.id.clone(),
                name: m.name.clone(),
                manifest: m.manifest.clone(),
                kind: m.kind.clone(),
                ..GraphNode::default()
            })
            .collect(),
        ..ModuleGraph::default()
    };

    // ── Edges, and the third-party count that is not one ──────────────────────────
    let index = |id: &str| modules.iter().position(|m| m.id == id);
    let mut externals: Vec<String> = Vec::new();
    for (from, module) in modules.iter().enumerate() {
        let mut seen_external: Vec<String> = Vec::new();
        for dep in &module.dependencies {
            let Some(to) = index(internal_name(eco, dep)) else {
                // Third-party. Counted per module by distinct coordinate, so `serde` declared twice
                // (normal and dev) is one dependency and not two.
                let coord = dep.coord();
                if !seen_external.contains(&coord) {
                    seen_external.push(coord.clone());
                }
                if !externals.contains(&coord) {
                    externals.push(coord);
                }
                continue;
            };
            // A module depending on itself is a manifest mistake, not an edge: drawing a self-loop
            // would put it in a "cycle" that no walk below can reason about.
            if to == from {
                continue;
            }
            // One edge per (pair, scope). A dependency written in two target tables — the usual
            // `cfg(unix)` / `cfg(windows)` pair — is one relationship, and two identical arrows
            // would be drawn on top of each other.
            if graph
                .edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.scope == dep.scope)
            {
                continue;
            }
            graph.edges.push(GraphEdge {
                from,
                to,
                scope: dep.scope.clone(),
                optional: dep.optional,
                condition: dep.condition.clone(),
                in_cycle: false,
                structural: structural(eco, &dep.scope),
            });
        }
        graph.nodes[from].external = seen_external.len();
    }
    graph.external_total = externals.len();

    // ── Degrees ───────────────────────────────────────────────────────────────────
    // Distinct neighbours, not edges: two crates related by both a normal and a dev dependency are
    // one dependency of one module, and counting the arrows would double it.
    let degrees: Vec<(usize, usize)> = (0..graph.nodes.len())
        .map(|i| {
            (
                distinct(graph.edges.iter().filter(|e| e.from == i).map(|e| e.to)),
                distinct(graph.edges.iter().filter(|e| e.to == i).map(|e| e.from)),
            )
        })
        .collect();
    for (i, (dependencies, dependents)) in degrees.into_iter().enumerate() {
        graph.nodes[i].dependencies = dependencies;
        graph.nodes[i].dependents = dependents;
    }

    // ── Cycles, layers, and the closures ──────────────────────────────────────────
    let n = graph.nodes.len();
    let structural_out = adjacency(n, &graph.edges, true);
    let component = components(n, &structural_out);
    graph.cycles = cycle_groups(&component, n);
    let mut ringed = vec![false; n];
    for group in &graph.cycles {
        for &i in group {
            ringed[i] = true;
            graph.nodes[i].in_cycle = true;
        }
    }
    // An edge is in the ring when it is structural, both ends are in the same component, and that
    // component is one of the cyclic ones — the third condition is what keeps a lone module's
    // component (which is trivially "the same" at both ends) out of it.
    for edge in &mut graph.edges {
        edge.in_cycle =
            edge.structural && component[edge.from] == component[edge.to] && ringed[edge.from];
    }

    let layers = layer_of(n, &structural_out, &component);
    for (i, layer) in layers.iter().enumerate() {
        graph.nodes[i].layer = *layer;
    }
    graph.depth = layers.iter().copied().max().map(|m| m + 1).unwrap_or(0);

    // Reach and impact walk **every** edge, not just the structural ones: a dev dependency does
    // cause a rebuild, which is the question `impact` answers.
    let out = adjacency(n, &graph.edges, false);
    let incoming = reversed(n, &graph.edges);
    for i in 0..n {
        graph.nodes[i].reach = reachable(i, &out);
        graph.nodes[i].impact = reachable(i, &incoming);
    }

    graph
}

/// How many distinct values an iterator yields.
fn distinct(it: impl Iterator<Item = usize>) -> usize {
    let mut seen: Vec<usize> = Vec::new();
    for v in it {
        if !seen.contains(&v) {
            seen.push(v);
        }
    }
    seen.len()
}

/// Outgoing adjacency, optionally restricted to structural edges.
fn adjacency(n: usize, edges: &[GraphEdge], structural_only: bool) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); n];
    for e in edges {
        if structural_only && !e.structural {
            continue;
        }
        if !out[e.from].contains(&e.to) {
            out[e.from].push(e.to);
        }
    }
    out
}

/// Incoming adjacency over every edge — "who depends on me", for [`GraphNode::impact`].
fn reversed(n: usize, edges: &[GraphEdge]) -> Vec<Vec<usize>> {
    let mut inc = vec![Vec::new(); n];
    for e in edges {
        if !inc[e.to].contains(&e.from) {
            inc[e.to].push(e.from);
        }
    }
    inc
}

/// How many nodes are reachable from `start`, excluding itself.
fn reachable(start: usize, adj: &[Vec<usize>]) -> usize {
    let mut seen = vec![false; adj.len()];
    let mut stack = vec![start];
    seen[start] = true;
    let mut count = 0;
    while let Some(node) = stack.pop() {
        for &next in &adj[node] {
            if !seen[next] {
                seen[next] = true;
                count += 1;
                stack.push(next);
            }
        }
    }
    count
}

/// Strongly-connected components, by node — Tarjan, iterative.
///
/// Iterative rather than recursive on purpose: the recursion depth is the length of the longest path,
/// and a deep workspace would blow the stack on a graph that is otherwise perfectly fine.
///
/// The returned ids are dense but arbitrary; only equality is meaningful (`component[a] ==
/// component[b]` ⇔ `a` and `b` reach each other).
fn components(n: usize, adj: &[Vec<usize>]) -> Vec<usize> {
    const UNVISITED: usize = usize::MAX;
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut component = vec![UNVISITED; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut next_component = 0usize;

    for root in 0..n {
        if index[root] != UNVISITED {
            continue;
        }
        // (node, position in its adjacency list) — the explicit call stack.
        let mut call: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;

        // Copied out of the stack each turn rather than held as a borrow: the body pushes and pops
        // the same vector, which a live `last_mut()` would forbid.
        while let Some(&(node, cursor)) = call.last() {
            if cursor < adj[node].len() {
                if let Some(top) = call.last_mut() {
                    top.1 = cursor + 1;
                }
                let next = adj[node][cursor];
                if index[next] == UNVISITED {
                    index[next] = next_index;
                    low[next] = next_index;
                    next_index += 1;
                    stack.push(next);
                    on_stack[next] = true;
                    call.push((next, 0));
                } else if on_stack[next] {
                    low[node] = low[node].min(index[next]);
                }
                continue;
            }
            // Exhausted: close this node off and fold its low-link into its parent's.
            if low[node] == index[node] {
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component[member] = next_component;
                    if member == node {
                        break;
                    }
                }
                next_component += 1;
            }
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[node]);
            }
        }
    }
    component
}

/// The components holding more than one module, as ascending node-index groups.
fn cycle_groups(component: &[usize], n: usize) -> Vec<Vec<usize>> {
    let mut by_component: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        let c = component[i];
        while by_component.len() <= c {
            by_component.push(Vec::new());
        }
        by_component[c].push(i);
    }
    let mut groups: Vec<Vec<usize>> =
        by_component.into_iter().filter(|group| group.len() > 1).collect();
    // Deterministic order for a list a human reads and a test asserts on.
    groups.sort_by_key(|group| group[0]);
    groups
}

/// Longest-path layering: `0` for a module that depends on nothing internal, otherwise one more
/// than the deepest thing it depends on.
///
/// Run on the **condensation** — every member of a cycle shares one layer — which is what makes this
/// terminate on a graph that has one. Without that step a cycle is an infinite descent, and a
/// crate graph with a cycle is exactly when you most want to look at the picture.
fn layer_of(n: usize, adj: &[Vec<usize>], component: &[usize]) -> Vec<usize> {
    // Memo per component, plus an in-progress mark. A cycle cannot recurse into itself because
    // every edge inside one lands on the same component, which is skipped.
    let mut layer_of_component: Vec<Option<usize>> = vec![None; n];
    let mut computing = vec![false; n];

    fn walk(
        c: usize,
        n: usize,
        adj: &[Vec<usize>],
        component: &[usize],
        memo: &mut [Option<usize>],
        computing: &mut [bool],
    ) -> usize {
        if let Some(v) = memo[c] {
            return v;
        }
        if computing[c] {
            return 0; // unreachable given the condensation; a safe answer beats a hang
        }
        computing[c] = true;
        let mut deepest: Option<usize> = None;
        for node in (0..n).filter(|&i| component[i] == c) {
            for &next in &adj[node] {
                let other = component[next];
                if other == c {
                    continue; // inside the cycle
                }
                let d = walk(other, n, adj, component, memo, computing);
                deepest = Some(deepest.map_or(d, |cur: usize| cur.max(d)));
            }
        }
        let value = deepest.map_or(0, |d| d + 1);
        computing[c] = false;
        memo[c] = Some(value);
        value
    }

    (0..n)
        .map(|i| walk(component[i], n, adj, component, &mut layer_of_component, &mut computing))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dependency, Module};

    fn dep(name: &str, scope: &str) -> Dependency {
        Dependency { name: name.to_string(), scope: scope.to_string(), ..Dependency::default() }
    }

    fn module(id: &str, kind: &str, deps: Vec<Dependency>) -> Module {
        Module {
            name: id.to_string(),
            id: id.to_string(),
            manifest: format!("/p/{id}/Cargo.toml"),
            kind: kind.to_string(),
            dependencies: deps,
        }
    }

    /// `app → core → util`, plus a third-party crate on each.
    fn chain() -> Report {
        Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("app", "bin", vec![dep("core", "normal"), dep("serde", "normal")]),
                module("core", "lib", vec![dep("util", "normal"), dep("serde", "normal")]),
                module("util", "lib", vec![dep("log", "normal")]),
            ],
            ..Report::default()
        }
    }

    #[test]
    fn an_internal_dependency_is_an_edge_and_a_third_party_one_is_a_count() {
        let g = module_graph(&chain());
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 2, "serde and log are not nodes");
        let (app, _) = g.node("app").unwrap();
        let (core, _) = g.node("core").unwrap();
        assert!(g.edges.iter().any(|e| e.from == app && e.to == core));
        assert_eq!(g.nodes[app].external, 1);
        assert_eq!(g.external_total, 2, "serde is declared twice and counted once");
    }

    #[test]
    fn layers_put_the_foundation_at_the_bottom() {
        let g = module_graph(&chain());
        assert_eq!(g.nodes[g.node("util").unwrap().0].layer, 0);
        assert_eq!(g.nodes[g.node("core").unwrap().0].layer, 1);
        assert_eq!(g.nodes[g.node("app").unwrap().0].layer, 2);
        assert_eq!(g.depth, 3);
    }

    #[test]
    fn impact_is_the_transitive_dependents_and_reach_the_transitive_dependencies() {
        let g = module_graph(&chain());
        let util = g.node("util").unwrap().0;
        let app = g.node("app").unwrap().0;
        assert_eq!(g.nodes[util].impact, 2, "touching util rebuilds core and app");
        assert_eq!(g.nodes[util].reach, 0);
        assert_eq!(g.nodes[app].reach, 2);
        assert_eq!(g.nodes[app].impact, 0);
        assert!(g.nodes[app].is_root());
        assert!(g.nodes[util].is_leaf());
    }

    #[test]
    fn a_layer_is_the_longest_path_and_not_the_shortest() {
        // `app` depends on `core` (which needs `util`) AND directly on `util`. The direct edge must
        // not pull `app` down to layer 1 — it has to sit above everything it depends on, or an arrow
        // in the drawing would point sideways or up.
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("app", "bin", vec![dep("core", "normal"), dep("util", "normal")]),
                module("core", "lib", vec![dep("util", "normal")]),
                module("util", "lib", vec![]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.nodes[g.node("app").unwrap().0].layer, 2);
    }

    #[test]
    fn a_dev_dependency_cycle_is_legal_in_cargo() {
        // The common arrangement: `core` is a library, and `util`'s tests use it.
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("core", "lib", vec![dep("util", "normal")]),
                module("util", "lib", vec![dep("core", "dev")]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert!(g.cycles.is_empty(), "cargo compiles the test harness separately");
        assert!(g.nodes.iter().all(|n| !n.in_cycle));
        assert_eq!(g.edges.len(), 2, "the dev edge is still drawn");
        // ...and it still counts as impact: changing `core` does rebuild `util`'s tests.
        assert_eq!(g.nodes[g.node("core").unwrap().0].impact, 1);
    }

    #[test]
    fn the_same_cycle_is_real_in_a_maven_reactor() {
        // Maven builds each module as one unit, so a test-scoped edge closes a genuine cycle.
        let report = Report {
            ecosystem: "maven".to_string(),
            modules: vec![
                module("core", "jar", vec![dep("util", "compile")]),
                module("util", "jar", vec![dep("core", "test")]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.cycles, vec![vec![0, 1]]);
        assert!(g.nodes.iter().all(|n| n.in_cycle));
        assert!(g.edges.iter().all(|e| e.in_cycle));
    }

    #[test]
    fn a_normal_dependency_cycle_is_reported_with_every_member() {
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("a", "lib", vec![dep("b", "normal")]),
                module("b", "lib", vec![dep("c", "normal")]),
                module("c", "lib", vec![dep("a", "normal")]),
                module("outside", "lib", vec![dep("a", "normal")]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.cycles, vec![vec![0, 1, 2]], "all three, not the pair cargo names");
        assert!(!g.nodes[3].in_cycle);
        // Layering still terminates, and the ring shares a layer.
        assert_eq!(g.nodes[0].layer, g.nodes[1].layer);
        assert_eq!(g.nodes[3].layer, g.nodes[0].layer + 1);
    }

    #[test]
    fn a_renamed_cargo_dependency_still_resolves_to_its_crate() {
        let renamed = Dependency {
            name: "helpers".to_string(),
            variant: "util".to_string(),
            scope: "normal".to_string(),
            ..Dependency::default()
        };
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![module("app", "bin", vec![renamed]), module("util", "lib", vec![])],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.edges.len(), 1, "matched on the real crate name, not the local one");
        assert_eq!(g.nodes[1].dependents, 1);
    }

    #[test]
    fn a_maven_classifier_is_not_a_module_name() {
        // The mirror image: `variant` is a `<classifier>` here, and matching it would invent an edge
        // to a module called `tests`.
        let classified = Dependency {
            name: "spring-core".to_string(),
            variant: "tests".to_string(),
            scope: "test".to_string(),
            ..Dependency::default()
        };
        let report = Report {
            ecosystem: "maven".to_string(),
            modules: vec![module("app", "jar", vec![classified]), module("tests", "jar", vec![])],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert!(g.edges.is_empty());
        assert_eq!(g.nodes[0].external, 1);
    }

    #[test]
    fn one_pair_keeps_one_edge_per_scope_and_counts_as_one_neighbour() {
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("app", "bin", vec![dep("util", "normal"), dep("util", "dev")]),
                module("util", "lib", vec![]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.edges.len(), 2, "normal and dev are two different facts");
        assert_eq!(g.nodes[0].dependencies, 1, "but util is one dependency");
        assert_eq!(g.nodes[1].dependents, 1);
    }

    #[test]
    fn a_dependency_written_in_two_target_tables_is_one_edge() {
        let unix = Dependency {
            name: "util".to_string(),
            scope: "normal".to_string(),
            condition: "cfg(unix)".to_string(),
            ..Dependency::default()
        };
        let windows = Dependency { condition: "cfg(windows)".to_string(), ..unix.clone() };
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![
                module("app", "bin", vec![unix, windows]),
                module("util", "lib", vec![]),
            ],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].condition, "cfg(unix)", "the first one seen labels it");
    }

    #[test]
    fn a_module_depending_on_itself_is_not_a_cycle() {
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![module("app", "bin", vec![dep("app", "normal")])],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert!(g.edges.is_empty());
        assert!(g.cycles.is_empty());
        assert_eq!(g.nodes[0].layer, 0);
    }

    #[test]
    fn an_empty_report_is_an_empty_graph() {
        let g = module_graph(&Report::default());
        assert!(g.nodes.is_empty() && g.edges.is_empty() && g.cycles.is_empty());
        assert_eq!(g.depth, 0);
        assert!(!g.truncated);
    }

    #[test]
    fn an_optional_or_conditional_edge_is_labelled_rather_than_dropped() {
        let optional = Dependency {
            name: "util".to_string(),
            scope: "normal".to_string(),
            optional: true,
            ..Dependency::default()
        };
        let report = Report {
            ecosystem: "cargo".to_string(),
            modules: vec![module("app", "bin", vec![optional]), module("util", "lib", vec![])],
            ..Report::default()
        };
        let g = module_graph(&report);
        assert_eq!(g.edges.len(), 1);
        assert!(g.edges[0].optional);
    }
}
