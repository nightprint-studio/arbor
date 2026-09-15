//! Apache Tiles config parser — `<definition name template>` (+ `extends` +
//! `<put-attribute name="body">`) → the JSP the definition renders.
//!
//! In this codebase 96/97 definitions use `extends="main.layout"` and carry the real
//! per-action view in `<put-attribute name="body" value="…jsp">`, not in a `template=`
//! attribute (docs §8 #2, the Tiles two-hop indirection). So resolving a Tiles result
//! to a JSP means: prefer the definition's own `template=`, else its `body` JSP, else
//! walk `extends` to the parent layout.

use std::collections::HashMap;
use std::path::Path;

use crate::model::TilesDefRecord;
use crate::xml;

/// Parse every `<definition>` in a tiles.xml `file`.
pub fn parse_file(file: &Path, out: &mut Vec<TilesDefRecord>) {
    let text = match crate::io::read_to_string_lf(file) {
        Ok(t) => t,
        Err(_) => return,
    };
    let doc = match xml::parse(&text) {
        Some(d) => d,
        None => return,
    };
    let source_file = file.display().to_string();
    for def in doc.descendants().filter(|n| n.has_tag_name("definition")) {
        let name = def.attribute("name").unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let template = def.attribute("template").unwrap_or("").to_string();
        let extends = def.attribute("extends").unwrap_or("").to_string();
        let body_jsp = def
            .children()
            .filter(|n| n.has_tag_name("put-attribute"))
            .find(|n| n.attribute("name") == Some("body"))
            .and_then(|n| n.attribute("value"))
            .unwrap_or("")
            .to_string();
        out.push(TilesDefRecord { name, template, extends, body_jsp, source_file: source_file.clone() });
    }
}

/// Resolve a Tiles definition name to the JSP it renders. Prefers the def's own view
/// (`template=` or `body` put-attribute); if the def has neither but `extends` a parent,
/// walks up to the parent's view (the layout template). Returns the JSP path, or `None`
/// when the def is unknown or its whole chain carries no JSP.
pub fn resolve_view<'a>(
    defs: &'a HashMap<&str, &TilesDefRecord>,
    name: &str,
    depth: usize,
) -> Option<&'a str> {
    if depth > 16 {
        return None;
    }
    let def = defs.get(name)?;
    let own = def.view_jsp();
    if !own.is_empty() {
        return Some(own);
    }
    if def.extends.is_empty() {
        return None;
    }
    resolve_view(defs, &def.extends, depth + 1)
}

/// Index definitions by name for resolution.
pub fn index(defs: &[TilesDefRecord]) -> HashMap<&str, &TilesDefRecord> {
    defs.iter().map(|d| (d.name.as_str(), d)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, template: &str, extends: &str, body: &str) -> TilesDefRecord {
        TilesDefRecord {
            name: name.into(),
            template: template.into(),
            extends: extends.into(),
            body_jsp: body.into(),
            source_file: String::new(),
        }
    }

    #[test]
    fn resolve_prefers_body_then_walks_extends() {
        let defs = vec![
            def("main.layout", "/WEB-INF/layout.jsp", "", ""),
            def("admin.Cat.viewTree", "", "main.layout", "/WEB-INF/cat/tree.jsp"),
            def("admin.Cat.onlyExtends", "", "main.layout", ""),
        ];
        let idx = index(&defs);
        // body put-attribute wins as the per-action view
        assert_eq!(resolve_view(&idx, "admin.Cat.viewTree", 0), Some("/WEB-INF/cat/tree.jsp"));
        // def with no own view falls back to the extended layout template
        assert_eq!(resolve_view(&idx, "admin.Cat.onlyExtends", 0), Some("/WEB-INF/layout.jsp"));
        // direct template
        assert_eq!(resolve_view(&idx, "main.layout", 0), Some("/WEB-INF/layout.jsp"));
        // unknown def
        assert_eq!(resolve_view(&idx, "nope", 0), None);
    }

    #[test]
    fn parses_definition_with_body_put_attribute() {
        let xml = r#"<tiles-definitions>
            <definition name="admin.Cat.viewTree" extends="main.layout">
              <put-attribute name="title" value="t"/>
              <put-attribute name="body" value="/WEB-INF/cat/tree.jsp"/>
            </definition>
          </tiles-definitions>"#;
        let file = crate::test_support::tmp("tiles-def.xml", xml);
        let mut out = Vec::new();
        parse_file(&file, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "admin.Cat.viewTree");
        assert_eq!(out[0].extends, "main.layout");
        assert_eq!(out[0].body_jsp, "/WEB-INF/cat/tree.jsp");
        assert_eq!(out[0].view_jsp(), "/WEB-INF/cat/tree.jsp");
    }
}
