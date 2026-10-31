//! A receiver that exists nowhere in the method, while another variable of the right type is in scope.
mod common;
use common::*;

fn errors_in(body: &str) -> Vec<String> {
    let p = Project::new(&[
        ("Holder.java", "package shop;\npublic class Holder {\n    public String getEncryption() { return \"\"; }\n}\n"),
        ("Use.java", &format!("package shop;\npublic class Use {{\n    void go(Holder holder) {{\n{body}\n    }}\n}}\n")),
    ]);
    p.validate_errors("Use.java")
}

/// A receiver that exists nowhere in the method is an unresolved symbol, whatever the call after it: this is what
/// lets the ghost-text check drop a suggestion that borrowed a variable name from another method.
#[test]
fn a_receiver_that_exists_nowhere_in_the_method_is_unresolved() {
    for body in ["        Object e = com.getEncryption();", "        Object e = other.getEncryption();"] {
        assert!(errors_in(body).iter().any(|e| e.starts_with("unresolved-symbol")), "{body}");
    }
    assert!(errors_in("        Object e = holder.getEncryption();").is_empty());
}

/// Why the ghost-text check judges a suggestion as the finished statement it will become: with its `;` missing the file
/// has a syntax error, and the checker then reports that and nothing about the unknown receiver.
#[test]
fn a_statement_missing_its_semicolon_hides_the_unknown_receiver() {
    let unfinished = errors_in("        Object e = com.getEncryption()");
    assert!(unfinished.iter().all(|e| !e.starts_with("unresolved-symbol")), "{unfinished:?}");
    assert!(errors_in("        Object e = com.getEncryption();").iter().any(|e| e.starts_with("unresolved-symbol")));
}
