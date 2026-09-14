//! The project's type table: every scanned type, its supertypes resolved through the imports, and
//! the modifiers proxyability is decided on.
//!
//! ## Three answers, not two
//!
//! A written type name resolves to a [`TypeRef`], and the variant that matters most is the middle
//! one. `Project` is a type this crate has read. `Library` is one it is **sure** the project does
//! not declare — imported from a package with no project type of that name, or a simple name no
//! project file carries. `Unresolved` is everything in between: a name that could be a project
//! type in a file that was never read. Every check treats it as "could be anything", which is what
//! keeps a half-read hierarchy from turning into a report.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use bennu_facts::prelude::{scan_java, JavaFacts, TypeFacts};
use bennu_java::prelude::{modifier_words, parse_java};
use tree_sitter::Node;

use crate::known;
use crate::text::{erase, is_primitive, line_of};

/// How many types an ancestry walk visits before it gives up and calls the hierarchy open.
const MAX_ANCESTORS: usize = 64;

/// One scanned Java file: its facts, its text, and its declarations' keyword modifiers.
#[derive(Debug, Clone)]
pub struct Unit {
    pub facts: JavaFacts,
    pub text: String,
    pub mods: Mods,
}

impl Unit {
    /// Scan a source. `None` when the grammar cannot be loaded.
    pub fn new(path: &str, text: &str) -> Option<Self> {
        Some(Self { facts: scan_java(path, text)?, text: text.to_string(), mods: Mods::of(text) })
    }
}

/// Keyword modifiers per declaration, keyed by the byte offset of the declaration's name — the
/// offset the facts already carry, so a type or method is looked up without a second walk.
///
/// Needed because the facts carry `static`/`public` but not `final` or `private`, and those two
/// words are exactly what decides whether a container can proxy a bean.
#[derive(Debug, Clone, Default)]
pub struct Mods(HashMap<usize, Vec<String>>);

impl Mods {
    pub fn of(source: &str) -> Self {
        let mut out = HashMap::new();
        if let Some(tree) = parse_java(source) {
            collect_mods(tree.root_node(), source, &mut out);
        }
        Self(out)
    }

    pub fn has(&self, name_offset: usize, word: &str) -> bool {
        self.0.get(&name_offset).is_some_and(|w| w.iter().any(|m| m == word))
    }
}

fn collect_mods(node: Node<'_>, source: &str, out: &mut HashMap<usize, Vec<String>>) {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    for child in children {
        let is_decl = matches!(
            child.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration"
        );
        if is_decl {
            if let Some(name) = child.child_by_field_name("name") {
                out.insert(name.start_byte(), modifier_words(child, source));
            }
        }
        collect_mods(child, source, out);
    }
}

/// What a written type name refers to. See the module docs for why there are three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A type this crate has read, by FQCN.
    Project(String),
    /// Certainly not a project type. Dotted when an import said where it lives.
    Library(String),
    /// Could be a project type nobody read.
    Unresolved(String),
}

impl TypeRef {
    pub fn name(&self) -> &str {
        match self {
            TypeRef::Project(n) | TypeRef::Library(n) | TypeRef::Unresolved(n) => n,
        }
    }

    pub fn simple(&self) -> &str {
        crate::text::simple_name(self.name())
    }

    pub fn project(&self) -> Option<&str> {
        match self {
            TypeRef::Project(n) => Some(n),
            _ => None,
        }
    }
}

/// The names resolution needs to know exist.
pub trait Names {
    fn has_fqcn(&self, fqcn: &str) -> bool;
    /// A project type or a project `.java` file of this simple name.
    fn has_simple(&self, simple: &str) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct NameIndex {
    fqcns: HashSet<String>,
    simples: HashSet<String>,
}

impl NameIndex {
    pub fn add_type(&mut self, fqcn: &str) {
        self.simples.insert(crate::text::simple_name(fqcn).to_string());
        self.fqcns.insert(fqcn.to_string());
    }

    pub fn add_simple(&mut self, simple: &str) {
        self.simples.insert(simple.to_string());
    }
}

impl Names for NameIndex {
    fn has_fqcn(&self, fqcn: &str) -> bool {
        self.fqcns.contains(fqcn)
    }
    fn has_simple(&self, simple: &str) -> bool {
        self.simples.contains(simple)
    }
}

/// Two indexes read as one — the project's, and the buffer's own declarations on top.
pub struct WithExtra<'a> {
    pub base: &'a NameIndex,
    pub extra: &'a NameIndex,
}

impl Names for WithExtra<'_> {
    fn has_fqcn(&self, fqcn: &str) -> bool {
        self.base.has_fqcn(fqcn) || self.extra.has_fqcn(fqcn)
    }
    fn has_simple(&self, simple: &str) -> bool {
        self.base.has_simple(simple) || self.extra.has_simple(simple)
    }
}

/// Resolve a written type in a file, the way the compiler would, as far as the names read allow.
pub fn resolve(written: &str, facts: &JavaFacts, names: &dyn Names) -> TypeRef {
    resolve_raw(&erase(written).raw, facts, names)
}

fn resolve_raw(raw: &str, facts: &JavaFacts, names: &dyn Names) -> TypeRef {
    if raw.is_empty() {
        return TypeRef::Unresolved(String::new());
    }
    if is_primitive(raw) {
        return TypeRef::Library(raw.to_string());
    }
    if let Some((head, rest)) = raw.split_once('.') {
        return resolve_dotted(raw, head, rest, facts, names);
    }
    if let Some(t) = facts.types.iter().find(|t| t.name == raw) {
        return TypeRef::Project(t.fqcn.clone());
    }
    let suffix = format!(".{raw}");
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        return match names.has_fqcn(import) {
            true => TypeRef::Project(import.clone()),
            false => TypeRef::Library(import.clone()),
        };
    }
    let local = if facts.package.is_empty() { raw.to_string() } else { format!("{}.{raw}", facts.package) };
    if names.has_fqcn(&local) {
        return TypeRef::Project(local);
    }
    for pkg in facts.imports.iter().filter_map(|i| i.strip_suffix(".*")) {
        let candidate = format!("{pkg}.{raw}");
        if names.has_fqcn(&candidate) {
            return TypeRef::Project(candidate);
        }
    }
    match names.has_simple(raw) {
        true => TypeRef::Unresolved(raw.to_string()),
        false => TypeRef::Library(raw.to_string()),
    }
}

/// `Outer.Inner` (a nested type named through its owner) or `com.acme.Foo` (a qualified name).
fn resolve_dotted(raw: &str, head: &str, rest: &str, facts: &JavaFacts, names: &dyn Names) -> TypeRef {
    if names.has_fqcn(raw) {
        return TypeRef::Project(raw.to_string());
    }
    if !head.starts_with(|c: char| c.is_uppercase()) {
        return TypeRef::Library(raw.to_string());
    }
    match resolve_raw(head, facts, names) {
        TypeRef::Project(owner) => {
            let full = format!("{owner}.{rest}");
            match names.has_fqcn(&full) {
                true => TypeRef::Project(full),
                false => TypeRef::Unresolved(full),
            }
        }
        TypeRef::Library(owner) => TypeRef::Library(format!("{owner}.{rest}")),
        TypeRef::Unresolved(owner) => TypeRef::Unresolved(format!("{owner}.{rest}")),
    }
}

/// A supertype reference, and whether it was written with type arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub target: TypeRef,
    pub parameterized: bool,
}

/// A constructor, as far as bean construction cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctor {
    pub params: usize,
    pub private: bool,
    pub inject: bool,
}

/// A `final` instance method a client proxy cannot override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalMethod {
    pub name: String,
    pub offset: usize,
}

/// What a project stereotype carries onto the classes annotated with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stereotype {
    pub scope: Option<&'static str>,
    pub alternative: bool,
    pub named: bool,
}

/// One project type.
#[derive(Debug, Clone)]
pub struct TypeRow {
    pub fqcn: String,
    pub simple: String,
    /// `"class"` | `"interface"` | `"enum"` | `"record"` | `"annotation"`.
    pub kind: &'static str,
    pub is_abstract: bool,
    pub is_final: bool,
    /// Top-level, or a `static` nested type — an inner class is never a bean.
    pub bean_capable: bool,
    pub extends: Option<Edge>,
    pub implements: Vec<Edge>,
    pub ctors: Vec<Ctor>,
    pub final_methods: Vec<FinalMethod>,
    /// An annotation type meta-annotated `@Qualifier`.
    pub qualifier: bool,
    /// An annotation type meta-annotated `@Stereotype`.
    pub stereotype: Option<Stereotype>,
    /// Carries any annotation at all — which a subclass may inherit, `@Inherited` qualifiers included.
    pub annotated: bool,
    pub file: String,
    pub offset: usize,
    pub line: u32,
}

impl TypeRow {
    pub fn is_class(&self) -> bool {
        matches!(self.kind, "class" | "record")
    }
}

/// Build the row for one type of a unit.
pub fn row_of(t: &TypeFacts, unit: &Unit, names: &dyn Names) -> TypeRow {
    let facts = &unit.facts;
    let edge = |w: &String| Edge { target: resolve(w, facts, names), parameterized: w.contains('<') };
    let nested = t
        .fqcn
        .rsplit_once('.')
        .is_some_and(|(owner, _)| facts.types.iter().any(|o| o.fqcn == owner));
    let implicitly_static = matches!(t.kind, "interface" | "enum" | "record" | "annotation");
    let is_annotation = t.kind == "annotation";
    TypeRow {
        fqcn: t.fqcn.clone(),
        simple: t.name.clone(),
        kind: t.kind,
        is_abstract: t.is_abstract || t.kind == "interface",
        is_final: unit.mods.has(t.name_offset, "final") || matches!(t.kind, "record" | "enum"),
        bean_capable: !nested || implicitly_static || unit.mods.has(t.name_offset, "static"),
        extends: (!t.extends.is_empty()).then(|| edge(&t.extends)),
        implements: t.implements.iter().map(edge).collect(),
        ctors: t
            .methods
            .iter()
            .filter(|m| m.is_constructor)
            .map(|m| Ctor {
                params: m.params.len(),
                private: unit.mods.has(m.name_offset, "private"),
                inject: known::has(&m.annotations, facts, "Inject"),
            })
            .collect(),
        final_methods: t
            .methods
            .iter()
            .filter(|m| !m.is_constructor && !m.is_static)
            .filter(|m| unit.mods.has(m.name_offset, "final") && !unit.mods.has(m.name_offset, "private"))
            .map(|m| FinalMethod { name: m.name.clone(), offset: m.name_offset })
            .collect(),
        qualifier: is_annotation && known::has(&t.annotations, facts, "Qualifier"),
        stereotype: (is_annotation && known::has(&t.annotations, facts, "Stereotype")).then(|| Stereotype {
            scope: known::scope_of(&t.annotations, facts),
            alternative: known::has(&t.annotations, facts, "Alternative"),
            named: known::has(&t.annotations, facts, "Named"),
        }),
        annotated: !t.annotations.is_empty(),
        file: facts.file.clone(),
        offset: t.name_offset,
        line: line_of(&unit.text, t.name_offset),
    }
}

/// Every type read, by FQCN, plus the names resolution consults.
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    pub rows: BTreeMap<String, Arc<TypeRow>>,
    pub names: NameIndex,
}

impl TypeTable {
    /// Build from scanned units. `simples` are the stems of every project `.java` file, read or not
    /// — what turns a name nobody read into `Unresolved` rather than `Library`.
    ///
    /// Two passes, because resolving a supertype needs to know every name first.
    pub fn build(units: &[Unit], simples: impl IntoIterator<Item = String>) -> Self {
        let mut table = TypeTable::default();
        for simple in simples {
            table.names.add_simple(&simple);
        }
        for unit in units {
            for t in &unit.facts.types {
                table.names.add_type(&t.fqcn);
            }
        }
        for unit in units {
            for t in &unit.facts.types {
                let row = row_of(t, unit, &table.names);
                table.rows.insert(row.fqcn.clone(), Arc::new(row));
            }
        }
        table
    }
}

/// Where a type's hierarchy reaches.
#[derive(Debug, Clone, Default)]
pub struct Ancestry {
    /// Project types reached, the start included — each with whether some edge naming it was written
    /// **raw**. A type reached only as `Repo<Order>` is not assignable to a raw `Repo` in CDI.
    pub project: BTreeMap<String, bool>,
    /// Library supertypes, as resolved.
    pub library: Vec<String>,
    /// Some reference could not be followed — the walk is not the whole answer.
    pub open: bool,
}

/// The project's table, optionally with one file's declarations replaced by a live buffer's.
pub struct TypeView<'a> {
    base: &'a TypeTable,
    hidden_file: Option<&'a str>,
    extra: BTreeMap<String, Arc<TypeRow>>,
    extra_names: NameIndex,
}

impl<'a> TypeView<'a> {
    pub fn of(base: &'a TypeTable) -> Self {
        Self { base, hidden_file: None, extra: BTreeMap::new(), extra_names: NameIndex::default() }
    }

    /// The table with `file`'s rows swapped for `rows`.
    pub fn overlay(base: &'a TypeTable, file: &'a str, rows: Vec<TypeRow>, extra_names: NameIndex) -> Self {
        let extra = rows.into_iter().map(|r| (r.fqcn.clone(), Arc::new(r))).collect();
        Self { base, hidden_file: Some(file), extra, extra_names }
    }

    pub fn get(&self, fqcn: &str) -> Option<&TypeRow> {
        if let Some(row) = self.extra.get(fqcn) {
            return Some(row.as_ref());
        }
        let row = self.base.rows.get(fqcn)?;
        (Some(row.file.as_str()) != self.hidden_file).then_some(row.as_ref())
    }

    pub fn rows(&self) -> impl Iterator<Item = &TypeRow> + '_ {
        let hidden = self.hidden_file;
        self.base
            .rows
            .values()
            .filter(move |r| Some(r.file.as_str()) != hidden)
            .map(|r| r.as_ref())
            .chain(self.extra.values().map(|r| r.as_ref()))
    }

    pub fn names(&self) -> WithExtra<'_> {
        WithExtra { base: &self.base.names, extra: &self.extra_names }
    }

    /// Walk a type's supertypes breadth-first.
    pub fn ancestry(&self, fqcn: &str) -> Ancestry {
        let mut out = Ancestry::default();
        out.project.insert(fqcn.to_string(), true);
        let mut queue = VecDeque::from([fqcn.to_string()]);
        while let Some(current) = queue.pop_front() {
            if out.project.len() > MAX_ANCESTORS {
                out.open = true;
                break;
            }
            let Some(row) = self.get(&current) else {
                out.open = true;
                continue;
            };
            for edge in row.extends.iter().chain(row.implements.iter()) {
                match &edge.target {
                    TypeRef::Project(t) => {
                        let seen = out.project.contains_key(t);
                        let raw = out.project.get(t).copied().unwrap_or(false) || !edge.parameterized;
                        out.project.insert(t.clone(), raw);
                        if !seen {
                            queue.push_back(t.clone());
                        }
                    }
                    TypeRef::Library(n) => out.library.push(n.clone()),
                    TypeRef::Unresolved(_) => out.open = true,
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(path: &str, src: &str) -> Unit {
        Unit::new(path, src).unwrap()
    }

    fn table(units: &[Unit]) -> TypeTable {
        TypeTable::build(units, Vec::<String>::new())
    }

    #[test]
    fn a_name_resolves_through_file_import_package_and_wildcard() {
        let units = [
            unit("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            unit("/p/b/Other.java", "package b;\npublic class Other {}\n"),
        ];
        let t = table(&units);
        let f = scan_java("/p/a/Impl.java", "package a;\nimport b.*;\nimport x.Lib;\nclass Impl {}\n").unwrap();
        assert_eq!(resolve("Svc", &f, &t.names), TypeRef::Project("a.Svc".into()), "same package");
        assert_eq!(resolve("Other", &f, &t.names), TypeRef::Project("b.Other".into()), "on-demand import");
        assert_eq!(resolve("Lib", &f, &t.names), TypeRef::Library("x.Lib".into()));
        assert_eq!(resolve("Impl", &f, &t.names), TypeRef::Project("a.Impl".into()), "declared here");
        assert_eq!(resolve("List<Svc>", &f, &t.names), TypeRef::Library("List".into()));
    }

    #[test]
    fn a_name_some_project_file_carries_is_unresolved_not_library() {
        let mut t = TypeTable::default();
        t.names.add_simple("Hidden");
        let f = scan_java("/p/a/Impl.java", "package a;\nclass Impl {}\n").unwrap();
        assert_eq!(resolve("Hidden", &f, &t.names), TypeRef::Unresolved("Hidden".into()));
    }

    #[test]
    fn ancestry_follows_project_supertypes_and_notes_generic_edges() {
        let units = [unit(
            "/p/a/All.java",
            "package a;\ninterface Svc {}\ninterface Repo<T> {}\nabstract class Base implements Svc {}\nclass Impl extends Base implements Repo<String> {}\n",
        )];
        let t = table(&units);
        let view = TypeView::of(&t);
        let a = view.ancestry("a.Impl");
        assert_eq!(a.project.get("a.Svc"), Some(&true));
        assert_eq!(a.project.get("a.Repo"), Some(&false), "only reached as Repo<String>");
        assert!(!a.open);
    }

    #[test]
    fn an_unresolved_supertype_leaves_the_hierarchy_open() {
        let mut t = TypeTable::default();
        t.names.add_simple("Unseen");
        let u = unit("/p/a/Impl.java", "package a;\nclass Impl extends Unseen {}\n");
        t.names.add_type("a.Impl");
        let row = row_of(&u.facts.types[0], &u, &t.names);
        t.rows.insert(row.fqcn.clone(), Arc::new(row));
        assert!(TypeView::of(&t).ancestry("a.Impl").open);
    }

    #[test]
    fn modifiers_are_read_as_tokens() {
        let u = unit(
            "/p/a/C.java",
            "package a;\npublic final class C {\n  @Named(\"final\") public C() {}\n  private C(int x) {}\n  public final void go() {}\n  private final void hidden() {}\n  static class N {}\n  class Inner {}\n}\n",
        );
        let t = table(std::slice::from_ref(&u));
        let c = &t.rows["a.C"];
        assert!(c.is_final);
        assert_eq!(c.ctors.len(), 2);
        assert!(!c.ctors[0].private && c.ctors[1].private);
        assert_eq!(c.final_methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["go"]);
        assert!(t.rows["a.C.N"].bean_capable);
        assert!(!t.rows["a.C.Inner"].bean_capable, "an inner class is never a bean");
    }
}
