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
//! different namespaces collapse into one entry.
//!
//! ## One name, several declarations
//!
//! That collapse is not only a cross-namespace accident — a single schema does it on purpose.
//! `plugin` in the Maven POM is `Plugin` under `<build>` and `ReportPlugin` under `<reporting>`,
//! two different types under one name, and neither is wrong. A flat model has to answer for both.
//!
//! It answers by **union**: [`Element::merge`] folds every declaration of a name into one entry
//! whose children and attributes are all of them. That is the under-reporting direction and it is
//! chosen for the usual reason — a completion list carrying a name that is legal one level away
//! costs a rejected suggestion, while a check that has forgotten half a declaration reports valid
//! markup as an error, which is what makes people turn the whole thing off. Where go-to jumps
//! still comes from the first declaration seen.
//!
//! Union is the wrong direction for exactly one thing, and [`Child::required`] is it. What a
//! document *may* contain is the union of every declaration; what it *must* contain is the
//! intersection, because a declaration that lets you leave a child out is proof that leaving it
//! out can be right. Merging therefore unions the names and intersects the demands.
//!
//! [`bennu-dtd`]: https://docs.rs/bennu-dtd
//! [`bennu-xsd`]: https://docs.rs/bennu-xsd

use std::collections::HashSet;

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
        e.children.iter().filter_map(|c| self.element(&c.name)).collect()
    }

    /// Merge another grammar in. Used to fold an `xs:include` chain into one answer.
    ///
    /// A name declared on both sides is merged rather than dropped ([`Element::merge`]): the
    /// document's own schema still owns the identity — its documentation, the place go-to jumps to
    /// — but what an included schema says may go inside an element is not thereby forgotten.
    pub fn absorb(&mut self, other: Grammar) {
        for e in other.elements {
            match self.elements.iter_mut().find(|x| x.name == e.name) {
                Some(mine) => mine.merge(e),
                None => self.elements.push(e),
            }
        }
        for r in other.roots {
            if !self.roots.contains(&r) {
                self.roots.push(r);
            }
        }
    }
}

/// One element name a parent may contain, and whether leaving it out is an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Child {
    /// Local name.
    pub name: String,
    /// A document that omits it is invalid — **and the grammar is sure of it**.
    ///
    /// False is the answer wherever a schema leaves room: a branch of an `xs:choice`, anything
    /// under a `minOccurs="0"` group, a DTD `?`/`*`, a substitution group head, a name one of two
    /// declarations lets you skip. Nothing built from a curated table sets it at all — see
    /// [`crate::builtin`], which knows the vocabulary and not the cardinality.
    pub required: bool,
}

/// One element the grammar declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Local name.
    pub name: String,
    /// The children it may contain, in declaration order.
    pub children: Vec<Child>,
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

    /// The children a document must write inside this element. Empty whenever the grammar cannot
    /// be certain, which for a curated or a permissive schema is always.
    pub fn required_children(&self) -> impl Iterator<Item = &Child> {
        self.children.iter().filter(|c| c.required)
    }

    /// Just the names, for the places that only ever wanted the vocabulary.
    pub fn child_names(&self) -> Vec<&str> {
        self.children.iter().map(|c| c.name.as_str()).collect()
    }

    pub fn child(&self, name: &str) -> Option<&Child> {
        let local = local_name(name);
        self.children.iter().find(|c| c.name == local)
    }

    /// Fold another declaration of the same name into this one.
    ///
    /// Union on everything that says what is *legal* — children, attributes, and both "anything
    /// goes" flags — because a name declared twice is two ways of being right, and a check built
    /// on half of them reports the other half as an error. `plugin` in the Maven POM is the case
    /// that proves it: `<build>` gives it `executions`, `<reporting>` gives it `reportSets`, and a
    /// grammar holding only one of the two flags a perfectly ordinary POM.
    ///
    /// First wins on everything that is *identity* — the declaration site, so go-to stays stable
    /// and points at the schema loaded first, which for an `xs:include` chain is the document's
    /// own. Documentation is the first **non-empty** one: only one of two declarations usually
    /// carries an `xs:annotation`, and letting the bare one win would lose the prose for no reason.
    pub fn merge(&mut self, other: Element) {
        // Demands are intersected before anything is added, so a name only one side declares
        // ends up optional whichever side declared it: the other declaration is a way of being
        // right that leaves the name out entirely.
        for mine in self.children.iter_mut() {
            mine.required &= other.children.iter().any(|c| c.name == mine.name && c.required);
        }
        for c in other.children {
            if !self.children.iter().any(|x| x.name == c.name) {
                self.children.push(Child { required: false, ..c });
            }
        }
        for a in other.attributes {
            if !self.attributes.iter().any(|x| x.name == a.name) {
                self.attributes.push(a);
            }
        }
        self.text |= other.text;
        self.open |= other.open;
        if self.doc.is_empty() {
            self.doc = other.doc;
        }
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
        .map(|e| {
            // What the content model *demands*, which the flat name list cannot carry: `?`, `*`
            // and every branch of a choice contribute a name without contributing an obligation.
            let demanded = e.content.required_child_names();
            Element {
                name: e.name.clone(),
                children: e
                    .content
                    .child_names()
                    .into_iter()
                    .map(|n| Child { required: demanded.contains(&n), name: n })
                    .collect(),
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
            }
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
/// Bounded by a visited set of **declarations**, not of names, which also terminates the recursive
/// schemas that are entirely normal in XML (an element that may contain itself).
///
/// Declarations rather than names because a schema reuses a name for genuinely different content —
/// `plugin` is `Plugin` under `<build>` and `ReportPlugin` under `<reporting>` — and stopping at
/// the first one reached would silently adopt whichever the walk happened to pop first and report
/// the other's children as illegal. Every declaration is visited; the ones that share a name are
/// merged ([`Element::merge`]). Identity is a pointer into the schema, so each `xs:element` node is
/// walked exactly once however many paths lead to it.
pub fn from_xsd(schema: &xsd::Xsd, source: &str) -> Grammar {
    let mut grammar = Grammar {
        source: source.to_string(),
        kind: Some(GrammarKind::Xsd),
        roots: schema.elements.iter().map(|e| e.name.clone()).collect(),
        elements: Vec::new(),
    };
    let mut queue: Vec<&xsd::XsdElement> = schema.elements.iter().collect();
    let mut seen: HashSet<usize> = HashSet::new();
    while let Some(e) = queue.pop() {
        if !seen.insert(e as *const xsd::XsdElement as usize) {
            continue;
        }
        let children = schema.children_of(e);
        let declared = Element {
            name: e.name.clone(),
            children: children
                .iter()
                .map(|c| Child { name: c.name.clone(), required: c.required })
                .collect(),
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
        };
        match grammar.elements.iter_mut().find(|x| x.name == declared.name) {
            Some(existing) => existing.merge(declared),
            None => grammar.elements.push(declared),
        }
        queue.extend(children);
    }
    grammar
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kid(name: &str, required: bool) -> Child {
        Child { name: name.to_string(), required }
    }

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
        assert_eq!(g.element("struts").unwrap().child_names(), ["package"]);
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
        assert_eq!(g.element("project").unwrap().child_names(), ["dependencies"]);
        assert_eq!(g.element("dependencies").unwrap().child_names(), ["dependency"]);
        // Reached through a NAMED type — the walk has to follow those or it stops one level in.
        let dep = g.element("dependency").unwrap();
        assert_eq!(dep.child_names(), ["groupId"]);
        assert_eq!(dep.attributes[0].values, ["compile", "test"]);
        assert!(dep.attributes[0].required);
    }

    /// Cardinality has to survive the trip through both adapters, or the check that reads it is
    /// reading a flag nobody set.
    #[test]
    fn what_a_document_must_contain_survives_both_adapters() {
        let d = bennu_dtd::prelude::parse(
            "<!ELEMENT servlet (servlet-name, (servlet-class | jsp-file), init-param*)>\n             <!ELEMENT servlet-name (#PCDATA)>",
        );
        let g = from_dtd(&d, "/p/web-app.dtd");
        let e = g.element("servlet").unwrap();
        assert_eq!(e.required_children().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["servlet-name"]);
        assert_eq!(e.child_names().len(), 4, "and everything legal is still offered");

        let x = bennu_xsd::prelude::parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:element name="bean"><xs:complexType><xs:sequence>
                   <xs:element name="class" type="xs:string"/>
                   <xs:element name="property" type="xs:string" minOccurs="0"/>
                 </xs:sequence></xs:complexType></xs:element>
               </xs:schema>"#,
        )
        .unwrap();
        let g = from_xsd(&x, "/p/beans.xsd");
        let e = g.element("bean").unwrap();
        assert_eq!(e.required_children().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["class"]);
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

    /// The Maven POM in miniature, and the bug it caused: `plugin` is declared twice with two
    /// different types, and a walk that stopped at the first name it reached adopted whichever it
    /// popped first — so `<executions>` inside a perfectly ordinary `<build><plugins><plugin>` was
    /// reported as not allowed there.
    #[test]
    fn a_name_declared_twice_with_two_types_keeps_both_sets_of_children() {
        let x = bennu_xsd::prelude::parse(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:element name="project"><xs:complexType><xs:sequence>
                   <xs:element name="build"><xs:complexType><xs:sequence>
                     <xs:element name="plugin" type="Plugin"/>
                   </xs:sequence></xs:complexType></xs:element>
                   <xs:element name="reporting"><xs:complexType><xs:sequence>
                     <xs:element name="plugin" type="ReportPlugin"/>
                   </xs:sequence></xs:complexType></xs:element>
                 </xs:sequence></xs:complexType></xs:element>
                 <xs:complexType name="Plugin"><xs:all>
                   <xs:element name="artifactId" type="xs:string"/>
                   <xs:element name="executions" minOccurs="0"><xs:complexType><xs:sequence>
                     <xs:element name="execution" type="xs:string" maxOccurs="unbounded"/>
                   </xs:sequence></xs:complexType></xs:element>
                 </xs:all></xs:complexType>
                 <xs:complexType name="ReportPlugin"><xs:all>
                   <xs:element name="artifactId" type="xs:string"/>
                   <xs:element name="reportSets" type="xs:string" minOccurs="0"/>
                 </xs:all></xs:complexType>
               </xs:schema>"#,
        )
        .unwrap();
        let g = from_xsd(&x, "/p/maven-4.0.0.xsd");
        let plugin = g.element("plugin").unwrap();
        for expected in ["artifactId", "executions", "reportSets"] {
            assert!(plugin.child_names().contains(&expected), "{expected} is missing");
        }
        // And the walk reached inside the type that only one of the two declarations named.
        assert_eq!(g.element("executions").unwrap().child_names(), ["execution"]);
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
        assert_eq!(g.element("node").unwrap().child_names(), ["node"]);
    }

    #[test]
    fn merging_unions_what_is_legal_and_keeps_the_first_identity() {
        let mut a = Grammar {
            elements: vec![
                Element {
                    name: "bean".into(),
                    doc: "mine".into(),
                    children: vec![kid("property", true)],
                    ..Element::default()
                },
                Element { name: "alias".into(), ..Element::default() },
            ],
            roots: vec!["beans".into()],
            ..Grammar::default()
        };
        a.absorb(Grammar {
            elements: vec![
                Element {
                    name: "bean".into(),
                    doc: "imported".into(),
                    children: vec![kid("property", true), kid("constructor-arg", true)],
                    open: true,
                    ..Element::default()
                },
                // A declaration the other side documents and this one does not: losing the prose
                // to a bare redeclaration would be a loss for nothing.
                Element { name: "alias".into(), doc: "an alias".into(), ..Element::default() },
                Element { name: "import".into(), ..Element::default() },
            ],
            roots: vec!["beans".into(), "other".into()],
            ..Grammar::default()
        });
        let bean = a.element("bean").unwrap();
        assert_eq!(bean.doc, "mine", "identity is the document's own schema");
        assert_eq!(bean.child_names(), ["property", "constructor-arg"], "but nothing legal is dropped");
        assert!(bean.child("property").unwrap().required, "both sides demanded it");
        assert!(
            !bean.child("constructor-arg").unwrap().required,
            "and one side has no opinion on it at all, which settles it"
        );
        assert!(bean.open, "and neither is a reason to stop checking");
        assert_eq!(a.element("alias").unwrap().doc, "an alias");
        assert!(a.element("import").is_some());
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
