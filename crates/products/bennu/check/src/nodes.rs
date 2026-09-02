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
//! [`crate::method_sig`] (signatures) or [`crate::resolve`] (type names) instead.

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
/// [`is_written_type_node`](crate::erasure_clash), and keeping the two apart is why they are named
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
/// which is the right side to err on.
const MEMBER_GENERATING_ANNOTATIONS: &[&str] = &[
    // Lombok — members added to the annotated type itself.
    "Data", "Value", "Getter", "Setter", "Builder", "SuperBuilder", "Accessors", "With",
    "RequiredArgsConstructor", "AllArgsConstructor", "NoArgsConstructor", "EqualsAndHashCode",
    "ToString", "UtilityClass", "FieldNameConstants",
    // Lombok's loggers — each injects a `log` field.
    "Slf4j", "XSlf4j", "Log", "Log4j", "Log4j2", "CommonsLog", "JBossLog", "Flogger", "CustomLog",
    // Other generators whose output lands on the annotated type.
    "AutoValue", "Immutable", "Generated",
];

/// Whether the type declaration `decl` carries an annotation from
/// [`MEMBER_GENERATING_ANNOTATIONS`] — i.e. whether its members are partly invisible to the index.
///
/// A `true` means every "does this name exist on this type?" check must stay silent for the file:
/// the honest answer is "we cannot see all of them".
pub(crate) fn has_generated_members(decl: Node, bytes: &[u8]) -> bool {
    let mut c = decl.walk();
    for ch in decl.named_children(&mut c) {
        // Annotations sit in the declaration's `modifiers` node, before the `class`/`enum` keyword.
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for m in ch.named_children(&mut mc) {
            if !matches!(m.kind(), "annotation" | "marker_annotation") {
                continue;
            }
            let Some(name) = m.child_by_field_name("name") else { continue };
            let Ok(t) = name.utf8_text(bytes) else { continue };
            let simple = t.rsplit('.').next().unwrap_or(t);
            if MEMBER_GENERATING_ANNOTATIONS.contains(&simple) {
                return true;
            }
        }
    }
    false
}
