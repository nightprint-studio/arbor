//! Completion asked with the caret after a `.` that is followed by MORE of the same expression —
//! `Headers.|USERNAME.header_name()` — rather than by whitespace or a closer.
//!
//! This is what asking for completions on an expression you have already written looks like, and it
//! is the state an explicit request is most often made in.

mod common;
use common::{at, Project};

const HEADERS: &str = r#"package p;
public enum Headers {
    USERNAME("x-username"),
    TENANT("x-tenant");
    private final String header;
    Headers(String header) { this.header = header; }
    public String header_name() { return header; }
}
"#;

const USE: &str = r#"package p;
public class Doc {
    void build() {
        name(Headers.USERNAME.header_name());
    }
    void name(String s) {}
}
"#;

fn project() -> Project {
    Project::new(&[("p/Headers.java", HEADERS), ("p/Doc.java", USE)])
}

/// The enum's constants are what `Headers.` offers, even with `USERNAME.header_name()` already
/// written to the right of the caret.
#[test]
fn an_enum_receiver_completes_with_text_still_to_the_right() {
    let p = project();
    let src = p.source("p/Doc.java");
    let caret = at(src, "Headers.USERNAME") + "Headers.".len();
    let labels = p.complete_labels("p/Doc.java", caret);
    assert!(
        labels.iter().any(|l| l == "USERNAME") && labels.iter().any(|l| l == "TENANT"),
        "the enum constants were not offered: {labels:?}"
    );
}

/// The same caret at the END of the buffer (nothing to the right) — the state every existing test
/// exercises, kept alongside so a regression shows WHICH of the two broke.
#[test]
fn an_enum_receiver_completes_at_the_end_of_a_line() {
    const OPEN: &str = r#"package p;
public class Doc2 {
    void build() {
        Headers h = Headers.
    }
}
"#;
    let p = Project::new(&[("p/Headers.java", HEADERS), ("p/Doc2.java", OPEN)]);
    let src = p.source("p/Doc2.java");
    let caret = at(src, "= Headers.") + "= Headers.".len();
    let labels = p.complete_labels("p/Doc2.java", caret);
    assert!(
        labels.iter().any(|l| l == "USERNAME"),
        "the enum constants were not offered: {labels:?}"
    );
}
