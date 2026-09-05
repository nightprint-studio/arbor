//! Declarations nothing reads — the **unused local** and the **unused private member**.
//!
//! Two checks with one shape: a name declared here, and no use of it anywhere it could be used
//! from. They earn their place in a legacy codebase more than most, because dead weight there is
//! measured in hundreds and almost every hit is actionable — unlike a style inspection, "nothing
//! reads this" is a fact about the program rather than an opinion about it.
//!
//! ## Why `private` and not any member
//!
//! A `private` member's uses are all in one file, so this file is the whole world and silence here
//! means silence everywhere. A package-private or public one is used from files this check never
//! sees, and reporting it would be a false positive on every well-used API in the project. The
//! wider question needs the reference index and belongs with safe delete, which asks it.
//!
//! That boundary is the whole reason this check can be trusted: it never guesses about what it
//! cannot see, it just declines to look at it.
//!
//! ## What it will not report
//!
//! * **Anything annotated.** `@Autowired`, `@Column`, `@Test`, `@JsonProperty` — a framework
//!   reaches these by name at run time and no source file mentions them. This is the single largest
//!   source of false positives in this family and the reason many people turn the inspection off.
//! * **A serialization member.** `serialVersionUID`, `readObject`, `writeObject`, `readResolve`:
//!   used by the runtime, named in no source.
//! * **A private constructor.** A utility class's private constructor exists precisely so nobody
//!   calls it.
//! * **A local whose initialiser does something.** `int ignored = register(x);` is unused as a
//!   value and load-bearing as a call, so the report says *the name* is unused and never suggests
//!   the line can go.

use std::collections::HashSet;

use tree_sitter::Node;

use bennu_proto::prelude::Diagnostic;
use crate::check_id::CheckId;

/// The names a framework or the runtime reaches without any source file naming them.
const RUNTIME_NAMES: &[&str] = &[
    "serialVersionUID",
    "readObject",
    "writeObject",
    "readObjectNoData",
    "readResolve",
    "writeReplace",
    "main",
];

/// Report every private member and local this file declares and never reads.
pub fn unused_member_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for type_decl in types_in(root) {
        out.extend(unused_privates(&type_decl, source));
    }
    for body in callable_bodies(root) {
        out.extend(unused_locals(&body, source));
    }
    out.sort_by_key(|d| d.start);
    out
}

// ── private members ──────────────────────────────────────────────────────────

fn unused_privates(type_decl: &Node<'_>, source: &str) -> Vec<Diagnostic> {
    let Some(body) = type_decl.child_by_field_name("body") else { return Vec::new() };
    // Every identifier ANYWHERE in the type, including its nested types — a private is visible to
    // the whole top-level class, so a use from an inner class is a use.
    let mentions = mention_counts(type_decl, source);
    let mut out = Vec::new();

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if !is_private(&member, source) || is_annotated(&member) {
            continue;
        }
        match member.kind() {
            "field_declaration" => {
                for (name_node, name) in declared_names(&member, source) {
                    if RUNTIME_NAMES.contains(&name.as_str()) {
                        continue;
                    }
                    // One mention is the declaration itself.
                    if mentions.get(&name).copied().unwrap_or(0) <= 1 {
                        out.push(CheckId::UnusedMember.at(
                            name_node,
                            format!("Private field `{name}` is never read or written"),
                        ));
                    }
                }
            }
            "method_declaration" => {
                let Some(name_node) = member.child_by_field_name("name") else { continue };
                let name = text(&name_node, source).to_string();
                if RUNTIME_NAMES.contains(&name.as_str()) {
                    continue;
                }
                if mentions.get(&name).copied().unwrap_or(0) <= 1 {
                    out.push(CheckId::UnusedMember.at(
                        name_node,
                        format!("Private method `{name}` is never called"),
                    ));
                }
            }
            // A private constructor is how a utility class says "do not instantiate me". Its whole
            // job is to be uncalled.
            _ => {}
        }
    }
    out
}

// ── locals ───────────────────────────────────────────────────────────────────

fn unused_locals(body: &Node<'_>, source: &str) -> Vec<Diagnostic> {
    let mentions = mention_counts(body, source);
    let mut out = Vec::new();
    for decl in descendants(*body, "local_variable_declaration") {
        // A declaration inside a lambda or an anonymous class belongs to that body, and this walk
        // reaches it from the outer one too. Counting mentions over the outer body is still
        // correct — a name used nowhere in the outer body is used nowhere in the inner one either.
        for (name_node, name) in declared_names(&decl, source) {
            if mentions.get(&name).copied().unwrap_or(0) <= 1 {
                out.push(CheckId::UnusedMember.at(
                    name_node,
                    format!("`{name}` is never read"),
                ));
            }
        }
    }
    out
}

// ── the shared pieces ────────────────────────────────────────────────────────

/// How many times each identifier is written inside `scope`, counting the declaration.
///
/// Deliberately **textual over the identifier nodes** rather than resolved. A resolved count would
/// be better and is not available here — but the direction of the error matters: an over-count
/// (some other `total` in the same type) makes the check stay quiet, and a check that stays quiet
/// too often is a missed report. An under-count would be a false positive, and there is no way to
/// under-count by looking at every identifier.
fn mention_counts(scope: &Node<'_>, source: &str) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for id in identifiers(*scope) {
        *counts.entry(text(&id, source).to_string()).or_insert(0) += 1;
    }
    counts
}

/// The `(node, name)` of every name a declaration introduces — several for `int a, b;`.
fn declared_names<'t>(decl: &Node<'t>, source: &str) -> Vec<(Node<'t>, String)> {
    let mut out = Vec::new();
    let mut cursor = decl.walk();
    for d in decl.named_children(&mut cursor) {
        if d.kind() == "variable_declarator" {
            if let Some(name) = d.child_by_field_name("name") {
                out.push((name, text(&name, source).to_string()));
            }
        }
    }
    out
}

fn is_private(member: &Node<'_>, source: &str) -> bool {
    let mut cursor = member.walk();
    let private = member.named_children(&mut cursor).any(|c| {
        c.kind() == "modifiers" && text(&c, source).split_whitespace().any(|w| w == "private")
    });
    private
}

/// Whether the member carries any annotation — see the module docs for why one is enough.
fn is_annotated(member: &Node<'_>) -> bool {
    let mut cursor = member.walk();
    let annotated = member.named_children(&mut cursor).any(|c| {
        c.kind() == "modifiers" && {
            let mut inner = c.walk();
            let has = c
                .named_children(&mut inner)
                .any(|m| matches!(m.kind(), "annotation" | "marker_annotation"));
            has
        }
    });
    annotated
}

fn types_in(root: Node<'_>) -> Vec<Node<'_>> {
    const KINDS: &[&str] = &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
        "record_declaration",
    ];
    let mut out = Vec::new();
    collect(root, &|n| KINDS.contains(&n.kind()), &mut out);
    out
}

fn callable_bodies(root: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    collect(
        root,
        &|n| matches!(n.kind(), "method_declaration" | "constructor_declaration"),
        &mut out,
    );
    out.into_iter().filter_map(|n| n.child_by_field_name("body")).collect()
}

fn descendants<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect(node, &|n| n.kind() == kind, &mut out);
    out
}

fn identifiers<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect(node, &|n| n.kind() == "identifier", &mut out);
    out
}

/// Iterative, because a generated file's expression depth is unbounded and a recursive walk over it
/// aborts the process rather than erroring — see `bennu-java`'s deep-expression test.
fn collect<'t>(root: Node<'t>, keep: &dyn Fn(&Node<'t>) -> bool, out: &mut Vec<Node<'t>>) {
    let mut stack = vec![root];
    let mut seen: HashSet<usize> = HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node.id()) {
            continue;
        }
        if keep(&node) {
            out.push(node);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn messages(src: &str) -> Vec<String> {
        let tree = parse_java(src).unwrap();
        unused_member_errors(tree.root_node(), src)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn a_private_field_nothing_reads_is_reported() {
        let msgs = messages("class A {\n    private int unused;\n    private int used;\n    int f() { return used; }\n}");
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert!(msgs[0].contains("`unused`"), "{msgs:?}");
    }

    #[test]
    fn a_private_method_nothing_calls_is_reported() {
        let msgs = messages("class A {\n    private void helper() {}\n    void f() {}\n}");
        assert!(msgs.iter().any(|m| m.contains("`helper`")), "{msgs:?}");
    }

    /// The boundary that makes this check trustworthy: it never looks at what it cannot see all of.
    #[test]
    fn a_public_member_is_not_this_checks_business() {
        let msgs = messages("class A {\n    public int api;\n    void f() {}\n}");
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    /// The single largest source of false positives in this family, and why people switch it off.
    #[test]
    fn an_annotated_member_is_left_alone() {
        let msgs = messages("class A {\n    @Autowired\n    private Service svc;\n}");
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn the_runtime_reaches_serialization_members_without_naming_them() {
        let msgs = messages(
            "class A implements java.io.Serializable {\n    private static final long serialVersionUID = 1L;\n    private void writeObject(java.io.ObjectOutputStream o) {}\n}",
        );
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    /// A private constructor's whole job is to be uncalled.
    #[test]
    fn a_private_constructor_is_not_reported() {
        let msgs = messages("class A {\n    private A() {}\n    static int f() { return 1; }\n}");
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn an_unread_local_is_reported() {
        let msgs = messages("class A {\n    void f() {\n        int n = compute();\n    }\n}");
        assert!(msgs.iter().any(|m| m.contains("`n` is never read")), "{msgs:?}");
    }

    #[test]
    fn a_local_read_once_is_not_reported() {
        let msgs = messages("class A {\n    int f() {\n        int n = compute();\n        return n;\n    }\n}");
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    /// A private used only from an inner class is used: a private is visible to the whole
    /// top-level class, and a check that missed that would report half the builder patterns ever
    /// written.
    #[test]
    fn a_use_from_a_nested_class_counts() {
        let msgs = messages(
            "class A {\n    private int shared;\n    class Inner {\n        int read() { return shared; }\n    }\n}",
        );
        assert!(msgs.is_empty(), "{msgs:?}");
    }
}
