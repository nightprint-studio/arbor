//! Reading an XSD.
//!
//! A schema on disk is well-formed by construction, so this uses a document parser
//! (`roxmltree`) rather than the tolerant lexer `bennu-xml` runs over a buffer being edited. It
//! also gives byte ranges for free, which is what "go to the declaration of this tag" needs.
//!
//! ## Groups are expanded at the point of use
//!
//! `xs:group ref` and `xs:attributeGroup ref` are resolved into the type that references them,
//! rather than left as a reference for the consumer to chase. Two reasons, and the second is the
//! important one: it keeps the consumer from having to know XSD's resolution rules at all, and
//! it means a group defined *after* its use still resolves — which is common, and which a
//! single-pass consumer would get wrong.
//!
//! That needs two passes: collect the named groups, then read the types. Extension chains are
//! **not** flattened the same way — those stay as a `base` name and are walked by
//! [`Xsd::children_of`], because a type's own particles and its inherited ones are worth telling
//! apart when reporting where something came from.
//!
//! ## Flattened names, but not flattened cardinality
//!
//! The particle *tree* is not kept — `sequence`, `choice` and `all` all collapse into one list of
//! names. What survives the collapse is the one thing that cannot be recovered afterwards:
//! whether a document that omits a name is thereby invalid. It is computed **during** the walk,
//! by carrying a single `mandatory` flag down through the particles, because by the time the list
//! is flat the `xs:choice` that made three of its five names optional is gone.
//!
//! That flag only ever narrows. A name is left required exactly when every particle between the
//! type and the declaration demanded it; a multi-branch `xs:choice`, a `minOccurs="0"` on any
//! wrapper, or a substitution group head clears it and it is never set again.

use roxmltree::{Document, Node, ParsingOptions};

use crate::model::*;

/// Parse a schema. `None` when the text is not well-formed XML or is not an `xs:schema` —
/// there is nothing partial to salvage from a broken schema, unlike a DTD.
pub fn parse(source: &str) -> Option<Xsd> {
    let doc = Document::parse_with_options(
        source,
        ParsingOptions { allow_dtd: true, ..ParsingOptions::default() },
    )
    .ok()?;
    let root = doc.root_element();
    if local(root.tag_name().name()) != "schema" {
        return None;
    }

    let mut xsd = Xsd {
        target_namespace: root.attribute("targetNamespace").unwrap_or_default().to_string(),
        qualified: root.attribute("elementFormDefault") == Some("qualified"),
        substitution_heads: substitution_heads(root),
        ..Xsd::default()
    };

    // Pass one: the named groups, so a reference to one declared further down still resolves.
    for child in elements_of(root) {
        match tag(child) {
            "group" => {
                if let Some(name) = child.attribute("name") {
                    xsd.groups.push(Group {
                        name: name.to_string(),
                        members: Vec::new(), // filled below, once groups can reference groups
                        offset: child.range().start,
                    });
                }
            }
            "attributeGroup" => {
                if let Some(name) = child.attribute("name") {
                    xsd.attribute_groups.push(Group {
                        name: name.to_string(),
                        members: Vec::new(),
                        offset: child.range().start,
                    });
                }
            }
            _ => {}
        }
    }
    // Pass two: fill the groups' members. A group referencing another that has not been filled
    // yet simply contributes nothing on this round; one more round settles the common case
    // (a group of groups) without needing a topological sort.
    for _ in 0..2 {
        for child in elements_of(root) {
            let Some(name) = child.attribute("name") else { continue };
            match tag(child) {
                "group" => {
                    let members = collect_elements(child, source, &xsd);
                    if let Some(g) = xsd.groups.iter_mut().find(|g| g.name == name) {
                        g.members = members;
                    }
                }
                "attributeGroup" => {
                    let members = collect_attributes(child, source, &xsd);
                    if let Some(g) = xsd.attribute_groups.iter_mut().find(|g| g.name == name) {
                        g.members = members;
                    }
                }
                _ => {}
            }
        }
    }

    // Pass three: everything else.
    for child in elements_of(root) {
        match tag(child) {
            "element" => {
                if child.attribute("name").is_some() {
                    xsd.elements.push(element_decl(child, source, &xsd));
                }
            }
            "complexType" => {
                if child.attribute("name").is_some() {
                    xsd.complex_types.push(complex_type(child, source, &xsd));
                }
            }
            "simpleType" => {
                if child.attribute("name").is_some() {
                    xsd.simple_types.push(simple_type(child, source));
                }
            }
            "include" | "import" | "redefine" => {
                if let Some(loc) = child.attribute("schemaLocation") {
                    xsd.includes.push(loc.to_string());
                }
            }
            _ => {}
        }
    }
    Some(xsd)
}

// ── Declarations ─────────────────────────────────────────────────────────────

fn element_decl<'a>(node: Node<'a, 'a>, source: &str, xsd: &Xsd) -> XsdElement {
    let inline_complex = children_named(node, "complexType")
        .next()
        .map(|t| Box::new(complex_type(t, source, xsd)));
    let inline_simple = children_named(node, "simpleType").next();
    XsdElement {
        // A `ref` may be prefixed; the prefix is the schema's own business (see `model::local`).
        name: local(node.attribute("name").or_else(|| node.attribute("ref")).unwrap_or_default())
            .to_string(),
        type_name: node.attribute("type").unwrap_or_default().to_string(),
        values: inline_simple.map(enumeration_of).unwrap_or_default(),
        inline_type: inline_complex,
        // What this declaration's OWN particle demands. The particles above it can only take it
        // away, and `collect_elements` is where that happens — a global declaration, which has no
        // particle above it at all, keeps this answer.
        required: demands_one(node),
        repeats: !matches!(node.attribute("maxOccurs"), None | Some("1")),
        doc: documentation(node),
        offset: node.range().start,
        line: line_at(source, node.range().start),
    }
}

fn complex_type<'a>(node: Node<'a, 'a>, source: &str, xsd: &Xsd) -> ComplexType {
    // `complexContent`/`simpleContent` wrap the real body one level down; the base is on the
    // extension or restriction inside them.
    let body = children_named(node, "complexContent")
        .chain(children_named(node, "simpleContent"))
        .flat_map(|c| c.children().filter(|n| n.is_element()))
        .find(|c| matches!(tag(*c), "extension" | "restriction"));

    ComplexType {
        name: node.attribute("name").unwrap_or_default().to_string(),
        base: body.and_then(|b| b.attribute("base")).unwrap_or_default().to_string(),
        mixed: node.attribute("mixed") == Some("true")
            || body.and_then(|b| b.attribute("mixed")) == Some("true"),
        any: has_any(body.unwrap_or(node)),
        elements: collect_elements(body.unwrap_or(node), source, xsd),
        attributes: collect_attributes(body.unwrap_or(node), source, xsd),
        doc: documentation(node),
        offset: node.range().start,
        line: line_at(source, node.range().start),
    }
}

fn simple_type<'a>(node: Node<'a, 'a>, source: &str) -> SimpleType {
    SimpleType {
        name: node.attribute("name").unwrap_or_default().to_string(),
        base: children_named(node, "restriction")
            .next()
            .and_then(|r| r.attribute("base"))
            .unwrap_or_default()
            .to_string(),
        values: enumeration_of(node),
        doc: documentation(node),
        offset: node.range().start,
        line: line_at(source, node.range().start),
    }
}

// ── Particles ────────────────────────────────────────────────────────────────

/// Every element a node's particles permit, flattened — with the cardinality the *whole* path of
/// particles implies, not the one written on each declaration.
///
/// `sequence`, `choice` and `all` contribute the same names on purpose — see the crate docs. A
/// `group ref` contributes the members of the group it names, demoted to optional when the
/// reference itself is.
///
/// A name reached twice keeps the **weaker** answer. A schema that writes an element once in a
/// sequence and once inside a choice has said, between the two, that a document may leave it out;
/// taking whichever the walk saw first would make the answer depend on declaration order.
fn collect_elements<'a>(node: Node<'a, 'a>, source: &str, xsd: &Xsd) -> Vec<XsdElement> {
    let mut out: Vec<XsdElement> = Vec::new();
    walk_particles(node, true, &mut |child, mandatory| match tag(child) {
        "element" => {
            let mut e = element_decl(child, source, xsd);
            if !e.name.is_empty() {
                e.required = mandatory && !xsd.substitution_heads.contains(&e.name);
                add(&mut out, e);
            }
        }
        "group" => {
            if let Some(name) = child.attribute("ref") {
                if let Some(g) = xsd.groups.iter().find(|g| g.name == local(name)) {
                    for e in &g.members {
                        let mut e = e.clone();
                        e.required &= mandatory;
                        add(&mut out, e);
                    }
                }
            }
        }
        _ => {}
    });
    out
}

/// Add a declaration to a flattened list, keeping the **weaker** demand on a name already there.
fn add(out: &mut Vec<XsdElement>, e: XsdElement) {
    match out.iter_mut().find(|x| x.name == e.name) {
        Some(seen) => seen.required &= e.required,
        None => out.push(e),
    }
}

fn collect_attributes<'a>(node: Node<'a, 'a>, source: &str, xsd: &Xsd) -> Vec<XsdAttribute> {
    let mut out: Vec<XsdAttribute> = Vec::new();
    walk_particles(node, true, &mut |child, _| match tag(child) {
        "attribute" => {
            let Some(name) = child.attribute("name").or_else(|| child.attribute("ref")) else {
                return;
            };
            let name = local(name);
            if out.iter().any(|x| x.name == name) {
                return;
            }
            out.push(XsdAttribute {
                name: name.to_string(),
                type_name: child.attribute("type").unwrap_or_default().to_string(),
                values: children_named(child, "simpleType").next().map(enumeration_of).unwrap_or_default(),
                required: child.attribute("use") == Some("required"),
                default: child.attribute("default").unwrap_or_default().to_string(),
                fixed: child.attribute("fixed").unwrap_or_default().to_string(),
                doc: documentation(child),
                offset: child.range().start,
                line: line_at(source, child.range().start),
            });
        }
        "attributeGroup" => {
            if let Some(name) = child.attribute("ref") {
                if let Some(g) = xsd.attribute_groups.iter().find(|g| g.name == local(name)) {
                    for a in &g.members {
                        if !out.iter().any(|x| x.name == a.name) {
                            out.push(a.clone());
                        }
                    }
                }
            }
        }
        _ => {}
    });
    out
}

/// Nodes worth descending into when looking for particles: the model groups themselves, and the
/// content wrappers. **Not** `element` — an element's own children describe *its* content, not
/// its parent's, and walking into them is how a flattening pass turns a schema into one
/// undifferentiated bag of names.
const TRANSPARENT: &[&str] =
    &["sequence", "choice", "all", "complexContent", "simpleContent", "extension", "restriction"];

/// The particles that count as branches of an `xs:choice`. `annotation` is not one of them, and
/// counting it would turn a single-branch choice with a comment on it into a multi-branch one.
const PARTICLES: &[&str] = &["element", "group", "choice", "sequence", "all", "any"];

/// Walk a type's particles, carrying whether the path so far **demands** what it reaches.
fn walk_particles<'a>(node: Node<'a, 'a>, mandatory: bool, f: &mut impl FnMut(Node<'a, 'a>, bool)) {
    // A choice offering more than one branch makes every one of them optional on its own: a
    // document satisfies it by taking a different branch, so nothing below may be demanded.
    let picks_one = tag(node) == "choice"
        && node.children().filter(|n| n.is_element() && PARTICLES.contains(&tag(*n))).count() > 1;
    for child in node.children().filter(|n| n.is_element()) {
        let here = mandatory && !picks_one && demands_one(child);
        f(child, here);
        if TRANSPARENT.contains(&tag(child)) {
            walk_particles(child, here, f);
        }
    }
}

/// Whether a particle demands at least one occurrence of itself. `minOccurs` defaults to `1` —
/// the schema language's own rule, and the reason a declaration with no occurrence attributes at
/// all is required. A wrapper that cannot carry `minOccurs` at all (`extension`,
/// `complexContent`) is simply never optional, which is the same answer.
fn demands_one<'a>(node: Node<'a, 'a>) -> bool {
    node.attribute("minOccurs").unwrap_or("1") != "0"
}

/// Every name this schema uses as a substitution group head.
///
/// A document may write any member of the group where the head is expected, and nothing here
/// resolves that — so a head is a name whose absence proves nothing, and the flatten clears its
/// `required` for exactly that reason.
fn substitution_heads<'a>(root: Node<'a, 'a>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if let Some(head) = n.attribute("substitutionGroup") {
            let head = local(head).to_string();
            if !out.contains(&head) {
                out.push(head);
            }
        }
        stack.extend(n.children().filter(|c| c.is_element()));
    }
    out
}

/// Whether an `xs:any` appears in this type's particles — the signal that anything at all may be
/// written, and therefore that nothing here may be reported as unexpected.
fn has_any<'a>(node: Node<'a, 'a>) -> bool {
    let mut found = false;
    walk_particles(node, true, &mut |child, _| {
        found |= matches!(tag(child), "any" | "anyAttribute");
    });
    found
}

// ── Small readers ────────────────────────────────────────────────────────────

fn tag<'a>(node: Node<'a, 'a>) -> &'a str {
    node.tag_name().name()
}

fn elements_of<'a>(node: Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children().filter(|n| n.is_element())
}

fn children_named<'a>(node: Node<'a, 'a>, name: &'static str) -> impl Iterator<Item = Node<'a, 'a>> {
    elements_of(node).filter(move |n| tag(*n) == name)
}

/// Every `xs:enumeration` value under a node, at any depth — the enumeration may sit inside a
/// `restriction` inside a `simpleType` inside an `attribute`, and which of those wrappers a
/// particular schema used is not information worth branching on.
fn enumeration_of<'a>(node: Node<'a, 'a>) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        for child in n.children().filter(|c| c.is_element()) {
            if tag(child) == "enumeration" {
                if let Some(v) = child.attribute("value") {
                    out.push(v.to_string());
                }
            } else {
                stack.push(child);
            }
        }
    }
    out
}

/// The `xs:documentation` under a node's `xs:annotation`, as prose.
///
/// Only the node's OWN annotation — descending would attach a child element's documentation to
/// its parent, which reads as a schema saying something it never said.
fn documentation<'a>(node: Node<'a, 'a>) -> String {
    children_named(node, "annotation")
        .flat_map(|a| a.children().filter(|c| c.is_element() && tag(*c) == "documentation"))
        .filter_map(|d| d.text())
        .flat_map(|t| t.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn line_at(source: &str, offset: usize) -> u32 {
    source[..offset.min(source.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
      targetNamespace="http://maven.apache.org/POM/4.0.0" elementFormDefault="qualified">"#;

    fn schema(body: &str) -> Xsd {
        parse(&format!("{HEAD}\n{body}\n</xs:schema>")).expect("a well-formed schema")
    }

    #[test]
    fn the_schemas_identity_is_read_off_the_root() {
        let x = schema("");
        assert_eq!(x.target_namespace, "http://maven.apache.org/POM/4.0.0");
        assert!(x.qualified);
        assert!(parse("<not-a-schema/>").is_none());
        assert!(parse("<xs:schema").is_none(), "malformed XML has nothing to salvage");
    }

    #[test]
    fn an_element_with_an_inline_type_carries_its_children_and_attributes() {
        let x = schema(
            r#"<xs:element name="project">
                 <xs:complexType>
                   <xs:sequence>
                     <xs:element name="modelVersion" type="xs:string"/>
                     <xs:element name="dependencies" type="Dependencies" minOccurs="0"/>
                   </xs:sequence>
                   <xs:attribute name="child.project.url" type="xs:string"/>
                 </xs:complexType>
               </xs:element>"#,
        );
        let root = x.element("project").unwrap();
        let kids: Vec<&str> = x.children_of(root).iter().map(|e| e.name.as_str()).collect();
        assert_eq!(kids, ["modelVersion", "dependencies"]);
        assert!(x.children_of(root)[0].required, "minOccurs defaults to 1");
        assert!(!x.children_of(root)[1].required);
        assert_eq!(x.attributes_of(root).len(), 1);
    }

    /// The one that decides whether this is usable on a real schema: nearly every non-trivial
    /// XSD is written as a chain of extensions, and a reader that stopped at the derived type
    /// would miss most of what a document legally contains.
    #[test]
    fn an_extension_chain_is_folded_in() {
        let x = schema(
            r#"<xs:complexType name="Base">
                 <xs:sequence><xs:element name="id" type="xs:string"/></xs:sequence>
                 <xs:attribute name="scope" use="required"/>
               </xs:complexType>
               <xs:complexType name="Derived">
                 <xs:complexContent>
                   <xs:extension base="Base">
                     <xs:sequence><xs:element name="extra" type="xs:string"/></xs:sequence>
                   </xs:extension>
                 </xs:complexContent>
               </xs:complexType>
               <xs:element name="thing" type="Derived"/>"#,
        );
        let e = x.element("thing").unwrap();
        let kids: Vec<&str> = x.children_of(e).iter().map(|k| k.name.as_str()).collect();
        assert_eq!(kids, ["extra", "id"], "own particles first, inherited after");
        let attrs = x.attributes_of(e);
        assert_eq!(attrs.len(), 1);
        assert!(attrs[0].required);
    }

    /// The reason `minOccurs` cannot be read on its own. Every branch of a choice carries the
    /// default `minOccurs="1"`, so reading the attribute alone calls all four of these required
    /// and a document that picks one — which is what a choice is for — is reported three times.
    #[test]
    fn a_branch_of_a_choice_is_never_required() {
        let x = schema(
            r#"<xs:complexType name="Result">
                 <xs:sequence>
                   <xs:element name="name" type="xs:string"/>
                   <xs:choice>
                     <xs:element name="dispatcher" type="xs:string"/>
                     <xs:element name="redirect" type="xs:string"/>
                   </xs:choice>
                 </xs:sequence>
               </xs:complexType>
               <xs:element name="result" type="Result"/>"#,
        );
        let kids = x.children_of(x.element("result").unwrap());
        let must: Vec<&str> =
            kids.iter().filter(|k| k.required).map(|k| k.name.as_str()).collect();
        assert_eq!(must, ["name"]);
    }

    /// A choice offering exactly one thing is not a choice, and treating it as one would throw
    /// away a demand the schema really made.
    #[test]
    fn a_choice_with_one_branch_still_demands_it() {
        let x = schema(
            r#"<xs:element name="e"><xs:complexType>
                 <xs:choice><xs:element name="only" type="xs:string"/></xs:choice>
               </xs:complexType></xs:element>"#,
        );
        assert!(x.children_of(x.element("e").unwrap())[0].required);
    }

    /// `minOccurs` on a *wrapper* is the other half: the elements inside a group nobody has to
    /// write are not required either, however their own declarations read.
    #[test]
    fn an_optional_group_makes_everything_under_it_optional() {
        let x = schema(
            r#"<xs:element name="e"><xs:complexType>
                 <xs:sequence>
                   <xs:element name="always" type="xs:string"/>
                   <xs:sequence minOccurs="0">
                     <xs:element name="inner" type="xs:string"/>
                   </xs:sequence>
                   <xs:group ref="Extra" minOccurs="0"/>
                 </xs:sequence>
               </xs:complexType></xs:element>
               <xs:group name="Extra">
                 <xs:sequence><xs:element name="fromGroup" type="xs:string"/></xs:sequence>
               </xs:group>"#,
        );
        let kids = x.children_of(x.element("e").unwrap());
        let must: Vec<&str> =
            kids.iter().filter(|k| k.required).map(|k| k.name.as_str()).collect();
        assert_eq!(must, ["always"]);
        assert_eq!(kids.len(), 3, "and all three are still legal there");
    }

    /// A group referenced where it *is* mandatory keeps what it demands — otherwise the whole
    /// mechanism would only ever subtract.
    #[test]
    fn a_mandatory_group_reference_keeps_its_demands() {
        let x = schema(
            r#"<xs:element name="e"><xs:complexType>
                 <xs:sequence><xs:group ref="Core"/></xs:sequence>
               </xs:complexType></xs:element>
               <xs:group name="Core">
                 <xs:sequence>
                   <xs:element name="id" type="xs:string"/>
                   <xs:element name="note" type="xs:string" minOccurs="0"/>
                 </xs:sequence>
               </xs:group>"#,
        );
        let kids = x.children_of(x.element("e").unwrap());
        let must: Vec<&str> =
            kids.iter().filter(|k| k.required).map(|k| k.name.as_str()).collect();
        assert_eq!(must, ["id"]);
    }

    /// A document may write any member of a substitution group where the head is expected, and
    /// nothing here resolves that — so the head's absence proves nothing.
    #[test]
    fn a_substitution_group_head_is_never_required() {
        let x = schema(
            r#"<xs:element name="shape" type="xs:string"/>
               <xs:element name="square" type="xs:string" substitutionGroup="shape"/>
               <xs:element name="drawing"><xs:complexType><xs:sequence>
                 <xs:element ref="shape"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        assert_eq!(x.substitution_heads, ["shape"]);
        assert!(!x.children_of(x.element("drawing").unwrap())[0].required);
    }

    /// Written twice, once where it must appear and once where it need not: between them the
    /// schema has said a document may leave it out.
    #[test]
    fn a_name_reached_twice_keeps_the_weaker_demand() {
        let x = schema(
            r#"<xs:element name="e"><xs:complexType>
                 <xs:sequence>
                   <xs:element name="a" type="xs:string"/>
                   <xs:choice>
                     <xs:element name="a" type="xs:string"/>
                     <xs:element name="b" type="xs:string"/>
                   </xs:choice>
                 </xs:sequence>
               </xs:complexType></xs:element>"#,
        );
        let kids = x.children_of(x.element("e").unwrap());
        assert!(!kids.iter().find(|k| k.name == "a").unwrap().required);
    }

    #[test]
    fn a_cycle_in_the_extension_chain_terminates() {
        let x = schema(
            r#"<xs:complexType name="A"><xs:complexContent><xs:extension base="B"/></xs:complexContent></xs:complexType>
               <xs:complexType name="B"><xs:complexContent><xs:extension base="A"/></xs:complexContent></xs:complexType>
               <xs:element name="a" type="A"/>"#,
        );
        assert!(x.children_of(x.element("a").unwrap()).is_empty());
    }

    /// Resolved at the point of use, and a group declared *after* its reference still resolves —
    /// which is common, and which a single-pass reader gets wrong.
    #[test]
    fn groups_are_expanded_where_they_are_referenced_regardless_of_order() {
        let x = schema(
            r#"<xs:complexType name="T">
                 <xs:sequence><xs:group ref="Later"/></xs:sequence>
                 <xs:attributeGroup ref="Common"/>
               </xs:complexType>
               <xs:group name="Later">
                 <xs:sequence><xs:element name="fromGroup" type="xs:string"/></xs:sequence>
               </xs:group>
               <xs:attributeGroup name="Common">
                 <xs:attribute name="id" type="xs:ID"/>
               </xs:attributeGroup>
               <xs:element name="t" type="T"/>"#,
        );
        let e = x.element("t").unwrap();
        assert_eq!(x.children_of(e)[0].name, "fromGroup");
        assert_eq!(x.attributes_of(e)[0].name, "id");
    }

    #[test]
    fn an_enumeration_is_found_through_whatever_wrappers_the_schema_used() {
        let x = schema(
            r#"<xs:simpleType name="Scope">
                 <xs:restriction base="xs:string">
                   <xs:enumeration value="compile"/>
                   <xs:enumeration value="test"/>
                 </xs:restriction>
               </xs:simpleType>
               <xs:complexType name="D">
                 <xs:attribute name="phase">
                   <xs:simpleType><xs:restriction base="xs:string">
                     <xs:enumeration value="clean"/>
                   </xs:restriction></xs:simpleType>
                 </xs:attribute>
               </xs:complexType>"#,
        );
        assert_eq!(x.simple_type("Scope").unwrap().values, ["compile", "test"]);
        assert_eq!(x.simple_type("xs:Scope").unwrap().values.len(), 2, "prefixes are dropped");
        assert_eq!(x.complex_type("D").unwrap().attributes[0].values, ["clean"]);
    }

    /// An element's own children describe ITS content, not its parent's. Walking into them is
    /// how a flattening pass turns a schema into one undifferentiated bag of names.
    #[test]
    fn a_nested_elements_children_do_not_leak_into_its_parent() {
        let x = schema(
            r#"<xs:element name="outer">
                 <xs:complexType><xs:sequence>
                   <xs:element name="middle">
                     <xs:complexType><xs:sequence>
                       <xs:element name="inner" type="xs:string"/>
                     </xs:sequence></xs:complexType>
                   </xs:element>
                 </xs:sequence></xs:complexType>
               </xs:element>"#,
        );
        let outer = x.element("outer").unwrap();
        let kids: Vec<&str> = x.children_of(outer).iter().map(|e| e.name.as_str()).collect();
        assert_eq!(kids, ["middle"]);
        let middle = &x.children_of(outer)[0];
        assert_eq!(x.children_of(middle)[0].name, "inner");
    }

    #[test]
    fn a_type_with_xs_any_is_open_and_nothing_in_it_may_be_reported() {
        let x = schema(
            r#"<xs:complexType name="Config">
                 <xs:sequence><xs:any processContents="skip"/></xs:sequence>
               </xs:complexType>
               <xs:element name="c" type="Config"/>
               <xs:element name="closed"><xs:complexType/></xs:element>"#,
        );
        assert!(x.is_open(x.element("c").unwrap()));
        assert!(!x.is_open(x.element("closed").unwrap()));
    }

    #[test]
    fn documentation_belongs_to_the_declaration_it_was_written_on() {
        let x = schema(
            r#"<xs:element name="dependency">
                 <xs:annotation><xs:documentation>
                   A library this project needs.
                 </xs:documentation></xs:annotation>
                 <xs:complexType><xs:sequence>
                   <xs:element name="groupId">
                     <xs:annotation><xs:documentation>The group.</xs:documentation></xs:annotation>
                   </xs:element>
                 </xs:sequence></xs:complexType>
               </xs:element>"#,
        );
        let e = x.element("dependency").unwrap();
        assert_eq!(e.doc, "A library this project needs.");
        assert_eq!(x.children_of(e)[0].doc, "The group.", "and not its parent's");
    }

    #[test]
    fn includes_are_recorded_rather_than_followed() {
        let x = schema(
            r#"<xs:include schemaLocation="common.xsd"/>
               <xs:import namespace="urn:x" schemaLocation="../x.xsd"/>"#,
        );
        assert_eq!(x.includes, ["common.xsd", "../x.xsd"]);
    }

    #[test]
    fn a_declaration_knows_where_it_is_in_the_file() {
        let src = format!("{HEAD}\n<xs:element name=\"project\" type=\"xs:string\"/>\n</xs:schema>");
        let x = parse(&src).unwrap();
        let e = x.element("project").unwrap();
        assert!(src[e.offset..].starts_with("<xs:element name=\"project\""));
        assert_eq!(e.line, 3);
    }
}
