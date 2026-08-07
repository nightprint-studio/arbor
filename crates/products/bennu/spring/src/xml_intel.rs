//! Editor answers for a **Spring bean XML** buffer.
//!
//! This is the half of Spring support that modern tooling has quietly stopped caring
//! about, and the half a legacy codebase lives in. `<property name="repositry" ref="x"/>`
//! is a perfectly well-formed XML document, an unreadable stack trace at startup, and —
//! until now — something no editor in this project had an opinion about.
//!
//! ## The three checks, and how each earns the right to exist
//!
//! - **`<property name=>` names nothing.** Reported only when the bean's class is a
//!   project type whose writable-property set is *known to be complete*
//!   ([`TypeInfo::properties_complete`]). An unresolved supertype or an unmodelled Lombok
//!   annotation turns the check off for that class entirely.
//! - **`class=` doesn't exist.** Reported only when the class's package is one the project
//!   itself declares — if we know the package, we know its classes, so absence is real.
//!   A `org.springframework.…` class we've never heard of is simply not our business.
//! - **`ref=` names no bean.** Reported only when the id *looks like a typo* of one that
//!   does exist. A bean can legitimately come from a jar, a parent context, or an XML this
//!   scan never saw, so plain absence proves nothing — but `orderRepo` in a project that
//!   defines `orderRepository` is a mistake, not a context boundary.
//!
//! [`TypeInfo::properties_complete`]: crate::model::TypeInfo::properties_complete

use bennu_ext::prelude::{ExtGutterMark, ExtHighlight, ExtHover, ExtTarget};
use bennu_proto::prelude::{CompletionItem, Diagnostic};
use bennu_spel::prelude as spel;

use crate::model::{line_at, simple_name, SpringModel};
use crate::xml::{attribute_at, parse_bean_xml, XmlAttrHit};

/// Attributes whose value is a bean id.
const REF_ATTRS: &[&str] = &["ref", "bean", "local", "parent", "depends-on", "factory-bean"];

/// Whether this buffer is one we answer for at all.
pub fn is_bean_xml(source: &str) -> bool {
    crate::xml::is_spring_bean_xml(source)
}

// ── Highlighting ─────────────────────────────────────────────────────────────

/// Placeholders and SpEL inside `value=` attributes — the same colours as in a Java
/// annotation, because it is the same syntax doing the same job.
pub fn highlights(path: &str, source: &str) -> Vec<ExtHighlight> {
    let Some(file) = parse_bean_xml(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for bean in &file.beans {
        for p in &bean.properties {
            let Some((start, _)) = p.value_span else { continue };
            crate::highlight::expression_highlights(&p.value, start, &mut out);
        }
    }
    out
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

/// Problems in a bean XML. See the module docs for why each one is allowed to speak.
pub fn diagnostics(model: &SpringModel, path: &str, source: &str) -> Vec<Diagnostic> {
    let Some(file) = parse_bean_xml(path, source) else { return Vec::new() };
    let mut out = Vec::new();

    for bean in &file.beans {
        // `class=` naming a type from a package we own.
        if let (Some((s, e)), false) = (bean.class_span, bean.class.is_empty()) {
            if is_missing_project_class(model, &bean.class) {
                out.push(diag(
                    &format!("Class `{}` was not found in the project", bean.class),
                    "warning",
                    "spring-unknown-class",
                    s,
                    e,
                ));
            }
        }
        // `<property name=>` against the bean's class.
        let owner = (!bean.class.is_empty()).then(|| model.type_of(&bean.class)).flatten();
        for p in &bean.properties {
            if let Some(t) = owner {
                if t.properties_complete && !t.properties.contains(&p.name) {
                    out.push(diag(
                        &format!(
                            "`{}` is not a writable property of {}",
                            p.name,
                            simple_name(&t.fqcn)
                        ),
                        "warning",
                        "spring-unknown-property",
                        p.name_span.0,
                        p.name_span.1,
                    ));
                }
            }
            // A placeholder in a `value=` is checked for syntax like anywhere else.
            if let Some((start, _)) = p.value_span {
                for issue in spel::placeholder_issues(&p.value) {
                    out.push(diag(
                        &issue.message,
                        "warning",
                        "spring-placeholder-syntax",
                        start + issue.start,
                        start + issue.end,
                    ));
                }
            }
        }
    }

    // `ref=` that looks like a typo of a bean that does exist.
    for r in &file.refs {
        if model.has_bean(&r.name) {
            continue;
        }
        if let Some(near) = nearest_bean(model, &r.name) {
            out.push(diag(
                &format!("No bean named `{}` — did you mean `{near}`?", r.name),
                "warning",
                "spring-unknown-bean-ref",
                r.start,
                r.end,
            ));
        }
    }
    out
}

/// Whether `fqcn` is a class we should have seen but didn't: its package is one the
/// project declares, and no type in it carries that name.
fn is_missing_project_class(model: &SpringModel, fqcn: &str) -> bool {
    let Some((package, _)) = fqcn.rsplit_once('.') else { return false };
    if model.types.contains_key(fqcn) {
        return false;
    }
    let prefix = format!("{package}.");
    // Some project type lives in exactly this package → we indexed the package, so a
    // class of ours in it would be here.
    model.types.keys().any(|k| {
        k.starts_with(&prefix) && !k[prefix.len()..].contains('.')
    })
}

/// The existing bean name closest to `name`, when it is close enough to be a typo rather
/// than a different bean altogether.
fn nearest_bean(model: &SpringModel, name: &str) -> Option<String> {
    if name.len() < 4 {
        return None; // too short for "close" to mean anything
    }
    model
        .beans
        .iter()
        .map(|b| (edit_distance(&b.name.to_ascii_lowercase(), &name.to_ascii_lowercase()), &b.name))
        .filter(|(d, _)| *d > 0 && *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n.clone())
}

/// Levenshtein distance, capped implicitly by the short strings it is used on.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn diag(message: &str, severity: &str, code: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        severity: severity.to_string(),
        code: code.to_string(),
        start,
        end,
    }
}

// ── Navigation / hover / completion ──────────────────────────────────────────

/// Go-to from a bean XML: a class name, a bean reference, a property name, or a
/// placeholder key.
pub fn navigate(model: &SpringModel, source: &str, offset: usize) -> Vec<ExtTarget> {
    let Some(hit) = attribute_at(source, offset) else { return Vec::new() };
    // A placeholder inside the value wins — it is the more specific thing under the caret.
    if let Some(t) = placeholder_target(model, &hit, offset) {
        return vec![t];
    }
    if hit.attribute == "class" || hit.attribute == "value-type" {
        return model
            .type_of(&hit.value)
            .map(|t| {
                vec![ExtTarget {
                    file: t.file.clone(),
                    offset: t.offset,
                    label: simple_name(&t.fqcn).to_string(),
                    detail: t.fqcn.clone(),
                }]
            })
            .unwrap_or_default();
    }
    if REF_ATTRS.contains(&hit.attribute.as_str()) {
        return model.bean(&hit.value).map(|b| vec![bean_target(b)]).unwrap_or_default();
    }
    if hit.element == "property" && hit.attribute == "name" {
        if let Some(t) = model.type_of(&hit.owner_class) {
            if let Some(i) = t.properties.iter().position(|p| *p == hit.value) {
                return vec![ExtTarget {
                    file: t.file.clone(),
                    offset: t.property_offsets[i],
                    label: hit.value.clone(),
                    detail: t.fqcn.clone(),
                }];
            }
        }
    }
    Vec::new()
}

/// The property-file target for a `${key}` under the caret inside an attribute value.
fn placeholder_target(
    model: &SpringModel,
    hit: &XmlAttrHit,
    offset: usize,
) -> Option<ExtTarget> {
    let rel = offset.checked_sub(hit.start)?;
    let p = spel::placeholder_at(&hit.value, rel)?;
    if !p.is_resolvable_key() {
        return None;
    }
    let (f, e) = model.props.lookup(&p.key)?;
    Some(ExtTarget {
        file: f.path.clone(),
        offset: e.key_start,
        label: e.key.clone(),
        detail: format!("{} · {}", f.name, e.value),
    })
}

fn bean_target(b: &crate::model::BeanDef) -> ExtTarget {
    ExtTarget {
        file: b.file.clone(),
        offset: b.offset,
        label: b.name.clone(),
        detail: if b.fqcn.is_empty() { b.stereotype.clone() } else { b.fqcn.clone() },
    }
}

/// Hover for a bean XML.
pub fn hover(model: &SpringModel, source: &str, offset: usize) -> Option<ExtHover> {
    let hit = attribute_at(source, offset)?;
    let rel = offset.checked_sub(hit.start)?;
    if let Some(p) = spel::placeholder_at(&hit.value, rel) {
        if p.is_resolvable_key() {
            return Some(match model.props.lookup(&p.key) {
                Some((f, e)) => ExtHover {
                    title: p.key.clone(),
                    signature: if e.value.is_empty() { "(empty)".into() } else { e.value.clone() },
                    doc: format!("Declared in {}", f.name),
                },
                None => ExtHover {
                    title: p.key,
                    signature: p.default.unwrap_or_else(|| "(unresolved)".to_string()),
                    doc: "Not declared in any property file.".to_string(),
                },
            });
        }
    }
    if REF_ATTRS.contains(&hit.attribute.as_str()) {
        if let Some(b) = model.bean(&hit.value) {
            return Some(ExtHover {
                title: b.name.clone(),
                signature: b.fqcn.clone(),
                doc: format!("Declared by {} in {}", b.stereotype, file_name(&b.file)),
            });
        }
    }
    if hit.attribute == "class" {
        if let Some(t) = model.type_of(&hit.value) {
            return Some(ExtHover {
                title: simple_name(&t.fqcn).to_string(),
                signature: t.fqcn.clone(),
                doc: format!("{} writable properties", t.properties.len()),
            });
        }
    }
    None
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Completion inside a bean XML attribute value.
pub fn completions(model: &SpringModel, source: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(hit) = attribute_at(source, offset) else { return Vec::new() };
    // Inside an open `${` → property keys, whatever the attribute is.
    let rel = offset.saturating_sub(hit.start);
    if in_open_placeholder(&hit.value, rel) {
        return model
            .props
            .keys()
            .into_iter()
            .map(|k| CompletionItem {
                detail: model.props.lookup(&k).map(|(_, e)| e.value.clone()),
                label: k,
                kind: "property".to_string(),
                auto_import: None,
                ..Default::default()
            })
            .collect();
    }
    if REF_ATTRS.contains(&hit.attribute.as_str()) {
        return model
            .beans
            .iter()
            .map(|b| CompletionItem {
                label: b.name.clone(),
                kind: "bean".to_string(),
                detail: Some(b.fqcn.clone()),
                auto_import: None,
                ..Default::default()
            })
            .collect();
    }
    if hit.attribute == "class" {
        return model
            .types
            .keys()
            .map(|fqcn| CompletionItem {
                label: fqcn.clone(),
                kind: "class".to_string(),
                detail: None,
                auto_import: None,
                ..Default::default()
            })
            .collect();
    }
    if hit.element == "property" && hit.attribute == "name" {
        if let Some(t) = model.type_of(&hit.owner_class) {
            return t
                .properties
                .iter()
                .map(|p| CompletionItem {
                    label: p.clone(),
                    kind: "property".to_string(),
                    detail: Some(simple_name(&t.fqcn).to_string()),
                    auto_import: None,
                    ..Default::default()
                })
                .collect();
        }
    }
    Vec::new()
}

fn in_open_placeholder(text: &str, offset: usize) -> bool {
    let before = &text[..offset.min(text.len())];
    match (before.rfind("${"), before.rfind('}')) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

/// A gutter mark on every `<bean>` that names a project class, pointing at it.
pub fn gutter(model: &SpringModel, path: &str, source: &str) -> Vec<ExtGutterMark> {
    let Some(file) = parse_bean_xml(path, source) else { return Vec::new() };
    file.beans
        .iter()
        .filter(|b| !b.class.is_empty())
        .filter_map(|b| {
            let t = model.type_of(&b.class)?;
            Some(ExtGutterMark {
                line: line_at(source, b.offset),
                kind: "bean".to_string(),
                tooltip: format!("Bean `{}` → {}", b.id, b.class),
                targets: vec![ExtTarget {
                    file: t.file.clone(),
                    offset: t.offset,
                    label: simple_name(&t.fqcn).to_string(),
                    detail: t.fqcn.clone(),
                }],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beans::JavaUnit;
    use crate::props::{parse_property_file, PropertySources};
    use crate::scan::scan_java;

    const XML_PATH: &str = "/p/src/main/resources/beans.xml";

    /// The Spring imports on one line — `known` resolves each annotation through them, so a
    /// fixture without them declares its own and registers no bean.
    const IMPORTS: &str = "import org.springframework.stereotype.*;";

    /// Splice [`IMPORTS`] onto the `package` line. Exposed so a test that indexes into the
    /// Java source by a model offset indexes the text the model was actually built from.
    fn with_imports(src: &str) -> String {
        match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        }
    }

    fn model(src: &str) -> SpringModel {
        let java = with_imports(src);
        let u = JavaUnit {
            facts: scan_java("/p/src/main/java/com/acme/Beans.java", &java).unwrap(),
            text: java.clone(),
        };
        let units = std::slice::from_ref(&u);
        SpringModel {
            beans: crate::beans::annotation_beans(units),
            types: crate::beans::type_index(units),
            ..SpringModel::default()
        }
    }

    const JAVA: &str = "package com.acme;\n\
        public class OrderService {\n\
          public void setRepository(Object r) {}\n\
          public void setTimeout(int t) {}\n\
        }\n";

    #[test]
    fn an_unknown_property_is_flagged_against_a_known_class() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"repositry\" ref=\"x\"/></bean></beans>";
        let d = diagnostics(&m, XML_PATH, xml);
        let p = d.iter().find(|d| d.code == "spring-unknown-property").expect("flagged");
        assert_eq!(&xml[p.start..p.end], "repositry");
        assert!(p.message.contains("OrderService"));
    }

    #[test]
    fn a_correct_property_is_silent() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"repository\" value=\"1\"/>\
                   <property name=\"timeout\" value=\"2\"/></bean></beans>";
        assert!(diagnostics(&m, XML_PATH, xml).is_empty());
    }

    #[test]
    fn an_incomplete_property_set_silences_the_check_entirely() {
        // The class extends something outside the scan, so what it accepts is unknown —
        // and unknown must never be reported as wrong.
        let m = model("package com.acme;\npublic class OrderService extends FrameworkBase {}\n");
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"anything\" value=\"1\"/></bean></beans>";
        assert!(diagnostics(&m, XML_PATH, xml).is_empty());
    }

    #[test]
    fn a_library_class_is_never_reported_as_missing() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"tx\" class=\"org.springframework.jdbc.DataSourceTransactionManager\"/></beans>";
        assert!(diagnostics(&m, XML_PATH, xml).is_empty(), "not our package, not our business");
    }

    #[test]
    fn a_missing_class_in_our_own_package_is_flagged() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderServic\"/></beans>";
        let d = diagnostics(&m, XML_PATH, xml);
        let c = d.iter().find(|d| d.code == "spring-unknown-class").expect("flagged");
        assert_eq!(&xml[c.start..c.end], "com.acme.OrderServic");
    }

    #[test]
    fn a_ref_typo_is_flagged_but_an_unknown_id_alone_is_not() {
        let m = model("package com.acme;\n@Service class OrderRepository {}\n");
        let typo = "<beans><bean id=\"a\" class=\"X\"><property name=\"r\" ref=\"orderRepositry\"/></bean></beans>";
        let d = diagnostics(&m, XML_PATH, typo);
        assert!(d.iter().any(|d| d.code == "spring-unknown-bean-ref"));
        assert!(d[0].message.contains("orderRepository"));

        // A bean that simply isn't in this scan — a jar, a parent context — says nothing.
        let elsewhere = "<beans><bean id=\"a\" class=\"X\"><property name=\"r\" ref=\"someJarBean\"/></bean></beans>";
        assert!(!diagnostics(&m, XML_PATH, elsewhere)
            .iter()
            .any(|d| d.code == "spring-unknown-bean-ref"));
    }

    #[test]
    fn go_to_from_a_class_attribute_lands_on_the_type() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\"/></beans>";
        let at = xml.find("com.acme.OrderService").unwrap() + 3;
        let t = navigate(&m, xml, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "OrderService");
        let java = with_imports(JAVA);
        assert_eq!(&java[t[0].offset..t[0].offset + 12], "OrderService");
    }

    #[test]
    fn go_to_from_a_property_name_lands_on_its_setter() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"timeout\" value=\"2\"/></bean></beans>";
        let at = xml.find("timeout").unwrap() + 1;
        let t = navigate(&m, xml, at);
        assert_eq!(t.len(), 1);
        let java = with_imports(JAVA);
        assert_eq!(&java[t[0].offset..t[0].offset + 10], "setTimeout");
    }

    #[test]
    fn go_to_from_a_ref_lands_on_the_bean() {
        let java = "package com.acme;\n@Service class OrderRepository {}\n";
        let m = model(java);
        let xml = "<beans><bean id=\"a\" class=\"X\"><property name=\"r\" ref=\"orderRepository\"/></bean></beans>";
        let at = xml.find("orderRepository\"").unwrap() + 2;
        let t = navigate(&m, xml, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "orderRepository");
    }

    #[test]
    fn a_placeholder_in_a_value_navigates_to_the_property_file() {
        let mut m = model(JAVA);
        m.props = PropertySources::new(vec![
            parse_property_file("/p/application.yml", "app:\n  timeout: 30\n").unwrap()
        ]);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"timeout\" value=\"${app.timeout}\"/></bean></beans>";
        let at = xml.find("app.timeout}").unwrap() + 2;
        let t = navigate(&m, xml, at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, "/p/application.yml");
        assert_eq!(hover(&m, xml, at).unwrap().signature, "30");
    }

    #[test]
    fn completion_is_offered_per_attribute() {
        let m = model(JAVA);
        let xml = "<beans><bean id=\"s\" class=\"com.acme.OrderService\">\
                   <property name=\"\" ref=\"\"/></bean></beans>";
        let name_at = xml.find("name=\"\"").unwrap() + 6;
        let names: Vec<_> =
            completions(&m, xml, name_at).into_iter().map(|c| c.label).collect();
        assert!(names.contains(&"repository".to_string()));
        assert!(names.contains(&"timeout".to_string()));

        let class_at = xml.find("com.acme.OrderService").unwrap();
        let classes: Vec<_> =
            completions(&m, xml, class_at).into_iter().map(|c| c.label).collect();
        assert_eq!(classes, ["com.acme.OrderService"]);
    }

    #[test]
    fn placeholder_highlights_reach_into_attribute_values() {
        let xml = "<beans><bean id=\"s\" class=\"C\"><property name=\"t\" value=\"${app.timeout}\"/></bean></beans>";
        let hs = highlights(XML_PATH, xml);
        let key = hs.iter().find(|h| h.kind == "spring.placeholder.key").expect("key span");
        assert_eq!(&xml[key.start..key.end], "app.timeout");
    }

    #[test]
    fn a_non_spring_xml_yields_nothing_at_all() {
        let m = model(JAVA);
        let xml = "<struts><package name=\"x\"/></struts>";
        assert!(!is_bean_xml(xml));
        assert!(diagnostics(&m, XML_PATH, xml).is_empty());
        assert!(highlights(XML_PATH, xml).is_empty());
        assert!(navigate(&m, xml, 12).is_empty());
    }

    #[test]
    fn edit_distance_is_the_usual_one() {
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("orderRepo", "orderRepos"), 1);
        assert_eq!(edit_distance("", "ab"), 2);
    }
}
