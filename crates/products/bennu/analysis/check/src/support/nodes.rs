//! Small readings of the tree that every check needs and none of them owns: what a node's text is,
//! which field it fills in its parent, what modifiers a declaration carries, whether a binary name is
//! a primitive.
//!
//! Each of these was written between two and five times across the crate. None of the copies was
//! wrong — they are four-line functions — but a copy is a place a fix does not reach, and this crate
//! has already paid for that twice: once where a parameter type resolved against the file in one
//! check and against its owner in the next, and once where a name shadowed by an `instanceof` pattern
//! was invisible to one check and not the other. These are here so the count stops at one.
//!
//! Nothing here resolves anything. A helper that needs a `TypeResolver` belongs in
//! [`crate::support::method_sig`] (signatures) or [`crate::support::resolve`] (type names) instead.

use tree_sitter::Node;

/// A node's source text.
pub(crate) fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

/// The first direct named child of `n` with the given kind.
pub(crate) fn child_of_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == kind {
            return Some(ch);
        }
    }
    None
}

/// The field name that immediate child `child` occupies in `parent` (`name`, `value`, `object`,
/// `field`, …), or `None` if it fills no named field.
///
/// The slot is the reliable discriminator between a binding and a reference: the same `identifier`
/// node means one thing in a declarator's `name` and another in its `value`.
pub(crate) fn child_field_name(parent: Node, child: Node) -> Option<String> {
    let mut c = parent.walk();
    if c.goto_first_child() {
        loop {
            if c.node().id() == child.id() {
                return c.field_name().map(str::to_string);
            }
            if !c.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Whether a declaration carries `keyword` among its modifiers.
///
/// Reads the `modifiers` node's text and splits it on whitespace, which is why an annotation in
/// front does not confuse it: `@Override public` splits into words and none of them is `private`.
pub(crate) fn has_keyword(node: Node, bytes: &[u8], keyword: &str) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            if let Ok(t) = ch.utf8_text(bytes) {
                return t.split_whitespace().any(|w| w == keyword);
            }
        }
    }
    false
}

/// The keyword modifiers on a declaration — `["public", "abstract"]`. Annotations, which are named
/// nodes inside `modifiers`, are excluded.
pub(crate) fn modifier_keywords<'a>(node: Node, bytes: &'a [u8]) -> Vec<&'a str> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut out = Vec::new();
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() {
                    if let Ok(t) = m.utf8_text(bytes) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// The last segment of a binary name — `java/util/Map$Entry` → `Entry`.
///
/// Splits on both separators because a nested type has two spellings in circulation (`Outer/Inner`
/// from source, `Outer$Inner` from bytecode) and a message should read the same either way.
/// Whether `kind` names a node that is a written REFERENCE type: `Foo`, `a.b.Foo`, `Foo<Bar>`.
///
/// Excludes primitives and arrays on purpose — the callers are all asking "is this a class or
/// interface name I can resolve". There were three copies of this list, and a fourth that also
/// accepted array and primitive nodes under the same name; that one is
/// [`is_written_type_node`](crate::decls::erasure_clash), and keeping the two apart is why they are named
/// differently.
pub(crate) fn is_class_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

pub(crate) fn simple_name(binary: &str) -> &str {
    binary.rsplit(['/', '$']).next().unwrap_or(binary)
}

/// Whether a binary name is one of Java's primitives (or `void`).
pub(crate) fn is_primitive(binary: &str) -> bool {
    matches!(
        binary,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

/// Whether a binary name looks like an unresolved type VARIABLE (`T`, `K`, `E`) rather than a type.
///
/// A single uppercase letter, which is the convention every generic declaration follows and the only
/// signal available once a name has failed to resolve. Deliberately narrow: a real one-letter class
/// would be misread, and the cost of that is a skipped check rather than a wrong one.
pub(crate) fn is_type_var(binary: &str) -> bool {
    binary.len() == 1 && binary.chars().all(|c| c.is_ascii_uppercase())
}

/// The simple names of the type-level annotations that make a class's member list **partly
/// generated** — the members exist in the compiled class and nowhere in the source, so the index
/// cannot see them and a "this name resolves to nothing" check would report every one of them.
///
/// Lombok is the whole reason this list exists: `@Data` on a class means `getName()` / `setName(…)`
/// are legal bare calls inside it with no declaration anywhere to point at, and `@Slf4j` means the
/// bare field `log` is too. A checker that does not know this reports a page of errors on a class
/// that compiles, which is the single fastest way to make someone turn the Problems panel off.
///
/// Matched on the annotation's LAST name segment, so `@Data` and `@lombok.Data` read the same. Only
/// ever used to SUPPRESS, so an over-broad entry costs coverage on one file, never correctness —
/// which is the right side to err on. Deliberately NOT import-gated for the same reason: a project's
/// own `@Data` generating nothing costs one file's coverage, and demanding the import here would
/// re-introduce false "no such member" reports on the files this exists to protect.
///
/// Lombok's own names come from [`bennu_lombok`], so an annotation added to the catalogue is
/// suppressed here without a second edit. Only the non-Lombok generators are listed.
const OTHER_MEMBER_GENERATING_ANNOTATIONS: &[&str] = &["AutoValue", "Immutable", "Generated"];

/// Which **bare** names a type's generator annotations add, one flag per namespace.
///
/// One bit used to answer both, and it was far too coarse: `@RequiredArgsConstructor` generates a
/// constructor, which nobody calls by name, yet it silenced the undeclared-variable and
/// undeclared-call checks for the whole class — in a Spring project, most classes. A variable and a
/// method are separate namespaces (JLS §6.5), so each check asks only about its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GeneratedNames {
    /// A bare field read — Lombok's logger `log`.
    pub(crate) values: bool,
    /// A bare call — accessors, `builder()`, a `staticName` factory, `@Delegate`'s forwards.
    pub(crate) calls: bool,
}

/// What the type declaration `decl` gets generated — from its own annotations and from those on its
/// fields (`@Getter private String name;` is a `getName()` too).
///
/// A flag set means the matching "does this name exist here?" check must stay silent for the file:
/// the honest answer is "we cannot see all of them".
pub(crate) fn generated_names(decl: Node, bytes: &[u8]) -> GeneratedNames {
    let mut out = GeneratedNames::default();
    for a in crate::support::lombok::annotations_of(decl, bytes) {
        note_generator(&mut out, &a);
    }
    let Some(body) = decl.child_by_field_name("body") else { return out };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for member in n.named_children(&mut c) {
            match member.kind() {
                "field_declaration" => {
                    for a in crate::support::lombok::annotations_of(member, bytes) {
                        note_generator(&mut out, &a);
                    }
                }
                // An enum's fields sit one wrapper deeper than a class's.
                "enum_body_declarations" => stack.push(member),
                _ => {}
            }
        }
    }
    out
}

fn note_generator(out: &mut GeneratedNames, a: &crate::support::lombok::AnnotationRef) {
    if OTHER_MEMBER_GENERATING_ANNOTATIONS.contains(&a.simple) {
        out.values = true;
        out.calls = true;
    }
    out.values |= bennu_lombok::prelude::generates_fields(a.simple);
    out.calls |= bennu_lombok::prelude::generates_methods(a.simple, a.args);
}
