//! Inherited-members category — the Structure panel's lazy "Inherited" bucket.
//!
//! `inherited_members(file, type, line)` lists the members of a type's SUPERCLASS + INTERFACES
//! (recursively), excluding the type's own members, deduping overrides so the nearest declaration
//! wins, and tagging each with its declaring FQCN + visibility + project source. The engine
//! resolver is project-only, so only PROJECT supertypes contribute (a bare `extends Object`
//! yields nothing) — which makes the set deterministic without a live JDK.

mod common;
use bennu_query::prelude::InheritedMember;
use common::*;

fn zoo() -> Project {
    Project::new(&[
        (
            "LivingThing.java",
            "package zoo;\n\
             public class LivingThing {\n\
             \x20   public boolean alive() { return true; }\n\
             \x20   public int age() { return 0; }\n\
             }\n",
        ),
        (
            "Animal.java",
            "package zoo;\n\
             public class Animal extends LivingThing {\n\
             \x20   protected String name;\n\
             \x20   public String speak() { return name; }\n\
             \x20   public int legs() { return 4; }\n\
             \x20   @Override public boolean alive() { return false; }\n\
             }\n",
        ),
        (
            "Pet.java",
            "package zoo;\n\
             public interface Pet {\n\
             \x20   String owner();\n\
             }\n",
        ),
        (
            "Dog.java",
            "package zoo;\n\
             public class Dog extends Animal implements Pet {\n\
             \x20   private int barks;\n\
             \x20   public void bark() { }\n\
             \x20   @Override public String speak() { return \"woof\"; }\n\
             \x20   public String owner() { return \"me\"; }\n\
             }\n",
        ),
        (
            "Loner.java",
            "package zoo;\n\
             public class Loner {\n\
             \x20   public int x() { return 0; }\n\
             }\n",
        ),
    ])
}

/// The inherited members of `Dog`, resolved off its declaration line.
fn dog_inherited(p: &Project) -> Vec<InheritedMember> {
    let dog = p.source("Dog.java").to_string();
    let line = line_of(&dog, "class Dog") as i64;
    p.inherited("Dog.java", "Dog", line)
}

fn find<'a>(ms: &'a [InheritedMember], name: &str) -> Option<&'a InheritedMember> {
    ms.iter().find(|m| m.name == name)
}

#[test]
fn lists_superclass_members() {
    let p = zoo();
    let ms = dog_inherited(&p);
    for name in ["name", "speak", "legs"] {
        assert!(
            find(&ms, name).is_some(),
            "expected inherited {name:?} from Animal, got {ms:?}"
        );
    }
}

#[test]
fn lists_interface_members() {
    let p = zoo();
    let ms = dog_inherited(&p);
    let owner = find(&ms, "owner").expect("interface member owner present");
    assert_eq!(
        owner.declaring_type, "zoo.Pet",
        "owner is declared on the interface"
    );
}

#[test]
fn walks_up_to_grandparent() {
    let p = zoo();
    let ms = dog_inherited(&p);
    let age = find(&ms, "age").expect("grandparent member age present");
    assert_eq!(age.declaring_type, "zoo.LivingThing");
}

#[test]
fn override_dedup_keeps_nearest_declaration() {
    // Animal overrides LivingThing.alive() → `alive` appears once, declared by the nearer Animal.
    let p = zoo();
    let ms = dog_inherited(&p);
    let alives: Vec<&InheritedMember> = ms.iter().filter(|m| m.name == "alive").collect();
    assert_eq!(
        alives.len(),
        1,
        "overridden alive() appears once, got {alives:?}"
    );
    assert_eq!(
        alives[0].declaring_type, "zoo.Animal",
        "nearest declaration wins"
    );
}

#[test]
fn excludes_the_types_own_members() {
    let p = zoo();
    let ms = dog_inherited(&p);
    assert!(
        find(&ms, "bark").is_none(),
        "Dog's own bark() is not an inherited member"
    );
    assert!(
        find(&ms, "barks").is_none(),
        "Dog's own field is not inherited"
    );
}

#[test]
fn tags_kind_and_visibility() {
    let p = zoo();
    let ms = dog_inherited(&p);
    let name = find(&ms, "name").expect("field name inherited");
    assert_eq!(name.kind, "field");
    assert_eq!(name.visibility, "protected");
    let speak = find(&ms, "speak").expect("method speak inherited");
    assert_eq!(speak.kind, "method");
    assert_eq!(speak.visibility, "public");
}

#[test]
fn inherited_member_carries_project_source() {
    let p = zoo();
    let ms = dog_inherited(&p);
    let name = find(&ms, "name").expect("field name inherited");
    let src = name
        .source
        .as_ref()
        .expect("project source location present");
    assert_eq!(src.file, "Animal.java", "name is declared in Animal.java");
    assert_eq!(
        src.line,
        line_of(p.source("Animal.java"), "class Animal") as i64
    );
}

#[test]
fn ordering_is_fields_then_methods() {
    let p = zoo();
    let ms = dog_inherited(&p);
    let first_method = ms.iter().position(|m| m.kind == "method");
    let last_field = ms.iter().rposition(|m| m.kind == "field");
    if let (Some(fm), Some(lf)) = (first_method, last_field) {
        assert!(lf < fm, "fields precede methods, got {ms:?}");
    }
}

#[test]
fn type_without_project_supertype_has_no_inherited() {
    // Loner extends nothing (implicit Object, a JDK type the project-only resolver won't decode).
    let p = zoo();
    let s = p.source("Loner.java").to_string();
    let ms = p.inherited("Loner.java", "Loner", line_of(&s, "class Loner") as i64);
    assert!(
        ms.is_empty(),
        "no project supertype → empty inherited bucket, got {ms:?}"
    );
}

#[test]
fn unknown_type_is_empty() {
    let p = zoo();
    let ms = p.inherited("Dog.java", "NoSuchType", 2);
    assert!(ms.is_empty(), "an unresolvable type yields an empty bucket");
}

#[test]
fn stale_line_is_empty_not_panic() {
    let p = zoo();
    // A wildly out-of-range declaration line must not panic; it just fails to resolve → empty.
    let ms = p.inherited("Dog.java", "Dog", 9999);
    assert!(ms.is_empty(), "a stale line resolves nothing");
}
