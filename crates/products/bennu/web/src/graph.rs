//! Top-level config-graph builder + the load-bearing resolution chains (docs §10 C1).
//!
//! Ties Struts + Spring + Tiles into one [`WebConfigGraph`], and offers the two chains
//! the integration needs to make JSP/action navigation work:
//!   - action → bean-id → FQCN         ([`resolve_action_class`])
//!   - action result → Tiles def → JSP ([`resolve_action_view`])
//!
//! File **discovery** (which struts roots, which bean XMLs, which tiles files, which
//! classpath resource roots) is the caller's job — `bennu-project` walks the module tree
//! and hands the lists in via [`WebInputs`]. This crate owns parsing + resolution, not
//! filesystem walking, so it stays a leaf that depends only on `bennu-index` + the inputs.

use std::path::PathBuf;

use crate::model::{RelKind, Relation, WebConfigGraph};
use crate::{spring, struts, tiles};

/// Inputs for a full project config-graph parse.
#[derive(Debug, Default, Clone)]
pub struct WebInputs {
    /// Root struts config files (`struts.xml`, `*-struts-plugin.xml`).
    pub struts_roots: Vec<PathBuf>,
    /// Classpath resource roots for resolving `<include file>` (e.g. `src/main/resources`).
    pub resource_roots: Vec<PathBuf>,
    /// Spring bean XML files (may overlap struts fragments — one file can hold both).
    pub spring_files: Vec<PathBuf>,
    /// Tiles config files.
    pub tiles_files: Vec<PathBuf>,
}

/// Diagnostics gathered during the build (non-fatal — reported, never a hard error;
/// docs §8 lesson 10: "unresolved from a missing jar" is a normal state).
#[derive(Debug, Default, Clone)]
pub struct BuildReport {
    /// `<include file="…">` targets not found on disk — on a non-vendored install these
    /// come from a dependency jar (docs §8 #3).
    pub unresolved_includes: Vec<String>,
}

/// Parse everything and assemble the graph.
pub fn build(inputs: &WebInputs) -> (WebConfigGraph, BuildReport) {
    let mut graph = WebConfigGraph::default();
    let mut report = BuildReport::default();

    for root in &inputs.struts_roots {
        let parse = struts::parse_include_graph(root, &inputs.resource_roots);
        graph.actions.extend(parse.actions);
        graph.results.extend(parse.results);
        graph.relations.extend(parse.relations);
        report.unresolved_includes.extend(parse.unresolved_includes);
    }

    let mut spring_parse = spring::SpringParse::default();
    for f in &inputs.spring_files {
        spring::parse_file(f, &mut spring_parse);
    }
    graph.beans.extend(spring_parse.beans);
    graph.relations.extend(spring_parse.relations);

    for f in &inputs.tiles_files {
        tiles::parse_file(f, &mut graph.tiles_defs);
    }

    report.unresolved_includes.sort();
    report.unresolved_includes.dedup();
    (graph, report)
}

/// The C1 chain: resolve an action's `class` (a bean-id) to its real FQCN via the Spring
/// bean map. Returns `None` when the bean-id is unknown (would live in a dep jar) or the
/// action declares no class.
pub fn resolve_action_class(graph: &WebConfigGraph, action_qname: &str) -> Option<String> {
    let action = graph.actions.iter().find(|a| a.qualified_name == action_qname)?;
    if action.class_ref.is_empty() {
        return None;
    }
    let bean_map = spring::resolve_map(&graph.beans);
    bean_map.get(&action.class_ref).cloned()
}

/// The view chain: action → `<result type="tiles">` def name → JSP. Returns the JSP path
/// for the first Tiles result found, or `None`. (Non-tiles results — a `dispatcher` JSP,
/// a `chain` to another action — are left to the caller: the former is already a JSP
/// path, the latter another action name.)
pub fn resolve_action_view(graph: &WebConfigGraph, action_qname: &str) -> Option<String> {
    let tiles_idx = tiles::index(&graph.tiles_defs);
    let result = graph
        .results
        .iter()
        .find(|r| r.action_qualified_name == action_qname && r.result_type == "tiles")?;
    tiles::resolve_view(&tiles_idx, &result.target, 0).map(|s| s.to_string())
}

/// Relations of a given kind (small helper for consumers / tests).
pub fn relations_of(
    graph: &WebConfigGraph,
    kind: RelKind,
) -> impl Iterator<Item = &Relation> {
    graph.relations.iter().filter(move |r| r.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full load-bearing chain over inline fixtures: JSP action name → struts action
    /// → Spring bean-id → FQCN, and action → tiles result → JSP (docs §10 C1).
    #[test]
    fn resolves_action_class_and_view_end_to_end() {
        let struts = crate::test_support::tmp(
            "g-struts.xml",
            r#"<struts><package name="p" namespace="/do/Cat" extends="japs-default">
                <action name="viewTree" class="categoryAction">
                  <result type="tiles">admin.Cat.viewTree</result>
                </action>
              </package></struts>"#,
        );
        let beans = crate::test_support::tmp(
            "g-beans.xml",
            r#"<beans>
                <bean id="abstractBase" abstract="true" class="com.x.Base"/>
                <bean id="categoryAction" parent="abstractBase" class="com.x.CategoryAction"/>
              </beans>"#,
        );
        let tiles = crate::test_support::tmp(
            "g-tiles.xml",
            r#"<tiles-definitions>
                <definition name="main.layout" template="/WEB-INF/layout.jsp"/>
                <definition name="admin.Cat.viewTree" extends="main.layout">
                  <put-attribute name="body" value="/WEB-INF/cat/tree.jsp"/>
                </definition>
              </tiles-definitions>"#,
        );

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![tiles],
        };
        let (g, report) = build(&inputs);

        assert!(report.unresolved_includes.is_empty());
        assert_eq!(g.actions.len(), 1);

        // C1: action → bean-id → FQCN
        assert_eq!(
            resolve_action_class(&g, "/do/Cat/viewTree").as_deref(),
            Some("com.x.CategoryAction")
        );
        // view: action → tiles def → JSP
        assert_eq!(
            resolve_action_view(&g, "/do/Cat/viewTree").as_deref(),
            Some("/WEB-INF/cat/tree.jsp")
        );

        // seam mapping is 1:1
        assert_eq!(RelKind::ActionToClass.into_index(), bennu_index::prelude::RelationKind::ActionToClass);
        assert_eq!(RelKind::BeanIdToImpl.into_index(), bennu_index::prelude::RelationKind::BeanIdToImpl);
        assert_eq!(RelKind::ResultToView.into_index(), bennu_index::prelude::RelationKind::ResultToView);
    }
}
