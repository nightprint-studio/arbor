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

use crate::model::{
    InterceptorRefUse, RelKind, Relation, StatementKind, StatementRecord, ValidationRecord,
    WebConfigGraph,
};
use crate::{mybatis, spring, struts, tiles, validation};

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
    /// `<Action>-validation.xml` files (interceptor defs are parsed out of the struts
    /// fragments themselves, so they need no separate input list).
    pub validation_files: Vec<PathBuf>,
    /// MyBatis mapper XML files (root `<mapper namespace=…>`), project-wide.
    pub mapper_files: Vec<PathBuf>,
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
        // Interceptors were collected in the same include-graph pass.
        graph.interceptors.extend(parse.interceptors.interceptors);
        graph.interceptor_stacks.extend(parse.interceptors.stacks);
        graph.interceptor_refs.extend(parse.interceptors.refs);
        graph.relations.extend(parse.interceptors.relations);
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

    for f in &inputs.validation_files {
        if let Some(rec) = validation::parse_file(f) {
            graph.validations.push(rec);
        }
    }

    for f in &inputs.mapper_files {
        if let Some(parse) = mybatis::parse_mybatis_file(f) {
            graph.mappers.extend(parse.mappers);
            graph.statements.extend(parse.statements);
            graph.relations.extend(parse.relations);
        }
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

/// A resolved `<interceptor>` / `<interceptor-stack>` declaration site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterceptorDef<'a> {
    pub name: &'a str,
    /// The struts fragment the def lives in (go-to file).
    pub source_file: &'a str,
    /// Byte offset of the def's `name` attribute value (go-to target).
    pub name_offset: usize,
    /// The impl class FQCN for a plain `<interceptor>`, `None` for a stack (or a built-in
    /// with no declared class).
    pub class: Option<&'a str>,
    /// True when the name resolves to an `<interceptor-stack>`, false for an `<interceptor>`.
    pub is_stack: bool,
}

/// Resolve an `<interceptor-ref name>` to the def it names — the file + offset of the
/// `<interceptor>` / `<interceptor-stack>` declaration, and (for a plain interceptor) its
/// impl class FQCN. `None` when the name is provided only by a dependency jar (e.g. the
/// built-in `defaultStack`) → the caller treats it as Inconclusive, never a hard "missing".
pub fn resolve_interceptor_ref<'a>(
    graph: &'a WebConfigGraph,
    name: &str,
) -> Option<InterceptorDef<'a>> {
    if let Some(i) = graph.interceptors.iter().find(|i| i.name == name) {
        return Some(InterceptorDef {
            name: &i.name,
            source_file: &i.source_file,
            name_offset: i.name_offset,
            class: (!i.class.is_empty()).then_some(i.class.as_str()),
            is_stack: false,
        });
    }
    let s = graph.interceptor_stacks.iter().find(|s| s.name == name)?;
    Some(InterceptorDef {
        name: &s.name,
        source_file: &s.source_file,
        name_offset: s.name_offset,
        class: None,
        is_stack: true,
    })
}

/// Every `<interceptor-ref>` use naming `name` (in a stack, an action, or a package
/// default) — the find-usages set for an interceptor / stack def.
pub fn interceptor_usages<'a>(graph: &'a WebConfigGraph, name: &str) -> Vec<&'a InterceptorRefUse> {
    graph.interceptor_refs.iter().filter(|r| r.ref_name == name).collect()
}

/// The validation rulesets bound to an action class by its **simple name** (`FooAction`) —
/// usually one, or a base + per-alias set. The caller resolves the simple name to a project
/// FQCN via the Java index.
pub fn validations_for_class<'a>(
    graph: &'a WebConfigGraph,
    simple_name: &str,
) -> Vec<&'a ValidationRecord> {
    graph.validations.iter().filter(|v| v.action_class == simple_name).collect()
}

/// A resolved MyBatis statement declaration site (go-to XML target). Mirrors
/// [`InterceptorDef`] — a graph-only, by-name resolution (no fst symbol).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementTarget<'a> {
    /// The mapper `.xml` the statement lives in (go-to file).
    pub file: &'a str,
    /// Byte offset of the statement `id` value start (go-to target).
    pub offset: usize,
    /// Which element declared it (`select`/`insert`/`update`/`delete`).
    pub kind: StatementKind,
    /// The statement `id` (== the mapped method name).
    pub id: &'a str,
}

/// Go-to XML: a mapper interface method → its `<select|...|delete id>` statement (the
/// file + byte offset of the id value). `None` when `interface_fqcn` isn't a known mapper
/// namespace or has no statement with `id == method`.
pub fn statement_for_method<'a>(
    graph: &'a WebConfigGraph,
    interface_fqcn: &str,
    method: &str,
) -> Option<StatementTarget<'a>> {
    let s = graph
        .statements
        .iter()
        .find(|s| s.mapper_namespace == interface_fqcn && s.id == method)?;
    let mapper = graph.mappers.iter().find(|m| m.namespace == interface_fqcn)?;
    Some(StatementTarget { file: &mapper.source_file, offset: s.start, kind: s.kind, id: &s.id })
}

/// find-usages / outline: every statement declared in a mapper interface's XML.
pub fn methods_for_mapper<'a>(
    graph: &'a WebConfigGraph,
    interface_fqcn: &str,
) -> Vec<&'a StatementRecord> {
    graph.statements.iter().filter(|s| s.mapper_namespace == interface_fqcn).collect()
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

        let mapper = crate::test_support::tmp(
            "g-FooMapper.xml",
            r#"<mapper namespace="com.x.FooMapper">
                <select id="findById">select 1</select>
                <insert id="insert">insert</insert>
              </mapper>"#,
        );

        let inputs = WebInputs {
            struts_roots: vec![struts],
            resource_roots: vec![],
            spring_files: vec![beans],
            tiles_files: vec![tiles],
            validation_files: vec![],
            mapper_files: vec![mapper],
        };
        let (g, report) = build(&inputs);

        assert!(report.unresolved_includes.is_empty());
        assert_eq!(g.actions.len(), 1);

        // MyBatis: mapper namespace + statements parsed, method↔statement resolution.
        assert_eq!(g.mappers.len(), 1);
        assert_eq!(g.statements.len(), 2);
        let target = statement_for_method(&g, "com.x.FooMapper", "findById").expect("findById");
        assert_eq!(target.kind, StatementKind::Select);
        assert!(target.offset > 0);
        assert_eq!(methods_for_mapper(&g, "com.x.FooMapper").len(), 2);
        assert!(statement_for_method(&g, "com.x.FooMapper", "ghost").is_none());

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
