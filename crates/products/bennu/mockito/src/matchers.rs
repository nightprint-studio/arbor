//! Argument matchers beside plain values.
//!
//! `verify(repo).save(any(), 5)` reads perfectly well and throws `InvalidUseOfMatchersException`:
//! when one argument of a stubbed or verified call is a matcher, every argument must be one. The fix
//! is mechanical — `eq(5)` — which is exactly why it is worth doing in the editor rather than after
//! reading the exception's paragraph of advice.
//!
//! Judged only on the call Mockito is actually recording: the method inside `when(…)` / `given(…)`,
//! the method called on `verify(mock)`, on a stubber's `when(mock)`, and on BDD's
//! `then(mock).should()`. Arguments anywhere else are somebody else's business.

use tree_sitter::Node;

use crate::file::{arguments, call_name, descendants, static_qualifier, unwrap_expression, JavaFile};
use crate::owners::matcher_owners;
use crate::shapes::{is_bdd_then, is_stubber, is_verify, stubbed_invocation};
use crate::values::is_plain_value;

/// One recorded call whose arguments mix matchers and plain values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MixedCall {
    /// The span of every plain argument, in source order — each one squiggled, each one wrapped.
    pub raw: Vec<(usize, usize)>,
    /// The qualifiers the call's matchers were written with (`ArgumentMatchers` in
    /// `ArgumentMatchers.any()`), so a fix can spell `eq` the way the file already does.
    pub qualifiers: Vec<String>,
}

pub(crate) fn mixed_calls(file: &JavaFile<'_>) -> Vec<MixedCall> {
    let mut calls: Vec<MixedCall> = descendants(file.root())
        .into_iter()
        .filter(|n| n.kind() == "method_invocation")
        .filter_map(|n| recorded_call(file, n))
        .filter_map(|call| mixed(file, call))
        .collect();
    calls.sort_by_key(|c| c.raw.first().map_or(0, |r| r.0));
    calls
}

/// The call whose arguments Mockito counts, when `call` is one of the recorded shapes.
fn recorded_call<'t>(file: &JavaFile<'_>, call: Node<'t>) -> Option<Node<'t>> {
    if let Some(stubbed) = stubbed_invocation(file, call) {
        // `when(mock.find(…))`, not `when(mock.child().find(…))`: a deep stub records the chain, and
        // its earlier arguments count too.
        let on_a_mock = stubbed.child_by_field_name("object").is_some_and(is_mock_reference);
        return on_a_mock.then_some(stubbed);
    }
    let receiver = call.child_by_field_name("object")?;
    if receiver.kind() != "method_invocation" {
        return None;
    }
    let inner = receiver.child_by_field_name("object");
    let recorded = match call_name(receiver, file.source) {
        "verify" => is_verify(file, receiver),
        "when" => arguments(receiver).len() == 1 && inner.is_some_and(|o| is_stubber(file, o)),
        "should" => arguments(receiver).len() <= 1 && inner.is_some_and(|o| is_bdd_then(file, o)),
        _ => false,
    };
    recorded.then_some(call)
}

/// `repo`, `this.repo`, `fixture.repo` — a mock by name, not the result of a call.
fn is_mock_reference(node: Node<'_>) -> bool {
    match node.kind() {
        "identifier" => true,
        "field_access" => node
            .child_by_field_name("object")
            .is_some_and(|o| o.kind() == "this" || is_mock_reference(o)),
        _ => false,
    }
}

enum Argument {
    /// A Mockito matcher, with the qualifier it was written with.
    Matcher(Option<String>),
    Plain,
    /// Anything that might register a matcher without being recognisably one.
    Unsure,
}

fn mixed(file: &JavaFile<'_>, call: Node<'_>) -> Option<MixedCall> {
    let mut raw = Vec::new();
    let mut qualifiers = Vec::new();
    let mut matchers = 0usize;
    for argument in arguments(call) {
        match classify(file, argument) {
            Argument::Matcher(qualifier) => {
                matchers += 1;
                qualifiers.extend(qualifier);
            }
            Argument::Plain => raw.push((argument.start_byte(), argument.end_byte())),
            // One argument this cannot read makes the count unknowable, and the report with it.
            Argument::Unsure => return None,
        }
    }
    if matchers == 0 || raw.is_empty() {
        return None;
    }
    Some(MixedCall { raw, qualifiers })
}

fn classify(file: &JavaFile<'_>, argument: Node<'_>) -> Argument {
    let inner = unwrap_expression(argument);
    if inner.kind() != "method_invocation" {
        return if is_plain_value(file, argument) { Argument::Plain } else { Argument::Unsure };
    }
    let name = call_name(inner, file.source);
    match matcher_owners(name).zip(static_qualifier(inner, file.source)) {
        Some((owners, qualifier)) if file.resolves(name, qualifier, owners) => {
            Argument::Matcher(qualifier.map(str::to_string))
        }
        // A call that is not certainly Mockito's matcher may still register one — a helper, a
        // captor, a matcher nested in something else. Not a plain value either way.
        _ => Argument::Unsure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "import static org.mockito.Mockito.*;\n\
                          import static org.mockito.BDDMockito.given;\n\
                          import static org.mockito.BDDMockito.then;\n";

    /// The text of every plain argument reported in `src`.
    fn raw_in(src: &str) -> Vec<String> {
        let file = JavaFile::parse(src).expect("parses");
        mixed_calls(&file)
            .into_iter()
            .flat_map(|c| c.raw)
            .map(|(s, e)| src[s..e].to_string())
            .collect()
    }

    fn raw(body: &str) -> Vec<String> {
        raw_in(&format!("{HEADER}class T {{ void t() {{ {body} }} }}"))
    }

    #[test]
    fn a_plain_value_beside_a_matcher_is_reported_in_every_recorded_shape() {
        assert_eq!(raw("verify(repo).save(any(), 5);"), ["5"]);
        assert_eq!(raw("verify(repo, times(2)).save(\"a\", anyInt(), B);"), ["\"a\"", "B"]);
        assert_eq!(raw("when(repo.find(anyString(), 10)).thenReturn(null);"), ["10"]);
        assert_eq!(raw("given(repo.find(any(), 1)).willReturn(null);"), ["1"]);
        assert_eq!(raw("doReturn(1).when(repo).find(eq(1), 2);"), ["2"]);
        assert_eq!(raw("then(repo).should(times(1)).save(any(), \"x\");"), ["\"x\""]);
        assert_eq!(raw("verify(repo).save((String) any(), -1);"), ["-1"], "a cast matcher is still one");
    }

    #[test]
    fn a_local_that_holds_a_literal_is_a_plain_value() {
        assert_eq!(raw("String v = \"x\"; verify(repo).save(any(), v);"), ["v"]);
    }

    #[test]
    fn calls_that_are_all_matchers_or_all_values_are_fine() {
        for body in [
            "verify(repo).call(any());",
            "verify(repo).save(1, 2);",
            "verify(repo).save(eq(1), any());",
            "when(repo.find(1)).thenReturn(2);",
            "doReturn(1).when(repo).call();",
            "verify(repo).save(and(gt(1), lt(5)), anyInt());",
        ] {
            assert!(raw(body).is_empty(), "{body}");
        }
    }

    /// Everything here might register a matcher — so the count is unknown and nothing is said.
    #[test]
    fn an_argument_that_might_be_a_matcher_silences_the_call() {
        for body in [
            "verify(repo).save(any(), foo(eq(1)));",
            "verify(repo).save(any(), captor.capture());",
            "verify(repo).save(any(), order.getId());",
            "verify(repo).save(any(), orderWith(1));",
            "String v = make(); verify(repo).save(any(), v);",
            "list.forEach(v -> verify(repo).save(any(), v));",
        ] {
            assert!(raw(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn a_helper_parameter_may_carry_a_matcher_but_a_test_parameter_does_not() {
        let helper = format!("{HEADER}class T {{ void check(String v) {{ verify(repo).save(any(), v); }} }}");
        assert!(raw_in(&helper).is_empty());
        let test = format!(
            "{HEADER}class T {{ @ParameterizedTest void check(String v) {{ verify(repo).save(any(), v); }} }}"
        );
        assert_eq!(raw_in(&test), ["v"]);
    }

    #[test]
    fn a_matcher_that_is_not_certainly_mockitos_silences_the_call() {
        let foreign = "import static com.acme.My.any;\nimport static org.mockito.Mockito.verify;\n\
                       class T { void t() { verify(repo).save(any(), 1); } }";
        assert!(raw_in(foreign).is_empty());
        let hamcrest = "import static org.mockito.Mockito.*;\nimport static org.hamcrest.Matchers.*;\n\
                        class T { void t() { verify(repo).save(any(), 1); } }";
        assert!(raw_in(hamcrest).is_empty(), "Hamcrest has an `any` too");
    }

    #[test]
    fn a_deep_stub_is_not_judged() {
        assert!(raw("when(repo.child().find(any(), 1)).thenReturn(null);").is_empty());
    }
}
