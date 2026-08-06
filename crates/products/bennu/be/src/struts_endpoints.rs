//! Struts actions as rows of the **Endpoints** catalog.
//!
//! Contributed by the host rather than by a `FrameworkExtension`, for the same reason the
//! library beans are: the config graph belongs to the host. It is discovered by
//! [`crate::web_discovery`], parsed once during the index build, and kept resolved on the
//! project slot — an extension would have to re-walk and re-parse every struts fragment to
//! produce a second copy of it.
//!
//! The rows join the **generic** `endpoints` catalog, alongside Spring's request mappings. An
//! action is a route in every sense a `@GetMapping` is one; the reason it is harder to read is
//! that its URL, its class and its response live in three files, which is exactly what a row
//! here collapses.

use bennu_ext::prelude::{ExtEntry, ExtStat};
use bennu_web::prelude::{endpoints, Endpoint};

use crate::index_service::IndexService;

/// The catalog kind these rows answer, namespaced like every other.
pub const CATALOG_KIND: &str = "struts.endpoints";

/// The endpoints of the project rooted at `root`. Empty when the project has no config graph —
/// either it is not a Struts project, or its index has not landed yet.
pub fn catalog_entries(root: &str) -> Vec<ExtEntry> {
    endpoints_of(root).into_iter().map(entry).collect()
}

/// The headline count, for the overview. `None` rather than zero when there is nothing: the
/// frontend decides whether a panel is worth offering from this, and a door onto an empty list
/// is worse than no door.
pub fn stat(root: &str) -> Option<ExtStat> {
    let count = endpoints_of(root).len();
    (count > 0).then(|| ExtStat {
        label: "Struts actions".to_string(),
        value: count,
        catalog: Some(CATALOG_KIND.to_string()),
    })
}

fn endpoints_of(root: &str) -> Vec<Endpoint> {
    match IndexService::global().config_for_root(root) {
        Some(cfg) => endpoints(cfg.graph()),
        None => Vec::new(),
    }
}

/// One action as a catalog row, expanding into the chain the request takes through it.
///
/// The children ARE the request map. Reading them top to bottom is the whole journey: which
/// interceptors run, then — per outcome — what the action returns and which page that finally
/// renders. Nesting rather than a second panel, because `ExtEntry::children` exists so a list
/// with detail rows renders in the one catalog panel every other extension already uses.
fn entry(e: Endpoint) -> ExtEntry {
    let mut children: Vec<ExtEntry> = e
        .interceptors
        .iter()
        .map(|name| ExtEntry {
            id: format!("{}!{name}", e.url),
            primary: name.clone(),
            secondary: String::new(),
            kind: "interceptor".to_string(),
            ..ExtEntry::default()
        })
        .collect();

    children.extend(e.results.iter().map(|r| ExtEntry {
        id: format!("{}#{}", e.url, r.name),
        primary: r.name.clone(),
        // What the config says, then — when they differ — the page it ends up at. A Tiles
        // definition name tells you nothing about which JSP you are about to open.
        secondary: match (&r.view, &r.target) {
            (view, target) if view.is_empty() || view == target => target.clone(),
            (view, target) => format!("{target} → {view}"),
        },
        kind: if r.result_type.is_empty() { "dispatcher".to_string() } else { r.result_type.clone() },
        // The row jumps to the page it renders, when it renders one — which is where you were
        // going anyway.
        file: (!r.view.is_empty()).then(|| r.view.clone()),
        tags: r.inferred.then(|| vec!["computed".to_string()]).unwrap_or_default(),
        ..ExtEntry::default()
    }));

    let mut tags = Vec::new();
    if e.wildcard {
        tags.push("wildcard".to_string());
    }
    if !e.namespace.is_empty() {
        tags.push(e.namespace.clone());
    }

    ExtEntry {
        id: e.url.clone(),
        primary: e.url.clone(),
        secondary: e.handler(),
        // The badge slot holds the HTTP verb for a Spring route; an action answers any of them,
        // and saying so is more honest than leaving the column blank in a mixed list.
        kind: "ACTION".to_string(),
        file: Some(e.file.clone()),
        offset: Some(e.offset),
        line: None,
        tags,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_web::prelude::ResultStep;

    fn endpoint() -> Endpoint {
        Endpoint {
            url: "/do/Cat/save".into(),
            namespace: "/do/Cat".into(),
            name: "save".into(),
            method: "save".into(),
            bean_id: "categoryAction".into(),
            class_fqcn: "com.acme.CategoryAction".into(),
            file: "/p/struts.xml".into(),
            offset: 90,
            wildcard: false,
            interceptors: vec!["validationStack".into()],
            results: vec![
                ResultStep {
                    name: "success".into(),
                    result_type: "tiles".into(),
                    target: "admin.Cat.tree".into(),
                    view: "/WEB-INF/jsp/tree.jsp".into(),
                    inferred: false,
                },
                ResultStep {
                    name: "input".into(),
                    result_type: "dispatcher".into(),
                    target: "/WEB-INF/jsp/entry.jsp".into(),
                    view: "/WEB-INF/jsp/entry.jsp".into(),
                    inferred: false,
                },
            ],
        }
    }

    #[test]
    fn the_row_is_the_url_and_what_handles_it() {
        let r = entry(endpoint());
        assert_eq!(r.primary, "/do/Cat/save");
        assert_eq!(r.secondary, "com.acme.CategoryAction#save");
        assert_eq!(r.kind, "ACTION");
        assert_eq!(r.file.as_deref(), Some("/p/struts.xml"));
        assert_eq!(r.offset, Some(90));
        assert_eq!(r.tags, ["/do/Cat"]);
    }

    #[test]
    fn the_children_are_the_request_map_interceptors_first() {
        let r = entry(endpoint());
        let kinds: Vec<&str> = r.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["interceptor", "tiles", "dispatcher"]);
    }

    #[test]
    fn a_tiles_result_shows_the_definition_and_the_page_it_reaches() {
        let r = entry(endpoint());
        let tiles = r.children.iter().find(|c| c.kind == "tiles").unwrap();
        assert_eq!(tiles.secondary, "admin.Cat.tree → /WEB-INF/jsp/tree.jsp");
        assert_eq!(tiles.file.as_deref(), Some("/WEB-INF/jsp/tree.jsp"), "the row opens the page");
    }

    #[test]
    fn a_dispatcher_result_is_not_written_twice() {
        let r = entry(endpoint());
        let d = r.children.iter().find(|c| c.kind == "dispatcher").unwrap();
        assert_eq!(d.secondary, "/WEB-INF/jsp/entry.jsp", "target and view are the same thing");
    }
}
