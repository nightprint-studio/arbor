//! What a schema says, as data.
//!
//! Names are stored **unqualified** — `element`, not `xs:element` and not
//! `{http://maven.apache.org/POM/4.0.0}project`. The target namespace is recorded once on the
//! [`Xsd`], and matching a document's prefixes back to it is the consumer's job: a document
//! mixing four namespaces is the normal case in Spring XML, and the prefix bindings live in the
//! document rather than here.

/// A parsed schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Xsd {
    /// `targetNamespace`, empty for a no-namespace schema.
    pub target_namespace: String,
    /// Whether `elementFormDefault="qualified"` — the document's own elements carry the
    /// namespace. Almost always true in practice, and the consumer needs it to decide whether an
    /// unprefixed element in the document is in the namespace or not.
    pub qualified: bool,
    /// Top-level `xs:element` declarations — the legal document roots.
    pub elements: Vec<XsdElement>,
    /// Named `xs:complexType`s.
    pub complex_types: Vec<ComplexType>,
    /// Named `xs:simpleType`s — carried for their enumerations, which are what makes attribute
    /// value completion honest.
    pub simple_types: Vec<SimpleType>,
    /// Named `xs:attributeGroup`s.
    pub attribute_groups: Vec<Group<XsdAttribute>>,
    /// Named `xs:group`s (model groups).
    pub groups: Vec<Group<XsdElement>>,
    /// `schemaLocation`s of `xs:include` / `xs:import` / `xs:redefine`, in document order.
    /// **Recorded, not followed** — see the crate docs.
    pub includes: Vec<String>,
}

impl Xsd {
    pub fn element(&self, name: &str) -> Option<&XsdElement> {
        self.elements.iter().find(|e| e.name == name)
    }

    pub fn complex_type(&self, name: &str) -> Option<&ComplexType> {
        self.complex_types.iter().find(|t| t.name == local(name))
    }

    pub fn simple_type(&self, name: &str) -> Option<&SimpleType> {
        self.simple_types.iter().find(|t| t.name == local(name))
    }

    /// The complex type an element's content follows: its inline one, or the named one it
    /// references.
    pub fn type_of<'a>(&'a self, element: &'a XsdElement) -> Option<&'a ComplexType> {
        element
            .inline_type
            .as_deref()
            .or_else(|| self.complex_type(&element.type_name))
    }

    /// Every child element `element` may contain, with the extension chain of its type folded in.
    ///
    /// The chain is why this is a method rather than a field: `xs:extension base="…"` is how
    /// every non-trivial schema is written, and a consumer that only read the derived type's own
    /// particles would miss most of what a document legally contains.
    pub fn children_of<'a>(&'a self, element: &'a XsdElement) -> Vec<&'a XsdElement> {
        let mut out: Vec<&XsdElement> = Vec::new();
        self.walk_type(self.type_of(element), &mut |t| {
            for e in &t.elements {
                if !out.iter().any(|x| x.name == e.name) {
                    out.push(e);
                }
            }
        });
        out
    }

    /// Every attribute `element` may carry, extension chain folded in.
    pub fn attributes_of<'a>(&'a self, element: &'a XsdElement) -> Vec<&'a XsdAttribute> {
        let mut out: Vec<&XsdAttribute> = Vec::new();
        self.walk_type(self.type_of(element), &mut |t| {
            for a in &t.attributes {
                if !out.iter().any(|x| x.name == a.name) {
                    out.push(a);
                }
            }
        });
        out
    }

    /// Whether `element`'s type (or anything it derives from) permits arbitrary content —
    /// `xs:any`, or `mixed`. A consumer must never report an unexpected child under one.
    pub fn is_open<'a>(&'a self, element: &'a XsdElement) -> bool {
        let mut open = false;
        self.walk_type(self.type_of(element), &mut |t| open |= t.any || t.mixed);
        open
    }

    /// Run `f` over a type and everything it derives from. Bounded, so a schema with a cycle in
    /// its extension chain terminates instead of hanging the editor that asked.
    fn walk_type<'a>(&'a self, start: Option<&'a ComplexType>, f: &mut impl FnMut(&'a ComplexType)) {
        let mut current = start;
        for _ in 0..16 {
            let Some(t) = current else { return };
            f(t);
            if t.base.is_empty() {
                return;
            }
            let next = self.complex_type(&t.base);
            // A type that extends itself, directly or through a loop of names we have already
            // visited, is malformed — stop rather than trust the bound alone.
            if next.map(|n| std::ptr::eq(n, t)).unwrap_or(true) {
                return;
            }
            current = next;
        }
    }
}

/// An `xs:element`, global or local.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XsdElement {
    pub name: String,
    /// `type="…"` as written (possibly prefixed), empty when the type is inline or absent.
    pub type_name: String,
    /// An `xs:complexType` written inside the element rather than referenced.
    pub inline_type: Option<Box<ComplexType>>,
    /// Enumerated values, for an element whose type is a simple type with an enumeration.
    /// Resolved at parse time when the type is inline; a named one is looked up by the consumer.
    pub values: Vec<String>,
    /// `minOccurs="0"` → optional. Kept per element rather than per group: see the crate docs on
    /// why particles are flattened.
    pub required: bool,
    /// `maxOccurs` other than `1`.
    pub repeats: bool,
    /// `xs:documentation`, joined into prose.
    pub doc: String,
    /// Byte offset of the declaration in the schema file.
    pub offset: usize,
    pub line: u32,
}

/// An `xs:complexType`, named or inline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComplexType {
    /// Empty for an inline type.
    pub name: String,
    /// The child elements its particles permit, flattened across `sequence` / `choice` / `all`.
    pub elements: Vec<XsdElement>,
    pub attributes: Vec<XsdAttribute>,
    /// `xs:extension`/`xs:restriction` `base`, as written. Empty when it derives from nothing.
    pub base: String,
    /// `mixed="true"` — character data is legal between the children.
    pub mixed: bool,
    /// The type contains an `xs:any`, so anything at all may appear.
    pub any: bool,
    pub doc: String,
    pub offset: usize,
    pub line: u32,
}

/// An `xs:attribute`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XsdAttribute {
    pub name: String,
    /// `type="…"` as written, empty for an inline simple type.
    pub type_name: String,
    /// The closed set of values, from an inline `xs:restriction`. Empty when the type is open or
    /// named — the consumer resolves a named one through [`Xsd::simple_type`].
    pub values: Vec<String>,
    pub required: bool,
    pub default: String,
    /// `fixed="…"` — the only legal value.
    pub fixed: String,
    pub doc: String,
    pub offset: usize,
    pub line: u32,
}

/// An `xs:simpleType` — carried for its enumeration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimpleType {
    pub name: String,
    pub base: String,
    pub values: Vec<String>,
    pub doc: String,
    pub offset: usize,
    pub line: u32,
}

/// A named `xs:group` / `xs:attributeGroup`. Generic because the two differ only in what they
/// hold, and a second near-identical struct would be two places to fix one bug in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group<T> {
    pub name: String,
    pub members: Vec<T>,
    pub offset: usize,
}

/// `xs:string` → `string`. Prefixes are dropped everywhere: which prefix a schema binds to the
/// XSD namespace is its own business, and comparing local names is what makes two schemas
/// written by different people agree.
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}
