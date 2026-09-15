//! The project index registers a type under its simple name AND every member under its own name.
//! Resolving a simple name to "whatever symbol carries it" therefore answered a FIELD name with the
//! class that declares it — and a member symbol's `fqn` is its owner's binary, so it looked exactly
//! like a type.
//!
//! On a real project that turned `gare_genere_repository.get_genere(…)` — a call into a dependency
//! — into a use of the enclosing class's own `get_genere`, and the rename rewrote it. The call then
//! named a method the jar does not declare. In one case the answer was not even the enclosing
//! class: it was whichever class happened to declare a field of that name.

mod common;
use common::{at, Project};

const OWNER: &str = r#"package p;
public class Owner {
    private final Unknown gare_repo = null;

    Integer read() {
        return gare_repo.get_it();
    }

    Integer get_it() {
        return 1;
    }
}
"#;

/// `gare_repo` is a FIELD, not a type. The call on it is not a use of `Owner.get_it`, whatever the
/// two happen to have in common.
#[test]
fn a_field_name_does_not_resolve_as_a_type() {
    let p = Project::new(&[("p/Owner.java", OWNER)]);
    let src = p.source("p/Owner.java");
    let decl = at(src, "Integer get_it() {");
    let edits = p.rename_edits("p/Owner.java", decl + "Integer ".len(), "getIt");
    let foreign_call = at(src, "gare_repo.get_it()") + "gare_repo.".len();
    assert!(
        !edits.iter().any(|e| e.start == foreign_call),
        "a call on a field of an unresolved type was rewritten as if it were the enclosing \
         class's own method; edits at {:?}",
        edits.iter().map(|e| e.start).collect::<Vec<_>>()
    );
}

/// And because the engine cannot see whose method that call is, it must refuse the rename rather
/// than rewrite the declaration and leave it behind.
#[test]
fn the_rename_is_refused_because_that_call_cannot_be_placed() {
    let p = Project::new(&[("p/Owner.java", OWNER)]);
    let src = p.source("p/Owner.java");
    let decl = at(src, "Integer get_it() {");
    let plan = p
        .rename("p/Owner.java", decl + "Integer ".len(), "getIt")
        .expect("a plan");
    assert!(
        plan.blocked.is_some(),
        "expected a refusal, got {:?}",
        plan.blocked
    );
}
