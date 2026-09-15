//! Servlets and filters: what a web module deploys, and at which URLs.
//!
//! Two sources, one list — `@WebServlet` / `@WebFilter` and `web.xml` — merged the way Tomcat merges
//! them: a `web.xml` mapping for a servlet name replaces that servlet's annotation patterns, and a
//! `metadata-complete` descriptor switches the annotations of its module off altogether.
//!
//! A module is the directory before `/src/`. Two wars in one repository may map the same URL and
//! never meet, so nothing here is compared across modules.

use std::collections::{HashMap, HashSet};

use crate::known;
use crate::text::{line_of, module_root};
use crate::types::Unit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebKind {
    Servlet,
    Filter,
}

impl WebKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WebKind::Servlet => "SERVLET",
            WebKind::Filter => "FILTER",
        }
    }
}

/// One URL pattern, with the span of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlPattern {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub line: u32,
}

/// A servlet or a filter with its mapped patterns.
#[derive(Debug, Clone)]
pub struct WebComponent {
    pub kind: WebKind,
    /// The servlet or filter name — what the container keys mappings by.
    pub name: String,
    /// The implementing class, as written; empty when a descriptor names none.
    pub class: String,
    pub patterns: Vec<UrlPattern>,
    pub file: String,
    pub offset: usize,
    pub line: u32,
    pub module: String,
    pub from_xml: bool,
    /// The annotation (or the mapping element) that declares it.
    pub marker_start: usize,
    pub marker_end: usize,
}

/// The servlets and filters a Java unit declares by annotation.
pub fn annotated_components(unit: &Unit) -> Vec<WebComponent> {
    let facts = &unit.facts;
    let module = module_root(&facts.file).to_string();
    let mut out = Vec::new();
    for t in &facts.types {
        for (kind, simple, name_element) in [(WebKind::Servlet, "WebServlet", "name"), (WebKind::Filter, "WebFilter", "filterName")] {
            let Some(ann) = known::find(&t.annotations, facts, simple) else { continue };
            let name = ann
                .strings_for(name_element)
                .next()
                .map(|s| s.value.clone())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| t.fqcn.clone());
            let patterns = ann
                .strings
                .iter()
                .filter(|s| matches!(s.element.as_str(), "" | "value" | "urlPatterns"))
                .map(|s| UrlPattern { value: s.value.clone(), start: s.start, end: s.end, line: line_of(&unit.text, s.start) })
                .collect();
            out.push(WebComponent {
                kind,
                name,
                class: t.fqcn.clone(),
                patterns,
                file: facts.file.clone(),
                offset: t.name_offset,
                line: line_of(&unit.text, t.name_offset),
                module: module.clone(),
                from_xml: false,
                marker_start: ann.start,
                marker_end: ann.end,
            });
        }
    }
    out
}

/// What one `web.xml` says.
#[derive(Debug, Clone)]
pub struct WebXml {
    pub module: String,
    /// `metadata-complete="true"`: the module's annotations are not read by the container.
    pub metadata_complete: bool,
    pub components: Vec<WebComponent>,
}

/// Whether a path is a deployment descriptor this crate reads — not a build output's copy.
pub fn is_web_xml(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case("web.xml"))
        && !path.contains("/target/")
        && !path.contains("/build/")
}

/// Read the servlet and filter mappings of a `web.xml`.
pub fn parse_web_xml(path: &str, text: &str) -> WebXml {
    let clean = blank_comments(text);
    let module = module_root(path).to_string();
    let metadata_complete = clean.find("<web-app").is_some_and(|i| {
        let end = clean[i..].find('>').map(|j| i + j).unwrap_or(clean.len());
        let tag = &clean[i..end];
        tag.contains("metadata-complete=\"true\"") || tag.contains("metadata-complete='true'")
    });
    let mut components = Vec::new();
    let shapes = [
        (WebKind::Servlet, "servlet", "servlet-name", "servlet-class", "servlet-mapping"),
        (WebKind::Filter, "filter", "filter-name", "filter-class", "filter-mapping"),
    ];
    for (kind, decl, name_tag, class_tag, mapping) in shapes {
        let classes: HashMap<String, String> = blocks(&clean, decl)
            .into_iter()
            .filter_map(|b| {
                let (name, _, _) = children(&clean, b, name_tag).into_iter().next()?;
                let class = children(&clean, b, class_tag).into_iter().next().map(|c| c.0).unwrap_or_default();
                Some((name, class))
            })
            .collect();
        for b in blocks(&clean, mapping) {
            let Some((name, name_start, _)) = children(&clean, b, name_tag).into_iter().next() else { continue };
            let patterns: Vec<UrlPattern> = children(&clean, b, "url-pattern")
                .into_iter()
                .map(|(value, start, end)| UrlPattern { value, start, end, line: line_of(text, start) })
                .collect();
            if patterns.is_empty() {
                continue;
            }
            components.push(WebComponent {
                kind,
                class: classes.get(&name).cloned().unwrap_or_default(),
                name,
                patterns,
                file: path.to_string(),
                offset: name_start,
                line: line_of(text, name_start),
                module: module.clone(),
                from_xml: true,
                marker_start: b.0,
                marker_end: b.1,
            });
        }
    }
    WebXml { module, metadata_complete, components }
}

/// Comments replaced by spaces, byte for byte, so every offset still indexes the original.
fn blank_comments(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut from = 0;
    while let Some(i) = text[from..].find("<!--") {
        let start = from + i;
        let end = text[start..].find("-->").map(|j| start + j + 3).unwrap_or(text.len());
        for b in &mut bytes[start..end] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
        from = end;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_string())
}

/// The content spans of every `<tag>…</tag>` in `text` — `<servlet>` and not `<servlet-name>`.
fn blocks(text: &str, tag: &str) -> Vec<(usize, usize)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(&open) {
        let after = from + i + open.len();
        if !text[after..].starts_with(|c: char| c == '>' || c.is_whitespace()) {
            from = after;
            continue;
        }
        let Some(gt) = text[after..].find('>') else { break };
        let content = after + gt + 1;
        let Some(c) = text[content..].find(&close) else { break };
        out.push((content, content + c));
        from = content + c + close.len();
    }
    out
}

/// Every `<tag>` inside a block, as trimmed text with its span.
fn children(text: &str, block: (usize, usize), tag: &str) -> Vec<(String, usize, usize)> {
    let inner = &text[block.0..block.1];
    blocks(inner, tag)
        .into_iter()
        .map(|(s, e)| {
            let raw = &inner[s..e];
            let start = block.0 + s + (raw.len() - raw.trim_start().len());
            let value = raw.trim().to_string();
            let end = start + value.len();
            (value, start, end)
        })
        .collect()
}

/// Why a URL pattern is refused, or `None` when a container accepts it.
///
/// Tomcat's rule, the most lenient of the common containers — so what this refuses, all of them do:
/// `""`, a pattern starting with `/` that has no `*.` in it, or `*.ext` with no `/`.
pub fn pattern_problem(pattern: &str) -> Option<&'static str> {
    if pattern.contains('\n') || pattern.contains('\r') {
        return Some("a URL pattern cannot contain a line break");
    }
    if pattern.is_empty() {
        return None;
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return ext.contains('/').then_some("an extension mapping (`*.ext`) cannot contain a `/`");
    }
    if pattern.starts_with('/') {
        return pattern
            .contains("*.")
            .then_some("`*.` only makes an extension mapping at the start of the pattern — `/…/*.ext` is not one");
    }
    Some("a URL pattern must start with `/` or `*.`")
}

/// A pattern two different servlets claim in one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub component: usize,
    pub pattern: usize,
    /// The other servlet's name.
    pub other: String,
}

/// Every servlet pattern another servlet of the same module also maps.
pub fn clashes(components: &[WebComponent], metadata_complete: &HashSet<String>) -> Vec<Clash> {
    let xml_mapped: HashSet<(String, String)> = components
        .iter()
        .filter(|c| c.from_xml && c.kind == WebKind::Servlet)
        .map(|c| (c.module.clone(), c.name.clone()))
        .collect();
    let mut owners: HashMap<(&str, &str), Vec<(usize, usize)>> = HashMap::new();
    for (ci, c) in components.iter().enumerate() {
        let effective = c.kind == WebKind::Servlet
            && (c.from_xml
                || (!metadata_complete.contains(&c.module) && !xml_mapped.contains(&(c.module.clone(), c.name.clone()))));
        if !effective {
            continue;
        }
        for (pi, p) in c.patterns.iter().enumerate() {
            if pattern_problem(&p.value).is_none() {
                owners.entry((c.module.as_str(), p.value.as_str())).or_default().push((ci, pi));
            }
        }
    }
    let mut out = Vec::new();
    for sites in owners.values() {
        for &(ci, pi) in sites {
            if let Some(&(oi, _)) = sites.iter().find(|(oi, _)| components[*oi].name != components[ci].name) {
                out.push(Clash { component: ci, pattern: pi, other: components[oi].name.clone() });
            }
        }
    }
    out.sort_by_key(|c| (c.component, c.pattern));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEB_XML: &str = r#"<web-app>
  <!-- <servlet-mapping><servlet-name>ghost</servlet-name><url-pattern>/api/*</url-pattern></servlet-mapping> -->
  <servlet><servlet-name>legacy</servlet-name><servlet-class>com.acme.LegacyServlet</servlet-class></servlet>
  <servlet-mapping>
    <servlet-name>legacy</servlet-name>
    <url-pattern> /api/* </url-pattern>
  </servlet-mapping>
  <filter-mapping><filter-name>enc</filter-name><url-pattern>/*</url-pattern></filter-mapping>
</web-app>"#;

    const XML_PATH: &str = "/r/web/src/main/webapp/WEB-INF/web.xml";

    fn annotated(path: &str, src: &str) -> Vec<WebComponent> {
        annotated_components(&Unit::new(path, src).unwrap())
    }

    #[test]
    fn a_pattern_is_judged_by_the_most_lenient_container_rule() {
        for ok in ["", "/", "/*", "/api/*", "/exact", "*.do", "/odd*"] {
            assert_eq!(pattern_problem(ok), None, "{ok}");
        }
        for bad in ["api/*", "*.do/x", "/x/*.do", "orders"] {
            assert!(pattern_problem(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn web_xml_mappings_are_read_with_their_spans_and_comments_ignored() {
        let x = parse_web_xml(XML_PATH, WEB_XML);
        assert_eq!(x.module, "/r/web");
        assert!(!x.metadata_complete);
        assert_eq!(x.components.len(), 2);
        let servlet = &x.components[0];
        assert_eq!((servlet.kind, servlet.name.as_str(), servlet.class.as_str()), (WebKind::Servlet, "legacy", "com.acme.LegacyServlet"));
        let p = &servlet.patterns[0];
        assert_eq!(&WEB_XML[p.start..p.end], "/api/*");
        assert_eq!(x.components[1].kind, WebKind::Filter);
    }

    #[test]
    fn annotations_are_read_with_their_names() {
        let c = annotated(
            "/r/web/src/main/java/a/S.java",
            "package a;\nimport jakarta.servlet.annotation.*;\n@WebServlet(name = \"orders\", urlPatterns = {\"/orders\", \"/orders/*\"}) public class S {}\n@WebFilter(\"/*\") public class F {}\n",
        );
        assert_eq!(c[0].name, "orders");
        assert_eq!(c[0].patterns.iter().map(|p| p.value.as_str()).collect::<Vec<_>>(), ["/orders", "/orders/*"]);
        assert_eq!((c[1].kind, c[1].name.as_str()), (WebKind::Filter, "a.F"));
    }

    #[test]
    fn two_servlets_on_one_pattern_clash_and_filters_never_do() {
        let mut all = parse_web_xml(XML_PATH, WEB_XML).components;
        all.extend(annotated(
            "/r/web/src/main/java/a/Api.java",
            "package a;\nimport javax.servlet.annotation.*;\n@WebServlet(\"/api/*\") public class Api {}\n@WebFilter(\"/*\") public class F {}\n",
        ));
        let found = clashes(&all, &HashSet::new());
        assert_eq!(found.len(), 2, "both sides are reported: {found:?}");
        assert!(found.iter().any(|c| c.other == "legacy"));
    }

    #[test]
    fn a_web_xml_mapping_replaces_the_same_servlets_annotation() {
        let mut all = parse_web_xml(XML_PATH, WEB_XML).components;
        all.extend(annotated(
            "/r/web/src/main/java/a/Api.java",
            "package a;\nimport javax.servlet.annotation.*;\n@WebServlet(name = \"legacy\", value = \"/api/*\") public class Api {}\n",
        ));
        assert!(clashes(&all, &HashSet::new()).is_empty());
    }

    #[test]
    fn different_modules_and_metadata_complete_never_clash() {
        let mut all = parse_web_xml(XML_PATH, WEB_XML).components;
        all.extend(annotated("/r/other/src/main/java/a/Api.java", "package a;\nimport javax.servlet.annotation.*;\n@WebServlet(\"/api/*\") public class Api {}\n"));
        assert!(clashes(&all, &HashSet::new()).is_empty(), "another war");
        let mut same = parse_web_xml(XML_PATH, WEB_XML).components;
        same.extend(annotated("/r/web/src/main/java/a/Api.java", "package a;\nimport javax.servlet.annotation.*;\n@WebServlet(\"/api/*\") public class Api {}\n"));
        let complete: HashSet<String> = ["/r/web".to_string()].into_iter().collect();
        assert!(clashes(&same, &complete).is_empty(), "the container never reads the annotation");
    }
}
