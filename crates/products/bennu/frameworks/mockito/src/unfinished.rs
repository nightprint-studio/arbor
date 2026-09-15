//! Statements that start something with Mockito and never finish it.
//!
//! `when(repo.find(1));` compiles, runs, and leaves Mockito in the middle of a stubbing. Nothing
//! fails on that line: the next Mockito call finds the half-built stubbing and throws there — which
//! is often the first line of the next test, so the stack trace points at a test that is fine.
//!
//! Only a **statement** is judged. `when(x)` whose result is assigned, returned or passed along may
//! be finished somewhere this does not look, and saying otherwise would be a guess.

use tree_sitter::Node;

use crate::file::{arguments, call_name, descendants, JavaFile};
use crate::shapes::{is_bdd_then, is_stubber, is_verify, stubbed_invocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unfinished {
    /// `when(mock.m());` / `given(mock.m());` — nothing says what the call answers. `call` is the
    /// name as written, so the message can name the methods that would finish it.
    Answerless { call: String },
    /// `doReturn(x).when(mock);` — never says which method is stubbed.
    Targetless,
    /// `verify(mock);` / `then(mock).should();` — never says which method is verified.
    Unverified { call: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnfinishedStatement {
    pub kind: Unfinished,
    /// The statement's expression, without its `;`.
    pub start: usize,
    pub end: usize,
}

pub(crate) fn unfinished_statements(file: &JavaFile<'_>) -> Vec<UnfinishedStatement> {
    let mut out: Vec<UnfinishedStatement> = descendants(file.root())
        .into_iter()
        .filter(|n| n.kind() == "expression_statement")
        .filter_map(|statement| {
            let expression = statement.named_child(0)?;
            let kind = unfinished(file, expression)?;
            Some(UnfinishedStatement { kind, start: expression.start_byte(), end: expression.end_byte() })
        })
        .collect();
    out.sort_by_key(|s| s.start);
    out
}

fn unfinished(file: &JavaFile<'_>, call: Node<'_>) -> Option<Unfinished> {
    if call.kind() != "method_invocation" {
        return None;
    }
    let receiver = call.child_by_field_name("object");
    match call_name(call, file.source) {
        name @ ("when" | "given") if stubbed_invocation(file, call).is_some() => {
            Some(Unfinished::Answerless { call: name.to_string() })
        }
        "when" if arguments(call).len() == 1 && receiver.is_some_and(|o| is_stubber(file, o)) => {
            Some(Unfinished::Targetless)
        }
        "verify" if is_verify(file, call) => Some(Unfinished::Unverified { call: "verify(…)" }),
        "should" if arguments(call).len() <= 1 && receiver.is_some_and(|o| is_bdd_then(file, o)) => {
            Some(Unfinished::Unverified { call: "then(…).should(…)" })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "import static org.mockito.Mockito.*;\n\
                          import static org.mockito.BDDMockito.given;\n\
                          import static org.mockito.BDDMockito.then;\n";

    fn found_in(src: &str) -> Vec<UnfinishedStatement> {
        unfinished_statements(&JavaFile::parse(src).expect("parses"))
    }

    fn kinds(body: &str) -> Vec<Unfinished> {
        let src = format!("{HEADER}class T {{ void t() {{ {body} }} }}");
        found_in(&src).into_iter().map(|u| u.kind).collect()
    }

    #[test]
    fn a_stubbing_with_no_answer_is_unfinished() {
        assert_eq!(kinds("when(repo.find(1));"), [Unfinished::Answerless { call: "when".into() }]);
        assert_eq!(kinds("given(repo.find(1));"), [Unfinished::Answerless { call: "given".into() }]);
    }

    #[test]
    fn a_stubber_that_never_names_the_method_is_unfinished() {
        assert_eq!(kinds("doReturn(1).when(repo);"), [Unfinished::Targetless]);
        assert_eq!(kinds("doThrow(e).doNothing().when(repo);"), [Unfinished::Targetless]);
    }

    #[test]
    fn a_verification_that_never_names_the_method_is_unfinished() {
        let verify = Unfinished::Unverified { call: "verify(…)" };
        assert_eq!(kinds("verify(repo);"), [verify.clone()]);
        assert_eq!(kinds("verify(repo, times(2));"), [verify]);
        assert_eq!(kinds("then(repo).should();"), [Unfinished::Unverified { call: "then(…).should(…)" }]);
    }

    #[test]
    fn the_squiggle_is_the_expression_without_its_semicolon() {
        let src = format!("{HEADER}class T {{ void t() {{ when(repo.find(1)); }} }}");
        let found = found_in(&src);
        assert_eq!(&src[found[0].start..found[0].end], "when(repo.find(1))");
    }

    #[test]
    fn a_finished_chain_is_left_alone() {
        for body in [
            "when(repo.find(1)).thenReturn(2);",
            "given(repo.find(1)).willReturn(2);",
            "verify(repo).save(1);",
            "verify(repo, never()).save(any());",
            "doReturn(1).when(repo).find();",
            "then(repo).should().save(1);",
            "var s = when(repo.find(1));",
        ] {
            assert!(kinds(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn a_qualified_call_is_resolved_like_a_bare_one() {
        let src = "import org.mockito.Mockito;\nclass T { void t() { Mockito.when(repo.find()); Mockito.verify(repo); } }";
        assert_eq!(found_in(src).len(), 2);
    }

    /// `when` is not a reserved word: a helper of that name from somewhere else is not Mockito's.
    #[test]
    fn somebody_elses_when_is_not_judged() {
        let src = "import static com.acme.Stubs.when;\nimport static org.mockito.Mockito.verify;\n\
                   class T { void t() { when(repo.find(1)); } }";
        assert!(found_in(src).is_empty());
        let unimported = "class T { void t() { when(repo.find(1)); verify(repo); } }";
        assert!(found_in(unimported).is_empty(), "no import: a method next door");
        let declared = "import static org.mockito.Mockito.*;\nclass T { void when(Object o) {} void t() { when(repo.find(1)); } }";
        assert!(found_in(declared).is_empty(), "a method of the file shadows the import");
    }
}
