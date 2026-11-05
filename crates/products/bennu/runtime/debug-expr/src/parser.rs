//! Tokens → [`Expr`], by precedence climbing.
//!
//! ```text
//!   expr      := ternary
//!   ternary   := or ( "?" expr ":" ternary )?          -- Java only
//!   or        := and ( "||" and )*
//!   and       := equality ( "&&" equality )*
//!   equality  := relational ( ("==" | "!=") relational )*
//!   relational:= additive ( ("<" | "<=" | ">" | ">=") additive )*
//!   additive  := multiplicative ( ("+" | "-") multiplicative )*
//!   multiplicative := unary ( ("*" | "/" | "%") unary )*
//!   unary     := ("!" | "-") unary | "*" unary (Rust) | postfix
//!   postfix   := primary ( "." name | "[" expr "]" | "(" args ")" )*
//!   primary   := literal | name | "(" expr ")"
//! ```
//!
//! What is *not* here is refused by name, with the reason: a cast, `instanceof`, `new`, `as`, a
//! lambda, an assignment. A call **is** here — it parses to [`Expr::Call`] so that it can be
//! explained rather than reported as a stray parenthesis.

use crate::ast::{BinOp, Expr, UnOp};
use crate::lexer::{lex, Tok};
use crate::syntax::{Lang, Syntax};

/// Parentheses and unary operators nested deeper than this are refused: a recursive-descent parser
/// on hostile input is a stack overflow in a debugger's backend, and nobody types 64 levels.
const MAX_DEPTH: usize = 64;

/// Parse an expression, or say what is wrong with it in one line.
pub fn parse(text: &str, syntax: Syntax) -> Result<Expr, String> {
    if text.trim().is_empty() {
        return Err("there is nothing to evaluate".to_string());
    }
    let toks = lex(text, syntax)?;
    let mut parser = Parser { toks, at: 0, syntax, depth: 0 };
    let expr = parser.expr()?;
    match parser.peek() {
        None => Ok(expr),
        Some(t) => Err(leftover(t, syntax)),
    }
}

/// Why the token after a complete expression cannot be there. Almost always an operator or a
/// keyword this grammar does not have, so the common ones are named.
fn leftover(tok: &Tok, syntax: Syntax) -> String {
    match tok {
        Tok::Ident(w) if w == "instanceof" && syntax.lang == Lang::Java => {
            "`instanceof` is not evaluated — compare a field of the object instead".to_string()
        }
        Tok::Ident(w) if w == "as" && syntax.lang == Lang::Rust => {
            "an `as` cast is not evaluated here — compare the value itself".to_string()
        }
        Tok::RParen => "a `)` has no `(` to close".to_string(),
        Tok::RBracket => "a `]` has no `[` to close".to_string(),
        Tok::Colon => "a `:` has no `?` before it".to_string(),
        other => format!("`{}` is not something an expression can contain here", other.text()),
    }
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
    syntax: Syntax,
    depth: usize,
}

type Step = fn(&mut Parser) -> Result<Expr, String>;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.at += 1;
            return true;
        }
        false
    }

    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        if self.depth >= MAX_DEPTH {
            return Err("that is nested too deeply to be an expression anyone wrote".to_string());
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn expr(&mut self) -> Result<Expr, String> {
        self.nested(Self::ternary)
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let cond = self.or()?;
        if self.peek() != Some(&Tok::Question) {
            return Ok(cond);
        }
        if self.syntax.lang == Lang::Rust {
            return Err(
                "Rust has no `?:`, and a trailing `?` returns from a function, which a watch \
                 cannot do"
                    .to_string(),
            );
        }
        self.at += 1;
        let then = self.expr()?;
        if !self.eat(&Tok::Colon) {
            return Err("a `?` needs its `:`".to_string());
        }
        let otherwise = self.nested(Self::ternary)?;
        Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }

    fn or(&mut self) -> Result<Expr, String> {
        self.left_assoc(&[(Tok::OrOr, BinOp::Or)], Self::and)
    }

    fn and(&mut self) -> Result<Expr, String> {
        self.left_assoc(&[(Tok::AndAnd, BinOp::And)], Self::equality)
    }

    fn equality(&mut self) -> Result<Expr, String> {
        self.left_assoc(&[(Tok::EqEq, BinOp::Eq), (Tok::Ne, BinOp::Ne)], Self::relational)
    }

    fn relational(&mut self) -> Result<Expr, String> {
        self.left_assoc(
            &[
                (Tok::Lt, BinOp::Lt),
                (Tok::Le, BinOp::Le),
                (Tok::Gt, BinOp::Gt),
                (Tok::Ge, BinOp::Ge),
            ],
            Self::additive,
        )
    }

    fn additive(&mut self) -> Result<Expr, String> {
        self.left_assoc(&[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)], Self::multiplicative)
    }

    fn multiplicative(&mut self) -> Result<Expr, String> {
        self.left_assoc(
            &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)],
            Self::unary,
        )
    }

    /// One precedence level: `next ( op next )*`, folded to the left.
    fn left_assoc(&mut self, ops: &[(Tok, BinOp)], next: Step) -> Result<Expr, String> {
        let mut left = next(self)?;
        'level: loop {
            for (tok, op) in ops {
                if self.peek() == Some(tok) {
                    self.at += 1;
                    let right = next(self)?;
                    left = Expr::Binary(*op, Box::new(left), Box::new(right));
                    continue 'level;
                }
            }
            return Ok(left);
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.nested(|p| match p.peek() {
            Some(Tok::Bang) => {
                p.at += 1;
                Ok(Expr::Unary(UnOp::Not, Box::new(p.unary()?)))
            }
            Some(Tok::Minus) => {
                p.at += 1;
                Ok(Expr::Unary(UnOp::Neg, Box::new(p.unary()?)))
            }
            Some(Tok::Star) if p.syntax.deref => {
                p.at += 1;
                Ok(Expr::Deref(Box::new(p.unary()?)))
            }
            Some(Tok::Star) => Err(
                "`*` needs a number on its left — Java has no dereference operator to put in \
                 front of a value"
                    .to_string(),
            ),
            _ => p.postfix(),
        })
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::Dot) => {
                    self.at += 1;
                    match self.peek().cloned() {
                        Some(Tok::Ident(name)) => {
                            self.at += 1;
                            expr = Expr::Field(Box::new(expr), name);
                        }
                        _ => return Err("a `.` needs a field name after it".to_string()),
                    }
                }
                Some(Tok::LBracket) => {
                    self.at += 1;
                    let index = self.expr()?;
                    if !self.eat(&Tok::RBracket) {
                        return Err("a `[` is never closed".to_string());
                    }
                    expr = Expr::Index(Box::new(expr), Box::new(index));
                }
                Some(Tok::LParen) => {
                    self.at += 1;
                    let args = self.arguments()?;
                    expr = Expr::Call { callee: Box::new(expr), args };
                }
                _ => return Ok(expr),
            }
        }
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        if self.eat(&Tok::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            if self.eat(&Tok::Comma) {
                continue;
            }
            if self.eat(&Tok::RParen) {
                return Ok(args);
            }
            return Err("a `(` is never closed".to_string());
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Lit(lit)) => {
                self.at += 1;
                Ok(Expr::Lit(lit))
            }
            Some(Tok::Ident(word)) if word == "new" && self.syntax.lang == Lang::Java => Err(
                "`new` is not evaluated — a constructor would run application code inside the \
                 paused program"
                    .to_string(),
            ),
            Some(Tok::Ident(name)) => {
                self.at += 1;
                Ok(Expr::Name(name))
            }
            Some(Tok::LParen) => {
                self.at += 1;
                let inner = self.expr()?;
                if !self.eat(&Tok::RParen) {
                    return Err("a `(` is never closed".to_string());
                }
                self.reject_cast(&inner)?;
                Ok(inner)
            }
            Some(other) => Err(format!("`{}` is not something to evaluate", other.text())),
            None => Err("the expression stops after an operator".to_string()),
        }
    }

    /// `(Order) x`, `(int) total`: a parenthesised name directly followed by an operand. Without
    /// this the cast would be reported as a stray identifier, which explains nothing.
    fn reject_cast(&self, inner: &Expr) -> Result<(), String> {
        if self.syntax.lang != Lang::Java || inner.qualified_names().is_none() {
            return Ok(());
        }
        let operand_follows = matches!(
            self.peek(),
            Some(Tok::Ident(_) | Tok::Lit(_) | Tok::LParen | Tok::Bang)
        );
        if operand_follows {
            return Err(format!(
                "a cast `({inner}) …` is not evaluated — compare the value itself, which already \
                 has its real type"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Lit;
    use crate::syntax::{JAVA, RUST};

    fn java(text: &str) -> Expr {
        parse(text, JAVA).unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
    }

    fn refused(text: &str, syntax: Syntax) -> String {
        match parse(text, syntax) {
            Err(e) => e,
            Ok(other) => panic!("{text:?} should have been refused, got {other:?}"),
        }
    }

    fn name(n: &str) -> Box<Expr> {
        Box::new(Expr::Name(n.to_string()))
    }

    fn int(v: i32) -> Box<Expr> {
        Box::new(Expr::Lit(Lit::Int(v)))
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        // `a + b * c` is `a + (b * c)`.
        assert_eq!(
            java("a + b * c"),
            Expr::Binary(
                BinOp::Add,
                name("a"),
                Box::new(Expr::Binary(BinOp::Mul, name("b"), name("c")))
            )
        );
        // …and subtraction folds to the left: `a - b - c` is `(a - b) - c`.
        assert_eq!(
            java("a - b - c"),
            Expr::Binary(
                BinOp::Sub,
                Box::new(Expr::Binary(BinOp::Sub, name("a"), name("b"))),
                name("c")
            )
        );
    }

    #[test]
    fn and_binds_tighter_than_or_and_comparison_tighter_than_both() {
        let Expr::Binary(BinOp::Or, left, right) = java("a || b && c > 1") else {
            panic!("an or at the top")
        };
        assert_eq!(*left, Expr::Name("a".into()));
        let Expr::Binary(BinOp::And, _, cmp) = *right else { panic!("an and under it") };
        assert!(matches!(*cmp, Expr::Binary(BinOp::Gt, ..)));
    }

    #[test]
    fn parentheses_override_the_precedence() {
        let Expr::Binary(BinOp::Mul, left, _) = java("(a + b) * c") else { panic!("a product") };
        assert!(matches!(*left, Expr::Binary(BinOp::Add, ..)));
    }

    #[test]
    fn the_ternary_is_right_associative_and_loosest() {
        let Expr::Ternary(cond, _, otherwise) = java("a > 1 ? 10 : b > 2 ? 20 : 30") else {
            panic!("a ternary")
        };
        assert!(matches!(*cond, Expr::Binary(BinOp::Gt, ..)));
        assert!(matches!(*otherwise, Expr::Ternary(..)));
        assert!(refused("a ? 1", JAVA).contains("`:`"));
    }

    #[test]
    fn a_path_is_names_fields_and_subscripts() {
        assert_eq!(
            java("order.items[2].price"),
            Expr::Field(
                Box::new(Expr::Index(
                    Box::new(Expr::Field(name("order"), "items".into())),
                    int(2)
                )),
                "price".into()
            )
        );
        // A subscript may itself be an expression.
        assert!(matches!(java("items[i + 1]"), Expr::Index(..)));
    }

    #[test]
    fn a_star_in_front_applies_after_the_whole_path_in_rust() {
        // `*head.next` is `*(head.next)`.
        assert_eq!(
            parse("*head.next", RUST).unwrap(),
            Expr::Deref(Box::new(Expr::Field(name("head"), "next".into())))
        );
        assert_eq!(parse("a * b", RUST).unwrap().to_string(), "a * b", "between operands it multiplies");
        assert!(refused("*p", JAVA).contains('*'));
    }

    #[test]
    fn a_rust_tuple_field_is_a_field() {
        assert_eq!(
            parse("point.0.x", RUST).unwrap(),
            Expr::Field(Box::new(Expr::Field(name("point"), "0".into())), "x".into())
        );
    }

    #[test]
    fn a_call_parses_so_it_can_be_explained() {
        let Expr::Call { callee, args } = java("list.size()") else { panic!("a call") };
        assert_eq!(callee.to_string(), "list.size");
        assert!(args.is_empty());
        let with_args = java("map.get(key, 1)");
        let reason = with_args.refusal(JAVA).expect("refused");
        assert!(reason.contains("calls a method") && reason.contains("map.get"), "{reason}");
        let rust = parse("v.len()", RUST).unwrap().refusal(RUST).expect("refused");
        assert!(rust.contains("in the binary"), "{rust}");
        // A call buried in an operand is found too.
        assert!(java("x > list.size() + 1").refusal(JAVA).is_some());
        assert!(java("x > 1").refusal(JAVA).is_none());
    }

    #[test]
    fn the_shapes_that_are_refused_say_what_they_are() {
        assert!(refused("(Order) x", JAVA).contains("cast"));
        assert!(refused("(int) total", JAVA).contains("cast"));
        assert!(refused("x instanceof Order", JAVA).contains("instanceof"));
        assert!(refused("new Order()", JAVA).contains("`new`"));
        assert!(refused("x as u64", RUST).contains("`as`"));
        assert!(refused("a ? b : c", RUST).contains("`?:`") || refused("a ? b : c", RUST).contains("Rust has no"));
        assert!(refused("i >", JAVA).contains("stops after an operator"));
        assert!(refused("(i > 1", JAVA).contains("never closed"));
        assert!(refused("a b", JAVA).contains("`b`"));
        assert!(refused("a)", JAVA).contains("no `(`"));
        assert!(refused("a.", JAVA).contains("field name"));
        assert!(refused("", JAVA).contains("nothing"));
    }

    #[test]
    fn a_parenthesised_name_is_not_a_cast_unless_an_operand_follows() {
        assert!(parse("(a) + b", JAVA).is_ok());
        assert!(parse("(a)", JAVA).is_ok());
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = format!("{}a{}", "(".repeat(200), ")".repeat(200));
        assert!(refused(&deep, JAVA).contains("nested too deeply"));
        let ok = format!("{}a{}", "(".repeat(20), ")".repeat(20));
        assert!(parse(&ok, JAVA).is_ok());
    }

    #[test]
    fn the_expression_reads_back_as_source() {
        for text in ["order.items[2].price", "(a + b) * c", "!done", "a ? b : c", "-x"] {
            assert_eq!(java(text).to_string(), text);
        }
        assert_eq!(java("s == \"x\"").to_string(), "s == \"x\"");
    }

    #[test]
    fn a_pure_path_is_told_from_an_expression() {
        assert!(java("order.items[2].price").as_path().is_some());
        assert!(java("items[-1]").as_path().is_some());
        assert!(java("items[i]").as_path().is_none());
        assert!(java("a + b").as_path().is_none());
        assert!(parse("*self.next", RUST).unwrap().as_path().is_some());
    }

    #[test]
    fn what_an_expression_reads_is_listed_as_paths() {
        let e = java("order.total > 100 && order.items[0].qty != 0 || items[i].p < limit");
        let texts: Vec<String> = e.paths().iter().map(|p| p.to_string()).collect();
        assert_eq!(
            texts,
            vec!["order.total", "order.items[0].qty", "items", "i", "limit"],
            "a subscript that is itself read splits the path around it"
        );
    }
}
