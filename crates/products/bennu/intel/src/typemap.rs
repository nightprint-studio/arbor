//! Parse a written Java type text (`Map<String, Object>`) into a [`bennu_java`] seam
//! [`TypeRef`] with binary names, resolving simple names via imports + same-project
//! types.
//!
//! This is the build-time counterpart of `bennu-java`'s internal `typeparse` +
//! `simple_to_binary` (not on its public prelude), used to bake a project type's
//! resolved [`bennu_java::prelude::ClassMembers`] into the index at ingest time. At
//! query time the resolver reads that baked shape straight back — no re-parse.

use std::collections::BTreeMap;

use bennu_java::prelude::{Import, TypeRef};

/// A parsed simple-name type tree (before binary-name resolution).
#[derive(Debug, Clone)]
struct Parsed {
    name: String,
    args: Vec<Parsed>,
}

/// A simple type name resolved against the type that DECLARES it — its own nested types, then its
/// enclosing types', innermost first. `None` when no enclosing type declares one.
///
/// This is the scope Java looks in **first** (JLS §6.5.5.1: a member type of an enclosing class is
/// in a more inner scope than any import, and shadows one), and it is the only thing that tells two
/// same-named nested types apart. A project has them: `Upload.Checker` and `Download.Checker` both
/// spell `Checker`, and the project-wide simple→binary map keeps exactly one. Resolving a
/// `Download` member's `Checker` parameter to `Upload.Checker` handed every caller the wrong
/// signature — which surfaced as 27 false `argument-type` errors on a tree that compiles clean.
///
/// The climb stops the moment the scope is no longer a project TYPE: what is above the outermost
/// type is the package, and a package-level lookup is a different rule at a different precedence
/// (after single-type imports), which the callers already implement.
pub fn nested_in_scope(
    owner: &str,
    simple: &str,
    is_project: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if owner.is_empty() {
        return None;
    }
    let mut scope = owner;
    loop {
        let candidate = format!("{scope}/{simple}");
        if is_project(&candidate) {
            return Some(candidate);
        }
        let i = scope.rfind('/')?;
        scope = &scope[..i];
        if !is_project(scope) {
            return None;
        }
    }
}

/// Convert a written type `text` to a resolved [`TypeRef`]. Falls back to a bare token
/// on unparseable input (an unknown binary name is a benign resolver miss). `is_project` tests a
/// candidate binary for project membership, so a wildcard import (`import pkg.*;`) can pin the exact
/// package of a same-simple-name type.
pub fn type_text_to_ref(names: &FileNames, owner: &str, text: &str) -> TypeRef {
    let trimmed = text.trim();
    match parse_type(trimmed) {
        Some(p) => to_binary_ref(names, owner, &p),
        None => TypeRef::simple(trimmed.replace('.', "/")),
    }
}

fn to_binary_ref(names: &FileNames, owner: &str, p: &Parsed) -> TypeRef {
    TypeRef {
        binary_name: simple_to_binary(names, owner, &p.name),
        type_args: p
            .args
            .iter()
            .map(|a| to_binary_ref(names, owner, a))
            .collect(),
    }
}

/// Everything that binds a simple type name in ONE FILE during an index build.
///
/// One struct because these five travelled together through eleven call sites, and the seventh
/// thing they needed — the file's PACKAGE — is exactly what a long argument list discourages
/// adding. Without it the same-package rule derived the package by trimming the owner's last
/// segment, which for a NESTED type is its outer class: `Collections2.OrderedPermutationIterator`
/// looked for a sibling of `Collections2` instead of a sibling of `Collections2` itself, found
/// none, and fell through to the project-wide map — which keeps one binary per simple name. Guava
/// declares `AbstractIterator` in two packages, so every nested class in `common.collect` recorded
/// the one in `common.base` as its superclass.
#[derive(Clone, Copy)]
pub struct FileNames<'a> {
    /// The file's package, dotted; empty for the default package.
    pub package: &'a str,
    pub imports: &'a [Import],
    pub project_types: &'a BTreeMap<String, String>,
    pub is_project: &'a dyn Fn(&str) -> bool,
    /// The types this file declares — the only hierarchy available while the index is being built.
    pub file_types: &'a [bennu_java::prelude::TypeDecl],
}

impl<'a> FileNames<'a> {
    /// The same file, narrowed to one declared type — what a synthesiser working on a single
    /// `TypeDecl` has in hand.
    pub fn only(&self, td: &'a bennu_java::prelude::TypeDecl) -> FileNames<'a> {
        FileNames {
            file_types: std::slice::from_ref(td),
            ..*self
        }
    }

    /// The binary name of `simple`, were it a type in this file's own package.
    fn same_package(&self, simple: &str) -> String {
        if self.package.is_empty() {
            simple.to_string()
        } else {
            format!("{}/{simple}", self.package.replace('.', "/"))
        }
    }
}

/// What binds a simple type name during an INDEX BUILD: the declaring type's own scope, the file's
/// imports, the project's simple→binary map, then `java.lang`.
///
/// The order is Java's; the shape of the lookup — and in particular telling `Outer.Nested` from
/// `a.b.C` — is [`bennu_java::prelude::resolve_written_type`]. There used to be two copies of this
/// in this crate (here and in `java_index`) plus a third in `bennu-java`, differing only in which
/// `java.lang` names they happened to list, and the same nested-name bug had to be fixed in each.
pub(crate) struct ProjectNameScope<'a> {
    /// What the FILE binds — imports, package, the project map. Constant for every name in it.
    pub names: FileNames<'a>,
    /// The type that DECLARES what is being resolved, as a binary name — its own nested types and
    /// its enclosing types' are the innermost scope (see [`nested_in_scope`]).
    pub owner: &'a str,
}

impl bennu_java::prelude::NameScope for ProjectNameScope<'_> {
    fn simple(&self, simple: &str) -> Option<String> {
        // The declaring type's own scope comes first.
        if let Some(nested) = nested_in_scope(self.owner, simple, self.names.is_project) {
            return Some(nested);
        }
        // A single-type import wins over the collision-prone project map.
        for imp in self.names.imports {
            if imp.simple_name() == Some(simple) {
                return Some(imp.path.replace('.', "/"));
            }
        }
        // A member type inherited from a SUPERTYPE of the owner or of a type it is written inside.
        if let Some(b) = self.inherited_nested(simple) {
            return Some(b);
        }
        // A type in the OWNER's OWN PACKAGE is in scope with no import at all (JLS §6.5.5.1), and
        // its exact binary is derivable from the owner's. Missing, a bare same-package name fell
        // through to the project-wide map — which keeps ONE binary per simple name — so
        // `implements Builder` inside `org.apache.commons.lang3.builder` bound to whichever nested
        // `Builder` that map happened to hold, and the type was judged against the wrong contract.
        let candidate = self.names.same_package(simple);
        if (self.names.is_project)(&candidate) {
            return Some(candidate);
        }
        // A non-static wildcard import that brings in a PROJECT type of this simple name pins its
        // exact package — the fix for a supertype or a `throws` whose simple name collides across
        // packages (the JAXB `*Type` case), which the collapsed map below cannot express.
        for imp in self.names.imports {
            if imp.star && !imp.static_ {
                let candidate = format!("{}/{simple}", imp.path.replace('.', "/"));
                if (self.names.is_project)(&candidate) {
                    return Some(candidate);
                }
            }
        }
        if let Some(b) = self.names.project_types.get(simple) {
            return Some(b.clone());
        }
        bennu_java::prelude::java_lang_implicit(simple)
    }

    fn is_type(&self, binary: &str) -> bool {
        (self.names.is_project)(binary)
    }
}

impl ProjectNameScope<'_> {
    /// `simple` as a nested type of something the owner — or a type the owner is written inside —
    /// EXTENDS or IMPLEMENTS, read off this file's own declarations.
    ///
    /// One hop, deliberately: the supertype's own supertypes are in another file whose declarations
    /// this pass has not read. One hop covers `class Builder extends AbstractBuilder<…>` inside
    /// `class AtomicInitializer extends AbstractConcurrentInitializer`, which is the shape that
    /// broke — and going deeper would need the hierarchy the build is still producing.
    fn inherited_nested(&self, simple: &str) -> Option<String> {
        let mut scope = self.owner;
        loop {
            for td in self.names.file_types {
                if td.fqn.replace('.', "/") != scope {
                    continue;
                }
                let written = td.extends.iter().chain(td.implements.iter());
                for sup in written {
                    let sup_simple = sup
                        .split('<')
                        .next()
                        .unwrap_or(sup)
                        .trim()
                        .rsplit('.')
                        .next()
                        .unwrap_or(sup)
                        .trim();
                    // The supertype itself, by the cheap non-recursive routes only.
                    let sup_binary = self
                        .names
                        .imports
                        .iter()
                        .find(|i| i.simple_name() == Some(sup_simple))
                        .map(|i| i.path.replace('.', "/"))
                        .or_else(|| Some(self.names.same_package(sup_simple)))
                        .filter(|b| (self.names.is_project)(b))
                        .or_else(|| self.names.project_types.get(sup_simple).cloned())?;
                    let candidate = format!("{sup_binary}/{simple}");
                    if (self.names.is_project)(&candidate) {
                        return Some(candidate);
                    }
                }
            }
            let i = scope.rfind('/')?;
            scope = &scope[..i];
            if !(self.names.is_project)(scope) {
                return None;
            }
        }
    }
}

/// One written type name → its binary name, for the index build. The single entry point every
/// build-time caller shares — see [`ProjectNameScope`].
pub fn resolve_binary_name(names: &FileNames, owner: &str, simple: &str) -> String {
    simple_to_binary(names, owner, simple)
}

fn simple_to_binary(names: &FileNames, owner: &str, simple: &str) -> String {
    let scope = ProjectNameScope {
        names: *names,
        owner,
    };
    // The index keeps the written spelling for a name it could not bind: a later pass may resolve
    // it (a type indexed after this file), and a dotted spelling is recognisably not a binary name,
    // where the slashed guess this used to store was indistinguishable from a real one.
    bennu_java::prelude::resolve_written_type(simple, &scope)
        .text()
        .to_string()
}

/// Parse `Foo`, `a.b.Foo`, `List<Foo>`, `Map<K, V<X>>` into a [`Parsed`] tree.
fn parse_type(s: &str) -> Option<Parsed> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (name, rest) = match s.find('<') {
        Some(i) => (s[..i].trim().to_string(), Some(&s[i..])),
        None => (s.to_string(), None),
    };
    let args = match rest {
        Some(inner) => parse_args(inner)?,
        None => Vec::new(),
    };
    Some(Parsed { name, args })
}

/// Parse a `<A, B<C>>` argument list (including the surrounding angle brackets),
/// respecting nesting.
fn parse_args(s: &str) -> Option<Vec<Parsed>> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut args = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => {
                depth += 1;
                if depth == 1 {
                    start = i + 1;
                }
            }
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    push_arg(&s[start..i], &mut args);
                    break;
                }
            }
            b',' if depth == 1 => {
                push_arg(&s[start..i], &mut args);
                start = i + 1;
            }
            _ => {}
        }
    }
    Some(args)
}

fn push_arg(chunk: &str, out: &mut Vec<Parsed>) {
    let t = chunk.trim();
    if t.is_empty() {
        return;
    }
    // Wildcards `?` / `? extends X` / `? super X` collapse to their bound or Object.
    let resolved = if t == "?" {
        "Object"
    } else if let Some(rest) = t.strip_prefix("? extends ") {
        rest.trim()
    } else if let Some(rest) = t.strip_prefix("? super ") {
        rest.trim()
    } else {
        t
    };
    if let Some(p) = parse_type(resolved) {
        out.push(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `p/Upload/Checker` and `p/Download/Checker` both spell `Checker`; each must win inside its own
    /// outer type. This is the case the project-wide simple→binary map cannot express.
    #[test]
    fn a_nested_name_binds_to_the_enclosing_types_own() {
        let project = |b: &str| {
            matches!(
                b,
                "p/Upload" | "p/Download" | "p/Upload/Checker" | "p/Download/Checker"
            )
        };
        assert_eq!(
            nested_in_scope("p/Download", "Checker", &project).as_deref(),
            Some("p/Download/Checker")
        );
        assert_eq!(
            nested_in_scope("p/Upload", "Checker", &project).as_deref(),
            Some("p/Upload/Checker")
        );
    }

    /// From a member of a nested type, the enclosing type's nested types are in scope too.
    #[test]
    fn the_climb_reaches_the_enclosing_types_nested_names() {
        let project = |b: &str| matches!(b, "p/Outer" | "p/Outer/Inner" | "p/Outer/Helper");
        assert_eq!(
            nested_in_scope("p/Outer/Inner", "Helper", &project).as_deref(),
            Some("p/Outer/Helper")
        );
    }

    /// The climb stops at the outermost TYPE: what is above it is the package, whose types are
    /// resolved by a different rule at a lower precedence.
    #[test]
    fn the_climb_stops_before_the_package() {
        // `p/Sibling` is a same-package top-level type — reachable, but NOT through this rule.
        let project = |b: &str| matches!(b, "p/Outer" | "p/Sibling");
        assert_eq!(nested_in_scope("p/Outer", "Sibling", &project), None);
    }

    /// A same-package type needs no import, and its binary is derivable from the owner's — the
    /// project-wide map, which keeps one binary per simple name, cannot express it.
    #[test]
    fn a_same_package_type_beats_the_flat_map() {
        use bennu_java::prelude::NameScope;
        let project = |b: &str| matches!(b, "a/b/Owner" | "a/b/Builder" | "z/Builder");
        let mut map = BTreeMap::new();
        map.insert("Builder".to_string(), "z/Builder".to_string()); // the collapsed map's pick
        let names = FileNames {
            package: "a.b",
            imports: &[],
            project_types: &map,
            is_project: &project,
            file_types: &[],
        };
        let scope = ProjectNameScope {
            names,
            owner: "a/b/Owner",
        };
        assert_eq!(scope.simple("Builder").as_deref(), Some("a/b/Builder"));
    }

    /// The package is the FILE's, not "the owner minus its last segment". For a NESTED owner those
    /// differ, and the difference is a whole class of wrong supertypes: Guava declares
    /// `AbstractIterator` in both `common.base` and `common.collect`, so every nested class in
    /// `common.collect` that extends the local one recorded the OTHER one as its superclass — and
    /// a rename of `computeNext` then moved a family that did not contain them.
    #[test]
    fn a_nested_type_resolves_its_own_package_not_its_outer_class() {
        use bennu_java::prelude::NameScope;
        let project = |b: &str| {
            matches!(
                b,
                "a/b/Outer" | "a/b/AbstractIterator" | "z/AbstractIterator"
            )
        };
        let mut map = BTreeMap::new();
        map.insert(
            "AbstractIterator".to_string(),
            "z/AbstractIterator".to_string(),
        );
        let names = FileNames {
            package: "a.b",
            imports: &[],
            project_types: &map,
            is_project: &project,
            file_types: &[],
        };
        let scope = ProjectNameScope {
            names,
            owner: "a/b/Outer/Nested",
        };
        assert_eq!(
            scope.simple("AbstractIterator").as_deref(),
            Some("a/b/AbstractIterator")
        );
    }

    #[test]
    fn an_unknown_name_resolves_to_nothing() {
        let project = |b: &str| b == "p/Outer";
        assert_eq!(nested_in_scope("p/Outer", "Nope", &project), None);
        assert_eq!(nested_in_scope("", "Nope", &project), None);
    }
}
