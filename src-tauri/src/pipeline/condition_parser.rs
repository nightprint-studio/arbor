//! Recursive-descent parser for the free-form `if_block` condition syntax.
//!
//! ```text
//! expression  ::= or_expr
//! or_expr     ::= and_expr ('||' and_expr)*
//! and_expr    ::= not_expr ('&&' not_expr)*
//! not_expr    ::= '!' not_expr | primary
//! primary     ::= '(' expression ')' | call | comparison
//! call        ::= IDENT '(' (expression (',' expression)*)? ')'
//! comparison  ::= operand (op operand)?       // bare operand → truthy
//! op          ::= '==' | '!=' | '>=' | '<=' | '>' | '<' | '=~' | '~~'
//! operand     ::= '${' VAR_NAME (':-' fallback)? '}'
//!               | STRING                              // "..."
//!               | NUMBER
//!               | IDENT                               // bare ident → string literal
//! ```
//!
//! Built-in functions: `defined(name)`, `empty(value)`, `matches(value, "regex")`,
//! `i_eq(a, b)` (case-insensitive equality), `truthy(value)`.
//!
//! Examples:
//! ```text
//! ${has_pom}
//! ${has_pom} && !${skip_build}
//! ${count} > 10
//! ${os} == "windows" || ${os} == "darwin"
//! (${count} >= 1 && ${count} <= 10) || ${force}
//! ${ver} =~ "^3\."
//! defined(maven_version) && ${maven_version} ~~ "3."
//! ```

use super::condition::{CompareOp, Condition};

/// Parse a free-form condition expression. Returns a structured `Condition`
/// tree that the runtime evaluates without further parsing.
pub fn parse(input: &str) -> std::result::Result<Condition, String> {
    let mut p = Parser::new(input);
    let cond = p.parse_expression()?;
    p.skip_ws();
    if p.pos < p.src.len() {
        return Err(format!(
            "unexpected trailing input at offset {}: {:?}",
            p.pos,
            &p.src[p.pos..(p.pos + 20).min(p.src.len())]
        ));
    }
    Ok(cond)
}

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { src: input.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }
    #[allow(dead_code)]
    fn at_end(&self) -> bool      { self.pos >= self.src.len() }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// If the next non-ws bytes match `lit`, consume them and return true.
    fn eat(&mut self, lit: &str) -> bool {
        self.skip_ws();
        let bytes = lit.as_bytes();
        if self.src.len() - self.pos >= bytes.len()
            && &self.src[self.pos..self.pos + bytes.len()] == bytes
        {
            self.pos += bytes.len();
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> std::result::Result<T, String> {
        Err(format!("at offset {}: {}", self.pos, msg.into()))
    }

    // ---------------- Grammar productions ----------------

    fn parse_expression(&mut self) -> std::result::Result<Condition, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> std::result::Result<Condition, String> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if !self.eat("||") { break; }
            let right = self.parse_and()?;
            left = match left {
                Condition::AnyOf { mut conditions } => {
                    conditions.push(right);
                    Condition::AnyOf { conditions }
                }
                other => Condition::AnyOf { conditions: vec![other, right] },
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> std::result::Result<Condition, String> {
        let mut left = self.parse_not()?;
        loop {
            self.skip_ws();
            if !self.eat("&&") { break; }
            let right = self.parse_not()?;
            left = match left {
                Condition::AllOf { mut conditions } => {
                    conditions.push(right);
                    Condition::AllOf { conditions }
                }
                other => Condition::AllOf { conditions: vec![other, right] },
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> std::result::Result<Condition, String> {
        self.skip_ws();
        // `!=` is part of a comparison, not a unary `!`. Look ahead.
        if self.peek() == Some(b'!') && self.src.get(self.pos + 1) != Some(&b'=') {
            self.pos += 1;
            let inner = self.parse_not()?;
            return Ok(Condition::Not { condition: Box::new(inner) });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> std::result::Result<Condition, String> {
        self.skip_ws();
        if self.eat("(") {
            let inner = self.parse_expression()?;
            self.skip_ws();
            if !self.eat(")") {
                return self.err("expected ')'");
            }
            return Ok(inner);
        }
        // Try function call OR bare identifier-as-comparison-operand.
        // We need to know whether the upcoming token is an IDENT followed by
        // '(' (call) or otherwise (operand).
        let save = self.pos;
        if let Some(name) = self.peek_ident() {
            // peek_ident leaves pos at start; commit:
            self.pos = save;
            self.read_ident();
            self.skip_ws();
            if self.peek() == Some(b'(') {
                return self.parse_call(name);
            }
            // Wasn't a call — rewind and treat as operand-led comparison.
            self.pos = save;
        }
        self.parse_comparison()
    }

    /// Parse a function call. The `name` was already consumed by the caller.
    fn parse_call(&mut self, name: String) -> std::result::Result<Condition, String> {
        if !self.eat("(") {
            return self.err(format!("expected '(' after function name '{name}'"));
        }
        let mut args: Vec<String> = Vec::new();   // raw operand strings for ops that take string args
        let cond_args: Vec<Condition> = Vec::new(); // for ops that take conditions

        // For most built-ins we want STRING operands (defined/empty/matches/i_eq).
        // Parse a comma-separated operand list. Operands can be ${var} / "str"
        // / NUMBER / bare IDENT (treated as string).
        self.skip_ws();
        if !self.eat(")") {
            loop {
                self.skip_ws();
                let arg = self.read_operand_string()?;
                args.push(arg);
                self.skip_ws();
                if self.eat(",") { continue; }
                if self.eat(")") { break; }
                return self.err("expected ',' or ')' in function arg list");
            }
        }
        let _ = cond_args; // reserved for future condition-typed args

        match name.as_str() {
            "defined" => {
                if args.len() != 1 { return self.err("defined() takes exactly 1 argument"); }
                // Strip ${...} wrapper if present so the user can write defined(${x}) or defined(x).
                let var = strip_var_ref(&args[0]).unwrap_or(args[0].clone());
                Ok(Condition::Defined { var })
            }
            "empty" => {
                if args.len() != 1 { return self.err("empty() takes exactly 1 argument"); }
                Ok(Condition::Empty { value: args.into_iter().next().unwrap() })
            }
            "truthy" => {
                if args.len() != 1 { return self.err("truthy() takes exactly 1 argument"); }
                Ok(Condition::Truthy { value: args.into_iter().next().unwrap() })
            }
            "matches" => {
                if args.len() != 2 { return self.err("matches() takes exactly 2 arguments (value, regex)"); }
                let mut it = args.into_iter();
                let left  = it.next().unwrap();
                let right = it.next().unwrap();
                Ok(Condition::Compare { left, op: CompareOp::Matches, right })
            }
            "i_eq" => {
                if args.len() != 2 { return self.err("i_eq() takes exactly 2 arguments"); }
                let mut it = args.into_iter();
                let left  = it.next().unwrap();
                let right = it.next().unwrap();
                Ok(Condition::Compare { left, op: CompareOp::IEq, right })
            }
            other => self.err(format!("unknown function '{other}'")),
        }
    }

    /// Comparison: operand (op operand)?  — bare operand becomes truthy().
    fn parse_comparison(&mut self) -> std::result::Result<Condition, String> {
        let left = self.read_operand_string()?;
        self.skip_ws();
        // Look for a comparison operator. Order matters: try multi-char first.
        let op = if      self.eat("==") { Some(CompareOp::Eq) }
                 else if self.eat("!=") { Some(CompareOp::Ne) }
                 else if self.eat(">=") { Some(CompareOp::Gte) }
                 else if self.eat("<=") { Some(CompareOp::Lte) }
                 else if self.eat("=~") { Some(CompareOp::Matches) }
                 else if self.eat("~~") { Some(CompareOp::Contains) }
                 else if self.eat(">")  { Some(CompareOp::Gt) }
                 else if self.eat("<")  { Some(CompareOp::Lt) }
                 else { None };
        if let Some(op) = op {
            let right = self.read_operand_string()?;
            Ok(Condition::Compare { left, op, right })
        } else {
            Ok(Condition::Truthy { value: left })
        }
    }

    // ---------------- Operand reader ----------------

    /// Read an operand and return its STRING representation (preserving
    /// `${var}` references so `vars::resolve_vars` substitutes them at
    /// evaluation time). Numbers / quoted strings / bare idents all flatten
    /// to their string content.
    fn read_operand_string(&mut self) -> std::result::Result<String, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'$') => self.read_var_ref(),
            Some(b'"') => self.read_quoted_string(),
            Some(b'\'') => self.read_quoted_string(), // tolerate single-quotes for ergonomics
            Some(c) if c == b'-' || c.is_ascii_digit() => self.read_number(),
            Some(c) if is_ident_start(c) => Ok(self.read_ident()),
            _ => self.err("expected operand"),
        }
    }

    /// Returns the LITERAL `${name}` or `${name:-fallback}` text — vars are
    /// not substituted here; the evaluator does it. Caller checks bracket
    /// balance.
    fn read_var_ref(&mut self) -> std::result::Result<String, String> {
        if self.peek() != Some(b'$') { return self.err("expected '$'"); }
        let start = self.pos;
        self.pos += 1;
        if self.peek() != Some(b'{') {
            return self.err("expected '${' (bare $ not allowed in operand)");
        }
        self.pos += 1;
        // Read until '}', allowing a single ':-' fallback inside.
        while let Some(c) = self.peek() {
            if c == b'}' {
                self.pos += 1;
                return Ok(std::str::from_utf8(&self.src[start..self.pos])
                    .map_err(|e| e.to_string())?.to_string());
            }
            self.pos += 1;
        }
        self.err("unterminated '${...}' reference")
    }

    fn read_quoted_string(&mut self) -> std::result::Result<String, String> {
        let quote = self.bump().ok_or_else(|| "expected quote".to_string())?;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == quote {
                self.pos += 1;
                return Ok(out);
            }
            if c == b'\\' {
                self.pos += 1;
                match self.bump() {
                    Some(b'n')  => out.push('\n'),
                    Some(b't')  => out.push('\t'),
                    Some(b'r')  => out.push('\r'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'"')  => out.push('"'),
                    Some(b'\'') => out.push('\''),
                    Some(c)     => { out.push('\\'); out.push(c as char); }
                    None        => return self.err("unterminated escape in string"),
                }
                continue;
            }
            out.push(c as char);
            self.pos += 1;
        }
        self.err("unterminated string literal")
    }

    fn read_number(&mut self) -> std::result::Result<String, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') { self.pos += 1; }
        let mut saw_digit = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' {
                saw_digit |= c.is_ascii_digit();
                self.pos += 1;
            } else { break; }
        }
        if !saw_digit { return self.err("expected digit"); }
        Ok(std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string())
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_ident_continue(c) { self.pos += 1; } else { break; }
        }
        std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string()
    }

    /// Lookahead-only: returns the IDENT at the current position without
    /// consuming. Returns `None` when the next non-ws byte isn't an ident
    /// starter.
    fn peek_ident(&self) -> Option<String> {
        let mut i = self.pos;
        while i < self.src.len() {
            let c = self.src[i];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' { i += 1; continue; }
            if !is_ident_start(c) { return None; }
            let start = i;
            while i < self.src.len() && is_ident_continue(self.src[i]) { i += 1; }
            return Some(std::str::from_utf8(&self.src[start..i]).ok()?.to_string());
        }
        None
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}
fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Strip a surrounding `${...}` wrapper, returning the inner name. Used by
/// `defined()` so users can write `defined(x)` or `defined(${x})`
/// interchangeably.
fn strip_var_ref(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with("${") && s.ends_with('}') {
        Some(s[2 .. s.len()-1].to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vars::{RunContext, VarValue};

    fn eval(expr: &str, set: &[(&str, VarValue)]) -> bool {
        let mut ctx = RunContext::new();
        for (k, v) in set { ctx.set(*k, v.clone()); }
        let cond = parse(expr).expect("parse");
        super::super::condition::evaluate(&cond, &ctx)
    }

    #[test]
    fn truthy_var() {
        assert!( eval("${flag}",   &[("flag", VarValue::Bool(true))]));
        assert!(!eval("${flag}",   &[("flag", VarValue::Bool(false))]));
    }

    #[test]
    fn compare_eq_string() {
        assert!( eval("${os} == \"windows\"", &[("os", VarValue::String("windows".into()))]));
        assert!(!eval("${os} == \"linux\"",   &[("os", VarValue::String("windows".into()))]));
    }

    #[test]
    fn numeric_compare() {
        assert!( eval("${count} > 10", &[("count", VarValue::Number(12.0))]));
        assert!(!eval("${count} > 10", &[("count", VarValue::Number(5.0))]));
        assert!( eval("${count} >= 10", &[("count", VarValue::Number(10.0))]));
    }

    #[test]
    fn and_or_precedence() {
        // `a && b || c && d` == `(a && b) || (c && d)`
        assert!( eval(
            "${a} && ${b} || ${c} && ${d}",
            &[("a", VarValue::Bool(true)),
              ("b", VarValue::Bool(true)),
              ("c", VarValue::Bool(false)),
              ("d", VarValue::Bool(true))],
        ));
        assert!(!eval(
            "${a} && ${b} || ${c} && ${d}",
            &[("a", VarValue::Bool(false)),
              ("b", VarValue::Bool(true)),
              ("c", VarValue::Bool(false)),
              ("d", VarValue::Bool(true))],
        ));
    }

    #[test]
    fn parens_override() {
        assert!( eval(
            "(${a} || ${b}) && ${c}",
            &[("a", VarValue::Bool(false)),
              ("b", VarValue::Bool(true)),
              ("c", VarValue::Bool(true))],
        ));
    }

    #[test]
    fn not_unary() {
        assert!(eval("!${flag}", &[("flag", VarValue::Bool(false))]));
        assert!(eval("!${missing}", &[]));   // missing → falsy → !false = true
    }

    #[test]
    fn ne_distinct_from_not() {
        // The lookahead in parse_not makes `!=` parse as comparison not unary
        assert!(eval("${a} != ${b}",
            &[("a", VarValue::String("x".into())),
              ("b", VarValue::String("y".into()))]));
    }

    #[test]
    fn defined_function() {
        assert!( eval("defined(name)", &[("name", VarValue::String("x".into()))]));
        assert!(!eval("defined(name)", &[]));
        // ${...} wrapper accepted too
        assert!( eval("defined(${name})", &[("name", VarValue::String("x".into()))]));
    }

    #[test]
    fn matches_regex() {
        assert!( eval("${ver} =~ \"^3\\\\.\"",
            &[("ver", VarValue::String("3.1.4".into()))]));
        assert!(!eval("${ver} =~ \"^3\\\\.\"",
            &[("ver", VarValue::String("4.0.0".into()))]));
    }

    #[test]
    fn substring_contains() {
        assert!( eval("${msg} ~~ \"ERROR\"",
            &[("msg", VarValue::String("FATAL ERROR: oops".into()))]));
        assert!(!eval("${msg} ~~ \"ERROR\"",
            &[("msg", VarValue::String("all good".into()))]));
    }

    #[test]
    fn complex_real_example() {
        let expr = "(${count} >= 1 && ${count} <= 10) || defined(force)";
        assert!( eval(expr, &[("count", VarValue::Number(5.0))]));
        assert!( eval(expr, &[("count", VarValue::Number(99.0)),
                              ("force", VarValue::Bool(true))]));
        assert!(!eval(expr, &[("count", VarValue::Number(99.0))]));
    }

    #[test]
    fn fallback_in_var() {
        // ${missing:-default} renders as "default" → truthy
        assert!(eval("${missing:-default}", &[]));
    }

    #[test]
    fn parse_errors() {
        assert!(parse("${unterminated").is_err());
        assert!(parse("(missing close").is_err());
        assert!(parse("unknown_func(x)").is_err());
        assert!(parse("${a} == ").is_err());
    }
}
