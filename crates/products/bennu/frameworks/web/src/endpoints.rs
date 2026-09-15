//! Actions as **endpoints** — the URL a request hits, and what happens to it after that.
//!
//! A Struts action is a route in every sense Spring's `@GetMapping` is one: a URL, a class, a
//! method, a response. What makes it harder to read is that none of that is in one place —
//! the URL is a package `namespace` joined to an action `name`, the class is a Spring bean id,
//! the response is a `<result>` naming a Tiles definition that names a JSP. Every one of those
//! hops is a file somebody has to open.
//!
//! This module walks the hops once and hands back the whole chain, so the question "what does
//! `/do/Category/viewTree` actually do" is answered by reading one row instead of four files.
//!
//! Pure over a parsed [`WebConfigGraph`] — no filesystem, no index. Which is also what makes it
//! testable off a fixture graph.

use crate::model::WebConfigGraph;
use crate::{spring, tiles};

/// What one `<result>` sends the request to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStep {
    /// The result name (`success`, `input`, `error`).
    pub name: String,
    /// The result type as declared (`tiles`, `dispatcher`, `chain`, `redirectAction`); empty
    /// when the config leaves it to the package default.
    pub result_type: String,
    /// The target as written — a Tiles definition name, a JSP path, another action.
    pub target: String,
    /// The JSP this result finally renders, when the chain resolves to one. For a `tiles`
    /// result that is the definition's template or body; for a `dispatcher` it is the target
    /// itself. Empty when the chain leaves the project (a redirect, a definition that only
    /// inherits its view from a parent we do not have).
    pub view: String,
    /// True when the target is computed at runtime (a `{1}` backref, a wildcard action) — a
    /// candidate, never a fact.
    pub inferred: bool,
}

/// One action, with everything the request passes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The URL the action answers: `<namespace>/<name>`.
    pub url: String,
    pub namespace: String,
    pub name: String,
    /// The `method` attribute, or `execute` when the config omits it — because that is what
    /// actually runs, and a blank cell here is the reader's job to remember.
    pub method: String,
    /// The `class` attribute, which in a Spring-wired app is a **bean id**, not a class.
    pub bean_id: String,
    /// The implementation FQCN, when the bean id resolves to one.
    pub class_fqcn: String,
    /// The struts fragment declaring the action, and the byte offset of the `<action>` element.
    pub file: String,
    pub offset: usize,
    /// True when the action `name` carries a `*` — its method and results are patterns.
    pub wildcard: bool,
    /// The `<interceptor-ref>`s the action declares for itself, in document order. Empty is the
    /// normal case: the package default applies, and this list is what OVERRIDES it.
    pub interceptors: Vec<String>,
    pub results: Vec<ResultStep>,
}

impl Endpoint {
    /// What runs: `beanId#method`, or just the method when the action declares no class.
    pub fn handler(&self) -> String {
        let owner = if self.class_fqcn.is_empty() { &self.bean_id } else { &self.class_fqcn };
        if owner.is_empty() {
            self.method.clone()
        } else {
            format!("{owner}#{}", self.method)
        }
    }
}

/// Every action in the graph, as an endpoint with its chain resolved.
pub fn endpoints(graph: &WebConfigGraph) -> Vec<Endpoint> {
    let beans = spring::resolve_map(&graph.beans);
    let tiles_idx = tiles::index(&graph.tiles_defs);

    graph
        .actions
        .iter()
        .map(|a| {
            let results = graph
                .results
                .iter()
                .filter(|r| r.action_qualified_name == a.qualified_name)
                .map(|r| ResultStep {
                    name: r.name.clone(),
                    result_type: r.result_type.clone(),
                    target: r.target.clone(),
                    view: view_of(&tiles_idx, &r.result_type, &r.target),
                    inferred: r.is_inferred,
                })
                .collect();
            Endpoint {
                url: a.qualified_name.clone(),
                namespace: a.namespace.clone(),
                name: a.name.clone(),
                // An omitted `method` is `execute` — the framework's default, not "none".
                method: if a.method.is_empty() { "execute".to_string() } else { a.method.clone() },
                bean_id: a.class_ref.clone(),
                class_fqcn: beans.get(&a.class_ref).cloned().unwrap_or_default(),
                file: a.source_file.replace('\\', "/"),
                offset: a.decl_offset,
                wildcard: a.is_wildcard,
                interceptors: graph
                    .interceptor_refs
                    .iter()
                    .filter(|r| r.referrer == a.qualified_name)
                    .map(|r| r.ref_name.clone())
                    .collect(),
                results,
            }
        })
        .collect()
}

/// The JSP a result renders, following the Tiles indirection when there is one.
fn view_of(
    tiles_idx: &std::collections::HashMap<&str, &crate::model::TilesDefRecord>,
    result_type: &str,
    target: &str,
) -> String {
    if target.is_empty() {
        return String::new();
    }
    if result_type == "tiles" {
        return tiles::resolve_view(tiles_idx, target, 0).unwrap_or_default().to_string();
    }
    // A `dispatcher` (or a type-less result, which defaults to one) already names the page. A
    // `chain` / `redirectAction` names another ACTION, and calling that a view would be a lie.
    if matches!(result_type, "" | "dispatcher" | "freemarker" | "velocity") {
        return target.to_string();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActionRecord, BeanRecord, InterceptorRefUse, ResultRecord, TilesDefRecord};

    fn graph() -> WebConfigGraph {
        WebConfigGraph {
            actions: vec![
                ActionRecord {
                    qualified_name: "/do/Cat/viewTree".into(),
                    namespace: "/do/Cat".into(),
                    name: "viewTree".into(),
                    class_ref: "categoryAction".into(),
                    method: String::new(),
                    is_wildcard: false,
                    source_file: r"C:\p\struts.xml".into(),
                    decl_offset: 42,
                },
                ActionRecord {
                    qualified_name: "/do/Cat/save".into(),
                    namespace: "/do/Cat".into(),
                    name: "save".into(),
                    class_ref: "categoryAction".into(),
                    method: "save".into(),
                    is_wildcard: false,
                    source_file: "/p/struts.xml".into(),
                    decl_offset: 90,
                },
            ],
            results: vec![
                ResultRecord {
                    action_qualified_name: "/do/Cat/viewTree".into(),
                    name: "success".into(),
                    result_type: "tiles".into(),
                    target: "admin.Cat.viewTree".into(),
                    is_inferred: false,
                },
                ResultRecord {
                    action_qualified_name: "/do/Cat/save".into(),
                    name: "input".into(),
                    result_type: "dispatcher".into(),
                    target: "/WEB-INF/jsp/entry.jsp".into(),
                    is_inferred: false,
                },
                ResultRecord {
                    action_qualified_name: "/do/Cat/save".into(),
                    name: "success".into(),
                    result_type: "redirectAction".into(),
                    target: "viewTree".into(),
                    is_inferred: false,
                },
            ],
            beans: vec![BeanRecord {
                id: "categoryAction".into(),
                class: "com.acme.CategoryAction".into(),
                parent: String::new(),
                source_file: "/p/beans.xml".into(),
            }],
            tiles_defs: vec![TilesDefRecord {
                name: "admin.Cat.viewTree".into(),
                template: String::new(),
                extends: "main.layout".into(),
                body_jsp: "/WEB-INF/jsp/tree.jsp".into(),
                source_file: "/p/tiles.xml".into(),
            }],
            interceptor_refs: vec![InterceptorRefUse {
                referrer: "/do/Cat/save".into(),
                ref_name: "validationStack".into(),
                is_default: false,
                source_file: "/p/struts.xml".into(),
                name_offset: 5,
            }],
            ..WebConfigGraph::default()
        }
    }

    #[test]
    fn an_action_is_a_url_a_handler_and_a_view() {
        let e = endpoints(&graph());
        let view = e.iter().find(|e| e.name == "viewTree").unwrap();
        assert_eq!(view.url, "/do/Cat/viewTree");
        assert_eq!(view.method, "execute", "an omitted method is what actually runs");
        assert_eq!(view.class_fqcn, "com.acme.CategoryAction", "the bean id resolved");
        assert_eq!(view.handler(), "com.acme.CategoryAction#execute");
        assert_eq!(view.file, "C:/p/struts.xml", "forward-slashed for the editor");
        assert_eq!(view.offset, 42);
    }

    #[test]
    fn a_tiles_result_is_followed_to_the_page_it_renders() {
        let e = endpoints(&graph());
        let view = e.iter().find(|e| e.name == "viewTree").unwrap();
        assert_eq!(view.results[0].target, "admin.Cat.viewTree", "what the config says");
        assert_eq!(view.results[0].view, "/WEB-INF/jsp/tree.jsp", "what it renders");
    }

    #[test]
    fn a_result_that_is_not_a_view_does_not_claim_to_be_one() {
        let e = endpoints(&graph());
        let save = e.iter().find(|e| e.name == "save").unwrap();
        let by_name = |n: &str| save.results.iter().find(|r| r.name == n).unwrap();
        assert_eq!(by_name("input").view, "/WEB-INF/jsp/entry.jsp", "a dispatcher IS the page");
        assert_eq!(by_name("success").view, "", "a redirect names an action, not a view");
        assert_eq!(by_name("success").target, "viewTree", "the target is still shown");
    }

    #[test]
    fn an_action_lists_the_interceptors_it_declares_for_itself() {
        let e = endpoints(&graph());
        assert_eq!(e.iter().find(|e| e.name == "save").unwrap().interceptors, ["validationStack"]);
        assert!(
            e.iter().find(|e| e.name == "viewTree").unwrap().interceptors.is_empty(),
            "no override means the package default applies — not that none run",
        );
    }
}
