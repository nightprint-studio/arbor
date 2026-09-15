//! Whether a rename would collide with a name that is ALREADY there.
//!
//! The engine could plan a perfect set of edits and still produce code that does not compile — or,
//! worse, code that compiles and means something else. Renaming a method onto a name a supertype
//! declares turns it into an override; renaming a field onto an inherited one hides it; renaming a
//! type onto a sibling in its own package is a duplicate class. None of these are visible in the
//! edit list, because every edit in it is individually correct.
//!
//! So this asks the question the edit list cannot: after the rename, is the new name still free
//! where it has to be? A `Some` here becomes the plan's `blocked` reason — the edits are still
//! computed and still shown, because seeing what it *would* do is what makes the refusal legible.
//!
//! Separate from the naming-fix driver's own collision check, which is textual and file-local and
//! answers a different question: *two violations wanting the same spelling*. That one cannot see a
//! superclass in another file, and a manual rename from the UI never reaches it at all.

use std::collections::HashSet;

use bennu_java::prelude::TypeResolver;

use crate::refs::DeclKey;

/// Every type in `start`'s hierarchy, itself included.
///
/// The shared walk — see [`bennu_java::prelude::supertype_names`]. A rename's conflict search and
/// the inference that decides what a name MEANS have to agree about what is in a hierarchy, and
/// two walks that could drift apart is how they would stop agreeing.
fn hierarchy(resolver: &dyn TypeResolver, start: &str) -> Vec<String> {
    bennu_java::prelude::supertype_names(resolver, start)
}


/// Why renaming `key` to `new_name` must not be applied, or `None` when the name is free.
///
/// `family` is every owner the rename moves together (a method's override family); the members
/// being renamed are excluded from the search by construction, since we only ever look for the NEW
/// name and the new name is not what they are called yet.
pub fn member_conflict(
    resolver: &dyn TypeResolver,
    family: &[String],
    key: &DeclKey,
    new_name: &str,
) -> Option<String> {
    match key {
        DeclKey::Method { name, .. } => method_conflict(resolver, family, name, new_name),
        DeclKey::Field { owner, name } => field_conflict(resolver, owner, name, new_name),
        DeclKey::Type { .. } => None,
    }
}

/// A method rename collides when some type in reach already declares `new_name` at an arity this
/// rename is about to produce.
///
/// Arity is the whole test, because that is what Java uses to tell two methods apart: an existing
/// `size()` is no obstacle to renaming `count(int)` to `size(int)`… and every obstacle to renaming
/// `length()`. Overloads of the old name all move together (they are one rename), so every one of
/// their arities has to be free.
fn method_conflict(
    resolver: &dyn TypeResolver,
    family: &[String],
    old_name: &str,
    new_name: &str,
) -> Option<String> {
    let mut arities: HashSet<usize> = HashSet::new();
    for owner in family {
        let Some(cm) = resolver.members_of(owner) else {
            continue;
        };
        for m in cm.methods.iter().filter(|m| m.name == old_name) {
            arities.insert(m.params.len());
        }
    }
    if arities.is_empty() {
        return None;
    }
    for owner in family {
        for ty in hierarchy(resolver, owner) {
            let Some(cm) = resolver.members_of(&ty) else {
                continue;
            };
            let Some(hit) = cm
                .methods
                .iter()
                .find(|m| m.name == new_name && arities.contains(&m.params.len()))
            else {
                continue;
            };
            let where_ = if &ty == owner {
                format!("`{}` already declares", ty.replace('/', "."))
            } else {
                format!("`{}` already declares", ty.replace('/', "."))
            };
            let consequence = if &ty == owner {
                "two methods with the same name and parameter count in one type"
            } else {
                "the renamed method would silently start overriding it"
            };
            return Some(format!(
                "{where_} `{new_name}` taking {} argument(s) — renaming would produce {consequence}.",
                hit.params.len()
            ));
        }
    }
    None
}

/// A field rename collides when the owner, or anything it inherits from, already has that field.
///
/// A same-type clash is a duplicate declaration; an inherited one is *hiding*, which compiles and
/// changes what the code means — the worse of the two, and the one no edit list can show.
fn field_conflict(
    resolver: &dyn TypeResolver,
    owner: &str,
    old_name: &str,
    new_name: &str,
) -> Option<String> {
    // Only when the owner really has the field under its old name: without that guard a stale or
    // half-built index turns every rename into a refusal.
    let has_old = resolver
        .members_of(owner)
        .map(|cm| cm.fields.iter().any(|f| f.name == old_name))
        .unwrap_or(false);
    if !has_old {
        return None;
    }
    for ty in hierarchy(resolver, owner) {
        let Some(cm) = resolver.members_of(&ty) else {
            continue;
        };
        if !cm.fields.iter().any(|f| f.name == new_name) {
            continue;
        }
        return Some(if ty == owner {
            format!(
                "`{}` already declares a field `{new_name}` — renaming would declare it twice.",
                ty.replace('/', ".")
            )
        } else {
            format!(
                "`{}` already declares a field `{new_name}` — the renamed field would hide it, \
                 which compiles and changes what the code means.",
                ty.replace('/', ".")
            )
        });
    }
    None
}

/// A type rename collides when its own package already holds that name.
///
/// Asked of the exact binary rather than the project's simple-name map, which keeps one binary per
/// simple name and so cannot answer "in THIS package" at all.
pub fn type_conflict(resolver: &dyn TypeResolver, binary: &str, new_name: &str) -> Option<String> {
    let (scope, _) = binary.rsplit_once('/')?;
    let candidate = format!("{scope}/{new_name}");
    if candidate == binary || resolver.members_of(&candidate).is_none() {
        return None;
    }
    // The scope is the package for a top-level type and the outer type for a nested one, and the
    // sentence reads correctly either way: both are "the place this name has to be unique in".
    Some(format!(
        "`{}` already exists — renaming would declare that name twice in one scope.",
        candidate.replace('/', ".")
    ))
}

/// Whether renaming a local/parameter to `new_name` would CAPTURE a name already visible in its
/// scope.
///
/// One rule covers every shape this takes: if the spelling already occurs in the scope as a bare
/// name, then after the rename those occurrences read as the renamed variable. It does not matter
/// whether the thing they used to name was another local, a parameter, or a field of the enclosing
/// class used without `this.` — the result is the same, it compiles, and it means something else.
///
/// Two kinds of occurrence are excluded, and both are excluded because Java has separate namespaces
/// for them: a member selector (`obj.total`) names somebody else's member, and the callee of a bare
/// call (`total()`) is a method — `int size = size();` is legal Java, and refusing it would block a
/// correct rename on a spelling that was never taken.
pub fn local_capture(scope: tree_sitter::Node, bytes: &[u8], new_name: &str) -> Option<String> {
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.kind() != "identifier" || crate::refs::is_member_selector_node(&n) || is_callee(&n) {
            continue;
        }
        if n.utf8_text(bytes).ok() == Some(new_name) {
            return Some(format!(
                "`{new_name}` is already used in this scope — renaming here would capture it, \
                 which compiles and changes what the code means."
            ));
        }
    }
    None
}

/// Whether this identifier is the METHOD being called in a bare `name(...)`.
fn is_callee(n: &tree_sitter::Node) -> bool {
    n.parent()
        .filter(|p| p.kind() == "method_invocation")
        .and_then(|p| p.child_by_field_name("name"))
        .is_some_and(|name| name.id() == n.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct R(HashMap<String, ClassMembers>);
    impl TypeResolver for R {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(
            &self,
            _n: &str,
            _i: &[bennu_java::prelude::Import],
        ) -> Option<String> {
            None
        }
        fn is_project_type(&self, b: &str) -> bool {
            self.0.contains_key(b)
        }
    }

    fn cm(superclass: Option<&str>, methods: Vec<Member>, fields: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods,
            fields,
            flags: Default::default(),
        }
    }

    fn m(name: &str, arity: usize) -> Member {
        Member::method(
            name,
            TypeRef::simple("void"),
            vec![TypeRef::simple("int"); arity],
        )
    }

    fn f(name: &str) -> Member {
        Member::field(name, TypeRef::simple("int"))
    }

    fn resolver(entries: Vec<(&str, ClassMembers)>) -> R {
        R(entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect())
    }

    #[test]
    fn a_free_name_is_not_a_conflict() {
        let r = resolver(vec![("p/A", cm(None, vec![m("count", 0)], vec![]))]);
        let key = DeclKey::Method {
            owner: "p/A".into(),
            name: "count".into(),
        };
        assert_eq!(
            member_conflict(&r, &["p/A".to_string()], &key, "size"),
            None
        );
    }

    /// The case an edit list can never show: after the rename the method is an override, and the
    /// code still compiles.
    #[test]
    fn renaming_onto_an_inherited_method_of_the_same_arity_is_refused() {
        let r = resolver(vec![
            ("p/A", cm(Some("p/Base"), vec![m("count", 0)], vec![])),
            ("p/Base", cm(None, vec![m("size", 0)], vec![])),
        ]);
        let key = DeclKey::Method {
            owner: "p/A".into(),
            name: "count".into(),
        };
        let reason = member_conflict(&r, &["p/A".to_string()], &key, "size").expect("conflict");
        assert!(reason.contains("overriding"), "{reason}");
    }

    /// Arity is what Java uses to tell two methods apart, so an existing name at another arity is
    /// an overload, not a clash — refusing there would block a correct rename.
    #[test]
    fn an_existing_name_at_another_arity_is_an_overload_not_a_clash() {
        let r = resolver(vec![(
            "p/A",
            cm(None, vec![m("count", 1), m("size", 0)], vec![]),
        )]);
        let key = DeclKey::Method {
            owner: "p/A".into(),
            name: "count".into(),
        };
        assert_eq!(
            member_conflict(&r, &["p/A".to_string()], &key, "size"),
            None
        );
    }

    #[test]
    fn every_arity_of_an_overload_set_has_to_be_free() {
        let r = resolver(vec![(
            "p/A",
            cm(
                None,
                vec![m("count", 0), m("count", 1), m("size", 1)],
                vec![],
            ),
        )]);
        let key = DeclKey::Method {
            owner: "p/A".into(),
            name: "count".into(),
        };
        assert!(member_conflict(&r, &["p/A".to_string()], &key, "size").is_some());
    }

    #[test]
    fn renaming_a_field_onto_an_inherited_one_is_refused_as_hiding() {
        let r = resolver(vec![
            ("p/A", cm(Some("p/Base"), vec![], vec![f("n")])),
            ("p/Base", cm(None, vec![], vec![f("total")])),
        ]);
        let key = DeclKey::Field {
            owner: "p/A".into(),
            name: "n".into(),
        };
        let reason = member_conflict(&r, &["p/A".to_string()], &key, "total").expect("conflict");
        assert!(reason.contains("hide"), "{reason}");
    }

    #[test]
    fn a_type_name_already_taken_in_the_package_is_refused() {
        let r = resolver(vec![
            ("p/A", cm(None, vec![], vec![])),
            ("p/B", cm(None, vec![], vec![])),
        ]);
        assert!(type_conflict(&r, "p/A", "B").is_some());
        assert_eq!(type_conflict(&r, "p/A", "C"), None);
    }

    /// A stale index that has never heard of the member must not turn every rename into a refusal.
    #[test]
    fn an_unknown_owner_stays_silent() {
        let r = resolver(vec![]);
        let key = DeclKey::Field {
            owner: "p/A".into(),
            name: "n".into(),
        };
        assert_eq!(
            member_conflict(&r, &["p/A".to_string()], &key, "total"),
            None
        );
    }
}
