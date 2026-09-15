//! `find_member_name_spans` decides, for one owner, which declarations in a file are its. An
//! anonymous class's override is one of them, and getting it wrong is not a missing edit but a
//! class that stops implementing what it claims to.
//!
//! Tested directly rather than through a rename, because a rename can reach the same span by a
//! second route — the override family — that only works on a project small enough for the
//! anonymous type to survive the simple→binary map. Every anonymous class in a project is called
//! `1`, so on a real one that map keeps exactly one of them and the route disappears.

use bennu_intel::prelude::*;

const SRC: &str = r#"package p;
public interface Download {
    String document_info(String username, String identifier);

    static Download publicOne(String fixed) {
        return new Public() {
            @Override
            public String document_info(String username, String identifier) {
                return fixed;
            }
        };
    }

    static Download plain(String fixed) {
        return new Download() {
            @Override
            public String document_info(String username, String identifier) {
                return fixed;
            }
        };
    }

    interface Public extends Download { }
}
"#;

fn spans_for(owner: &str, name: &str) -> Vec<(usize, usize)> {
    bennu_intel::rename::find_member_name_spans(
        SRC,
        &DeclKey::Method {
            owner: owner.to_string(),
            name: name.to_string(),
        },
    )
}

#[test]
fn the_interfaces_own_declaration_is_found() {
    let spans = spans_for("p/Download", "document_info");
    let first = SRC.find("String document_info").unwrap() + "String ".len();
    assert!(spans.iter().any(|(s, _)| *s == first), "spans {spans:?}");
}

#[test]
fn an_anonymous_override_of_the_interface_itself_is_found() {
    let spans = spans_for("p/Download", "document_info");
    let anon = SRC.find("new Download() {").unwrap();
    let decl =
        SRC[anon..].find("public String document_info").unwrap() + anon + "public String ".len();
    assert!(spans.iter().any(|(s, _)| *s == decl), "spans {spans:?}");
}

/// The one that was missing: the anonymous class names a SUB-interface, and the declaration being
/// renamed lives one level up.
#[test]
fn an_anonymous_override_of_a_subinterface_is_found() {
    let spans = spans_for("p/Download", "document_info");
    let anon = SRC.find("new Public() {").unwrap();
    let decl =
        SRC[anon..].find("public String document_info").unwrap() + anon + "public String ".len();
    assert!(spans.iter().any(|(s, _)| *s == decl), "spans {spans:?}");
}

/// And it stays scoped: an unrelated type's member is never claimed.
#[test]
fn a_different_owner_claims_nothing() {
    assert!(spans_for("p/Elsewhere", "document_info").is_empty());
}

/// An ENUM CONSTANT with a body is an anonymous subclass of its own enum, and its overrides have to
/// move with the method they override. There is no `new` to read the type from — the supertype is
/// the enclosing enum — so this shape was missed entirely.
const ENUM_BODIES: &str = r#"package p;
public enum State {
    OPEN {
        @Override
        public State opposite_state() { return CLOSED; }
    },
    CLOSED {
        @Override
        public State opposite_state() { return OPEN; }
    };

    public abstract State opposite_state();
}
"#;

#[test]
fn an_override_in_an_enum_constant_body_is_found() {
    let key = DeclKey::Method {
        owner: "p/State".to_string(),
        name: "opposite_state".to_string(),
    };
    let spans = bennu_intel::rename::find_member_name_spans(ENUM_BODIES, &key);
    let first = ENUM_BODIES
        .find("public State opposite_state() { return CLOSED; }")
        .unwrap()
        + "public State ".len();
    let second = ENUM_BODIES
        .find("public State opposite_state() { return OPEN; }")
        .unwrap()
        + "public State ".len();
    let decl = ENUM_BODIES
        .find("public abstract State opposite_state();")
        .unwrap()
        + "public abstract State ".len();
    for (what, off) in [
        ("the first constant's override", first),
        ("the second constant's override", second),
        ("the enum's own declaration", decl),
    ] {
        assert!(
            spans.iter().any(|(s, _)| *s == off),
            "{what} was missed; spans {spans:?}"
        );
    }
}
