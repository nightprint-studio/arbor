//! One model behind both DTD and XSD, and the two adapters.
//!
//! ## Why the model lives here and not in either parser
//!
//! Because "what does an editor need from a schema" is a different question from "what does this
//! file format say", and answering the first inside either parser would make that parser depend
//! on the other's shape. [`bennu-dtd`] stays a DTD parser; [`bennu-xsd`] stays a schema reader;
//! this is where they are asked the same four questions:
//!
//! 1. what may go inside this element;
//! 2. what attributes may it carry, and which are required;
//! 3. what values does this attribute accept, if the set is closed;
//! 4. where is it declared, so the editor can jump there.
//!
//! ## Local names
//!
//! Elements are keyed on their **local** name — `beans` and `context:component-scan` both reduce
//! to what follows the colon. That is a simplification, and a deliberate one: a Spring XML file
//! mixes four namespaces, only some of which resolve to a schema anyone has, and comparing
//! qualified names would report the rest as unknown. Under-reporting is the standing rule.
//!
//! The consequence is stated rather than hidden: two schemas that declare the same local name in
//! different namespaces merge into one entry, and the first one loaded wins.
//!
//! [`bennu-dtd`]: https://docs.rs/bennu-dtd
//! [`bennu-xsd`]: https://docs.rs/bennu-xsd

use bennu_dtd::prelude as dtd;
use bennu_xsd::prelude as xsd;

use crate::scan::local_name;

/// Where a declaration is, so an editor can jump to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decl {
    /// Display identity of the schema — an absolute path, or `<jar>!/<entry>`.
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

/// What kind of grammar a document turned out to be written against. Shown to the user, because
/// "this file is checked against `struts-2.5.dtd`" is the single most useful thing an editor can
/// say about an XML file it is being helpful in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarKind {
    Dtd,
    Xsd,
    /// Shipped with the editor — see [`crate::builtin`].
    Builtin,
}

impl GrammarKind {
    pub fn label(self) -> &'static str {
        match self {
            GrammarKind::Dtd => "DTD",
            GrammarKind::Xsd => "XSD",
            GrammarKind::Builtin => "built-in",
        }
    }
}

/// A schema, in the terms an editor asks about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grammar {
    /// Where it came from, for the status line and for go-to.
    pub source: String,
    pub kind: Option<GrammarKind>,
    pub elements: Vec<Element>,
    /// The names a document may have at its root. Empty when the schema does not say (a DTD
    /// says so in the `DOCTYPE`, not in the grammar), and then any known element is accepted.
    pub roots: Vec<String>,
}

impl Grammar {
    pub fn element(&self, name: &str) -> Option<&Element> {
        let local = local_name(name);
        self.elements.iter().find(|e| e.name == local)
    }

    /// Whether the grammar knows enough to answer at all. An empty one must behave exactly like
    /// no grammar: silent.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// The elements that may appear inside `parent`.
    ///
    /// An unknown parent yields nothing rather than everything — a completion list built from a
    /// parent the schema never declared would be a list of plausible-looking wrong answers.
    pub fn children_of(&self, parent: &str) -> Vec<&Element> {
        let Some(e) = self.element(parent) else { return Vec::new() };
        e.children.iter().filter_map(|n| self.element(n)).collect()
    }

    /// Merge another grammar in. Used to fold an `xs:include` chain into one answer; the first
    /// declaration of a name wins, so the document's own schema beats what it imports.
    pub fn absorb(&mut self, other: Grammar) {
        for e in other.elements {
            if !self.elements.iter().any(|x| x.name == e.name) {
                self.elements.push(e);
            }
        }
        for r in other.roots {
            if !self.roots.contains(&r) {
                self.roots.push(r);
            }
        }
    }
}

/// One element the grammar declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Local name.
    pub name: String,
    /// Local names of the children it may contain, in declaration order.
    pub children: Vec<String>,
    pub attributes: Vec<Attribute>,
    /// Character data is legal inside it.
    pub text: bool,
    /// **Anything** is legal inside it — `ANY`, `xs:any`, `mixed`. Every check under such an
    /// element is off; a schema that declines to say what goes here has not given anyone the
    /// right to complain about it.
    pub open: bool,
    pub doc: String,
    pub decl: Decl,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        let local = local_name(name);
        self.attributes.iter().find(|a| a.name == local)
    }
}

/// One attribute an element may carry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub required: bool,
    /// The closed set of legal values, empty when the set is open. The one thing that makes
    /// value completion a fact rather than a guess.
    pub values: Vec<String>,
    /// What is used when it is omitted.
    pub default: String,
    /// The only legal value, when the schema fixes one.
    pub fixed: String,
    pub doc: String,
    pub decl: Decl,
}

// ── Adapters ─────────────────────────────────────────────────────────────────

/// A DTD, as a grammar.
pub fn from_dtd(dtd: &dtd::Dtd, source: &str) -> Grammar {
    let elements = dtd
        .elements
        .iter()
        .map(|e| Element {
            name: e.name.clone(),
            children: e.content.child_names(),
            text: e.content.allows_text(),
            open: matches!(e.content, dtd::Content::Any),
            doc: e.doc.clone(),
            decl: Decl { file: source.to_string(), offset: e.offset, line: e.line },
            attributes: dtd
                .attributes_of(&e.name)
                .into_iter()
                .map(|a| Attribute {
                    name: a.name.clone(),
                    required: a.required(),
                    values: a.values().to_vec(),
                    default: a.default.value().to_string(),
                    fixed: match &a.default {
                        dtd::DefaultDecl::Fixed(v) => v.clone(),
                        _ => String::new(),
                    },
                    doc: String::new(),
                    decl: Decl { file: source.to_string(), offset: a.offset, line: a.line },
                })
                .collect(),
        })
        .collect();
    // A DTD names no root: the document's own `DOCTYPE` does. Left empty, which the checks read
    // as "any declared element may be the root".
    Grammar { source: source.to_string(), kind: Some(GrammarKind::Dtd), elements, roots: Vec::new() }
}

/// An XSD, as a grammar.
///
/// The walk is the interesting part. A schema declares *global* elements and reaches the rest
/// through types, so the grammar is built by walking down from every global element and
/// collecting each local declaration on the way — folding in the extension chain at each step,
/// which [`bennu_xsd`] already does.
///
/// Bounded by a visited set on the local name, which also terminates the recursive schemas that
/// are entirely normal in XML (an element that may contain itself).
pub fn from_xsd(schema: &xsd::Xsd, source: &str) -> Grammar {
    let mut grammar = Grammar {
        source: source.to_string(),
        kind: Some(GrammarKind::Xsd),
        roots: schema.elements.iter().map(|e| e.name.clone()).collect(),
        elements: Vec::new(),
    };
    let mut queue: Vec<&xsd::XsdElement> = schema.elements.iter().collect();
    while let Some(e) = queue.pop() {
        if grammar.elements.iter().any(|x| x.name == e.name) {
            continue;
        }
        let children = schema.children_of(e);
        grammar.elements.push(Element {
            name: e.name.clone(),
            children: children.iter().map(|c| c.name.clone()).collect(),
            attributes: schema
                .attributes_of(e)
                .into_iter()
                .map(|a| Attribute {
                    name: a.name.clone(),
                    required: a.required,
                    // An inline enumeration is already resolved; a named simple type is looked
                    // up here, which is the only place that knows the whole schema.
                    values: if a.values.is_empty() {
                        schema
                            .simple_type(&a.type_name)
                            .map(|t| t.values.clone())
                            .unwrap_or_default()
                    } else {
                        a.values.clone()
                    },
                    default: a.default.clone(),
                    fixed: a.fixed.clone(),
                    doc: a.doc.clone(),
                    decl: Decl { file: source.to_string(), offset: a.offset, line: a.line },
                })
                .collect(),
            // An element with no complex type at all holds text; a `mixed` one holds both. Both
            // are reasons not to complain about what is written inside.
            text: schema.type_of(e).map(|t| t.mixed).unwrap_or(true),
            open: schema.is_open(e),
            doc: e.doc.clone(),
            decl: Decl { file: source.to_string(), offset: e.offset, line: e.line },
        });
        queue.extend(children);
    }
    grammar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dtd_becomes_a_grammar_with_its_attributes_attached() {
        let d = bennu_dtd::prelude::parse(
            "<!-- The root. -->\n\
             <!ELEMENT struts (package*)>\n\
             <!ELEMENT package (action*)>\n\
             <!ATTLIST package name CDATA #REQUIRED extends CDATA #IMPLIED>\n\
             <!ELEMENT action ANY>",
        );
        let g = from_dtd(&d, "/p/struts-2.5.dtd");
        assert_eq!(g.kind, Some(GrammarKind::Dtd));
        assert_eq!(g.element("struts").unwrap().children, ["package"]);
        assert_eq!(g.element("struts").unwrap().doc, "The root.");

        let p = g.element("package").unwrap();
        assert_eq!(p.attributes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["name", "extends"]);
        assert!(p.attributes[0].required);
        assert!(!p.open);
        assert!(g.element("action").unwrap().open, "ANY silences every check under it");
        assert_eq!(g.children_of("struts")[0].name, "package");
        assert_eq!(g.element("struts").unwrap().decl.file, "/p/struts-2.5.dtd");
    }

    #[test]
    fn an_xsd_is_walked_down_from_its_global_elements() {
        let x = bennu_xsd::prelude::parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:element name="project">
                   <xs:complexType><xs:sequence>
                     <xs:element name="dependencies">
                       <xs:complexType><xs:sequence>
                         <xs:element name="dependency" type="Dep" maxOccurs="unbounded"/>
                       </xs:sequence></xs:complexType>
                     </xs:element>
                   </xs:sequence></xs:complexType>
                 </xs:element>
                 <xs:complexType name="Dep">
                   <xs:sequence><xs:element name="groupId" type="xs:string"/></xs:sequence>
                   <xs:attribute name="scope" use="required">
                     <xs:simpleType><xs:restriction base="xs:string">
                       <xs:enumeration value="compile"/><xs:enumeration value="test"/>
                     </xs:restriction></xs:simpleType>
                   </xs:attribute>
                 </xs:complexType>
               </xs:schema>"#,
        )
        .unwrap();
        let g = from_xsd(&x, "/p/maven-4.0.0.xsd");
        assert_eq!(g.roots, ["project"]);
        assert_eq!(g.element("project").unwrap().children, ["dependencies"]);
        assert_eq!(g.element("dependencies").unwrap().children, ["dependency"]);
        // Reached through a NAMED type — the walk has to follow those or it stops one level in.
        let dep = g.element("dependency").unwrap();
        assert_eq!(dep.children, ["groupId"]);
        assert_eq!(dep.attributes[0].values, ["compile", "test"]);
        assert!(dep.attributes[0].required);
    }

    #[test]
    fn a_named_simple_type_is_resolved_into_the_attributes_value_set() {
        let x = bennu_xsd::prelude::parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:simpleType name="Scope"><xs:restriction base="xs:string">
                   <xs:enumeration value="compile"/>
                 </xs:restriction></xs:simpleType>
                 <xs:element name="dep"><xs:complexType>
                   <xs:attribute name="scope" type="Scope"/>
                 </xs:complexType></xs:element>
               </xs:schema>"#,
        )
        .unwrap();
        let g = from_xsd(&x, "/p/x.xsd");
        assert_eq!(g.element("dep").unwrap().attributes[0].values, ["compile"]);
    }

    /// An element that may contain itself is ordinary in XML and would otherwise not terminate.
    #[test]
    fn a_recursive_schema_terminates() {
        let x = bennu_xsd::prelude::parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:complexType name="Node">
                   <xs:sequence><xs:element name="node" type="Node"/></xs:sequence>
                 </xs:complexType>
                 <xs:element name="node" type="Node"/>
               </xs:schema>"#,
        )
        .unwrap();
        let g = from_xsd(&x, "/p/x.xsd");
        assert_eq!(g.element("node").unwrap().children, ["node"]);
    }

    #[test]
    fn merging_keeps_the_first_declaration_of_a_name() {
        let mut a = Grammar {
            elements: vec![Element { name: "bean".into(), doc: "mine".into(), ..Element::default() }],
            roots: vec!["beans".into()],
            ..Grammar::default()
        };
        a.absorb(Grammar {
            elements: vec![
                Element { name: "bean".into(), doc: "imported".into(), ..Element::default() },
                Element { name: "alias".into(), ..Element::default() },
            ],
            roots: vec!["beans".into(), "other".into()],
            ..Grammar::default()
        });
        assert_eq!(a.element("bean").unwrap().doc, "mine");
        assert!(a.element("alias").is_some());
        assert_eq!(a.roots, ["beans", "other"]);
    }

    /// Under-reporting is the rule: an element the schema never declared yields nothing, not
    /// every name in the grammar.
    #[test]
    fn an_unknown_parent_offers_nothing_rather_than_everything() {
        let g = Grammar {
            elements: vec![Element { name: "a".into(), ..Element::default() }],
            ..Grammar::default()
        };
        assert!(g.children_of("nope").is_empty());
        assert!(Grammar::default().is_empty());
    }

    #[test]
    fn a_prefixed_name_resolves_to_its_local_declaration() {
        let g = Grammar {
            elements: vec![Element {
                name: "component-scan".into(),
                attributes: vec![Attribute { name: "base-package".into(), ..Attribute::default() }],
                ..Element::default()
            }],
            ..Grammar::default()
        };
        let e = g.element("context:component-scan").unwrap();
        assert!(e.attribute("xsi:base-package").is_some());
    }
}
