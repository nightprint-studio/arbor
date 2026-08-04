//! The editor's answers over the JPA model.
//!
//! Everything here parses the **live buffer** and then consults the indexed model, the same
//! division `bennu-spring` uses: spans must match what is on screen, meaning must come from the
//! whole project.

use bennu_ext::prelude::{ExtGutterMark, ExtHighlight, ExtHover, ExtTarget};
use bennu_proto::prelude::Diagnostic;

use crate::index::JavaUnit;
use crate::model::{simple_name, strip_generics, JpaModel};
use crate::scan::scan_java;

fn unit(path: &str, source: &str) -> Option<JavaUnit> {
    Some(JavaUnit { facts: scan_java(path, source)?, text: source.to_string() })
}

/// Colour the inside of every `@Query` — the string that Java shows as one flat literal.
pub fn highlights(path: &str, source: &str) -> Vec<ExtHighlight> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for r in crate::index::repositories(std::slice::from_ref(&u)) {
        for m in &r.methods {
            let Some(q) = &m.query else { continue };
            // The whole query gets a tint, so it reads as a different language at a glance;
            // the kind carries whether it is JPQL or SQL, because they are not the same thing
            // and the theme is entitled to say so.
            out.push(ExtHighlight {
                start: q.start,
                end: q.end,
                kind: if q.native { "jpa.query.native".into() } else { "jpa.query".into() },
            });
            for t in crate::hql::tokens(&q.text, q.native) {
                out.push(ExtHighlight {
                    start: q.start + t.start,
                    end: q.start + t.end,
                    kind: format!("jpa.query.{}", t.kind),
                });
            }
        }
    }
    out
}

/// A mark beside every entity and every repository, pointing at the other end.
pub fn gutter(model: &JpaModel, path: &str, source: &str) -> Vec<ExtGutterMark> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for e in crate::index::entities(std::slice::from_ref(&u)) {
        let repos = model.repositories_of(&e.simple);
        if repos.is_empty() {
            continue;
        }
        out.push(ExtGutterMark {
            line: e.line,
            kind: "entity".into(),
            tooltip: match repos.len() {
                1 => format!("Managed by {}", repos[0].simple),
                n => format!("Managed by {n} repositories"),
            },
            targets: repos
                .iter()
                .map(|r| ExtTarget {
                    file: r.file.clone(),
                    offset: r.offset,
                    label: r.simple.clone(),
                    detail: format!("extends {}", r.base),
                })
                .collect(),
        });
    }
    for r in crate::index::repositories(std::slice::from_ref(&u)) {
        let Some(e) = model.entity(&r.entity) else { continue };
        out.push(ExtGutterMark {
            line: r.line,
            kind: "repository".into(),
            tooltip: format!("Manages {}", e.simple),
            targets: vec![ExtTarget {
                file: e.file.clone(),
                offset: e.offset,
                label: e.simple.clone(),
                detail: if e.table.is_empty() { "@Entity".into() } else { format!("table {}", e.table) },
            }],
        });
    }
    out
}

/// Hover: what a repository method actually asks the database.
pub fn hover(model: &JpaModel, path: &str, source: &str, offset: usize) -> Option<ExtHover> {
    let u = unit(path, source)?;
    // A repository method name.
    for r in crate::index::repositories(std::slice::from_ref(&u)) {
        for m in &r.methods {
            if offset < m.offset || offset > m.offset + m.name.len() {
                continue;
            }
            let entity = model.entity(&r.entity);
            if let Some(q) = &m.query {
                let mut doc = String::new();
                if !q.named_params.is_empty() {
                    doc.push_str(&format!("Binds :{}\n", q.named_params.join(", :")));
                }
                doc.push_str(if q.native {
                    "Native SQL — sent to the database as written, so it is the schema's names \
                     in here, not the entity's."
                } else {
                    "JPQL — resolved against the entity model by the provider."
                });
                return Some(ExtHover {
                    title: m.name.clone(),
                    signature: q.text.clone(),
                    doc,
                });
            }
            // A derived name: the interesting case, because the query is invisible.
            let derived = crate::derived::parse(&m.name)?;
            let (resolved, _) = match entity {
                Some(e) => crate::derived::resolve(model, e, &derived),
                None => (derived, Vec::new()),
            };
            let expected = resolved.expected_args();
            let doc = format!(
                "Derived query — Spring Data builds it from the name at startup.\nTakes {expected} \
                 bound argument{}.",
                if expected == 1 { "" } else { "s" },
            );
            return Some(ExtHover {
                title: m.name.clone(),
                signature: resolved.describe(),
                doc,
            });
        }
    }
    // An entity type name.
    for e in crate::index::entities(std::slice::from_ref(&u)) {
        if offset < e.offset || offset > e.offset + e.simple.len() {
            continue;
        }
        let repos = model.repositories_of(&e.simple);
        return Some(ExtHover {
            title: e.simple.clone(),
            signature: if e.table.is_empty() {
                format!("{} · table defaulted", e.kind)
            } else {
                format!("{} · table {}", e.kind, e.table)
            },
            doc: match repos.len() {
                0 => "No repository manages this entity.".to_string(),
                n => format!(
                    "Managed by {}",
                    repos.iter().take(n.min(3)).map(|r| r.simple.as_str()).collect::<Vec<_>>().join(", "),
                ),
            },
        });
    }
    None
}

/// Go-to: from a repository to its entity, from a `@Query`'s `from Entity` to the entity.
pub fn navigate(model: &JpaModel, path: &str, source: &str, offset: usize) -> Vec<ExtTarget> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    for r in crate::index::repositories(std::slice::from_ref(&u)) {
        for m in &r.methods {
            let Some(q) = &m.query else { continue };
            if offset < q.start || offset > q.end {
                continue;
            }
            // Inside a query: the entity it selects from.
            let Some(name) = crate::hql::from_entity(&q.text) else { return Vec::new() };
            return model
                .entity(&name)
                .map(|e| {
                    vec![ExtTarget {
                        file: e.file.clone(),
                        offset: e.offset,
                        label: e.simple.clone(),
                        detail: "@Entity".into(),
                    }]
                })
                .unwrap_or_default();
        }
        if offset >= r.offset && offset <= r.offset + r.simple.len() {
            if let Some(e) = model.entity(&r.entity) {
                return vec![ExtTarget {
                    file: e.file.clone(),
                    offset: e.offset,
                    label: e.simple.clone(),
                    detail: "@Entity".into(),
                }];
            }
        }
    }
    Vec::new()
}

/// The checks. Two, both gated so silence is the default.
pub fn diagnostics(model: &JpaModel, path: &str, source: &str) -> Vec<Diagnostic> {
    let Some(u) = unit(path, source) else { return Vec::new() };
    let mut out = Vec::new();
    for r in crate::index::repositories(std::slice::from_ref(&u)) {
        let Some(entity) = model.entity(&r.entity) else { continue };
        for m in &r.methods {
            match &m.query {
                // A declared query: every `:name` it binds must exist among the parameters.
                // Checkable without knowing anything about the schema, and a mismatch is a
                // guaranteed startup failure.
                Some(q) => {
                    for name in &q.named_params {
                        if m.params.iter().any(|p| p.effective_name() == name) {
                            continue;
                        }
                        let at = crate::hql::placeholders(&q.text)
                            .into_iter()
                            .find(|p| &p.name == name)
                            .map(|p| (q.start + p.start, q.start + p.end))
                            .unwrap_or((q.start, q.end));
                        out.push(Diagnostic {
                            start: at.0,
                            end: at.1,
                            severity: "error".into(),
                            message: format!(
                                "`:{name}` is not bound — no parameter is called `{name}`. Add \
                                 `@Param(\"{name}\")` to the one that should be.",
                            ),
                            code: "jpa.unbound-param".into(),
                            ..Diagnostic::default()
                        });
                    }
                }
                // A derived name: every segment must be a real property path.
                None => {
                    let Some(derived) = crate::derived::parse(&m.name) else { continue };
                    let (resolved, issues) = crate::derived::resolve(model, entity, &derived);
                    for issue in issues {
                        out.push(Diagnostic {
                            start: m.offset,
                            end: m.offset + m.name.len(),
                            severity: "error".into(),
                            message: issue.message,
                            code: "jpa.unknown-property".into(),
                            ..Diagnostic::default()
                        });
                    }
                    // The arity check runs only when the name resolved cleanly — otherwise it
                    // would pile a second, derived complaint on top of the first.
                    let expected = resolved.expected_args();
                    let declared = m.params.iter().filter(|p| !is_special(&p.type_text)).count();
                    if out.iter().all(|d| d.start != m.offset) && declared != expected {
                        out.push(Diagnostic {
                            start: m.offset,
                            end: m.offset + m.name.len(),
                            severity: "warning".into(),
                            message: format!(
                                "This name asks for {expected} bound argument{}, but {declared} \
                                 {} declared.",
                                if expected == 1 { "" } else { "s" },
                                if declared == 1 { "is" } else { "are" },
                            ),
                            code: "jpa.argument-count".into(),
                            ..Diagnostic::default()
                        });
                    }
                }
            }
        }
    }
    out
}

/// Parameters Spring Data supplies itself — they are not bindings and must not be counted.
fn is_special(type_text: &str) -> bool {
    matches!(
        simple_name(&strip_generics(type_text)),
        "Pageable" | "Sort" | "Limit" | "ScrollPosition"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::JavaUnit;

    const IMPORTS: &str = "import jakarta.persistence.*; import org.springframework.data.jpa.repository.*; import org.springframework.data.repository.query.Param;";

    fn src(body: &str) -> String {
        format!("package p;{IMPORTS}\n{body}")
    }

    fn model_of(sources: &[String]) -> JpaModel {
        let units: Vec<JavaUnit> = sources
            .iter()
            .map(|s| JavaUnit { facts: scan_java("/p/T.java", s).unwrap(), text: s.clone() })
            .collect();
        JpaModel {
            entities: crate::index::entities(&units),
            repositories: crate::index::repositories(&units),
        }
    }

    const ORDER: &str = "@Entity class Order { @Id Long id; java.math.BigDecimal total; }\n";

    #[test]
    fn an_unbound_named_parameter_is_an_error_pointing_at_the_placeholder() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  @Query(\"select o from Order o where o.total > :min\") Object m(int other);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        let d = diagnostics(&m, "/p/R.java", &repo);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code.as_deref(), Some("jpa.unbound-param"));
        assert_eq!(&repo[d[0].start..d[0].end], ":min", "the squiggle is on the placeholder");
    }

    #[test]
    fn a_bound_parameter_is_silent_whether_named_by_param_or_by_position() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  @Query(\"select o from Order o where o.total > :min\") Object a(@Param(\"min\") int x);\n  @Query(\"select o from Order o where o.total > :total\") Object b(int total);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        assert!(diagnostics(&m, "/p/R.java", &repo).is_empty());
    }

    #[test]
    fn a_typo_in_a_derived_name_is_reported() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  Object findByTotl(int t);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        let d = diagnostics(&m, "/p/R.java", &repo);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code.as_deref(), Some("jpa.unknown-property"));
    }

    #[test]
    fn the_argument_count_is_checked_against_what_the_name_asks_for() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  Object findByTotalBetween(int a);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        let d = diagnostics(&m, "/p/R.java", &repo);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code.as_deref(), Some("jpa.argument-count"));
        assert!(d[0].message.contains("2 bound arguments"));
    }

    /// `Pageable` is supplied by Spring, not bound — counting it makes every paged finder wrong.
    #[test]
    fn a_pageable_is_not_a_bound_argument() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  Object findByTotal(int t, org.springframework.data.domain.Pageable page);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        assert!(diagnostics(&m, "/p/R.java", &repo).is_empty());
    }

    /// The gate: an entity we never scanned means no checking at all.
    #[test]
    fn a_repository_over_an_unknown_entity_is_not_checked() {
        let repo = src("interface R extends JpaRepository<Ghost, Long> {\n  Object findByAnything(int x);\n}\n");
        let m = model_of(&[repo.clone()]);
        assert!(diagnostics(&m, "/p/R.java", &repo).is_empty());
    }

    #[test]
    fn a_query_is_coloured_and_says_which_language_it_is() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  @Query(\"select o from Order o\") Object a();\n  @Query(value = \"select * from ORDINI\", nativeQuery = true) Object b();\n}\n");
        let hs = highlights("/p/R.java", &repo);
        assert!(hs.iter().any(|h| h.kind == "jpa.query"));
        assert!(hs.iter().any(|h| h.kind == "jpa.query.native"));
        assert!(hs.iter().any(|h| h.kind == "jpa.query.keyword" && &repo[h.start..h.end] == "select"));
    }

    #[test]
    fn hover_on_a_derived_name_says_what_it_asks_for() {
        let repo = src("interface R extends JpaRepository<Order, Long> {\n  Object findByTotalGreaterThan(int t);\n}\n");
        let m = model_of(&[src(ORDER), repo.clone()]);
        let at = repo.find("findByTotalGreaterThan").unwrap() + 3;
        let h = hover(&m, "/p/R.java", &repo, at).unwrap();
        assert_eq!(h.signature, "find where total greater than");
        assert!(h.doc.contains("1 bound argument"));
    }

    #[test]
    fn the_gutter_links_an_entity_to_the_repositories_that_manage_it() {
        let order = src(ORDER);
        let repo = src("interface R extends JpaRepository<Order, Long> {}\n");
        let m = model_of(&[order.clone(), repo]);
        let marks = gutter(&m, "/p/Order.java", &order);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].kind, "entity");
        assert_eq!(marks[0].targets[0].label, "R");
    }
}
