//! The MapStruct annotations on one mapping method, read off the syntax tree.
//!
//! Not off the facts model, deliberately: `@Mappings({ @Mapping(target = "a"), @Mapping(target = "b") })`
//! nests annotations inside an array, and the facts model collects every string under an argument as
//! a positional value of the OUTER annotation — `target` and `source` of the inner ones are gone. The
//! tree still has them.
//!
//! Which annotation a name *is* stays the caller's question (`resolve`), answered through the file's
//! imports like everything else; this module only reads what is written.

use bennu_java::prelude::{named_child_of, node_text};
use tree_sitter::Node;

/// A plain string literal's contents, with the byte span **inside** the quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lit {
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// One `@Mapping` element's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elem {
    Absent,
    Literal(Lit),
    /// A constant, a concatenation, a text block, an escape — something this crate will not read.
    Other,
}

/// One `@Mapping`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingAnn {
    /// The whole `@Mapping(...)`.
    pub start: usize,
    pub end: usize,
    pub target: Elem,
    pub source: Elem,
    /// Every element name written, in order.
    pub keys: Vec<String>,
    /// `ignore = true`, literally.
    pub ignore: bool,
}

impl MappingAnn {
    pub fn has(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }
}

/// What the annotations on a mapping method say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MethodAnns {
    /// Where the declaration starts — its first annotation or modifier. New annotations go on this line.
    pub decl_start: usize,
    pub mappings: Vec<MappingAnn>,
    /// `@BeanMapping(ignoreByDefault = true)`.
    pub ignore_by_default: bool,
    /// A `@BeanMapping` element that changes what the target is or how it is built (`resultType`,
    /// `qualifiedBy`, `builder`, …).
    pub opaque: bool,
    /// `@BeanMapping(unmappedTargetPolicy = …)`, raw.
    pub policy: Option<String>,
    /// `@InheritConfiguration` / `@InheritInverseConfiguration`: mappings come from somewhere else.
    pub inherits: bool,
    /// An annotation this crate does not recognise. MapStruct supports **composed** mapping annotations
    /// — a project's `@ToEntity` meta-annotated with `@Mapping`s — so an unknown one may be mapping
    /// half the target, and nothing is said about unmapped properties.
    pub foreign: bool,
}

/// The `method_declaration` whose name starts at `name_offset`.
pub fn method_node(root: Node<'_>, name_offset: usize) -> Option<Node<'_>> {
    let mut node = root.descendant_for_byte_range(name_offset, name_offset + 1)?;
    loop {
        if node.kind() == "method_declaration" {
            let named_here = node.child_by_field_name("name").is_some_and(|n| n.start_byte() == name_offset);
            return named_here.then_some(node);
        }
        node = node.parent()?;
    }
}

/// Read a method's annotations. `resolve` maps a name as written to the MapStruct (or harmless
/// `java.lang`) annotation it is, or `None` for anything else.
pub fn read_method(method: Node<'_>, source: &str, resolve: &dyn Fn(&str) -> Option<&'static str>) -> MethodAnns {
    let mut out = MethodAnns { decl_start: method.start_byte(), ..MethodAnns::default() };
    let Some(modifiers) = named_child_of(method, "modifiers") else { return out };
    let mut cursor = modifiers.walk();
    let anns: Vec<Node<'_>> = modifiers
        .named_children(&mut cursor)
        .filter(|c| matches!(c.kind(), "annotation" | "marker_annotation"))
        .collect();
    for ann in anns {
        let Some(name) = ann.child_by_field_name("name") else { continue };
        match resolve(node_text(&name, source)) {
            Some("Mapping") => out.mappings.push(read_mapping(ann, source)),
            Some("Mappings") => read_mappings(ann, source, resolve, &mut out),
            Some("BeanMapping") => read_bean_mapping(ann, source, &mut out),
            Some("InheritConfiguration") | Some("InheritInverseConfiguration") => out.inherits = true,
            Some(_) => {}
            None => out.foreign = true,
        }
    }
    out
}

/// `(element name, value node)` for every argument; a bare positional argument has the name `""`.
fn arguments<'t>(ann: Node<'t>, source: &str) -> Vec<(String, Node<'t>)> {
    let Some(args) = ann.child_by_field_name("arguments") else { return Vec::new() };
    let mut cursor = args.walk();
    let children: Vec<Node<'t>> = args.named_children(&mut cursor).collect();
    children
        .into_iter()
        .filter(|c| !is_comment(c))
        .filter_map(|c| {
            if c.kind() != "element_value_pair" {
                return Some((String::new(), c));
            }
            let key = c.child_by_field_name("key")?;
            let value = c.child_by_field_name("value")?;
            Some((node_text(&key, source).to_string(), value))
        })
        .collect()
}

fn is_comment(node: &Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

fn read_mapping(ann: Node<'_>, source: &str) -> MappingAnn {
    let mut m = MappingAnn {
        start: ann.start_byte(),
        end: ann.end_byte(),
        target: Elem::Absent,
        source: Elem::Absent,
        keys: Vec::new(),
        ignore: false,
    };
    for (key, value) in arguments(ann, source) {
        match key.as_str() {
            "target" => m.target = element(value, source),
            "source" => m.source = element(value, source),
            "ignore" => m.ignore = node_text(&value, source).trim() == "true",
            _ => {}
        }
        m.keys.push(key);
    }
    m
}

/// Only a plain one-line literal with no escapes is read: its contents are then byte-for-byte the
/// file, so every span inside it is exact.
fn element(value: Node<'_>, source: &str) -> Elem {
    if value.kind() != "string_literal" {
        return Elem::Other;
    }
    let raw = node_text(&value, source);
    let plain = raw.len() >= 2
        && raw.starts_with('"')
        && raw.ends_with('"')
        && !raw.starts_with("\"\"\"")
        && !raw.contains('\\');
    if !plain {
        return Elem::Other;
    }
    Elem::Literal(Lit {
        value: raw[1..raw.len() - 1].to_string(),
        start: value.start_byte() + 1,
        end: value.end_byte() - 1,
    })
}

fn read_mappings(ann: Node<'_>, source: &str, resolve: &dyn Fn(&str) -> Option<&'static str>, out: &mut MethodAnns) {
    for (key, value) in arguments(ann, source) {
        if !(key.is_empty() || key == "value") {
            continue;
        }
        let items: Vec<Node<'_>> = if value.kind() == "element_value_array_initializer" {
            let mut cursor = value.walk();
            let children: Vec<Node<'_>> = value.named_children(&mut cursor).collect();
            children
        } else {
            vec![value]
        };
        for item in items.into_iter().filter(|i| !is_comment(i)) {
            let is_mapping = matches!(item.kind(), "annotation" | "marker_annotation")
                && item
                    .child_by_field_name("name")
                    .is_some_and(|n| resolve(node_text(&n, source)) == Some("Mapping"));
            if is_mapping {
                out.mappings.push(read_mapping(item, source));
            } else {
                // A constant array, or an annotation that is not `@Mapping`: unreadable mappings.
                out.foreign = true;
            }
        }
    }
}

/// The `@BeanMapping` elements that leave the target and its properties as they are.
const INERT_BEAN_MAPPING: &[&str] = &[
    "ignoreUnmappedSourceProperties",
    "nullValueMappingStrategy",
    "nullValuePropertyMappingStrategy",
    "nullValueCheckStrategy",
    "subclassExhaustiveStrategy",
];

fn read_bean_mapping(ann: Node<'_>, source: &str, out: &mut MethodAnns) {
    for (key, value) in arguments(ann, source) {
        let text = node_text(&value, source).trim();
        match key.as_str() {
            "ignoreByDefault" if text == "true" => out.ignore_by_default = true,
            "ignoreByDefault" if text == "false" => {}
            "unmappedTargetPolicy" => out.policy = Some(text.to_string()),
            k if INERT_BEAN_MAPPING.contains(&k) => {}
            _ => out.opaque = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn resolve(written: &str) -> Option<&'static str> {
        match written {
            "Mapping" | "org.mapstruct.Mapping" => Some("Mapping"),
            "Mappings" => Some("Mappings"),
            "BeanMapping" => Some("BeanMapping"),
            "InheritConfiguration" => Some("InheritConfiguration"),
            "Override" => Some("Override"),
            _ => None,
        }
    }

    fn read(src: &str, method: &str) -> MethodAnns {
        let tree = parse_java(src).expect("grammar loads");
        let at = src.find(&format!(" {method}(")).expect("method in source") + 1;
        let node = method_node(tree.root_node(), at).expect("method node");
        read_method(node, src, &resolve)
    }

    fn lit(e: &Elem) -> &Lit {
        match e {
            Elem::Literal(l) => l,
            other => panic!("not a literal: {other:?}"),
        }
    }

    #[test]
    fn a_mapping_reads_its_strings_with_spans_inside_the_quotes() {
        let src = "interface M {\n    @Mapping(target = \"name\", source = \"first.name\")\n    Dto toDto(E e);\n}";
        let a = read(src, "toDto");
        let target = lit(&a.mappings[0].target);
        assert_eq!(target.value, "name");
        assert_eq!(&src[target.start..target.end], "name");
        assert_eq!(&src[lit(&a.mappings[0].source).start..lit(&a.mappings[0].source).end], "first.name");
        assert_eq!(&src[a.decl_start..a.decl_start + 8], "@Mapping", "the declaration starts at its annotations");
    }

    #[test]
    fn mappings_nested_in_an_array_are_read_one_by_one() {
        let src = "interface M { @Mappings({ @Mapping(target = \"a\", ignore = true), @org.mapstruct.Mapping(target = \"b\", constant = \"x\") }) Dto toDto(E e); }";
        let a = read(src, "toDto");
        assert_eq!(a.mappings.len(), 2);
        assert!(a.mappings[0].ignore);
        assert_eq!(lit(&a.mappings[1].target).value, "b");
        assert!(a.mappings[1].has("constant"));
        assert!(!a.foreign);
    }

    #[test]
    fn what_cannot_be_read_is_marked_rather_than_skipped() {
        let src = "interface M { @Mapping(target = NAME) @ToEntity @BeanMapping(resultType = X.class) Dto toDto(E e); }";
        let a = read(src, "toDto");
        assert_eq!(a.mappings[0].target, Elem::Other);
        assert!(a.foreign, "an unknown annotation may be a composed mapping");
        assert!(a.opaque);
    }

    #[test]
    fn bean_mapping_and_inheritance_flags() {
        let src = "interface M { @Override @InheritConfiguration @BeanMapping(ignoreByDefault = true, unmappedTargetPolicy = ReportingPolicy.IGNORE) Dto toDto(E e); }";
        let a = read(src, "toDto");
        assert!(a.ignore_by_default && a.inherits && !a.foreign && !a.opaque);
        assert_eq!(a.policy.as_deref(), Some("ReportingPolicy.IGNORE"));
    }
}
