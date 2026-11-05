//! A breakpoint condition: an expression that reads the program and answers yes or no.
//!
//! Everything a condition is refused for is refused **before the debugger ever runs**, and the
//! frontend checks it while you type — because the failure it prevents is silent. A watch that
//! answers something adjacent to what you typed shows a wrong number, which you might notice. A
//! condition that does it swallows the stop: the program runs past the line you were waiting for and
//! there is nothing on screen at all.
//!
//! Beyond what any expression is refused for (a call, a cast, a lambda, an assignment…), a condition
//! has two rules of its own:
//!
//! * **It has to read something.** `true`, `1 == 1`, `2 + 2 == 4` are constants: no stopped program
//!   could change the answer, so the breakpoint either always stops or never does, and both are
//!   certainly a mistake.
//! * **It has to be a boolean.** `i + 1` is a number, and "stop when `i + 1`" means nothing.
//!
//! What happens when a valid condition cannot be answered *at a hit* — a null halfway down the path,
//! a field that is not on this subclass — is the caller's call, and Bennu's is to **stop** and say
//! why: running on would turn a typo into a breakpoint that never fires and never explains itself.

use crate::ast::{Expr, UnOp};
use crate::parser::parse;
use crate::syntax::Syntax;

/// Parse a condition, or say what is wrong with it in one line.
///
/// An empty (or whitespace-only) condition is `None` rather than an error: that is what "no
/// condition" is written as everywhere else in the product, and refusing it would make clearing a
/// condition impossible.
pub fn parse_condition(text: &str, syntax: Syntax) -> Result<Option<Expr>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let expr = parse(text, syntax)?;
    if let Some(why) = expr.refusal(syntax) {
        return Err(why);
    }
    if !expr.reads_program() {
        return Err(constant_message(&expr));
    }
    if let Some(why) = not_a_boolean(&expr) {
        return Err(why);
    }
    Ok(Some(expr))
}

/// Why a constant is not a condition.
fn constant_message(expr: &Expr) -> String {
    match expr {
        Expr::Lit(lit) => format!(
            "`{lit}` is a constant — a condition has to read something from the program"
        ),
        Expr::Binary(op, a, b)
            if op.is_comparison() && matches!(**a, Expr::Lit(_)) && matches!(**b, Expr::Lit(_)) =>
        {
            "both sides are constants — a condition has to read something".to_string()
        }
        other => format!(
            "`{other}` is a constant — a condition has to read something from the program"
        ),
    }
}

/// An expression whose top node can only produce a number or a string. Caught statically because
/// the alternative is an error at the first hit, which is the first time anyone reads it.
fn not_a_boolean(expr: &Expr) -> Option<String> {
    let numeric = match expr {
        Expr::Binary(op, ..) => op.is_arithmetic(),
        Expr::Unary(UnOp::Neg, _) => true,
        _ => false,
    };
    numeric.then(|| {
        format!("`{expr}` is a number, not a condition — compare it with something")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Lit;
    use crate::syntax::{JAVA, RUST};

    fn refused(text: &str) -> String {
        match parse_condition(text, JAVA) {
            Err(e) => e,
            Ok(other) => panic!("{text:?} should have been refused, got {other:?}"),
        }
    }

    fn parsed(text: &str) -> Expr {
        parse_condition(text, JAVA).expect("parses").expect("not empty")
    }

    #[test]
    fn no_condition_is_not_an_error() {
        assert_eq!(parse_condition("", JAVA), Ok(None));
        assert_eq!(parse_condition("   \t ", JAVA), Ok(None));
    }

    #[test]
    fn the_conditions_that_always_worked_still_do() {
        for ok in [
            "i > 5",
            "count <= -3",
            "ratio < 0.5",
            "n == 10L",
            "x == 2f",
            "mask == 0xFF",
            "order.customer.name == \"acme\"",
            "items[2].price > 0",
            "c == 'x'",
            "c == '\\n'",
            "0 < i",
            "done",
            "!order.paid",
            "a || b && c",
            "(a || b) && c",
            "order != null",
            "flag == true",
            "status.name == \"ACTIVE\"",
        ] {
            assert!(parse_condition(ok, JAVA).is_ok(), "{ok:?}: {:?}", parse_condition(ok, JAVA));
        }
    }

    #[test]
    fn what_used_to_be_refused_as_arithmetic_is_now_a_condition() {
        assert!(parse_condition("i + 1 == n", JAVA).unwrap().is_some());
        assert!(parse_condition("total % 100 == 0", JAVA).unwrap().is_some());
        assert!(parse_condition("status == Status.ACTIVE", JAVA).unwrap().is_some());
        assert!(parse_condition("items[i].price > 0", JAVA).unwrap().is_some());
        assert!(parse_condition("(a ? b : c) > 1", JAVA).unwrap().is_some());
    }

    #[test]
    fn what_it_refuses_it_refuses_by_name() {
        // Each of these is refused with the reason, not with "syntax error" — the message IS the
        // feature, because the alternative to understanding it is a breakpoint that never fires.
        assert!(refused("list.size() > 3").contains("calls a method"));
        assert!(refused("i = 5").contains("=="));
        assert!(refused("a & b").contains("&&"));
        assert!(refused("i >").contains("stops after an operator"));
        assert!(refused("(i > 1").contains("never closed"));
        assert!(refused("name == \"unclosed").contains("never closed"));
        assert!(refused("(Order) x == y").contains("cast"));
        assert!(refused("x instanceof Order").contains("instanceof"));
        assert!(refused("list.stream().count() > 1").contains("calls a method"));
        assert!(refused("i++ > 1").contains("increments"));
        assert!(refused("f -> f > 1").contains("lambda"));
        assert!(refused("flags & 4 == 4").contains("&&"));
    }

    #[test]
    fn a_constant_condition_is_refused_because_the_program_could_not_change_the_answer() {
        // Both shapes: a comparison of two literals, and a literal on its own. Either always stops
        // or never does, and neither would ever explain itself.
        assert!(refused("1 == 1").contains("read something"));
        assert!(refused("true").contains("constant"));
        assert!(refused("2 + 2 == 4").contains("constant"));
    }

    #[test]
    fn a_condition_that_can_only_be_a_number_is_refused() {
        assert!(refused("i + 1").contains("not a condition"));
        assert!(refused("-i").contains("not a condition"));
        assert!(refused("a * b").contains("not a condition"));
    }

    #[test]
    fn a_call_is_refused_with_the_languages_own_reason() {
        let rust = parse_condition("v.len() > 3", RUST).unwrap_err();
        assert!(rust.contains("in the binary"), "{rust}");
        assert!(parse_condition("*p > 3", RUST).is_ok());
    }

    #[test]
    fn the_constant_message_names_the_literal() {
        assert_eq!(parsed("flag"), Expr::Name("flag".into()));
        assert_eq!(
            constant_message(&Expr::Lit(Lit::Bool(true))),
            "`true` is a constant — a condition has to read something from the program"
        );
    }
}
