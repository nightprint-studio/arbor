//! A Java breakpoint condition: an expression of the shared language that reads the stopped program
//! and answers yes or no.
//!
//! The language itself — lexer, precedence parser, evaluator, the numeric rules — is
//! `bennu-debug-expr`; what a Java condition *is* is decided there ([`parse_condition`]): it has to
//! read the program, it has to be a boolean, and everything outside the language is refused **by
//! name** before the debugger ever runs (the frontend checks it while you type, through
//! `bennu_debug_check_condition`). This module is the glue: parse once when the breakpoint is
//! installed, evaluate at each hit over [`JdwpTarget`], and list what the condition read.
//!
//! ## Why a condition is held to a stricter standard than a watch
//!
//! A watch that quietly answers something adjacent to what you typed shows a wrong number, which you
//! might notice. A **condition** that does it swallows the stop — the program runs past the line you
//! were waiting for, and there is nothing on screen at all. So a method call (`list.size() > 3`
//! would run application code inside a paused program), a cast, a lambda, an assignment are
//! refused as you type, not approximated.
//!
//! ```text
//!   i > 5 && order.customer.name == "acme"
//!   i + 1 == n            total % 100 == 0
//!   status == Status.ACTIVE            items[i].price > 0
//!   order != null && order.total > 100     // && short-circuits: the left guards the right
//! ```
//!
//! ## What happens when it goes wrong at runtime
//!
//! It **stops**, and says why. A condition that cannot be evaluated — a null halfway down the path,
//! a field that is not there in this subclass, a division by zero — is a bug in the condition, and
//! the only way to see it is to be standing there. Silently continuing would turn a typo into a
//! breakpoint that never fires and never explains itself, which is the exact failure this module
//! exists to prevent.

use bennu_debug_expr::prelude::{eval_condition, parse_condition, Expr, JAVA};
use bennu_jdwp::prelude::{Frame, Id};

use crate::debug::Session;
use crate::debug_jdwp_target::{self, JdwpTarget};

/// A condition, parsed.
pub(crate) type Cond = Expr;

/// Parse a condition, or say what is wrong with it in one line.
///
/// An empty (or whitespace-only) condition is `None` rather than an error: that is what "no
/// condition" is written as everywhere else in the product, and refusing it would make clearing a
/// condition impossible.
pub(crate) fn parse(text: &str) -> Result<Option<Cond>, String> {
    parse_condition(text, JAVA)
}

/// Whether the condition holds in this frame. `Err` is a condition that could not be answered —
/// which the caller turns into a stop, not into a skip. See the module doc.
pub(crate) fn holds(
    session: &Session,
    thread: Id,
    frame: &Frame,
    cond: &Cond,
) -> Result<bool, String> {
    eval_condition(cond, &JdwpTarget::new(session, thread, *frame), JAVA)
}

/// What the paths a condition mentions read in this frame — `total = 250, limit = 100` — for the stop the
/// condition let through, so a stop it should not have allowed is visible at once. A path that cannot be read
/// says why in place of its value.
pub(crate) fn describe_values(session: &Session, thread: Id, frame: &Frame, cond: &Cond) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut items: Vec<String> = Vec::new();
    for read in cond.paths() {
        let text = read.to_string();
        if seen.contains(&text) {
            continue;
        }
        let value = debug_jdwp_target::read(session, thread, frame, read, &text)
            .map(|row| row.value)
            .unwrap_or_else(|why| format!("<{why}>"));
        items.push(format!("{text} = {value}"));
        seen.push(text);
    }
    items.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(text: &str) -> String {
        match parse(text) {
            Err(e) => e,
            Ok(other) => panic!("{text:?} should have been refused, got {other:?}"),
        }
    }

    #[test]
    fn no_condition_is_not_an_error() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("   \t "), Ok(None));
    }

    #[test]
    fn every_shape_the_older_grammar_accepted_is_still_accepted() {
        for ok in [
            "i > 5",
            "count <= -3",
            "ratio < 0.5",
            "n == 10L",
            "mask == 0xFF",
            "order.customer.name == \"acme\"",
            "items[2].price > 0",
            "c == 'x'",
            "0 < i",
            "done",
            "!order.paid",
            "a || b && c",
            "(a || b) && c",
            "order != null",
            "flag == true",
            "status.name == \"ACTIVE\"",
        ] {
            assert!(matches!(parse(ok), Ok(Some(_))), "{ok:?}: {:?}", parse(ok));
        }
    }

    #[test]
    fn what_the_older_grammar_refused_as_beyond_it_now_works() {
        for ok in [
            "i + 1 == n",
            "total % 100 == 0",
            "status == Status.ACTIVE",
            "items[i].price > 0",
            "name + \"!\" == \"a!\"",
            "items.length > 3",
        ] {
            assert!(matches!(parse(ok), Ok(Some(_))), "{ok:?}: {:?}", parse(ok));
        }
    }

    #[test]
    fn what_it_refuses_it_refuses_by_name() {
        // Each of these is refused with the reason, not with "syntax error" — the message IS the
        // feature, because the alternative to understanding it is a breakpoint that never fires.
        assert!(refused("list.size() > 3").contains("calls a method"));
        assert!(refused("list.get(0).id == 1").contains("calls a method"));
        assert!(refused("i = 5").contains("=="));
        assert!(refused("a & b").contains("&&"));
        assert!(refused("i >").contains("stops after an operator"));
        assert!(refused("(i > 1").contains("never closed"));
        assert!(refused("name == \"unclosed").contains("never closed"));
        assert!(refused("(Order) x == y").contains("cast"));
        assert!(refused("x instanceof Order").contains("instanceof"));
        assert!(refused("new Order().id == 1").contains("`new`"));
        assert!(refused("i++ > 1").contains("increments"));
        assert!(refused("x -> x > 1").contains("lambda"));
    }

    #[test]
    fn a_constant_condition_is_refused_because_the_program_could_not_change_the_answer() {
        assert!(refused("1 == 1").contains("read something"));
        assert!(refused("true").contains("constant"));
        assert!(refused("i + 1").contains("not a condition"));
    }

    #[test]
    fn the_paths_of_a_condition_are_listed_as_written() {
        let cond = parse("order.total > 100 && order.items[0].qty != 0 || order.total < 5")
            .unwrap()
            .unwrap();
        let texts: Vec<String> = cond.paths().iter().map(|p| p.to_string()).collect();
        assert_eq!(
            texts,
            vec!["order.total", "order.items[0].qty", "order.total"],
            "duplicates are dropped later, by the caller"
        );
    }
}
