//! Struts2 / XWork interceptor config parser.
//!
//! Interceptors are the pervasive, XML-only cross-cutting layer of a Struts app (auth,
//! validation, file-upload, the `japs`/Entando custom stacks) — and navigating them by
//! hand across a pile of `struts.xml` fragments is exactly the "cazzo di file xml" pain.
//! We model three things per `<package>`:
//!
//!   - `<interceptors><interceptor name= class=></interceptors>` — a named interceptor
//!     bound to an impl class ([`InterceptorRecord`]);
//!   - `<interceptors><interceptor-stack name=><interceptor-ref name=>…` — a named stack
//!     that composes other interceptors/stacks ([`InterceptorStackRecord`]);
//!   - every `<interceptor-ref name=>` **use** — inside a stack, inside an `<action>`, or
//!     a package `<default-interceptor-ref name=>` ([`InterceptorRefUse`]).
//!
//! The uses become `InterceptorRefToDef` edges (go-to a ref → its def; find-usages a def →
//! its refs); the `class` on a def resolves to the real Java type via the index. A ref that
//! names nothing on disk is a *candidate* (the def may live in a dependency jar such as the
//! built-in `defaultStack`) — never a hard "missing" (docs §8).

use std::path::Path;

use roxmltree::Node;

use crate::model::{InterceptorRecord, InterceptorRefUse, InterceptorStackRecord, RelKind, Relation};
use crate::struts::join_ns;
use crate::xml;

/// Result of parsing the interceptor config out of one struts fragment.
#[derive(Debug, Default)]
pub struct InterceptorParse {
    pub interceptors: Vec<InterceptorRecord>,
    pub stacks: Vec<InterceptorStackRecord>,
    pub refs: Vec<InterceptorRefUse>,
    pub relations: Vec<Relation>,
}

/// Parse every `<package>`'s interceptor defs + ref-uses in `file`. Tolerant of a fragment
/// with no interceptors (yields nothing). Standalone entry point (reads + parses); the
/// project build folds interceptors into the [`crate::struts`] include-graph walk via
/// [`collect_from_root`] instead, so the fragments are read once.
pub fn parse_file(file: &Path, out: &mut InterceptorParse) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let Some(doc) = xml::parse(&text) else {
        return;
    };
    collect_from_root(&doc.root_element(), &file.display().to_string(), out);
}

/// Collect interceptor defs + ref-uses out of an already-parsed struts fragment root.
/// Shared by [`parse_file`] and the struts include-graph walk (one read per fragment).
pub fn collect_from_root(root: &Node, source_file: &str, out: &mut InterceptorParse) {
    for pkg in root.children().filter(|n| n.has_tag_name("package")) {
        let namespace = pkg.attribute("namespace").unwrap_or("").to_string();

        // <interceptors> block: named interceptors + stacks.
        for block in pkg.children().filter(|n| n.has_tag_name("interceptors")) {
            for n in block.children() {
                if n.has_tag_name("interceptor") {
                    parse_interceptor_def(&n, source_file, out);
                } else if n.has_tag_name("interceptor-stack") {
                    parse_stack_def(&n, source_file, out);
                }
            }
        }

        // Package default: <default-interceptor-ref name="…"/> — a ref with no concrete
        // referrer symbol (the package), kept for find-usages/diagnostics.
        for dref in pkg.children().filter(|n| n.has_tag_name("default-interceptor-ref")) {
            if let Some(name) = dref.attribute("name") {
                out.refs.push(InterceptorRefUse {
                    referrer: String::new(),
                    ref_name: name.to_string(),
                    is_default: true,
                    source_file: source_file.to_string(),
                    name_offset: attr_value_offset(&dref, "name"),
                });
            }
        }

        // Per-action <interceptor-ref name="…"/> — referrer is the action's qualified name.
        for action in pkg.children().filter(|n| n.has_tag_name("action")) {
            let Some(aname) = action.attribute("name") else { continue };
            let qname = join_ns(&namespace, aname);
            for iref in action.children().filter(|n| n.has_tag_name("interceptor-ref")) {
                if let Some(name) = iref.attribute("name") {
                    push_ref(out, &qname, name, false, source_file, attr_value_offset(&iref, "name"));
                }
            }
        }
    }
}

fn parse_interceptor_def(n: &Node, source_file: &str, out: &mut InterceptorParse) {
    let Some(name) = n.attribute("name") else { return };
    let class = n.attribute("class").unwrap_or("").to_string();
    out.interceptors.push(InterceptorRecord {
        name: name.to_string(),
        class,
        source_file: source_file.to_string(),
        name_offset: attr_value_offset(n, "name"),
    });
}

fn parse_stack_def(n: &Node, source_file: &str, out: &mut InterceptorParse) {
    let Some(name) = n.attribute("name") else { return };
    let mut refs = Vec::new();
    for iref in n.children().filter(|c| c.has_tag_name("interceptor-ref")) {
        if let Some(ref_name) = iref.attribute("name") {
            refs.push(ref_name.to_string());
            // The stack→member edge: referrer is the stack's own name.
            push_ref(out, name, ref_name, false, source_file, attr_value_offset(&iref, "name"));
        }
    }
    out.stacks.push(InterceptorStackRecord {
        name: name.to_string(),
        refs,
        source_file: source_file.to_string(),
        name_offset: attr_value_offset(n, "name"),
    });
}

/// Record an interceptor-ref use + its `InterceptorRefToDef` edge (referrer → target).
fn push_ref(
    out: &mut InterceptorParse,
    referrer: &str,
    ref_name: &str,
    is_default: bool,
    source_file: &str,
    name_offset: usize,
) {
    out.refs.push(InterceptorRefUse {
        referrer: referrer.to_string(),
        ref_name: ref_name.to_string(),
        is_default,
        source_file: source_file.to_string(),
        name_offset,
    });
    out.relations.push(Relation {
        from: referrer.to_string(),
        to: ref_name.to_string(),
        kind: RelKind::InterceptorRefToDef,
        // The referrer/target may resolve to a jar-provided def (e.g. `defaultStack`) — the
        // edge is emitted anyway; the integration drops it if an endpoint is unknown.
        inferred: false,
    });
}

/// Byte offset of an attribute's *value* (inside the quotes) on `node`, or 0 if absent.
/// roxmltree ranges are into the parsed source text (docs §5 #10 uses the same for beans).
fn attr_value_offset(node: &Node, attr: &str) -> usize {
    node.attributes()
        .find(|a| a.name() == attr)
        .map(|a| a.range_value().start)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interceptor_defs_stacks_and_refs() {
        let xml = r#"<struts><package name="p" namespace="/do/Sec">
            <interceptors>
              <interceptor name="auth" class="com.x.AuthInterceptor"/>
              <interceptor name="audit" class="com.x.AuditInterceptor"/>
              <interceptor-stack name="secureStack">
                <interceptor-ref name="defaultStack"/>
                <interceptor-ref name="auth"/>
                <interceptor-ref name="audit"/>
              </interceptor-stack>
            </interceptors>
            <default-interceptor-ref name="secureStack"/>
            <action name="edit" class="editAction">
              <interceptor-ref name="secureStack"/>
              <result type="tiles">x</result>
            </action>
          </package></struts>"#;
        let file = crate::test_support::tmp("struts-icept.xml", xml);
        let mut out = InterceptorParse::default();
        parse_file(&file, &mut out);

        // two interceptor defs, one stack.
        assert_eq!(out.interceptors.len(), 2);
        let auth = out.interceptors.iter().find(|i| i.name == "auth").unwrap();
        assert_eq!(auth.class, "com.x.AuthInterceptor");
        assert!(auth.name_offset > 0);

        let stack = out.stacks.iter().find(|s| s.name == "secureStack").unwrap();
        assert_eq!(stack.refs, vec!["defaultStack", "auth", "audit"]);

        // refs: 3 inside the stack + 1 default + 1 on the action = 5.
        assert_eq!(out.refs.len(), 5);
        // the action ref is keyed by the action qualified name.
        assert!(out
            .refs
            .iter()
            .any(|r| r.referrer == "/do/Sec/edit" && r.ref_name == "secureStack" && !r.is_default));
        // the default ref carries no referrer and is flagged.
        assert!(out.refs.iter().any(|r| r.is_default && r.ref_name == "secureStack" && r.referrer.is_empty()));
        // the stack members are keyed by the stack name.
        assert!(out.refs.iter().any(|r| r.referrer == "secureStack" && r.ref_name == "auth"));

        // one InterceptorRefToDef edge per non-default ref (4).
        assert_eq!(
            out.relations.iter().filter(|r| r.kind == RelKind::InterceptorRefToDef).count(),
            4
        );
    }

    #[test]
    fn fragment_without_interceptors_yields_nothing() {
        let xml = r#"<struts><package name="p" namespace="/do/X">
            <action name="a" class="b"><result>ok</result></action>
          </package></struts>"#;
        let file = crate::test_support::tmp("struts-noicept.xml", xml);
        let mut out = InterceptorParse::default();
        parse_file(&file, &mut out);
        assert!(out.interceptors.is_empty());
        assert!(out.stacks.is_empty());
        assert!(out.refs.is_empty());
    }
}
