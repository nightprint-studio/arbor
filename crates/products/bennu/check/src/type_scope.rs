//! The [`NameScope`] a CHECK reads a written type name in — the file's imports, its own types, then
//! the resolver.
//!
//! Its own module because two things need it and neither owns it: a check that resolves a type name
//! it found in the source, and (through `bennu-java`) the inference walk. Sharing the *policy* is
//! the point — see [`bennu_java::typename`], which exists because three copies of it had drifted.

use bennu_java::prelude::{FileSymbols, NameScope, TypeResolver};

/// A written type name resolved against one file.
pub struct FileScope<'a> {
    pub symbols: &'a FileSymbols,
    pub resolver: &'a dyn TypeResolver,
    /// The type the name was written INSIDE, as a binary name, when the caller knows it.
    ///
    /// It decides which supertype chain an inherited member type comes from, and a file with
    /// several nested classes has several. Guava's `Synchronized.java` declares classes implementing
    /// `Multiset` (whose `Entry` takes one type argument) and classes implementing `Map` (whose
    /// `Entry` takes two); without the owner, whichever was declared first answered for both, and
    /// every `Entry<K, V>` in the file was judged against the wrong arity.
    pub owner: Option<String>,
}

impl NameScope for FileScope<'_> {
    fn simple(&self, simple: &str) -> Option<String> {
        // A type declared in THIS file is authoritative: its FQN is right here, so it never depends
        // on a project-wide map that keeps one binary per simple name.
        if let Some(td) = self.symbols.types.iter().find(|t| t.name == simple) {
            return Some(td.fqn.replace('.', "/"));
        }
        // A member type INHERITED from a supertype is in scope with no import at all (JLS §8.1.5) —
        // `Entry` inside a `Map` implementation — and a nested class also sees what its ENCLOSING
        // classes inherit (JLS §8.1.3), so the search climbs out through them. This is CLASS scope,
        // which is inner to the compilation unit's, so it is asked before the file's imports.
        if let Some(owner) = self.owner.as_deref() {
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
