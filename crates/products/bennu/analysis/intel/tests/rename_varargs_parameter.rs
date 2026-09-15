//! A varargs parameter is a parameter. Renaming one must be scope-exact — and must not be mistaken
//! for a same-named FIELD, which is what happened on Apache Commons: a caret on
//! `final String... excludeFields` renamed the class's `excludeFields` field and every use of it,
//! leaving the parameter alone. The static method then read an instance field.

mod common;
use common::{at, Project};

const SRC: &str = r#"package p;
public class Builder {
    private String[] excludeFields;

    public Builder setExcludeFields(final String[] v) {
        this.excludeFields = v;
        return this;
    }

    public static boolean reflectionEquals(final Object a, final String... excludeFields) {
        return new Builder().setExcludeFields(excludeFields).ok();
    }

    boolean ok() {
        return excludeFields != null;
    }
}
"#;

#[test]
fn renaming_a_varargs_parameter_is_scope_exact() {
    let p = Project::new(&[("p/Builder.java", SRC)]);
    let src = p.source("p/Builder.java");
    let decl = at(src, "String... excludeFields") + "String... ".len();
    let edits = p.rename_edits("p/Builder.java", decl, "exclude_fields");

    // Its own declaration and its own use, and nothing else.
    let use_site = at(src, "setExcludeFields(excludeFields)") + "setExcludeFields(".len();
    assert!(
        edits.iter().any(|e| e.start == decl),
        "the parameter declaration was not renamed"
    );
    assert!(
        edits.iter().any(|e| e.start == use_site),
        "the parameter's use was not renamed"
    );

    let field_decl = at(src, "private String[] excludeFields") + "private String[] ".len();
    let field_use = at(src, "this.excludeFields") + "this.".len();
    let bare_field_use = at(src, "return excludeFields != null") + "return ".len();
    for (what, off) in [
        ("the field's declaration", field_decl),
        ("a qualified use of the field", field_use),
        ("a bare use of the field", bare_field_use),
    ] {
        assert!(
            !edits.iter().any(|e| e.start == off),
            "{what} was renamed by a rename of the varargs PARAMETER; edits at {:?}",
            edits.iter().map(|e| e.start).collect::<Vec<_>>()
        );
    }
}
