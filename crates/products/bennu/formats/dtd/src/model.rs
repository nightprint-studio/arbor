//! What a DTD says, as data.

/// A parsed DTD.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dtd {
    pub elements: Vec<ElementDecl>,
    /// One entry per `<!ATTLIST>`. **Not merged by element name** — a DTD may declare several
    /// lists for the same element and that is legal, so merging is the consumer's decision
    /// (see [`Dtd::attributes_of`]).
    pub attlists: Vec<AttListDecl>,
    pub entities: Vec<EntityDecl>,
}

impl Dtd {
    pub fn element(&self, name: &str) -> Option<&ElementDecl> {
        self.elements.iter().find(|e| e.name == name)
    }

    /// Every attribute declared for `element`, across all of its `<!ATTLIST>`s.
    ///
    /// First declaration wins on a repeat — the XML spec's own rule, and the one that makes a
    /// DTD with a customisation layer on top behave the way its author meant.
    pub fn attributes_of(&self, element: &str) -> Vec<&AttrDecl> {
        let mut out: Vec<&AttrDecl> = Vec::new();
        for list in self.attlists.iter().filter(|l| l.element == element) {
            for a in &list.attrs {
                if !out.iter().any(|x| x.name == a.name) {
                    out.push(a);
                }
            }
        }
        out
    }
}

/// `<!ELEMENT name content>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementDecl {
    pub name: String,
    pub content: Content,
    /// Byte offset of the `<!ELEMENT`.
    pub offset: usize,
    pub line: u32,
    /// The comment immediately above the declaration, when there is one — a DTD's only place to
    /// put documentation, and the reason hovering a tag can say anything at all.
    pub doc: String,
}

/// What may appear inside an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Empty,
    /// `ANY` — anything at all, so a consumer must never report an unexpected child here.
    Any,
    /// `(#PCDATA)` — text only.
    PcData,
    /// `(#PCDATA | a | b)*` — text interleaved with any of these, in any order.
    Mixed(Vec<String>),
    /// A structured model.
    Children(Particle),
}

impl Content {
    /// Every element name that may appear as a child, in declaration order.
    pub fn child_names(&self) -> Vec<String> {
        match self {
            Content::Empty | Content::Any | Content::PcData => Vec::new(),
            Content::Mixed(names) => names.clone(),
            Content::Children(p) => {
                let mut out = Vec::new();
                p.collect_names(&mut out);
                out
            }
        }
    }

    /// Every child name a valid document **must** contain, in declaration order.
    ///
    /// Only [`Content::Children`] can demand anything: `(#PCDATA | a | b)*` demands nothing by
    /// construction, and `ANY` and `EMPTY` say the opposite of a demand.
    pub fn required_child_names(&self) -> Vec<String> {
        match self {
            Content::Children(p) => {
                let mut out = Vec::new();
                p.collect_required(&mut out);
                out
            }
            _ => Vec::new(),
        }
    }

    /// Whether character data is legal here.
    pub fn allows_text(&self) -> bool {
        matches!(self, Content::PcData | Content::Mixed(_) | Content::Any)
    }
}

/// One node of a content model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Particle {
    Name(String),
    /// `(a, b)` — in this order.
    Seq(Vec<Particle>),
    /// `(a | b)` — one of these.
    Choice(Vec<Particle>),
    /// A particle with an occurrence indicator.
    Repeat(Box<Particle>, Occurs),
}

impl Particle {
    fn collect_names(&self, out: &mut Vec<String>) {
        match self {
            Particle::Name(n) => {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Particle::Seq(ps) | Particle::Choice(ps) => ps.iter().for_each(|p| p.collect_names(out)),
            Particle::Repeat(p, _) => p.collect_names(out),
        }
    }

    /// The names this particle demands of every document that satisfies it.
    ///
    /// A sequence demands whatever each of its members does. A **choice** demands only what
    /// *every* branch demands — a document satisfies it by taking one branch, so a name written
    /// in three of four is not required by any of them. `?` and `*` demand nothing; `+` and a
    /// bare particle demand what they wrap.
    fn collect_required(&self, out: &mut Vec<String>) {
        match self {
            Particle::Name(n) => {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Particle::Seq(ps) => ps.iter().for_each(|p| p.collect_required(out)),
            Particle::Choice(ps) => {
                let Some((first, rest)) = ps.split_first() else { return };
                let mut common = Vec::new();
                first.collect_required(&mut common);
                for p in rest {
                    let mut theirs = Vec::new();
                    p.collect_required(&mut theirs);
                    common.retain(|n| theirs.contains(n));
                }
                for n in common {
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            Particle::Repeat(p, o) => match o {
                Occurs::Opt | Occurs::Star => {}
                Occurs::One | Occurs::Plus => p.collect_required(out),
            },
        }
    }

    /// Whether this particle can be satisfied by writing nothing.
    pub fn optional(&self) -> bool {
        match self {
            Particle::Name(_) => false,
            Particle::Seq(ps) => ps.iter().all(Particle::optional),
            Particle::Choice(ps) => ps.iter().any(Particle::optional),
            Particle::Repeat(p, o) => {
                matches!(o, Occurs::Opt | Occurs::Star) || p.optional()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurs {
    /// Exactly one — no indicator.
    One,
    /// `?`
    Opt,
    /// `*`
    Star,
    /// `+`
    Plus,
}

/// `<!ATTLIST element …>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttListDecl {
    pub element: String,
    pub attrs: Vec<AttrDecl>,
    pub offset: usize,
    pub line: u32,
}

/// One attribute inside an `<!ATTLIST>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrDecl {
    pub name: String,
    pub kind: AttrKind,
    pub default: DefaultDecl,
    /// Byte offset of the attribute's name within the DTD.
    pub offset: usize,
    pub line: u32,
}

impl AttrDecl {
    /// The closed set of values this attribute accepts, empty when it is open.
    ///
    /// The one thing a DTD says that makes value completion honest rather than a guess.
    pub fn values(&self) -> &[String] {
        match &self.kind {
            AttrKind::Enumeration(v) | AttrKind::Notation(v) => v,
            _ => &[],
        }
    }

    pub fn required(&self) -> bool {
        matches!(self.default, DefaultDecl::Required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrKind {
    CData,
    Id,
    IdRef,
    IdRefs,
    Entity,
    Entities,
    NmToken,
    NmTokens,
    /// `NOTATION (a | b)`
    Notation(Vec<String>),
    /// `(a | b | c)`
    Enumeration(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultDecl {
    Required,
    Implied,
    /// `#FIXED "v"` — the only legal value.
    Fixed(String),
    /// A default value.
    Value(String),
}

impl DefaultDecl {
    /// The value written when the attribute is omitted, if any.
    pub fn value(&self) -> &str {
        match self {
            DefaultDecl::Fixed(v) | DefaultDecl::Value(v) => v,
            _ => "",
        }
    }
}

/// `<!ENTITY name "…">` or `<!ENTITY % name "…">`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityDecl {
    pub name: String,
    /// A parameter entity (`%name;`), used inside the DTD itself rather than in documents.
    pub parameter: bool,
    pub value: String,
    pub offset: usize,
}
