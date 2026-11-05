//! What a watch **path** is: `order`, `order.customer.name`, `items[2].price`, `*head` — in either
//! language.
//!
//! A path is the strict subset of an expression that can be *walked* by reading fields and elements
//! out of a stopped program, which both debuggers do exactly: the Rust watch walks the variables tree
//! with it, and mutation, logpoints and the assistant's read-only tools accept nothing wider.
//! [`crate::parser`] is the general expression parser; [`crate::ast::Expr::as_path`] tells the two
//! apart. Anything that is not a path is **refused by name** here rather than approximated — a watch
//! that quietly evaluates something adjacent to what you typed is the worst possible answer, because
//! you would trust the number.
//!
//! ## Where the `*` ends up
//!
//! In Rust, `*self.data` means `*(self.data)` — the star binds *looser* than the dots. So the leading
//! stars are parsed at the front and emitted at the **end** of the step list, which is the order they
//! are actually applied in. Writing them where they were typed would make `*head.next` read the
//! pointee's `next`, which is not what the same line means in the source file three panes away.

use crate::syntax::Syntax;
#[cfg(test)]
use crate::syntax::{JAVA, RUST};

/// One hop of a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A name: the first one is a variable, the rest are fields. `.0` on a tuple is a field whose
    /// name is `0`, which needs no special case.
    Field(String),
    /// A subscript.
    Index(i32),
    /// Read what this points at. Rust only — see the module docs on why it is last.
    Deref,
}



/// Split an expression into its path, or say why it is not one.
pub fn parse(expression: &str, syntax: Syntax) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    let mut name = String::new();
    let mut chars = expression.chars().peekable();

    // The prefix stars, counted here and pushed at the end — see the module docs.
    let mut derefs = 0usize;
    while chars.peek().is_some_and(|c| c.is_whitespace() || *c == '*') {
        let c = chars.next().unwrap();
        if c == '*' {
            if !syntax.deref {
                return Err(format!("`*` is not part of a watch — a watch is {}", syntax.shapes));
            }
            derefs += 1;
        }
    }

    // Whitespace around a `.` or a `[` is noise; whitespace *inside* a name is not. Without the
    // distinction `x as u64` parses as a variable called `xasu64` — a watch reading a plausible number
    // out of a variable that does not exist, which is the exact failure this module refuses to have.
    let mut gap = false;

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                gap |= !name.is_empty();
                continue;
            }
            // A `.` ends the step before it. What that step is depends on what came before: a
            // pending name (`order.total`), or a subscript that is already complete
            // (`items[2].price`). Only a dot with genuinely nothing in front of it — a leading one,
            // or a second in a row — is the error.
            '.' => {
                gap = false;
                match (name.is_empty(), steps.last()) {
                    (false, _) => steps.push(Step::Field(std::mem::take(&mut name))),
                    (true, Some(Step::Index(_))) => {}
                    (true, _) => return Err("an empty name in the path".to_string()),
                }
            }
            '[' => {
                gap = false;
                if !name.is_empty() {
                    steps.push(Step::Field(std::mem::take(&mut name)));
                }
                let mut digits = String::new();
                for d in chars.by_ref() {
                    if d == ']' {
                        break;
                    }
                    digits.push(d);
                }
                let at: i32 = digits
                    .trim()
                    .parse()
                    .map_err(|_| format!("`[{digits}]` is not an index"))?;
                steps.push(Step::Index(at));
            }
            c if c.is_alphanumeric() || c == '_' || c == '$' => {
                if gap {
                    return Err(format!(
                        "a watch is one path with no spaces in the middle — {}",
                        syntax.shapes
                    ));
                }
                name.push(c);
            }
            other => {
                return Err(format!(
                    "`{other}` is not part of a watch — a watch is {}",
                    syntax.shapes
                ))
            }
        }
    }
    if !name.is_empty() {
        steps.push(Step::Field(name));
    }
    if steps.is_empty() {
        return Err("an empty watch".to_string());
    }
    if !matches!(steps.first(), Some(Step::Field(_))) {
        return Err("a watch starts with a variable name".to_string());
    }
    steps.extend(std::iter::repeat_n(Step::Deref, derefs));
    Ok(steps)
}

/// `order.items[0].price`: a path as it was written. A trailing `*` is a prefix again, which is
/// where it was typed.
pub fn path_text(steps: &[Step]) -> String {
    let mut out = String::new();
    let mut derefs = 0usize;
    for step in steps {
        match step {
            Step::Field(name) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(name);
            }
            Step::Index(i) => out.push_str(&format!("[{i}]")),
            Step::Deref => derefs += 1,
        }
    }
    format!("{}{out}", "*".repeat(derefs))
}

/// Whether this expression is a path at all.
///
/// What lets a caller route: a path is walked by reading the stopped program, anything else has to be
/// handed to whatever evaluator the debugger has.
pub fn is_path(expression: &str, syntax: Syntax) -> bool {
    parse(expression, syntax).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> Step {
        Step::Field(name.to_string())
    }

    #[test]
    fn a_path_is_names_and_subscripts() {
        assert_eq!(parse("order", JAVA).unwrap(), vec![field("order")]);
        assert_eq!(
            parse("order.customer.name", JAVA).unwrap(),
            vec![field("order"), field("customer"), field("name")]
        );
        assert_eq!(
            parse("items[2].price", JAVA).unwrap(),
            vec![field("items"), Step::Index(2), field("price")]
        );
        // Whitespace is noise, not structure.
        assert_eq!(parse(" order . total ", JAVA).unwrap().len(), 2);
        // Two subscripts in a row, and a name after them.
        assert_eq!(
            parse("grid[1][2].label", JAVA).unwrap(),
            vec![field("grid"), Step::Index(1), Step::Index(2), field("label")]
        );
    }

    /// A tuple field needs no special case: `.0` is a field whose name happens to be a digit, which
    /// is exactly how both debuggers name it in the variables tree.
    #[test]
    fn a_rust_tuple_field_is_just_a_field() {
        assert_eq!(parse("pair.0", RUST).unwrap(), vec![field("pair"), field("0")]);
        assert_eq!(
            parse("point.0.x", RUST).unwrap(),
            vec![field("point"), field("0"), field("x")]
        );
    }

    /// The star binds looser than the dots, so it is applied last. `*head.next` in Rust is
    /// `*(head.next)`, and a watch that read the pointee's `next` instead would disagree with the
    /// source file in the next pane.
    #[test]
    fn a_leading_star_is_applied_after_the_whole_path() {
        assert_eq!(parse("*p", RUST).unwrap(), vec![field("p"), Step::Deref]);
        assert_eq!(
            parse("*head.next", RUST).unwrap(),
            vec![field("head"), field("next"), Step::Deref]
        );
        assert_eq!(
            parse("**pp", RUST).unwrap(),
            vec![field("pp"), Step::Deref, Step::Deref]
        );
        assert_eq!(parse("  * p ", RUST).unwrap(), vec![field("p"), Step::Deref]);
    }

    /// Java has no prefix operator a watch could honour, so the same input is refused there — by
    /// name, with the shapes that do work.
    #[test]
    fn a_star_is_refused_in_java_and_says_what_a_watch_is() {
        let err = parse("*p", JAVA).unwrap_err();
        assert!(err.contains('*'), "{err}");
        assert!(err.contains("`.field`"), "the message says what does work: {err}");
        assert!(parse("*p", RUST).is_ok(), "and is a path in Rust");
    }

    /// A dot with nothing in front of it. The check exists for these two and not for the
    /// `items[2].price` it used to reject along with them.
    #[test]
    fn a_dot_with_nothing_before_it_is_refused() {
        assert!(parse(".order", JAVA).is_err());
        assert!(parse("order..total", JAVA).is_err());
    }

    /// The one that would have been silent: with whitespace treated as pure noise, `x as u64` parses
    /// as a variable named `xasu64`. Refusing it is the difference between "no such variable" and a
    /// plausible number read out of the wrong place.
    #[test]
    fn a_space_in_the_middle_of_a_name_is_not_noise() {
        assert!(parse("x as u64", RUST).is_err());
        assert!(parse("a b", JAVA).is_err());
        assert!(parse("order as int", JAVA).is_err());
        // …and the whitespace that IS noise still is.
        assert_eq!(parse(" order . total ", JAVA).unwrap().len(), 2);
        assert_eq!(parse("items [ 2 ] . price", JAVA).unwrap().len(), 3);
    }

    #[test]
    fn anything_that_is_not_a_path_is_refused_rather_than_approximated() {
        for bad in ["a + b", "list.get(0)", "(Order) x", "", "[0]", "a..b", "a[x]"] {
            assert!(parse(bad, JAVA).is_err(), "{bad:?} is not a path");
            assert!(parse(bad, RUST).is_err(), "{bad:?} is not a path");
        }
    }

    /// What the routing turns on, so it is pinned: a Rust expression that needs a real evaluator is
    /// not a path, and one that does not is.
    #[test]
    fn is_path_tells_a_walk_from_an_evaluation() {
        for path in ["v", "v[0]", "self.inner.len", "*self.head", "t.0.1"] {
            assert!(is_path(path, RUST), "{path}");
        }
        for expression in ["v.len()", "a + b", "v.iter().count()", "x as u64", "foo!(x)"] {
            assert!(!is_path(expression, RUST), "{expression}");
        }
    }
}
