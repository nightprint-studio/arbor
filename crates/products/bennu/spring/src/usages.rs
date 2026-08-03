//! Who reads a configuration key — the index that makes a yaml navigable.
//!
//! Every other index here answers "what does this Java thing bind". This answers the opposite,
//! and it is the question you actually have while looking at an `application.yml`: is this line
//! still read by anything, and by what? A key nobody reads is dead weight nobody dares delete,
//! because proving it is dead means grepping for four different spellings of it.
//!
//! Four ways a key is read, all collected here:
//!
//! - `@Value("${key}")` — and any other annotation carrying a placeholder;
//! - `@ConditionalOnProperty(name = "key")` — where the key decides whether a bean exists at all;
//! - `@ConfigurationProperties` — the bound field, via the paths [`crate::config_props`] worked out;
//! - `<property value="${key}"/>` in a bean XML.
//!
//! ## Spelling
//!
//! Spring's relaxed binding treats `app.readTimeout` and `app.read-timeout` as one key. Both are
//! normalised to the canonical kebab-case form as the index is built, so a lookup is an exact
//! match and the caller never has to know.

use crate::beans::JavaUnit;
use crate::model::{canonical_key_segment, simple_name, ConfigBinding, PropertyUsage};
use crate::scan::{AnnFacts, JavaFacts};
use crate::xml::XmlBeanFile;

/// Normalise a dotted key to its canonical spelling, segment by segment.
pub fn canonical_key(key: &str) -> String {
    key.split('.').map(canonical_key_segment).collect::<Vec<_>>().join(".")
}

/// Every place a configuration key is read, across Java, XML and the bound-properties model.
pub fn property_usages(
    units: &[JavaUnit],
    xml: &[XmlBeanFile],
    bindings: &[ConfigBinding],
) -> Vec<PropertyUsage> {
    let mut out = Vec::new();

    for u in units {
        for t in &u.facts.types {
            let owner = simple_name(&t.fqcn).to_string();
            collect(&t.annotations, &u.facts, &owner, &u.facts.file, &mut out);
            for f in &t.fields {
                let label = format!("{owner}.{}", f.name);
                collect(&f.annotations, &u.facts, &label, &u.facts.file, &mut out);
            }
            for m in &t.methods {
                let label = format!("{owner}.{}()", m.name);
                collect(&m.annotations, &u.facts, &label, &u.facts.file, &mut out);
                for p in &m.params {
                    let plabel = format!("{owner}.{}({})", m.name, p.name);
                    collect(&p.annotations, &u.facts, &plabel, &u.facts.file, &mut out);
                }
            }
        }
    }

    // A bound field reads its key by existing — that IS the read.
    for b in bindings {
        out.push(PropertyUsage {
            key: canonical_key(&b.path),
            file: b.file.clone(),
            offset: b.offset,
            kind: "@ConfigurationProperties".to_string(),
            label: format!("{}.{}", simple_name(&b.owner_fqcn), b.field),
        });
    }

    for f in xml {
        for bean in &f.beans {
            for p in &bean.properties {
                let Some((start, _)) = p.value_span else { continue };
                for ph in bennu_spel::prelude::placeholders(&p.value) {
                    if !ph.is_resolvable_key() {
                        continue;
                    }
                    out.push(PropertyUsage {
                        key: canonical_key(&ph.key),
                        file: f.path.clone(),
                        offset: start + ph.key_start,
                        kind: "<bean>".to_string(),
                        label: format!("{}.{}", bean.id, p.name),
                    });
                }
            }
        }
    }

    out
}

/// Placeholders and conditional keys in one declaration's annotations.
fn collect(
    anns: &[AnnFacts],
    facts: &JavaFacts,
    label: &str,
    file: &str,
    out: &mut Vec<PropertyUsage>,
) {
    for ann in anns {
        // `@ConditionalOnProperty(name = "…")` names a key outright rather than through a
        // placeholder — the one shape that would be missed by a placeholder scan alone.
        if crate::known::is(ann, facts, "ConditionalOnProperty") {
            let key = crate::beans::conditional_property_key(ann);
            if !key.is_empty() {
                // Point at the string that holds it, whichever element that was.
                let at = ann
                    .strings
                    .iter()
                    .find(|s| key.ends_with(&s.value) && !s.value.is_empty())
                    .map(|s| s.start)
                    .unwrap_or(ann.start);
                out.push(PropertyUsage {
                    key: canonical_key(&key),
                    file: file.to_string(),
                    offset: at,
                    kind: "@ConditionalOnProperty".to_string(),
                    label: label.to_string(),
                });
            }
        }
        for s in &ann.strings {
            for ph in bennu_spel::prelude::placeholders(&s.value) {
                if !ph.is_resolvable_key() {
                    continue;
                }
                out.push(PropertyUsage {
                    key: canonical_key(&ph.key),
                    file: file.to_string(),
                    offset: s.start + ph.key_start,
                    kind: format!("@{}", ann.name),
                    label: label.to_string(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    const IMPORTS: &str = "import org.springframework.beans.factory.annotation.*; import org.springframework.boot.autoconfigure.condition.*;";

    fn unit(src: &str) -> JavaUnit {
        let text = match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        };
        JavaUnit { facts: scan_java("/p/S.java", &text).unwrap(), text }
    }

    #[test]
    fn a_value_placeholder_is_a_usage_of_its_key() {
        let u = unit("package p;\nclass S { @Value(\"${app.timeout:30}\") int t; }\n");
        let text = u.text.clone();
        let all = property_usages(&[u], &[], &[]);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "app.timeout");
        assert_eq!(all[0].label, "S.t");
        assert_eq!(all[0].kind, "@Value");
        assert_eq!(&text[all[0].offset..all[0].offset + 11], "app.timeout", "points at the key");
    }

    #[test]
    fn a_conditional_on_property_is_a_usage_too() {
        let u = unit(
            "package p;\n@ConditionalOnProperty(name = \"app.feature.enabled\", havingValue = \"true\")\nclass S {}\n",
        );
        let all = property_usages(&[u], &[], &[]);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "app.feature.enabled");
        assert_eq!(all[0].kind, "@ConditionalOnProperty");
    }

    #[test]
    fn a_prefixed_conditional_joins_its_key() {
        let u = unit(
            "package p;\n@ConditionalOnProperty(prefix = \"app.feature\", name = \"enabled\")\nclass S {}\n",
        );
        assert_eq!(property_usages(&[u], &[], &[])[0].key, "app.feature.enabled");
    }

    #[test]
    fn both_spellings_of_a_key_normalise_to_one() {
        let u = unit(
            "package p;\nclass S { @Value(\"${app.readTimeout}\") int a; @Value(\"${app.read-timeout}\") int b; }\n",
        );
        let all = property_usages(&[u], &[], &[]);
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|x| x.key == "app.read-timeout"), "relaxed binding, one key");
    }

    #[test]
    fn a_bound_field_counts_as_reading_its_key() {
        let binding = ConfigBinding {
            owner_fqcn: "p.Client".into(),
            field: "readTimeout".into(),
            path: "app.http.client.read-timeout".into(),
            type_text: "int".into(),
            root_prefix: "app.http".into(),
            file: "/p/Client.java".into(),
            offset: 42,
        };
        let all = property_usages(&[], &[], &[binding]);
        assert_eq!(all[0].key, "app.http.client.read-timeout");
        assert_eq!(all[0].label, "Client.readTimeout");
        assert_eq!(all[0].offset, 42);
    }

    #[test]
    fn an_xml_property_value_is_a_usage() {
        let xml = crate::xml::parse_bean_xml(
            "/p/beans.xml",
            "<beans><bean id=\"svc\" class=\"C\"><property name=\"timeout\" value=\"${app.timeout}\"/></bean></beans>",
        )
        .unwrap();
        let all = property_usages(&[], &[xml], &[]);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "app.timeout");
        assert_eq!(all[0].label, "svc.timeout");
        assert_eq!(all[0].kind, "<bean>");
    }

    #[test]
    fn a_placeholder_with_no_static_key_is_not_a_usage() {
        let u = unit("package p;\nclass S { @Value(\"${${platform}.url}\") String s; }\n");
        // The composed key only exists at runtime; the INNER one is still a real read.
        let keys: Vec<_> =
            property_usages(&[u], &[], &[]).into_iter().map(|x| x.key).collect();
        assert_eq!(keys, ["platform"]);
    }

    #[test]
    fn a_project_with_no_reads_has_an_empty_index() {
        assert!(property_usages(&[unit("package p;\nclass S { int x; }\n")], &[], &[]).is_empty());
    }
}
