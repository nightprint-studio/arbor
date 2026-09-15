//! The project's types, and resolving a written type name to one of them.
//!
//! Resolution follows the compiler's order — single-type import, a nested type of the enclosing
//! declaration, the same package, then on-demand imports — against **only** the types the project
//! declares. A name that lands anywhere else (`java.util.Date`, a class from a jar) resolves to
//! [`TypeRef::Unknown`], and every check reads that as "say nothing".

use std::collections::{HashMap, HashSet};

use crate::properties::{Property, Site};

/// One declared type, reduced to what the mapper checks read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    pub fqcn: String,
    pub name: String,
    /// Absolute, forward-slashed.
    pub file: String,
    pub package: String,
    pub imports: Vec<String>,
    pub name_offset: usize,
    pub extends: String,
    pub implements: Vec<String>,
    pub props: Vec<Property>,
    /// Its properties follow rules this crate does not model — see [`crate::properties`].
    pub opaque: bool,
}

impl TypeInfo {
    /// The scope a name written inside this type resolves in.
    pub fn scope(&self) -> Scope<'_> {
        Scope { package: &self.package, imports: &self.imports, owner: Some(self.fqcn.as_str()) }
    }
}

/// Where a type name was written: the file's package and imports, and the declaration around it.
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    pub package: &'a str,
    pub imports: &'a [String],
    pub owner: Option<&'a str>,
}

/// What a written type name turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A project type, by fqcn.
    Project(String),
    /// A primitive, a boxed primitive or `String` — a value with no bean properties of its own.
    Value,
    /// Generic, array, or a collection by name. MapStruct iterates these; property checks do not apply.
    Collection,
    /// Anything else. Silence.
    Unknown,
}

/// A type with its supertypes' properties merged in.
#[derive(Debug, Clone)]
pub struct TypeView {
    /// Every supertype is a project type (or `Object`) and none of them is opaque.
    pub complete: bool,
    pub props: Vec<Property>,
}

const PRIMITIVES: &[&str] = &["boolean", "byte", "short", "int", "long", "char", "float", "double"];
const LANG_VALUES: &[&str] =
    &["Boolean", "Byte", "Short", "Integer", "Long", "Character", "Float", "Double", "String"];
const COLLECTIONS: &[&str] = &[
    "Collection", "List", "Set", "Map", "Iterable", "Optional", "Stream", "ArrayList", "LinkedList",
    "HashSet", "LinkedHashSet", "TreeSet", "SortedSet", "HashMap", "LinkedHashMap", "TreeMap", "Queue",
    "Deque",
];
/// Interfaces that add no properties. Any other one may carry a `default` getter this crate does not
/// read, so a type implementing it is not complete.
const INERT_INTERFACES: &[&str] = &["Serializable", "java.io.Serializable", "Cloneable", "Comparable"];
/// Deep enough for any real DTO hierarchy; a cycle mid-edit stops here too.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Default)]
pub struct TypeTable {
    types: HashMap<String, TypeInfo>,
    /// Fqcns declared more than once (a test copy of a main class). Neither copy is trusted.
    ambiguous: HashSet<String>,
}

impl TypeTable {
    pub fn insert(&mut self, info: TypeInfo) {
        if self.ambiguous.contains(&info.fqcn) {
            return;
        }
        if self.types.remove(&info.fqcn).is_some() {
            self.ambiguous.insert(info.fqcn);
            return;
        }
        self.types.insert(info.fqcn.clone(), info);
    }

    pub fn get(&self, fqcn: &str) -> Option<&TypeInfo> {
        self.types.get(fqcn)
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Whether a simple name is already declared by some type in the table.
    pub fn declares_simple(&self, simple: &str) -> bool {
        self.types.values().any(|t| t.name == simple)
    }

    /// `Some` when the fqcn is known either way — so it shadows what follows it in resolution order.
    fn lookup(&self, fqcn: &str) -> Option<TypeRef> {
        if self.ambiguous.contains(fqcn) {
            return Some(TypeRef::Unknown);
        }
        self.types.contains_key(fqcn).then(|| TypeRef::Project(fqcn.to_string()))
    }

    pub fn resolve(&self, written: &str, scope: Scope<'_>) -> TypeRef {
        let written = written.trim();
        if written.is_empty() {
            return TypeRef::Unknown;
        }
        if written.contains('<') || written.ends_with(']') || written.ends_with("...") {
            return TypeRef::Collection;
        }
        if PRIMITIVES.contains(&written) {
            return TypeRef::Value;
        }
        let Some((head, tail)) = written.split_once('.') else {
            return self.resolve_simple(written, scope);
        };
        if let Some(found) = self.lookup(written) {
            return found;
        }
        if let Some(lang) = written.strip_prefix("java.lang.") {
            return if LANG_VALUES.contains(&lang) { TypeRef::Value } else { TypeRef::Unknown };
        }
        let simple = written.rsplit('.').next().unwrap_or(written);
        if written.starts_with("java.util.") && COLLECTIONS.contains(&simple) {
            return TypeRef::Collection;
        }
        // `Outer.Inner`, written through the outer type's simple name.
        if let TypeRef::Project(outer) = self.resolve_simple(head, scope) {
            return self.lookup(&format!("{outer}.{tail}")).unwrap_or(TypeRef::Unknown);
        }
        TypeRef::Unknown
    }

    fn resolve_simple(&self, simple: &str, scope: Scope<'_>) -> TypeRef {
        let suffix = format!(".{simple}");
        if let Some(import) = scope.imports.iter().find(|i| i.ends_with(&suffix)) {
            if import.starts_with("java.") {
                return if COLLECTIONS.contains(&simple) { TypeRef::Collection } else { TypeRef::Unknown };
            }
            return self.lookup(import).unwrap_or(TypeRef::Unknown);
        }
        if let Some(found) = self.nested(simple, scope) {
            return found;
        }
        let same = if scope.package.is_empty() {
            simple.to_string()
        } else {
            format!("{}.{simple}", scope.package)
        };
        if let Some(found) = self.lookup(&same) {
            return found;
        }
        if LANG_VALUES.contains(&simple) {
            return TypeRef::Value;
        }
        let mut hits = scope
            .imports
            .iter()
            .filter_map(|i| i.strip_suffix(".*"))
            .filter_map(|pkg| self.lookup(&format!("{pkg}.{simple}")));
        if let Some(first) = hits.next() {
            return if hits.next().is_none() { first } else { TypeRef::Unknown };
        }
        if COLLECTIONS.contains(&simple) {
            return TypeRef::Collection;
        }
        TypeRef::Unknown
    }

    /// A member type of the declaration the name was written in, or of any declaration enclosing it.
    fn nested(&self, simple: &str, scope: Scope<'_>) -> Option<TypeRef> {
        let mut prefix = scope.owner?;
        loop {
            if let Some(found) = self.lookup(&format!("{prefix}.{simple}")) {
                return Some(found);
            }
            match prefix.rsplit_once('.') {
                Some((parent, _)) if parent.len() > scope.package.len() => prefix = parent,
                _ => return None,
            }
        }
    }

    /// The type of a property, resolved where the property was declared.
    pub fn property_type(&self, p: &Property) -> TypeRef {
        match self.get(&p.owner) {
            Some(owner) => self.resolve(&p.type_text, owner.scope()),
            None => TypeRef::Unknown,
        }
    }

    /// A type with every supertype's properties merged in. A subclass's sighting of a name comes
    /// first, and the two directions of access combine — a getter below and a setter above make one
    /// readable, writable property.
    pub fn view(&self, fqcn: &str) -> Option<TypeView> {
        let mut current = self.get(fqcn)?;
        let mut props: Vec<Property> = Vec::new();
        let mut complete = true;
        let mut seen: HashSet<&str> = HashSet::new();
        loop {
            if !seen.insert(current.fqcn.as_str()) || seen.len() > MAX_DEPTH {
                complete = false;
                break;
            }
            complete &= !current.opaque && current.implements.iter().all(|i| is_inert(i));
            merge(&mut props, &current.props);
            let ext = current.extends.trim();
            if ext.is_empty() || ext == "Object" || ext == "java.lang.Object" {
                break;
            }
            match self.resolve(ext, current.scope()) {
                TypeRef::Project(parent) => match self.get(&parent) {
                    Some(next) => current = next,
                    None => {
                        complete = false;
                        break;
                    }
                },
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        Some(TypeView { complete, props })
    }
}

fn is_inert(written: &str) -> bool {
    let base = written.split('<').next().unwrap_or(written).trim();
    INERT_INTERFACES.contains(&base)
}

fn merge(into: &mut Vec<Property>, from: &[Property]) {
    for p in from {
        match into.iter_mut().find(|q| q.name == p.name) {
            Some(q) => {
                q.readable = q.readable.max(p.readable);
                q.writable = q.writable.max(p.writable);
                q.sites.extend(p.sites.iter().cloned().collect::<Vec<Site>>());
            }
            None => into.push(p.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::type_info;
    use bennu_facts::prelude::scan_java;

    fn table(files: &[&str]) -> TypeTable {
        let mut t = TypeTable::default();
        for (i, src) in files.iter().enumerate() {
            let facts = scan_java(&format!("/p/F{i}.java"), src).expect("grammar loads");
            for ty in &facts.types {
                t.insert(type_info(ty, &facts));
            }
        }
        t
    }

    fn scope<'a>(package: &'a str, imports: &'a [String]) -> Scope<'a> {
        Scope { package, imports, owner: None }
    }

    #[test]
    fn a_name_resolves_through_import_then_package_then_on_demand() {
        let t = table(&[
            "package com.acme.dto; public class UserDto {}",
            "package com.acme; public class User {}",
            "package com.other; public class Thing {}",
        ]);
        let imports = vec!["com.acme.dto.UserDto".to_string(), "com.other.*".to_string()];
        let s = scope("com.acme", &imports);
        assert_eq!(t.resolve("UserDto", s), TypeRef::Project("com.acme.dto.UserDto".into()));
        assert_eq!(t.resolve("User", s), TypeRef::Project("com.acme.User".into()));
        assert_eq!(t.resolve("Thing", s), TypeRef::Project("com.other.Thing".into()));
        assert_eq!(t.resolve("com.acme.User", s), TypeRef::Project("com.acme.User".into()));
    }

    #[test]
    fn what_the_project_does_not_declare_is_unknown_not_empty() {
        let t = table(&["package com.acme; public class User {}"]);
        let imports = vec!["org.lib.External".to_string(), "java.util.Date".to_string()];
        let s = scope("com.acme", &imports);
        assert_eq!(t.resolve("External", s), TypeRef::Unknown);
        assert_eq!(t.resolve("Date", s), TypeRef::Unknown);
        assert_eq!(t.resolve("Nowhere", s), TypeRef::Unknown);
        assert_eq!(t.resolve("String", s), TypeRef::Value);
        assert_eq!(t.resolve("int", s), TypeRef::Value);
        assert_eq!(t.resolve("List<User>", s), TypeRef::Collection);
        assert_eq!(t.resolve("User[]", s), TypeRef::Collection);
    }

    #[test]
    fn an_import_of_a_library_type_shadows_a_project_type_of_the_same_name() {
        let t = table(&["package com.acme; public class User {}"]);
        let imports = vec!["org.lib.User".to_string()];
        assert_eq!(t.resolve("User", scope("com.acme", &imports)), TypeRef::Unknown);
    }

    #[test]
    fn a_nested_type_resolves_from_inside_its_outer_type() {
        let t = table(&["package p; public class Outer { public static class Inner {} }"]);
        let s = Scope { package: "p", imports: &[], owner: Some("p.Outer") };
        assert_eq!(t.resolve("Inner", s), TypeRef::Project("p.Outer.Inner".into()));
        assert_eq!(t.resolve("Outer.Inner", scope("p", &[])), TypeRef::Project("p.Outer.Inner".into()));
    }

    #[test]
    fn a_view_merges_supertypes_and_knows_when_it_cannot() {
        let t = table(&[
            "package p; public class Base { public String getId() { return null; } }",
            "package p; public class Dto extends Base { public void setId(String id) {} }",
            "package p; import org.lib.Remote; public class Far extends Remote { }",
        ]);
        let dto = t.view("p.Dto").unwrap();
        assert!(dto.complete);
        let id = dto.props.iter().find(|p| p.name == "id").unwrap();
        assert_eq!(id.readable, crate::properties::Access::Yes);
        assert_eq!(id.writable, crate::properties::Access::Yes);
        assert!(!t.view("p.Far").unwrap().complete, "a supertype from a jar has properties we cannot see");
    }

    #[test]
    fn a_type_declared_twice_is_trusted_nowhere() {
        let t = table(&["package p; public class Dup {}", "package p; public class Dup {}"]);
        assert!(t.get("p.Dup").is_none());
        assert_eq!(t.resolve("Dup", scope("p", &[])), TypeRef::Unknown);
    }
}
