//! Spring bean XML — parsed with the spans an editor needs.
//!
//! `bennu-web` already reads these files for the Struts chain (`<action class="beanId">`
//! → FQCN), and reads exactly what that chain needs: id, class, parent. This reads the
//! rest — `<property name=>`, `<constructor-arg>`, `ref=`, `scope`, `primary`, `abstract`
//! — and, more importantly, reads it **with byte ranges**, because everything the editor
//! does here is positional: colour that value, jump from this attribute, squiggle that
//! name.
//!
//! Namespaces are matched by local name, so `<beans:bean>` and `<bean>` are the same
//! element — a legacy config that qualifies half its elements is the normal case, not an
//! edge one.
//!
//! Nothing here decides anything: `<bean>` elements become [`XmlBean`] records and the
//! bean registry is assembled in [`crate::beans`].

use roxmltree::{Document, Node, ParsingOptions};

use crate::model::line_at;

/// A byte span in the file.
pub type Span = (usize, usize);

/// One `<property name= value=|ref=>` inside a bean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlProperty {
    pub name: String,
    pub name_span: Span,
    /// `ref="…"` when written.
    pub ref_name: String,
    pub ref_span: Option<Span>,
    /// `value="…"` when written (may hold a `${placeholder}`).
    pub value: String,
    pub value_span: Option<Span>,
}

/// One `<bean>` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlBean {
    pub id: String,
    pub id_span: Option<Span>,
    /// The `class` attribute (a dotted FQCN), empty when inherited from `parent`.
    pub class: String,
    pub class_span: Option<Span>,
    pub parent: String,
    pub scope: String,
    pub primary: bool,
    pub lazy: bool,
    pub is_abstract: bool,
    /// Byte offset of the element's start — where go-to lands.
    pub offset: usize,
    pub line: u32,
    pub properties: Vec<XmlProperty>,
}

/// One `ref="beanId"` / `<ref bean="beanId"/>` use site, wherever it appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlBeanRef {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// A parsed Spring bean XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlBeanFile {
    /// Absolute path, forward-slashed.
    pub path: String,
    /// The root `<beans profile="…">`, empty when unconditional.
    pub profile: String,
    pub beans: Vec<XmlBean>,
    pub refs: Vec<XmlBeanRef>,
}

/// Parse XML tolerating a `<!DOCTYPE>` declaration (never fetched) — the legacy config
/// fragments in these trees carry one, and `roxmltree` rejects it by default.
fn parse_doc(text: &str) -> Option<Document<'_>> {
    let opts = ParsingOptions { allow_dtd: true, ..ParsingOptions::default() };
    Document::parse_with_options(text, opts).ok()
}

/// Whether `text` looks like a Spring bean definition file: a `<beans>` root, or (for the
/// merged Entando-style fragments where Spring beans live beside Struts packages) any
/// `<bean>` element at all.
pub fn is_spring_bean_xml(text: &str) -> bool {
    let Some(doc) = parse_doc(text) else { return false };
    doc.root_element().has_tag_name("beans")
        || doc.descendants().any(|n| n.is_element() && n.has_tag_name("bean"))
}

/// Parse a bean XML. `None` when the file doesn't parse or holds no beans — a malformed
/// fragment is skipped, never fatal (one bad file must not abort a project scan).
pub fn parse_bean_xml(path: &str, text: &str) -> Option<XmlBeanFile> {
    let doc = parse_doc(text)?;
    let root = doc.root_element();
    if !root.has_tag_name("beans") && !doc.descendants().any(|n| n.has_tag_name("bean")) {
        return None;
    }
    let mut file = XmlBeanFile {
        path: path.replace('\\', "/"),
        profile: root.attribute("profile").unwrap_or_default().to_string(),
        beans: Vec::new(),
        refs: Vec::new(),
    };

    for node in doc.descendants().filter(|n| n.is_element() && n.has_tag_name("bean")) {
        let mut bean = XmlBean {
            id: attr(&node, "id").or_else(|| attr(&node, "name")).unwrap_or_default(),
            id_span: span_of(&node, "id").or_else(|| span_of(&node, "name")),
            class: attr(&node, "class").unwrap_or_default(),
            class_span: span_of(&node, "class"),
            parent: attr(&node, "parent").unwrap_or_default(),
            scope: attr(&node, "scope").unwrap_or_default(),
            primary: attr(&node, "primary").as_deref() == Some("true"),
            lazy: attr(&node, "lazy-init").as_deref() == Some("true"),
            is_abstract: attr(&node, "abstract").as_deref() == Some("true"),
            offset: node.range().start,
            line: line_at(text, node.range().start),
            properties: Vec::new(),
        };
        for p in node.children().filter(|c| c.is_element() && c.has_tag_name("property")) {
            let Some(name) = attr(&p, "name") else { continue };
            let Some(name_span) = span_of(&p, "name") else { continue };
            bean.properties.push(XmlProperty {
                name,
                name_span,
                ref_name: attr(&p, "ref").unwrap_or_default(),
                ref_span: span_of(&p, "ref"),
                value: attr(&p, "value").unwrap_or_default(),
                value_span: span_of(&p, "value"),
            });
        }
        file.beans.push(bean);
    }

    // Bean references, wherever they are written: `ref=` on a property or a
    // constructor-arg, and the nested `<ref bean=|local=>` element.
    for node in doc.descendants().filter(Node::is_element) {
        for name in ["ref", "bean", "local"] {
            // `bean=`/`local=` only count on a `<ref>` element — elsewhere `bean` is the
            // element itself and `local` means nothing.
            if name != "ref" && !node.has_tag_name("ref") {
                continue;
            }
            if name == "ref" && node.has_tag_name("ref") {
                continue; // `<ref ref=>` isn't a thing
            }
            if let (Some(v), Some((s, e))) = (attr(&node, name), span_of(&node, name)) {
                if !v.is_empty() {
                    file.refs.push(XmlBeanRef { name: v, start: s, end: e });
                }
            }
        }
    }

    Some(file)
}

fn attr(node: &Node, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_string)
}

/// The byte span of an attribute's VALUE (quotes excluded).
fn span_of(node: &Node, name: &str) -> Option<Span> {
    let a = node.attributes().find(|a| a.name() == name)?;
    let r = a.range_value();
    Some((r.start, r.end))
}

// ── Positional queries (against the live buffer) ─────────────────────────────

/// What sits under a caret in a bean XML — the one thing every XML feature starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlAttrHit {
    /// Local name of the element the attribute is on (`bean`, `property`, `ref`).
    pub element: String,
    /// Local name of the attribute (`class`, `name`, `ref`, `value`).
    pub attribute: String,
    pub value: String,
    /// Span of the value (quotes excluded).
    pub start: usize,
    pub end: usize,
    /// The `class` of the nearest enclosing `<bean>`, when there is one — what a
    /// `<property name=>` must be checked against.
    pub owner_class: String,
    /// The `id` of the nearest enclosing `<bean>`.
    pub owner_bean: String,
}

/// The attribute value whose span covers `offset`. `None` outside any attribute value —
/// which is most of a file, so this is the cheap early-out every caller relies on.
pub fn attribute_at(text: &str, offset: usize) -> Option<XmlAttrHit> {
    let doc = parse_doc(text)?;
    for node in doc.descendants().filter(Node::is_element) {
        for a in node.attributes() {
            let r = a.range_value();
            // Inclusive at both ends: a caret sitting just after the last character is
            // still "in" the value, which is where it lands when you finish typing.
            if offset < r.start || offset > r.end {
                continue;
            }
            let (owner_bean, owner_class) = enclosing_bean(&node);
            return Some(XmlAttrHit {
                element: node.tag_name().name().to_string(),
                attribute: a.name().to_string(),
                value: a.value().to_string(),
                start: r.start,
                end: r.end,
                owner_class,
                owner_bean,
            });
        }
    }
    None
}

/// `(id, class)` of the nearest `<bean>` at or above `node`.
fn enclosing_bean(node: &Node) -> (String, String) {
    let mut cur = Some(*node);
    while let Some(n) = cur {
        if n.is_element() && n.has_tag_name("bean") {
            return (
                n.attribute("id").or_else(|| n.attribute("name")).unwrap_or_default().to_string(),
                n.attribute("class").unwrap_or_default().to_string(),
            );
        }
        cur = n.parent();
    }
    (String::new(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<beans xmlns="http://www.springframework.org/schema/beans" profile="dev">
  <bean id="orderService" class="com.acme.OrderServiceImpl" scope="prototype" primary="true">
    <property name="repository" ref="orderRepo"/>
    <property name="timeout" value="${app.timeout}"/>
  </bean>
  <bean id="orderRepo" class="com.acme.OrderRepo" abstract="false"/>
</beans>
"#;

    #[test]
    fn a_namespaced_beans_file_is_recognised_and_parsed() {
        assert!(is_spring_bean_xml(XML), "the default xmlns must not hide the elements");
        let f = parse_bean_xml("/p/beans.xml", XML).expect("parses");
        assert_eq!(f.profile, "dev");
        assert_eq!(f.beans.len(), 2);
        let b = &f.beans[0];
        assert_eq!(b.id, "orderService");
        assert_eq!(b.class, "com.acme.OrderServiceImpl");
        assert_eq!(b.scope, "prototype");
        assert!(b.primary && !b.is_abstract);
        assert_eq!(b.line, 3);
    }

    #[test]
    fn attribute_spans_exclude_the_quotes() {
        let f = parse_bean_xml("/p/beans.xml", XML).unwrap();
        let (s, e) = f.beans[0].class_span.unwrap();
        assert_eq!(&XML[s..e], "com.acme.OrderServiceImpl");
        let p = &f.beans[0].properties[0];
        assert_eq!(&XML[p.name_span.0..p.name_span.1], "repository");
        let (rs, re) = p.ref_span.unwrap();
        assert_eq!(&XML[rs..re], "orderRepo");
    }

    #[test]
    fn properties_carry_both_ref_and_value_forms() {
        let f = parse_bean_xml("/p/beans.xml", XML).unwrap();
        let props = &f.beans[0].properties;
        assert_eq!(props[0].name, "repository");
        assert_eq!(props[0].ref_name, "orderRepo");
        assert_eq!(props[1].name, "timeout");
        assert_eq!(props[1].value, "${app.timeout}");
        assert!(props[1].ref_span.is_none());
    }

    #[test]
    fn bean_references_are_collected_from_every_form() {
        let xml = r#"<beans>
  <bean id="a" class="A">
    <property name="x" ref="b"/>
    <constructor-arg ref="c"/>
    <property name="y"><ref bean="d"/></property>
  </bean>
</beans>"#;
        let f = parse_bean_xml("/p/b.xml", xml).unwrap();
        let mut names: Vec<_> = f.refs.iter().map(|r| r.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["b", "c", "d"]);
        let d = f.refs.iter().find(|r| r.name == "d").unwrap();
        assert_eq!(&xml[d.start..d.end], "d");
    }

    #[test]
    fn attribute_at_reports_the_element_and_its_owning_bean() {
        let at = XML.find("com.acme.OrderServiceImpl").unwrap() + 3;
        let hit = attribute_at(XML, at).expect("inside the class attribute");
        assert_eq!((hit.element.as_str(), hit.attribute.as_str()), ("bean", "class"));
        assert_eq!(hit.value, "com.acme.OrderServiceImpl");

        let at = XML.find("repository").unwrap() + 2;
        let hit = attribute_at(XML, at).unwrap();
        assert_eq!((hit.element.as_str(), hit.attribute.as_str()), ("property", "name"));
        assert_eq!(hit.owner_class, "com.acme.OrderServiceImpl");
        assert_eq!(hit.owner_bean, "orderService");
    }

    #[test]
    fn a_caret_outside_any_attribute_value_hits_nothing() {
        assert!(attribute_at(XML, 0).is_none());
        let between = XML.find("<property").unwrap() + 2;
        assert!(attribute_at(XML, between).is_none());
    }

    #[test]
    fn a_non_spring_xml_is_left_alone() {
        assert!(!is_spring_bean_xml("<struts><package name=\"x\"/></struts>"));
        assert!(parse_bean_xml("/p/struts.xml", "<struts><package name=\"x\"/></struts>").is_none());
    }

    #[test]
    fn a_malformed_fragment_is_skipped_not_fatal() {
        assert!(parse_bean_xml("/p/bad.xml", "<beans><bean id=").is_none());
        assert!(attribute_at("<beans><bean id=", 5).is_none());
        assert!(!is_spring_bean_xml("<beans><bean id="));
    }

    #[test]
    fn a_dtd_declaration_does_not_stop_the_parse() {
        let xml = "<!DOCTYPE beans PUBLIC \"-//SPRING//DTD BEAN//EN\" \"http://www.springframework.org/dtd/spring-beans.dtd\">\n<beans><bean id=\"a\" class=\"A\"/></beans>";
        assert_eq!(parse_bean_xml("/p/legacy.xml", xml).unwrap().beans.len(), 1);
    }

    #[test]
    fn a_qualified_element_name_matches_by_local_name() {
        let xml = "<beans:beans xmlns:beans=\"http://www.springframework.org/schema/beans\"><beans:bean id=\"a\" class=\"A\"/></beans:beans>";
        let f = parse_bean_xml("/p/q.xml", xml).unwrap();
        assert_eq!(f.beans[0].id, "a");
    }
}
