//! Lombok generates whole nested TYPES, not just members, and two of them carry a copy of every
//! field NAME: `@FieldNameConstants`'s `Fields` and `@Builder`'s builder. Renaming a field has to
//! move both, and to do that the engine has to know they exist.
//!
//! Neither was modelled, so on a real project a bulk naming fix renamed the field and left
//! `Dto.Fields.file_name` and `Dto.builder().file_name(x)` spelling a name nothing declared.

mod common;
use common::{at, Project};

const DTO: &str = r#"package p;
import lombok.Builder;
import lombok.Data;
import lombok.experimental.FieldNameConstants;

@Data
@Builder
@FieldNameConstants
public class Dto {
    private String file_name;
    private int idcom;
}
"#;

const FIELDS_USER: &str = r#"package p;
public class Uses {
    String constant() {
        return Dto.Fields.file_name;
    }
}
"#;

const BUILDER_USER: &str = r#"package p;
public class Builds {
    Dto make() {
        return Dto.builder().file_name("x").idcom(1).build();
    }
}
"#;

fn project() -> Project {
    Project::new(&[
        ("p/Dto.java", DTO),
        ("p/Uses.java", FIELDS_USER),
        ("p/Builds.java", BUILDER_USER),
    ])
}

fn edits_for_file_name(p: &Project) -> Vec<bennu_intel::prelude::Edit> {
    let decl = p.source("p/Dto.java");
    p.rename_edits(
        "p/Dto.java",
        at(decl, "String file_name") + "String ".len(),
        "fileName",
    )
}

#[test]
fn the_field_name_constant_moves_with_the_field() {
    let p = project();
    let edits = edits_for_file_name(&p);
    let user = p.source("p/Uses.java");
    let site = at(user, "Fields.file_name") + "Fields.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Uses.java" && e.start == site),
        "`Dto.Fields.file_name` was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

#[test]
fn the_builder_setter_moves_with_the_field() {
    let p = project();
    let edits = edits_for_file_name(&p);
    let user = p.source("p/Builds.java");
    let site = at(user, ".file_name(\"x\")") + ".".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "p/Builds.java" && e.start == site),
        "`Dto.builder().file_name(…)` was not renamed; edits: {:?}",
        edits.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
}

/// The chain must keep its type all the way to `build()`, or everything after the first call is
/// unresolved and the errors move rather than disappearing.
#[test]
fn the_builder_chain_keeps_its_type_to_the_end() {
    let p = project();
    let edits = edits_for_file_name(&p);
    let user = p.source("p/Builds.java");
    // `idcom` comes AFTER `file_name` in the chain: it only resolves if the first call returned the
    // builder rather than an unknown type.
    let decl = p.source("p/Dto.java");
    let idcom = p.rename_edits("p/Dto.java", at(decl, "int idcom") + "int ".len(), "idCom");
    let site = at(user, ".idcom(1)") + ".".len();
    assert!(
        idcom
            .iter()
            .any(|e| e.file == "p/Builds.java" && e.start == site),
        "the second call in the builder chain was not renamed; edits: {:?}",
        idcom.iter().map(|e| (&e.file, e.start)).collect::<Vec<_>>()
    );
    assert!(!edits.is_empty());
}
