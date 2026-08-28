//! Where in a file a written type name is being read, and the [`NameScope`] that reads it.
//!
//! Its own module because two things need it and neither owns it: a check that resolves a type name
//! it found in the source, and (through `bennu-java`) the inference walk. Sharing the *policy* is
//! the point — see [`bennu_java::typename`], which exists because three copies of it had drifted.

use bennu_java::prelude::{FileSymbols, NameScope, TypeResolver};

/// The innermost type scope a name is written in.
///
/// The distinction that matters is between a type's BODY and its HEADER. A member type's scope is
/// the body of its class (JLS §6.3), so the `extends` / `implements` clause of `Outer` does not see
/// `Outer`'s own member types — it is read one scope out. Commons-lang writes
/// `class HashCodeBuilder implements Builder<Integer>` in a class that also declares a nested
/// `Builder`, and reading the header inside the body bound the interface it implements to that
/// class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeScope {
    /// Inside the BODY of this type (a binary name): its member types, and its enclosing types',
    /// are in scope.
    Inside(String),
    /// The compilation unit itself — a top-level type's HEADER. Only the file's top-level types are
    /// in scope from here.
    CompilationUnit,
    /// The caller has no position in hand.
    ///
    /// Most checks resolve a name they found while walking, with no node to hand: they get the
    /// file's declarations searched FLAT, by simple name, which is what every caller had before
    /// this distinction existed. It is a guess — a nested type answers for a name written anywhere
    /// in the file — so prefer a real scope wherever the node is available.
    Unknown,
}

impl TypeScope {
    /// The enclosing type, when there is one.
    fn owner(&self) -> Option<&str> {
        match self {
            TypeScope::Inside(b) => Some(b),
            TypeScope::CompilationUnit | TypeScope::Unknown => None,
        }
    }
}

/// A written type name resolved against one file.
pub struct FileScope<'a> {
    pub symbols: &'a FileSymbols,
    pub resolver: &'a dyn TypeResolver,
    /// Where the name is written — see [`TypeScope`].
    ///
    /// It also decides which supertype chain an inherited member type comes from, and a file with
    /// several nested classes has several. Guava's `Synchronized.java` declares classes implementing
    /// `Multiset` (whose `Entry` takes one type argument) and classes implementing `Map` (whose
    /// `Entry` takes two); without it, whichever was declared first answered for both, and every
    /// `Entry<K, V>` in the file was judged against the wrong arity.
    pub scope: TypeScope,
}

impl NameScope for FileScope<'_> {
    fn simple(&self, simple: &str) -> Option<String> {
        // A type declared in THIS file is authoritative: its FQN is right here, so it never depends
        // on a project-wide map that keeps one binary per simple name. WHICH of them, though, is a
        // scope question — see `TypeScope`.
        match self.scope {
            TypeScope::Unknown => {
                if let Some(td) = self.symbols.types.iter().find(|t| t.name == simple) {
                    return Some(td.fqn.replace('.', "/"));
                }
            }
            _ => {
                if let Some(b) = bennu_java::prelude::declared_type_in_scope(
                    self.symbols,
                    self.scope.owner(),
                    simple,
                ) {
                    return Some(b);
                }
            }
        }
        // A member type INHERITED from a supertype is in scope with no import at all (JLS §8.1.5) —
        // `Entry` inside a `Map` implementation — and a nested class also sees what its ENCLOSING
        // classes inherit (JLS §8.1.3), so the search climbs out through them. This is CLASS scope,
        // which is inner to the compilation unit's, so it is asked before the file's imports.
        if let Some(owner) = self.scope.owner() {
            let mut scope = owner;
            loop {
                if let Some(bn) =
                    bennu_java::prelude::inherited_member_type_of(self.resolver, scope, simple)
                {
                    return Some(bn);
                }
                let Some(i) = scope.rfind('/') else { break };
                scope = &scope[..i];
                if self.resolver.members_of(scope).is_none() {
                    break;
                }
            }
        }
        for imp in &self.symbols.imports {
            if imp.simple_name() == Some(simple) {
                return Some(imp.path.replace('.', "/"));
            }
        }
        // A type in the file's OWN package needs no import. Resolved to its exact binary before the
        // flat lookup, for the same reason.
        if let Some(pkg) = self.symbols.package.as_deref() {
            if !pkg.is_empty() {
                let candidate = format!("{}/{simple}", pkg.replace('.', "/"));
                if self.resolver.members_of(&candidate).is_some() {
                    return Some(candidate);
                }
            }
        }
        // Last: what any type in the file inherits. This is NOT a scope Java has — a name written
        // inside one nested class is never bound by a sibling's supertypes — so it can only stand
        // in for an owner we do not know. Asked ahead of the file's imports, as it once was, it
        // answered for names the file imports outright: Guava's `Maps.java` says
        // `import java.util.Map.Entry;` and declares dozens of nested classes, one of whose
        // supertypes has an unrelated `Entry` with a different arity — so every `Entry<K, V>` in
        // the file was judged against a type taking one argument.
        if let Some(bn) =
            bennu_java::prelude::inherited_member_type(self.symbols, self.resolver, simple)
        {
            return Some(bn);
        }
        self.resolver
            .resolve_simple_name(simple, &self.symbols.imports)
            .or_else(|| bennu_java::prelude::java_lang_implicit(simple))
    }

    fn is_type(&self, binary: &str) -> bool {
        self.resolver.members_of(binary).is_some()
    }
}
