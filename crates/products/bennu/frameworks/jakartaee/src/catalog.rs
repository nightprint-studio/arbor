//! The catalogue rows: `"beans"`, with the points each bean satisfies underneath, and `"endpoints"`,
//! the servlet and filter URL patterns — the bare kind every framework contributes to.

use bennu_ext::prelude::ExtEntry;

use crate::beans::{Bean, BeanOrigin};
use crate::matching::{fit, Fit};
use crate::model::Model;
use crate::text::simple_name;
use crate::types::TypeView;

/// Every explicit bean and producer. `spring`: Spring answers injection, so no CDI children.
pub fn bean_rows(model: &Model, spring: bool) -> Vec<ExtEntry> {
    let view = TypeView::of(&model.table);
    let mut rows: Vec<ExtEntry> = model
        .beans
        .iter()
        .filter(|b| b.explicit || b.origin != BeanOrigin::Class)
        .map(|b| bean_row(&view, model, b, spring))
        .collect();
    rows.sort_by(|a, b| a.primary.cmp(&b.primary).then_with(|| a.id.cmp(&b.id)));
    rows
}

fn bean_row(view: &TypeView<'_>, model: &Model, b: &Bean, spring: bool) -> ExtEntry {
    let mut tags = Vec::new();
    if let Some(name) = &b.quals.named {
        tags.push(format!("@Named(\"{name}\")"));
    }
    tags.extend(b.quals.custom.iter().map(|c| format!("@{}", simple_name(&c.fqcn))));
    if b.alternative {
        tags.push("alternative".to_string());
    }
    let children = if spring {
        Vec::new()
    } else {
        model
            .points
            .iter()
            .filter(|p| fit(view, b, p) != Fit::No)
            .map(|p| ExtEntry {
                id: p.id(),
                primary: p.label(),
                secondary: p.type_text.clone(),
                kind: "inject".to_string(),
                file: Some(p.file.clone()),
                offset: Some(p.offset),
                line: Some(p.line),
                tags: Vec::new(),
                children: Vec::new(),
            })
            .collect()
    };
    ExtEntry {
        id: b.id.clone(),
        primary: b.label(),
        secondary: b.owner.clone(),
        kind: b.badge.to_string(),
        file: Some(b.file.clone()),
        offset: Some(b.offset),
        line: Some(b.line),
        tags,
        children,
    }
}

/// One row per servlet or filter URL pattern, from annotations and `web.xml` alike.
pub fn endpoint_rows(model: &Model) -> Vec<ExtEntry> {
    let mut rows: Vec<ExtEntry> = model
        .web
        .iter()
        .flat_map(|c| {
            let owner = if c.class.is_empty() { c.name.clone() } else { simple_name(&c.class).to_string() };
            c.patterns.iter().map(move |p| ExtEntry {
                id: format!("{} {}", c.kind.as_str(), p.value),
                primary: p.value.clone(),
                secondary: owner.clone(),
                kind: c.kind.as_str().to_string(),
                file: Some(c.file.clone()),
                offset: Some(p.start),
                line: Some(p.line),
                tags: if c.from_xml { vec!["web.xml".to_string()] } else { Vec::new() },
                children: Vec::new(),
            })
        })
        .collect();
    rows.sort_by(|a, b| a.primary.cmp(&b.primary).then_with(|| a.kind.cmp(&b.kind)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::Project;

    fn project() -> Project {
        Project::cdi_with_xml(
            &[
                ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
                ("/p/a/Impl.java", "package a;\n@Named(\"svc\") @Alternative @ApplicationScoped public class Impl implements Svc {\n  @Produces Clock clock() { return null; }\n}\n"),
                ("/p/a/Use.java", "package a;\nimport jakarta.servlet.http.HttpServlet;\n@WebServlet(\"/orders/*\") public class Use extends HttpServlet { @Inject Svc svc; }\n"),
                ("/p/a/Plain.java", "package a;\npublic class Plain implements Svc {}\n"),
            ],
            &[("/p/src/main/webapp/WEB-INF/web.xml", "<web-app><filter-mapping><filter-name>enc</filter-name><url-pattern>/*</url-pattern></filter-mapping></web-app>")],
        )
    }

    #[test]
    fn a_bean_row_carries_its_badge_tags_and_the_points_it_satisfies() {
        let rows = bean_rows(&project().model, false);
        assert_eq!(rows.iter().map(|r| r.primary.as_str()).collect::<Vec<_>>(), ["Impl", "Impl.clock()"], "a plain class is no row");
        let impl_row = &rows[0];
        assert_eq!((impl_row.id.as_str(), impl_row.kind.as_str(), impl_row.secondary.as_str()), ("a.Impl", "@ApplicationScoped", "a.Impl"));
        assert_eq!(impl_row.tags, ["@Named(\"svc\")", "alternative"]);
        let child = &impl_row.children[0];
        assert_eq!((child.id.as_str(), child.primary.as_str(), child.kind.as_str()), ("a.Use#svc", "Use.svc", "inject"));
        assert_eq!(rows[1].id, "a.Impl#clock");
        assert_eq!(rows[1].kind, "@Produces");
        assert!(bean_rows(&project().model, true)[0].children.is_empty(), "Spring answers injection");
    }

    #[test]
    fn servlet_and_filter_patterns_are_endpoint_rows() {
        let rows = endpoint_rows(&project().model);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["FILTER /*", "SERVLET /orders/*"]);
        assert_eq!((rows[1].secondary.as_str(), rows[1].kind.as_str()), ("Use", "SERVLET"));
        assert_eq!(rows[0].tags, ["web.xml"]);
    }
}
