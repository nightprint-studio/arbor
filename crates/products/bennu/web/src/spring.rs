//! Spring bean-XML parser — the `<action class="beanId">` → FQCN join (docs §10 C1).
//!
//! In this codebase the Struts action-config fragments and the Spring bean defs live in
//! the *same* `*ActionsConfig.xml` files (a `<beans>` root that also carries struts
//! `<package>`s alongside). We parse `<bean id class parent>` wherever it appears. The
//! id→FQCN map is what turns `<action class="categoryAction">` into
//! `com.agiletec.apsadmin.category.CategoryAction`.

use std::collections::HashMap;
use std::path::Path;

use crate::model::{BeanRecord, RelKind, Relation};
use crate::xml;

/// Result of parsing Spring bean XMLs.
#[derive(Debug, Default)]
pub struct SpringParse {
    pub beans: Vec<BeanRecord>,
    pub relations: Vec<Relation>,
}

/// Parse every `<bean>` in `file`. Tolerant of non-Spring roots (a struts fragment with
/// no beans yields nothing). `<bean>` may be namespaced (`beans:bean`) — we match on the
/// local name.
pub fn parse_file(file: &Path, out: &mut SpringParse) {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(_) => return,
    };
    let doc = match xml::parse(&text) {
        Some(d) => d,
        None => return,
    };
    let source_file = file.display().to_string();
    for node in doc.descendants().filter(|n| n.has_tag_name("bean")) {
        let id = node
            .attribute("id")
            .or_else(|| node.attribute("name"))
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue; // anonymous inner bean — not addressable by id
        }
        let class = node.attribute("class").unwrap_or("").to_string();
        let parent = node.attribute("parent").unwrap_or("").to_string();

        if !class.is_empty() {
            out.relations.push(Relation {
                from: id.clone(),
                to: class.clone(),
                kind: RelKind::BeanIdToImpl,
                inferred: false,
            });
        }
        out.beans.push(BeanRecord { id, class, parent, source_file: source_file.clone() });
    }
}

/// One `<bean class="FQCN">` attribute-value span in an XML fragment — the exact byte
/// range of the FQCN inside the quotes, for a class-rename edit (docs §5 #10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeanClassSpan {
    /// Start byte offset of the `class` attribute value (inside the quotes).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// Every `<bean class="fqcn">` value span in `xml_text` whose class equals `fqcn`
/// **exactly**. Empty on a parse error (skip-and-continue — one malformed fragment never
/// aborts a project-wide rename plan). Powers the class-rename config-aware edit: a
/// Struts `<action class="beanId">` uses a bean-**id**, not the FQCN, so it is correctly
/// never matched here (docs §5 #10).
pub fn bean_class_value_spans(xml_text: &str, fqcn: &str) -> Vec<BeanClassSpan> {
    let Some(doc) = xml::parse(xml_text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("bean")) {
        for attr in node.attributes() {
            if attr.name() == "class" && attr.value() == fqcn {
                let r = attr.range_value();
                out.push(BeanClassSpan { start: r.start, end: r.end });
            }
        }
    }
    out
}

/// Build the id→FQCN resolution map, walking `parent=` chains so a bean that inherits
/// its class from an abstract parent still resolves. Returns `id → FQCN`.
pub fn resolve_map(beans: &[BeanRecord]) -> HashMap<String, String> {
    let by_id: HashMap<&str, &BeanRecord> =
        beans.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut out = HashMap::new();
    for b in beans {
        if let Some(fqcn) = resolve_class(b, &by_id, 0) {
            out.insert(b.id.clone(), fqcn);
        }
    }
    out
}

fn resolve_class(
    bean: &BeanRecord,
    by_id: &HashMap<&str, &BeanRecord>,
    depth: usize,
) -> Option<String> {
    if !bean.class.is_empty() {
        return Some(bean.class.clone());
    }
    if depth > 16 || bean.parent.is_empty() {
        return None; // cycle guard / no class to inherit
    }
    let parent = by_id.get(bean.parent.as_str())?;
    resolve_class(parent, by_id, depth + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RelKind;

    #[test]
    fn parses_bean_and_resolves_parent_chain() {
        let xml = r#"<beans>
            <bean id="abstractBase" abstract="true" class="com.x.AbstractAction"/>
            <bean id="categoryAction" class="com.x.CategoryAction" parent="abstractBase"/>
            <bean id="inheritsClass" parent="abstractBase"/>
            <bean class="com.x.Anonymous"/>
          </beans>"#;
        let file = crate::test_support::tmp("beans.xml", xml);
        let mut sp = SpringParse::default();
        parse_file(&file, &mut sp);

        // anonymous (no id) is not addressable → not recorded
        assert_eq!(sp.beans.len(), 3);
        // BeanIdToImpl edges only for beans with their own class
        assert_eq!(sp.relations.iter().filter(|r| r.kind == RelKind::BeanIdToImpl).count(), 2);

        let map = resolve_map(&sp.beans);
        assert_eq!(map.get("categoryAction").map(String::as_str), Some("com.x.CategoryAction"));
        // bean with no own class inherits parent's class
        assert_eq!(map.get("inheritsClass").map(String::as_str), Some("com.x.AbstractAction"));
    }

    #[test]
    fn parent_cycle_does_not_loop() {
        let beans = vec![
            BeanRecord { id: "a".into(), class: String::new(), parent: "b".into(), source_file: String::new() },
            BeanRecord { id: "b".into(), class: String::new(), parent: "a".into(), source_file: String::new() },
        ];
        let map = resolve_map(&beans);
        // neither resolves to a class; the depth guard prevents an infinite loop
        assert!(map.is_empty());
    }
}
