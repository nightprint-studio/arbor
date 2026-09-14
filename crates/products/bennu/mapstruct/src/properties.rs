//! A type's properties as MapStruct sees them — read, written, and declared where.
//!
//! MapStruct reads a property through a getter (`getX()`, `isX()`), a public field or a record
//! component, and writes one through a setter, a public field, a constructor parameter or a record
//! component. Lombok adds accessors that are not in the source at all. So the property list is built
//! from all of those, and each property says **how sure** it is of each direction ([`Access`]).
//!
//! The certainty matters because the two checks built on it fail in opposite directions:
//!
//! - "`target = x` names nothing" is an error when wrong, so it asks whether the name exists **at
//!   all** — a private field with no setter still silences it.
//! - "`x` is never mapped" is a warning when wrong, so it counts only properties that are
//!   **certainly** writable. A property Lombok's constructor may or may not take is [`Access::Maybe`]
//!   and never reported.
//!
//! Where the rules cannot be followed from the source — `@Accessors`, a fluent setter, a hand-written
//! builder, several constructors — the type is marked `opaque`, and nothing is said about it.

use bennu_facts::prelude::{AnnFacts, FieldFacts, JavaFacts, MethodFacts, TypeFacts};
use bennu_lombok::prelude::{annotation_is_lombok, ImportPath};

use crate::table::TypeInfo;

/// How certain a direction of access is. Ordered, so merging two sightings keeps the stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    No,
    Maybe,
    Yes,
}

/// What declares a property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Field,
    Getter,
    Setter,
    Constructor,
    RecordComponent,
    Lombok,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Field => "field",
            Origin::Getter => "getter",
            Origin::Setter => "setter",
            Origin::Constructor => "constructor parameter",
            Origin::RecordComponent => "record component",
            Origin::Lombok => "Lombok",
        }
    }
}

/// One place a property is declared. `owner` rather than the property's own owner because a merged
/// view carries the subclass's getter and the superclass's field side by side, in different files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub origin: Origin,
    pub owner: String,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    /// The type as written at the first declaration seen (a field before its getter).
    pub type_text: String,
    /// The fqcn of the type that declares it — the scope `type_text` resolves in.
    pub owner: String,
    pub readable: Access,
    pub writable: Access,
    pub sites: Vec<Site>,
}

impl Property {
    /// The site go-to lands on: the field or component when there is one, else the first accessor.
    pub fn primary_site(&self) -> Option<&Site> {
        self.sites
            .iter()
            .find(|s| matches!(s.origin, Origin::Field | Origin::RecordComponent))
            .or_else(|| self.sites.iter().find(|s| s.origin != Origin::Lombok))
            .or_else(|| self.sites.first())
    }

    /// The distinct origins, in the order they were seen — for a hover.
    pub fn origins(&self) -> Vec<Origin> {
        let mut out: Vec<Origin> = Vec::new();
        for s in &self.sites {
            if !out.contains(&s.origin) {
                out.push(s.origin);
            }
        }
        out
    }
}

/// Everything MapStruct can see of one declared type.
pub fn type_info(t: &TypeFacts, facts: &JavaFacts) -> TypeInfo {
    let mut props = Props { list: Vec::new(), owner: t.fqcn.clone() };
    // An interface, enum or annotation is not a bean MapStruct constructs, and its accessors follow
    // rules this crate does not model — opaque from the start.
    let mut opaque = !matches!(t.kind, "class" | "record");

    let class = read_lombok(&t.annotations, facts);
    opaque |= class.opaque;

    for f in t.fields.iter().filter(|f| !f.is_static) {
        opaque |= field(&mut props, f, t, &class, facts);
    }
    for m in t.methods.iter().filter(|m| !m.is_constructor) {
        opaque |= method(&mut props, m, t);
    }
    opaque |= constructors(&mut props, t, &class);

    TypeInfo {
        fqcn: t.fqcn.clone(),
        name: t.name.clone(),
        file: facts.file.clone(),
        package: facts.package.clone(),
        imports: facts.imports.clone(),
        name_offset: t.name_offset,
        extends: t.extends.clone(),
        implements: t.implements.clone(),
        props: props.list,
        opaque,
    }
}

struct Props {
    list: Vec<Property>,
    owner: String,
}

impl Props {
    fn touch(&mut self, name: &str, type_text: &str) -> &mut Property {
        let index = match self.list.iter().position(|p| p.name == name) {
            Some(i) => i,
            None => {
                self.list.push(Property {
                    name: name.to_string(),
                    type_text: String::new(),
                    owner: self.owner.clone(),
                    readable: Access::No,
                    writable: Access::No,
                    sites: Vec::new(),
                });
                self.list.len() - 1
            }
        };
        let p = &mut self.list[index];
        if p.type_text.is_empty() {
            p.type_text = type_text.trim().to_string();
        }
        p
    }

    fn site(&mut self, name: &str, type_text: &str, origin: Origin, offset: usize) -> &mut Property {
        let owner = self.owner.clone();
        let p = self.touch(name, type_text);
        p.sites.push(Site { origin, owner, offset });
        p
    }
}

/// What the Lombok annotations on one declaration generate.
#[derive(Default)]
struct Lombok {
    getters: bool,
    setters: bool,
    /// A constructor or builder that takes (some of) the fields.
    generated_ctor: bool,
    /// An explicit constructor annotation, which next to a hand-written constructor makes the choice
    /// MapStruct has to make between them unreadable from here.
    explicit_ctor: bool,
    /// `@Value`: every field final, no setters.
    immutable: bool,
    opaque: bool,
}

fn read_lombok(anns: &[AnnFacts], facts: &JavaFacts) -> Lombok {
    let mut l = Lombok::default();
    for a in anns.iter().filter(|a| is_lombok(a, facts)) {
        match a.name.as_str() {
            "Data" => {
                l.getters = true;
                l.setters = true;
                l.generated_ctor = true;
            }
            "Value" => {
                l.getters = true;
                l.generated_ctor = true;
                l.immutable = true;
            }
            "Getter" => {
                if plain_access(a) {
                    l.getters = true;
                } else {
                    l.opaque = true;
                }
            }
            "Setter" => {
                if plain_access(a) {
                    l.setters = true;
                } else {
                    l.opaque = true;
                }
            }
            "AllArgsConstructor" | "RequiredArgsConstructor" | "NoArgsConstructor" => {
                l.generated_ctor = true;
                l.explicit_ctor = true;
            }
            "Builder" => {
                // A builder MapStruct uses takes the fields; one with a custom setter prefix or
                // method name takes names this crate would have to guess.
                l.generated_ctor = true;
                l.explicit_ctor = true;
                l.opaque |= !(a.args.is_empty() && a.strings.is_empty() && a.positional.is_empty());
            }
            // Renamed, fluent or inherited accessors: the property names are no longer the fields'.
            "Accessors" | "SuperBuilder" | "With" | "Wither" | "Delegate" => l.opaque = true,
            _ => {}
        }
    }
    l
}

/// `@Getter` / `@Setter` with no access level, or an explicitly public one. Any other level makes an
/// accessor MapStruct cannot call — or, for `NONE`, none at all.
fn plain_access(a: &AnnFacts) -> bool {
    a.args.is_empty() && a.strings.is_empty() && a.positional.iter().all(|p| p.ends_with("PUBLIC"))
}

fn is_lombok(ann: &AnnFacts, facts: &JavaFacts) -> bool {
    if let Some((pkg, _)) = ann.qualified.rsplit_once('.') {
        return pkg == "lombok" || pkg.starts_with("lombok.");
    }
    annotation_is_lombok(&ann.name, facts.imports.iter().map(|i| import_path(i)))
}

fn import_path(import: &str) -> ImportPath<'_> {
    match import.strip_suffix(".*") {
        Some(pkg) => ImportPath::new(pkg, true),
        None => ImportPath::new(import, false),
    }
}

/// One instance field. Returns whether it makes the type opaque.
fn field(props: &mut Props, f: &FieldFacts, t: &TypeFacts, class: &Lombok, facts: &JavaFacts) -> bool {
    if t.kind == "record" {
        // A component: read through its accessor, written through the canonical constructor —
        // whether that constructor is the one MapStruct picks is settled in `constructors`.
        let p = props.site(&f.name, &f.type_text, Origin::RecordComponent, f.name_offset);
        p.readable = Access::Yes;
        return false;
    }
    let own = read_lombok(&f.annotations, facts);
    let getter = class.getters || own.getters;
    let setter = (class.setters || own.setters) && !f.is_final && !class.immutable;
    // Lombok names the getter of `boolean isActive` `isActive()`, whose property is `active` — a name
    // no field carries. Rare, and not worth modelling against a check that must not be wrong.
    let renamed = getter && is_boolean(&f.type_text) && has_prefix(&f.name, "is");

    let p = props.site(&f.name, &f.type_text, Origin::Field, f.name_offset);
    if f.is_public {
        p.readable = p.readable.max(Access::Yes);
        if !f.is_final {
            p.writable = p.writable.max(Access::Yes);
        }
    }
    if getter || setter {
        p.sites.push(Site { origin: Origin::Lombok, owner: t.fqcn.clone(), offset: f.name_offset });
    }
    if getter {
        p.readable = p.readable.max(Access::Yes);
    }
    if setter {
        p.writable = p.writable.max(Access::Yes);
    }
    if class.generated_ctor || own.generated_ctor {
        p.writable = p.writable.max(Access::Maybe);
    }
    own.opaque || renamed
}

/// One method that is not a constructor. Returns whether it makes the type opaque.
fn method(props: &mut Props, m: &MethodFacts, t: &TypeFacts) -> bool {
    if m.is_static {
        // A static factory for a builder is what MapStruct uses instead of the setters — and the
        // builder's methods are somewhere this scan did not look.
        let builder = m.params.is_empty()
            && (m.name.to_ascii_lowercase().contains("builder") || m.return_type.contains("Builder"));
        return builder;
    }
    let certainty = if m.is_public { Access::Yes } else { Access::Maybe };
    if let Some(name) = getter_property(m) {
        let p = props.site(&name, &m.return_type, Origin::Getter, m.name_offset);
        p.readable = p.readable.max(certainty);
        return false;
    }
    if let Some(name) = setter_property(m) {
        let p = props.site(&name, &m.params[0].type_text, Origin::Setter, m.name_offset);
        p.writable = p.writable.max(certainty);
        return false;
    }
    // A fluent setter — `Dto name(String n)` returning its own type. MapStruct accepts those, under
    // naming rules that are its own business.
    let returns_self = m.return_type.trim() == t.name || m.return_type.starts_with(&format!("{}<", t.name));
    m.params.len() == 1 && returns_self
}

/// Constructors, and what they make writable. Returns whether they make the type opaque.
fn constructors(props: &mut Props, t: &TypeFacts, class: &Lombok) -> bool {
    let ctors: Vec<&MethodFacts> = t.methods.iter().filter(|m| m.is_constructor).collect();
    if t.kind == "record" {
        let arity = t.fields.iter().filter(|f| !f.is_static).count();
        // A compact constructor reads as one with no parameters; only a constructor of a DIFFERENT
        // arity is a second choice MapStruct would have to make.
        let extra = ctors.iter().any(|c| !c.params.is_empty() && c.params.len() != arity);
        let components = props
            .list
            .iter_mut()
            .filter(|p| p.sites.iter().any(|s| s.origin == Origin::RecordComponent));
        for p in components {
            p.writable = if extra { Access::Maybe } else { Access::Yes };
        }
        return false;
    }
    if t.kind != "class" || ctors.is_empty() {
        return false;
    }
    let renamed = ctors.iter().any(|c| {
        c.annotations.iter().any(|a| a.name == "ConstructorProperties" || a.name == "Default")
    });
    if renamed || class.explicit_ctor {
        return true;
    }
    let public: Vec<&&MethodFacts> = ctors.iter().filter(|c| c.is_public).collect();
    if public.iter().any(|c| c.params.is_empty()) {
        return false;
    }
    let [only] = public.as_slice() else {
        // Several public constructors and no no-arg one, or none public at all: which one MapStruct
        // picks is a question for `@Default`, and nothing here guesses it.
        return true;
    };
    for param in &only.params {
        let p = props.site(&param.name, &param.type_text, Origin::Constructor, param.name_offset);
        p.writable = Access::Yes;
    }
    false
}

fn getter_property(m: &MethodFacts) -> Option<String> {
    if !m.params.is_empty() || m.return_type.trim() == "void" {
        return None;
    }
    if let Some(rest) = accessor_rest(&m.name, "get") {
        return Some(decapitalize(rest));
    }
    let boolean = matches!(m.return_type.trim(), "boolean" | "Boolean" | "java.lang.Boolean");
    accessor_rest(&m.name, "is").filter(|_| boolean).map(decapitalize)
}

fn setter_property(m: &MethodFacts) -> Option<String> {
    if m.params.len() != 1 {
        return None;
    }
    accessor_rest(&m.name, "set").map(decapitalize)
}

/// What follows a `get` / `is` / `set` prefix, when an uppercase letter follows it.
fn accessor_rest<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    rest.chars().next().filter(|c| c.is_uppercase()).map(|_| rest)
}

fn has_prefix(name: &str, prefix: &str) -> bool {
    accessor_rest(name, prefix).is_some()
}

fn is_boolean(type_text: &str) -> bool {
    matches!(type_text.trim(), "boolean" | "Boolean" | "java.lang.Boolean")
}

/// `java.beans.Introspector.decapitalize`, which is what MapStruct names properties with: `URL`
/// stays `URL`, `Name` becomes `name`.
pub fn decapitalize(name: &str) -> String {
    let mut chars = name.chars();
    let (Some(first), second) = (chars.next(), chars.next()) else { return String::new() };
    if second.is_some_and(|c| c.is_uppercase()) && first.is_uppercase() {
        return name.to_string();
    }
    let mut out: String = first.to_lowercase().collect();
    out.push_str(&name[first.len_utf8()..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_facts::prelude::scan_java;

    fn info(src: &str) -> TypeInfo {
        let facts = scan_java("/p/T.java", src).expect("grammar loads");
        type_info(&facts.types[0], &facts)
    }

    fn prop<'a>(t: &'a TypeInfo, name: &str) -> &'a Property {
        t.props.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no `{name}` in {:?}", t.props))
    }

    #[test]
    fn getters_and_setters_name_their_properties() {
        let t = info(
            "class U { private String firstName; private boolean active;\n\
             public String getFirstName() { return firstName; }\n\
             public boolean isActive() { return active; }\n\
             public void setFirstName(String v) {}\n\
             public String getURL() { return null; } }",
        );
        assert_eq!(prop(&t, "firstName").readable, Access::Yes);
        assert_eq!(prop(&t, "firstName").writable, Access::Yes);
        assert_eq!(prop(&t, "active").readable, Access::Yes);
        assert_eq!(prop(&t, "active").writable, Access::No);
        assert!(t.props.iter().any(|p| p.name == "URL"), "Introspector keeps an all-caps name");
        assert!(!t.opaque);
    }

    #[test]
    fn a_non_public_setter_is_never_certain() {
        let t = info("class U { private String a; void setA(String a) {} }");
        assert_eq!(prop(&t, "a").writable, Access::Maybe);
    }

    #[test]
    fn a_public_field_is_both_ways_unless_final() {
        let t = info("class U { public String a; public final String b = \"\"; }");
        assert_eq!(prop(&t, "a").writable, Access::Yes);
        assert_eq!(prop(&t, "b").readable, Access::Yes);
        assert_eq!(prop(&t, "b").writable, Access::No);
    }

    #[test]
    fn a_record_is_read_and_written_through_its_components() {
        let t = info("record R(String name, int age) {}");
        assert_eq!(prop(&t, "name").readable, Access::Yes);
        assert_eq!(prop(&t, "age").writable, Access::Yes);
        assert_eq!(prop(&t, "name").primary_site().unwrap().origin, Origin::RecordComponent);
    }

    #[test]
    fn lombok_data_generates_both_directions_but_only_when_it_is_lombok() {
        let t = info("import lombok.Data;\n@Data class U { private String a; private final String b; }");
        assert_eq!(prop(&t, "a").readable, Access::Yes);
        assert_eq!(prop(&t, "a").writable, Access::Yes);
        assert_eq!(prop(&t, "b").writable, Access::Maybe, "a final field only reaches the constructor");

        let own = info("import com.acme.Data;\n@Data class U { private String a; }");
        assert_eq!(prop(&own, "a").readable, Access::No, "somebody else's @Data generates nothing");
    }

    #[test]
    fn lombok_value_and_builder_never_claim_a_certain_writer() {
        let t = info("import lombok.*;\n@Value @Builder class U { String a; }");
        assert_eq!(prop(&t, "a").readable, Access::Yes);
        assert_eq!(prop(&t, "a").writable, Access::Maybe);
    }

    #[test]
    fn renamed_accessors_make_the_type_opaque() {
        assert!(info("import lombok.*;\n@Data @Accessors(fluent = true) class U { String a; }").opaque);
        assert!(info("import lombok.*;\nclass U { @Getter(AccessLevel.NONE) String a; }").opaque);
        assert!(info("class U { String a; U a(String v) { return this; } }").opaque, "fluent setter");
        assert!(info("class U { public static Builder builder() { return null; } }").opaque);
        assert!(info("interface U { String getA(); }").opaque);
    }

    #[test]
    fn a_single_constructor_makes_its_parameters_writable() {
        let t = info("class U { private final String a; public U(String a) { this.a = a; } }");
        assert_eq!(prop(&t, "a").writable, Access::Yes);
        let two = info("class U { public U(String a) {} public U(int b) {} }");
        assert!(two.opaque, "which constructor MapStruct picks is not this crate's guess");
        let default = info("class U { private String a; public U() {} public U(String a) {} }");
        assert_eq!(prop(&default, "a").writable, Access::No, "the no-arg one is used");
    }
}
