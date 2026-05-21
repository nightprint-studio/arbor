//! Mini-expression language for query-driven bulk edit (FROZEN F13).
//!
//! Fluent-first by design: method chains on `old`, template strings,
//! a curated set of built-in functions. Strict typing on every
//! operator — no implicit coercion across kinds; explicit `.to_*()`
//! casts only. Methods on `null` short-circuit to a runtime error
//! that the bulk-edit pipeline turns into "skip this site" with a
//! visible warning (the user can guard with `??` for null-safe chains).
//!
//! Grammar (operator precedence ↓):
//! ```text
//! expr      := ternary
//! ternary   := coalesce ('?' coalesce ':' ternary)?
//! coalesce  := or       ('??' or)*
//! or        := and      ('||' and)*
//! and       := equality ('&&' equality)*
//! equality  := compare  (('==' | '!=') compare)*
//! compare   := add      (('<' | '<=' | '>' | '>=') add)*
//! add       := mul      (('+' | '-') mul)*
//! mul       := unary    (('*' | '/' | '%') unary)*
//! unary     := ('!' | '-') unary | postfix
//! postfix   := primary  ('.' ident '(' args ')')*
//! primary   := number | string | template | bool | null | 'old'
//!            | '(' expr ')'
//! template  := '`' (text | '${' expr '}')* '`'
//! args      := (expr (',' expr)*)?
//! ```
//!
//! Runtime values: `Null | Bool | Number(f64) | String(String)`. No
//! containers — the bulk edit pipeline rejects container query hits in
//! the preview step before any expression evaluation happens. Numbers
//! are stored as `f64`; integer/float distinction at apply time is the
//! backend's concern (see FROZEN F13 RON semantics).

use std::fmt;

// ─── Runtime values ──────────────────────────────────────────────────

/// One concrete value flowing through the expression evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null      => "null",
            Value::Bool(_)   => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
        }
    }

    fn is_truthy(&self) -> bool {
        match self {
            Value::Null      => false,
            Value::Bool(b)   => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
        }
    }

    /// Stringify for template interpolation. Auto-stringifies any kind
    /// (booleans render as `true`/`false`, numbers as the shortest
    /// round-trip representation, null as the literal `"null"`).
    pub fn to_display_string(&self) -> String {
        match self {
            Value::Null      => "null".into(),
            Value::Bool(b)   => b.to_string(),
            Value::Number(n) => format_number(*n),
            Value::String(s) => s.clone(),
        }
    }
}

/// Stringify a finite f64 with the shortest round-trip repr (integer
/// when it has no fractional part). Drives template interpolation +
/// the `to_string()` method. NaN/Inf surface verbatim so users see them
/// rather than getting a silent corruption.
fn format_number(n: f64) -> String {
    if n.is_nan() { return "NaN".into(); }
    if n.is_infinite() { return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    if n.fract() == 0.0 && n.abs() < 1e16 {
        // Integer-valued float — print without trailing `.0`.
        return format!("{}", n as i64);
    }
    format!("{n}")
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display_string())
    }
}

// ─── Errors ──────────────────────────────────────────────────────────

/// Compile- *or* run-time error surfaced from the mini-expr engine.
/// The bulk-edit pipeline maps compile errors to a top-level banner
/// and runtime errors to per-site "skipped" entries.
#[derive(Debug, Clone)]
pub struct ExprError(pub String);

impl ExprError {
    pub fn msg(s: impl Into<String>) -> Self { ExprError(s.into()) }
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for ExprError {}

pub type EvalResult = Result<Value, ExprError>;

// ─── AST ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Expr {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Old,
    Template(Vec<TemplatePart>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Coalesce(Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Method { target: Box<Expr>, name: String, args: Vec<Expr> },
}

#[derive(Debug, Clone)]
enum TemplatePart {
    Lit(String),
    Expr(Expr),
}

#[derive(Debug, Clone, Copy)]
enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

// ─── Public API ──────────────────────────────────────────────────────

/// A parsed expression, ready to evaluate many times against different
/// `old` values. Compiled once per bulk-edit batch.
#[derive(Debug, Clone)]
pub struct CompiledExpr {
    ast: Expr,
}

impl CompiledExpr {
    pub fn eval(&self, old: &Value) -> EvalResult {
        eval(&self.ast, old)
    }
}

/// Parse + check `src` into a [`CompiledExpr`]. Errors carry a short
/// human-readable message — no column tracking in v1 (the expressions
/// users write are typically one-liners; per-token position would
/// double the lexer state for marginal gain).
pub fn compile(src: &str) -> Result<CompiledExpr, ExprError> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Err(ExprError::msg("Expression is empty"));
    }
    let tokens = Lexer::new(trimmed).tokenise()?;
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_expr()?;
    parser.expect_eof()?;
    Ok(CompiledExpr { ast: expr })
}

// ─── Lexer ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    // Literals / identifiers
    Number(f64),
    String(String),
    /// Template parts: a flat sequence of `Lit("...")` strings and
    /// `Hole(<tokens>)` slots, pre-tokenised so the parser doesn't
    /// re-enter the lexer mid-stream. The hole carries its own token
    /// stream that the parser converts to an Expr on demand.
    Template(Vec<TemplateTok>),
    Bool(bool),
    Null,
    Old,
    Ident(String),
    // Operators / punctuation
    Plus, Minus, Star, Slash, Percent,
    Eq, Ne, Lt, Le, Gt, Ge,
    AndAnd, OrOr, Bang,
    QQ,          // ??
    Question,
    Colon,
    LParen, RParen, Comma, Dot,
}

#[derive(Debug, Clone, PartialEq)]
enum TemplateTok {
    Lit(String),
    Hole(Vec<Tok>),
}

struct Lexer<'a> {
    src:  &'a str,
    bytes: &'a [u8],
    pos:  usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0 }
    }

    fn tokenise(mut self) -> Result<Vec<Tok>, ExprError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.bytes.len() { break; }
            let b = self.bytes[self.pos];
            match b {
                b'(' => { self.pos += 1; out.push(Tok::LParen); }
                b')' => { self.pos += 1; out.push(Tok::RParen); }
                b',' => { self.pos += 1; out.push(Tok::Comma);  }
                b'.' => { self.pos += 1; out.push(Tok::Dot);    }
                b':' => { self.pos += 1; out.push(Tok::Colon);  }
                b'?' => {
                    if self.peek(1) == Some(b'?') { self.pos += 2; out.push(Tok::QQ); }
                    else                          { self.pos += 1; out.push(Tok::Question); }
                }
                b'+' => { self.pos += 1; out.push(Tok::Plus);  }
                b'-' => { self.pos += 1; out.push(Tok::Minus); }
                b'*' => { self.pos += 1; out.push(Tok::Star);  }
                b'/' => { self.pos += 1; out.push(Tok::Slash); }
                b'%' => { self.pos += 1; out.push(Tok::Percent); }
                b'=' => {
                    if self.peek(1) == Some(b'=') { self.pos += 2; out.push(Tok::Eq); }
                    else { return Err(ExprError::msg("Unexpected `=` (did you mean `==`?)")); }
                }
                b'!' => {
                    if self.peek(1) == Some(b'=') { self.pos += 2; out.push(Tok::Ne); }
                    else                          { self.pos += 1; out.push(Tok::Bang); }
                }
                b'<' => {
                    if self.peek(1) == Some(b'=') { self.pos += 2; out.push(Tok::Le); }
                    else                          { self.pos += 1; out.push(Tok::Lt); }
                }
                b'>' => {
                    if self.peek(1) == Some(b'=') { self.pos += 2; out.push(Tok::Ge); }
                    else                          { self.pos += 1; out.push(Tok::Gt); }
                }
                b'&' => {
                    if self.peek(1) == Some(b'&') { self.pos += 2; out.push(Tok::AndAnd); }
                    else { return Err(ExprError::msg("Unexpected `&` (did you mean `&&`?)")); }
                }
                b'|' => {
                    if self.peek(1) == Some(b'|') { self.pos += 2; out.push(Tok::OrOr); }
                    else { return Err(ExprError::msg("Unexpected `|` (did you mean `||`?)")); }
                }
                b'"' | b'\'' => {
                    let s = self.read_string(b)?;
                    out.push(Tok::String(s));
                }
                b'`' => {
                    let parts = self.read_template()?;
                    out.push(Tok::Template(parts));
                }
                b if b.is_ascii_digit() => {
                    let n = self.read_number()?;
                    out.push(Tok::Number(n));
                }
                b if is_ident_start(b) => {
                    let w = self.read_ident();
                    match w.as_str() {
                        "true"  => out.push(Tok::Bool(true)),
                        "false" => out.push(Tok::Bool(false)),
                        "null"  => out.push(Tok::Null),
                        "old"   => out.push(Tok::Old),
                        _       => out.push(Tok::Ident(w)),
                    }
                }
                _ => return Err(ExprError::msg(format!(
                    "Unexpected character `{}` at position {}",
                    b as char, self.pos,
                ))),
            }
        }
        Ok(out)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self, dx: usize) -> Option<u8> {
        self.bytes.get(self.pos + dx).copied()
    }

    fn read_string(&mut self, quote: u8) -> Result<String, ExprError> {
        // Skip the opening quote
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == quote {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(&esc) = self.bytes.get(self.pos) else {
                    return Err(ExprError::msg("Unterminated string escape"));
                };
                self.pos += 1;
                match esc {
                    b'n'  => out.push('\n'),
                    b't'  => out.push('\t'),
                    b'r'  => out.push('\r'),
                    b'"'  => out.push('"'),
                    b'\'' => out.push('\''),
                    b'\\' => out.push('\\'),
                    b'`'  => out.push('`'),
                    b'0'  => out.push('\0'),
                    c     => out.push(c as char),
                }
                continue;
            }
            // UTF-8 — copy a full char rather than a byte.
            let ch_start = self.pos;
            let ch_end = next_char_boundary(self.src, ch_start);
            out.push_str(&self.src[ch_start..ch_end]);
            self.pos = ch_end;
        }
        Err(ExprError::msg("Unterminated string literal"))
    }

    fn read_template(&mut self) -> Result<Vec<TemplateTok>, ExprError> {
        // Skip the opening backtick
        self.pos += 1;
        let mut parts: Vec<TemplateTok> = Vec::new();
        let mut current_lit = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'`' {
                if !current_lit.is_empty() {
                    parts.push(TemplateTok::Lit(std::mem::take(&mut current_lit)));
                }
                self.pos += 1;
                return Ok(parts);
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(&esc) = self.bytes.get(self.pos) else {
                    return Err(ExprError::msg("Unterminated escape in template"));
                };
                self.pos += 1;
                match esc {
                    b'n'  => current_lit.push('\n'),
                    b't'  => current_lit.push('\t'),
                    b'r'  => current_lit.push('\r'),
                    b'`'  => current_lit.push('`'),
                    b'\\' => current_lit.push('\\'),
                    b'$'  => current_lit.push('$'),
                    c     => current_lit.push(c as char),
                }
                continue;
            }
            if b == b'$' && self.peek(1) == Some(b'{') {
                if !current_lit.is_empty() {
                    parts.push(TemplateTok::Lit(std::mem::take(&mut current_lit)));
                }
                self.pos += 2; // past ${
                // Tokenise the hole, tracking brace depth so a nested
                // `{x ? a : b}` inside doesn't fool us — but we never
                // produce a `{` token, so the only `}` we close on is
                // the template's own closing one.
                let hole_start = self.pos;
                let mut depth = 1usize;
                while self.pos < self.bytes.len() && depth > 0 {
                    let c = self.bytes[self.pos];
                    if c == b'{' { depth += 1; }
                    if c == b'}' { depth -= 1; if depth == 0 { break; } }
                    // Skip strings inside the hole so a `}` in a string
                    // doesn't close the hole prematurely.
                    if c == b'"' || c == b'\'' {
                        let quote = c;
                        self.pos += 1;
                        while self.pos < self.bytes.len() && self.bytes[self.pos] != quote {
                            if self.bytes[self.pos] == b'\\' && self.pos + 1 < self.bytes.len() {
                                self.pos += 2;
                                continue;
                            }
                            self.pos += 1;
                        }
                        if self.pos < self.bytes.len() { self.pos += 1; }
                        continue;
                    }
                    self.pos += 1;
                }
                if depth != 0 {
                    return Err(ExprError::msg("Unterminated `${...}` in template"));
                }
                let hole_src = &self.src[hole_start..self.pos];
                self.pos += 1; // past the closing }
                let toks = Lexer::new(hole_src).tokenise()?;
                if toks.is_empty() {
                    return Err(ExprError::msg("Empty `${}` interpolation in template"));
                }
                parts.push(TemplateTok::Hole(toks));
                continue;
            }
            let ch_start = self.pos;
            let ch_end = next_char_boundary(self.src, ch_start);
            current_lit.push_str(&self.src[ch_start..ch_end]);
            self.pos = ch_end;
        }
        Err(ExprError::msg("Unterminated template literal"))
    }

    fn read_number(&mut self) -> Result<f64, ExprError> {
        let start = self.pos;
        let mut saw_dot = false;
        let mut saw_exp = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_digit() || b == b'_' {
                self.pos += 1;
            } else if b == b'.' && !saw_dot && !saw_exp
                && self.peek(1).map_or(false, |c| c.is_ascii_digit())
            {
                saw_dot = true;
                self.pos += 1;
            } else if (b == b'e' || b == b'E') && !saw_exp {
                saw_exp = true;
                self.pos += 1;
                if self.peek(0) == Some(b'+') || self.peek(0) == Some(b'-') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
        let raw = &self.src[start..self.pos];
        let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
        cleaned.parse::<f64>().map_err(|e| ExprError::msg(format!("Invalid number `{raw}`: {e}")))
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_ident_cont(self.bytes[self.pos]) {
            self.pos += 1;
        }
        self.src[start..self.pos].to_string()
    }
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = i + 1;
    while !s.is_char_boundary(j) && j < s.len() { j += 1; }
    j.min(s.len())
}

fn is_ident_start(b: u8) -> bool { b.is_ascii_alphabetic() || b == b'_' }
fn is_ident_cont(b: u8)  -> bool { b.is_ascii_alphanumeric() || b == b'_' }

// ─── Parser ──────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Tok>,
    pos:    usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> { self.tokens.get(self.pos) }

    fn bump(&mut self) -> Option<Tok> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else { None }
    }

    fn eat(&mut self, expected: &Tok) -> bool {
        if self.peek() == Some(expected) { self.pos += 1; true } else { false }
    }

    fn expect(&mut self, expected: &Tok, what: &str) -> Result<(), ExprError> {
        if self.eat(expected) { Ok(()) } else {
            Err(ExprError::msg(format!("Expected {what}, got {}", describe(self.peek()))))
        }
    }

    fn expect_eof(&self) -> Result<(), ExprError> {
        if self.pos == self.tokens.len() { Ok(()) }
        else { Err(ExprError::msg(format!("Unexpected trailing input: {}", describe(self.peek())))) }
    }

    fn parse_expr(&mut self) -> Result<Expr, ExprError> { self.parse_ternary() }

    fn parse_ternary(&mut self) -> Result<Expr, ExprError> {
        let cond = self.parse_coalesce()?;
        if self.eat(&Tok::Question) {
            let then = self.parse_coalesce()?;
            self.expect(&Tok::Colon, "`:` in ternary")?;
            let else_ = self.parse_ternary()?;
            Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(else_)))
        } else { Ok(cond) }
    }

    fn parse_coalesce(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_or()?;
        while self.eat(&Tok::QQ) {
            let rhs = self.parse_or()?;
            lhs = Expr::Coalesce(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_or(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_and()?;
        while self.eat(&Tok::OrOr) {
            let rhs = self.parse_and()?;
            lhs = Expr::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_equality()?;
        while self.eat(&Tok::AndAnd) {
            let rhs = self.parse_equality()?;
            lhs = Expr::Binary(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_equality(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_compare()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Eq) => BinOp::Eq,
                Some(Tok::Ne) => BinOp::Ne,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_compare()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_compare(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Lt) => BinOp::Lt,
                Some(Tok::Le) => BinOp::Le,
                Some(Tok::Gt) => BinOp::Gt,
                Some(Tok::Ge) => BinOp::Ge,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_add()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus)  => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star)    => BinOp::Mul,
                Some(Tok::Slash)   => BinOp::Div,
                Some(Tok::Percent) => BinOp::Mod,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Tok::Bang)  => { self.pos += 1; Ok(Expr::Not(Box::new(self.parse_unary()?))) }
            Some(Tok::Minus) => { self.pos += 1; Ok(Expr::Neg(Box::new(self.parse_unary()?))) }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ExprError> {
        let mut e = self.parse_primary()?;
        loop {
            if !self.eat(&Tok::Dot) { break; }
            let name = match self.bump() {
                Some(Tok::Ident(s)) => s,
                other => return Err(ExprError::msg(format!(
                    "Expected method name after `.`, got {}", describe(other.as_ref()),
                ))),
            };
            self.expect(&Tok::LParen, "`(` after method name")?;
            let mut args = Vec::new();
            if !matches!(self.peek(), Some(Tok::RParen)) {
                args.push(self.parse_expr()?);
                while self.eat(&Tok::Comma) {
                    args.push(self.parse_expr()?);
                }
            }
            self.expect(&Tok::RParen, "`)` closing method call")?;
            e = Expr::Method { target: Box::new(e), name, args };
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        match self.bump() {
            Some(Tok::Number(n)) => Ok(Expr::Number(n)),
            Some(Tok::String(s)) => Ok(Expr::String(s)),
            Some(Tok::Bool(b))   => Ok(Expr::Bool(b)),
            Some(Tok::Null)      => Ok(Expr::Null),
            Some(Tok::Old)       => Ok(Expr::Old),
            Some(Tok::Template(parts)) => {
                let mut compiled = Vec::with_capacity(parts.len());
                for p in parts {
                    match p {
                        TemplateTok::Lit(s) => compiled.push(TemplatePart::Lit(s)),
                        TemplateTok::Hole(toks) => {
                            let mut inner = Parser { tokens: toks, pos: 0 };
                            let e = inner.parse_expr()?;
                            inner.expect_eof()?;
                            compiled.push(TemplatePart::Expr(e));
                        }
                    }
                }
                Ok(Expr::Template(compiled))
            }
            Some(Tok::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => Err(ExprError::msg(format!(
                "Identifier `{name}` is not a value — methods only work as `target.{name}(...)`. \
                 Use `old` as the input variable.",
            ))),
            other => Err(ExprError::msg(format!(
                "Expected a value, got {}", describe(other.as_ref()),
            ))),
        }
    }
}

fn describe(t: Option<&Tok>) -> String {
    match t {
        None => "end of input".to_string(),
        Some(t) => match t {
            Tok::Number(n)   => format!("number `{n}`"),
            Tok::String(_)   => "string literal".into(),
            Tok::Template(_) => "template literal".into(),
            Tok::Bool(b)     => format!("`{b}`"),
            Tok::Null        => "`null`".into(),
            Tok::Old         => "`old`".into(),
            Tok::Ident(s)    => format!("identifier `{s}`"),
            Tok::Plus        => "`+`".into(),
            Tok::Minus       => "`-`".into(),
            Tok::Star        => "`*`".into(),
            Tok::Slash       => "`/`".into(),
            Tok::Percent     => "`%`".into(),
            Tok::Eq          => "`==`".into(),
            Tok::Ne          => "`!=`".into(),
            Tok::Lt          => "`<`".into(),
            Tok::Le          => "`<=`".into(),
            Tok::Gt          => "`>`".into(),
            Tok::Ge          => "`>=`".into(),
            Tok::AndAnd      => "`&&`".into(),
            Tok::OrOr        => "`||`".into(),
            Tok::Bang        => "`!`".into(),
            Tok::QQ          => "`??`".into(),
            Tok::Question    => "`?`".into(),
            Tok::Colon       => "`:`".into(),
            Tok::LParen      => "`(`".into(),
            Tok::RParen      => "`)`".into(),
            Tok::Comma       => "`,`".into(),
            Tok::Dot         => "`.`".into(),
        }
    }
}

// ─── Evaluator ───────────────────────────────────────────────────────

fn eval(e: &Expr, old: &Value) -> EvalResult {
    match e {
        Expr::Null      => Ok(Value::Null),
        Expr::Bool(b)   => Ok(Value::Bool(*b)),
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::String(s) => Ok(Value::String(s.clone())),
        Expr::Old       => Ok(old.clone()),
        Expr::Template(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    TemplatePart::Lit(s) => out.push_str(s),
                    TemplatePart::Expr(inner) => {
                        let v = eval(inner, old)?;
                        out.push_str(&v.to_display_string());
                    }
                }
            }
            Ok(Value::String(out))
        }
        Expr::Neg(inner) => {
            let v = eval(inner, old)?;
            match v {
                Value::Number(n) => Ok(Value::Number(-n)),
                _ => Err(ExprError::msg(format!("Cannot negate {}", v.type_name()))),
            }
        }
        Expr::Not(inner) => {
            let v = eval(inner, old)?;
            match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(ExprError::msg(format!("`!` requires bool, got {}", v.type_name()))),
            }
        }
        Expr::Coalesce(a, b) => {
            let lhs = eval(a, old)?;
            if matches!(lhs, Value::Null) { eval(b, old) } else { Ok(lhs) }
        }
        Expr::Ternary(c, t, e2) => {
            let cv = eval(c, old)?;
            let Value::Bool(b) = cv else {
                return Err(ExprError::msg(format!(
                    "Ternary condition must be bool, got {}", cv.type_name(),
                )));
            };
            if b { eval(t, old) } else { eval(e2, old) }
        }
        Expr::Binary(op, a, b) => {
            // Short-circuit logical ops before evaluating rhs.
            if matches!(op, BinOp::And | BinOp::Or) {
                let lhs = eval(a, old)?;
                let Value::Bool(lb) = lhs else {
                    return Err(ExprError::msg(format!(
                        "`{}` requires bool, got {}",
                        match op { BinOp::And => "&&", BinOp::Or => "||", _ => unreachable!() },
                        lhs.type_name(),
                    )));
                };
                if matches!(op, BinOp::And) && !lb { return Ok(Value::Bool(false)); }
                if matches!(op, BinOp::Or)  &&  lb { return Ok(Value::Bool(true));  }
                let rhs = eval(b, old)?;
                let Value::Bool(rb) = rhs else {
                    return Err(ExprError::msg(format!(
                        "`{}` requires bool, got {}",
                        match op { BinOp::And => "&&", BinOp::Or => "||", _ => unreachable!() },
                        rhs.type_name(),
                    )));
                };
                return Ok(Value::Bool(rb));
            }
            let lhs = eval(a, old)?;
            let rhs = eval(b, old)?;
            apply_binop(*op, lhs, rhs)
        }
        Expr::Method { target, name, args } => {
            let t = eval(target, old)?;
            let mut evaled_args = Vec::with_capacity(args.len());
            for a in args { evaled_args.push(eval(a, old)?); }
            call_method(t, name, evaled_args)
        }
    }
}

fn apply_binop(op: BinOp, lhs: Value, rhs: Value) -> EvalResult {
    use Value::*;
    match op {
        BinOp::Add => match (&lhs, &rhs) {
            (Number(a), Number(b)) => Ok(Number(a + b)),
            (String(a), String(b)) => Ok(String(format!("{a}{b}"))),
            _ => Err(ExprError::msg(format!(
                "`+` requires number+number or string+string, got {}+{}",
                lhs.type_name(), rhs.type_name(),
            ))),
        }
        BinOp::Sub => num_op(lhs, rhs, "-", |a, b| a - b),
        BinOp::Mul => num_op(lhs, rhs, "*", |a, b| a * b),
        BinOp::Div => match (&lhs, &rhs) {
            (Number(_), Number(b)) if *b == 0.0 => Err(ExprError::msg("Division by zero")),
            _ => num_op(lhs, rhs, "/", |a, b| a / b),
        }
        BinOp::Mod => match (&lhs, &rhs) {
            (Number(_), Number(b)) if *b == 0.0 => Err(ExprError::msg("Modulo by zero")),
            _ => num_op(lhs, rhs, "%", |a, b| a % b),
        }
        BinOp::Eq => Ok(Bool(values_equal(&lhs, &rhs))),
        BinOp::Ne => Ok(Bool(!values_equal(&lhs, &rhs))),
        BinOp::Lt => cmp_op(lhs, rhs, "<",  |o| o == std::cmp::Ordering::Less),
        BinOp::Le => cmp_op(lhs, rhs, "<=", |o| o != std::cmp::Ordering::Greater),
        BinOp::Gt => cmp_op(lhs, rhs, ">",  |o| o == std::cmp::Ordering::Greater),
        BinOp::Ge => cmp_op(lhs, rhs, ">=", |o| o != std::cmp::Ordering::Less),
        BinOp::And | BinOp::Or => unreachable!("short-circuited above"),
    }
}

fn num_op(lhs: Value, rhs: Value, op: &str, f: impl Fn(f64, f64) -> f64) -> EvalResult {
    match (&lhs, &rhs) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(f(*a, *b))),
        _ => Err(ExprError::msg(format!(
            "`{op}` requires number+number, got {}+{}",
            lhs.type_name(), rhs.type_name(),
        ))),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null,      Value::Null)      => true,
        (Value::Bool(a),   Value::Bool(b))   => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        _ => false,
    }
}

fn cmp_op(lhs: Value, rhs: Value, op: &str, f: impl Fn(std::cmp::Ordering) -> bool) -> EvalResult {
    match (&lhs, &rhs) {
        (Value::Number(a), Value::Number(b)) => {
            match a.partial_cmp(b) {
                Some(o) => Ok(Value::Bool(f(o))),
                None    => Ok(Value::Bool(false)), // NaN compares false to anything
            }
        }
        (Value::String(a), Value::String(b)) => Ok(Value::Bool(f(a.cmp(b)))),
        _ => Err(ExprError::msg(format!(
            "`{op}` requires number+number or string+string, got {}+{}",
            lhs.type_name(), rhs.type_name(),
        ))),
    }
}

// ─── Method dispatch ─────────────────────────────────────────────────

fn call_method(target: Value, name: &str, args: Vec<Value>) -> EvalResult {
    // Methods on null short-circuit with a clear error — the caller
    // (bulk-edit pipeline) maps this to a per-site "skipped" entry.
    // Users guard with `??` for null-safe chains (e.g. `(old ?? "").upper()`).
    if matches!(target, Value::Null) {
        return Err(ExprError::msg(format!(
            "Cannot call `.{name}()` on null. Use `??` for a fallback (e.g. `(old ?? \"\").{name}()`).",
        )));
    }
    macro_rules! arity {
        ($n:expr) => {
            if args.len() != $n {
                return Err(ExprError::msg(format!(
                    "`.{name}()` takes {} argument{}, got {}",
                    $n, if $n == 1usize { "" } else { "s" }, args.len(),
                )));
            }
        };
    }
    macro_rules! arity_range {
        ($lo:expr, $hi:expr) => {
            #[allow(unused_comparisons)]
            if args.len() < $lo || args.len() > $hi {
                return Err(ExprError::msg(format!(
                    "`.{name}()` takes {}-{} arguments, got {}",
                    $lo, $hi, args.len(),
                )));
            }
        };
    }

    match (name, &target) {
        // ── String ──────────────────────────────────────────────────
        ("length" | "size", Value::String(s)) => { arity!(0); Ok(Value::Number(s.chars().count() as f64)) }
        ("is_empty",        Value::String(s)) => { arity!(0); Ok(Value::Bool(s.is_empty())) }
        ("trim",            Value::String(s)) => { arity!(0); Ok(Value::String(s.trim().to_string())) }
        ("upper",           Value::String(s)) => { arity!(0); Ok(Value::String(s.to_uppercase())) }
        ("lower",           Value::String(s)) => { arity!(0); Ok(Value::String(s.to_lowercase())) }
        ("starts_with",     Value::String(s)) => { arity!(1); let p = expect_string(&args[0], "starts_with", 0)?; Ok(Value::Bool(s.starts_with(p))) }
        ("ends_with",       Value::String(s)) => { arity!(1); let p = expect_string(&args[0], "ends_with",   0)?; Ok(Value::Bool(s.ends_with(p))) }
        ("contains",        Value::String(s)) => { arity!(1); let p = expect_string(&args[0], "contains",    0)?; Ok(Value::Bool(s.contains(p))) }
        ("replace",         Value::String(s)) => {
            arity!(2);
            let from = expect_string(&args[0], "replace", 0)?;
            let to   = expect_string(&args[1], "replace", 1)?;
            Ok(Value::String(s.replace(from, to)))
        }
        ("substr", Value::String(s)) => {
            arity_range!(1, 2);
            let start = expect_uint(&args[0], "substr", 0)?;
            let chars: Vec<char> = s.chars().collect();
            let end = if args.len() == 2 {
                expect_uint(&args[1], "substr", 1)?
            } else { chars.len() };
            let lo = start.min(chars.len());
            let hi = end.min(chars.len()).max(lo);
            Ok(Value::String(chars[lo..hi].iter().collect()))
        }
        ("pad_start", Value::String(s)) => {
            arity_range!(1, 2);
            let n = expect_uint(&args[0], "pad_start", 0)?;
            let ch = if args.len() == 2 { expect_pad_char(&args[1], "pad_start")? } else { ' ' };
            Ok(Value::String(pad_left(s, n, ch)))
        }
        ("pad_end", Value::String(s)) => {
            arity_range!(1, 2);
            let n  = expect_uint(&args[0], "pad_end", 0)?;
            let ch = if args.len() == 2 { expect_pad_char(&args[1], "pad_end")? } else { ' ' };
            Ok(Value::String(pad_right(s, n, ch)))
        }
        ("repeat", Value::String(s)) => {
            arity!(1);
            let n = expect_uint(&args[0], "repeat", 0)?;
            if n > 1_000_000 {
                return Err(ExprError::msg("`.repeat()` count too large (max 1_000_000)"));
            }
            Ok(Value::String(s.repeat(n)))
        }

        // ── Number ──────────────────────────────────────────────────
        ("abs",   Value::Number(n)) => { arity!(0); Ok(Value::Number(n.abs())) }
        ("floor", Value::Number(n)) => { arity!(0); Ok(Value::Number(n.floor())) }
        ("ceil",  Value::Number(n)) => { arity!(0); Ok(Value::Number(n.ceil())) }
        ("round", Value::Number(n)) => {
            arity_range!(0, 1);
            if args.is_empty() {
                Ok(Value::Number(n.round()))
            } else {
                let d = expect_uint(&args[0], "round", 0)?.min(20) as i32;
                let m = 10f64.powi(d);
                Ok(Value::Number((n * m).round() / m))
            }
        }
        ("to_fixed", Value::Number(n)) => {
            arity!(1);
            let d = expect_uint(&args[0], "to_fixed", 0)?.min(20);
            Ok(Value::String(format!("{:.*}", d, n)))
        }
        ("min", Value::Number(n)) => {
            arity!(1);
            let o = expect_number(&args[0], "min", 0)?;
            Ok(Value::Number(n.min(o)))
        }
        ("max", Value::Number(n)) => {
            arity!(1);
            let o = expect_number(&args[0], "max", 0)?;
            Ok(Value::Number(n.max(o)))
        }
        ("clamp", Value::Number(n)) => {
            arity!(2);
            let lo = expect_number(&args[0], "clamp", 0)?;
            let hi = expect_number(&args[1], "clamp", 1)?;
            if hi < lo { return Err(ExprError::msg("`.clamp(lo, hi)` requires hi >= lo")); }
            Ok(Value::Number(n.clamp(lo, hi)))
        }

        // ── Coercion (universal) ────────────────────────────────────
        ("to_string", _) => { arity!(0); Ok(Value::String(target.to_display_string())) }
        ("to_number", _) => {
            arity!(0);
            match target {
                Value::Number(_) => Ok(target),
                Value::Bool(b)   => Ok(Value::Number(if b { 1.0 } else { 0.0 })),
                Value::String(s) => s.trim().parse::<f64>()
                    .map(Value::Number)
                    .map_err(|_| ExprError::msg(format!("Cannot parse `{s}` as number"))),
                Value::Null      => unreachable!("null guarded above"),
            }
        }
        ("to_bool", _) => {
            arity!(0);
            Ok(Value::Bool(target.is_truthy()))
        }

        // ── Predicate (universal) ───────────────────────────────────
        ("is_null",   _) => { arity!(0); Ok(Value::Bool(matches!(target, Value::Null))) }
        ("is_string", _) => { arity!(0); Ok(Value::Bool(matches!(target, Value::String(_)))) }
        ("is_number", _) => { arity!(0); Ok(Value::Bool(matches!(target, Value::Number(_)))) }
        ("is_bool",   _) => { arity!(0); Ok(Value::Bool(matches!(target, Value::Bool(_)))) }
        ("type",      _) => { arity!(0); Ok(Value::String(target.type_name().to_string())) }

        // ── Method exists but wrong receiver type ────────────────────
        (
            "length" | "size" | "is_empty" | "trim" | "upper" | "lower"
            | "starts_with" | "ends_with" | "contains" | "replace" | "substr"
            | "pad_start" | "pad_end" | "repeat",
            _,
        ) => Err(ExprError::msg(format!(
            "`.{name}()` requires a string receiver, got {}",
            target.type_name(),
        ))),
        (
            "abs" | "floor" | "ceil" | "round" | "to_fixed" | "min" | "max" | "clamp",
            _,
        ) => Err(ExprError::msg(format!(
            "`.{name}()` requires a number receiver, got {}",
            target.type_name(),
        ))),
        _ => Err(ExprError::msg(format!(
            "Unknown method `.{name}()` on {}", target.type_name(),
        ))),
    }
}

fn expect_string<'a>(v: &'a Value, fn_name: &str, idx: usize) -> Result<&'a str, ExprError> {
    match v {
        Value::String(s) => Ok(s.as_str()),
        _ => Err(ExprError::msg(format!(
            "`.{fn_name}()` argument #{} must be string, got {}",
            idx + 1, v.type_name(),
        ))),
    }
}

fn expect_number(v: &Value, fn_name: &str, idx: usize) -> Result<f64, ExprError> {
    match v {
        Value::Number(n) => Ok(*n),
        _ => Err(ExprError::msg(format!(
            "`.{fn_name}()` argument #{} must be number, got {}",
            idx + 1, v.type_name(),
        ))),
    }
}

fn expect_uint(v: &Value, fn_name: &str, idx: usize) -> Result<usize, ExprError> {
    let n = expect_number(v, fn_name, idx)?;
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 {
        return Err(ExprError::msg(format!(
            "`.{fn_name}()` argument #{} must be a non-negative integer, got {n}",
            idx + 1,
        )));
    }
    Ok(n as usize)
}

fn expect_pad_char(v: &Value, fn_name: &str) -> Result<char, ExprError> {
    let s = expect_string(v, fn_name, 1)?;
    let mut it = s.chars();
    let Some(c) = it.next() else {
        return Err(ExprError::msg(format!("`.{fn_name}()` pad-char must be a single character")));
    };
    if it.next().is_some() {
        return Err(ExprError::msg(format!("`.{fn_name}()` pad-char must be a single character")));
    }
    Ok(c)
}

fn pad_left(s: &str, target_len: usize, ch: char) -> String {
    let cur = s.chars().count();
    if cur >= target_len { return s.to_string(); }
    let pad_n = target_len - cur;
    let mut out = String::with_capacity(s.len() + pad_n);
    for _ in 0..pad_n { out.push(ch); }
    out.push_str(s);
    out
}

fn pad_right(s: &str, target_len: usize, ch: char) -> String {
    let cur = s.chars().count();
    if cur >= target_len { return s.to_string(); }
    let pad_n = target_len - cur;
    let mut out = String::with_capacity(s.len() + pad_n);
    out.push_str(s);
    for _ in 0..pad_n { out.push(ch); }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, old: Value) -> Value {
        compile(src).expect("compile").eval(&old).expect("eval")
    }

    fn run_str(src: &str, old: &str) -> String {
        let r = run(src, Value::String(old.to_string()));
        match r {
            Value::String(s) => s,
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn identity() {
        assert_eq!(run_str("old", "hello"), "hello");
    }

    #[test]
    fn upper_lower() {
        assert_eq!(run_str("old.upper()", "abc"), "ABC");
        assert_eq!(run_str("old.lower()", "ABC"), "abc");
    }

    #[test]
    fn chained_methods() {
        assert_eq!(run_str("old.trim().lower().replace(\"_\", \"-\")", "  Foo_Bar  "), "foo-bar");
    }

    #[test]
    fn template_simple() {
        assert_eq!(run_str("`hello ${old}`", "world"), "hello world");
    }

    #[test]
    fn template_nested_expr() {
        assert_eq!(run_str("`${old.upper()}_v${1 + 1}`", "x"), "X_v2");
    }

    #[test]
    fn string_concat_strict() {
        let r = run("old + \"!\"", Value::String("hi".into()));
        assert_eq!(r, Value::String("hi!".into()));
    }

    #[test]
    fn concat_string_plus_number_errors() {
        let err = compile("old + 1").unwrap().eval(&Value::String("x".into())).unwrap_err();
        assert!(err.0.contains("string+string"));
    }

    #[test]
    fn ternary() {
        let r = compile("old > 10 ? \"big\" : \"small\"").unwrap()
            .eval(&Value::Number(20.0)).unwrap();
        assert_eq!(r, Value::String("big".into()));
    }

    #[test]
    fn coalesce_null() {
        let r = compile("old ?? \"default\"").unwrap().eval(&Value::Null).unwrap();
        assert_eq!(r, Value::String("default".into()));
    }

    #[test]
    fn null_method_short_circuits_with_coalesce() {
        let r = compile("(old ?? \"\").upper()").unwrap().eval(&Value::Null).unwrap();
        assert_eq!(r, Value::String("".into()));
    }

    #[test]
    fn null_method_errors_without_coalesce() {
        let err = compile("old.upper()").unwrap().eval(&Value::Null).unwrap_err();
        assert!(err.0.contains("Cannot call"));
    }

    #[test]
    fn pad_start() {
        assert_eq!(run_str("old.pad_start(4, \"0\")", "5"), "0005");
    }

    #[test]
    fn round_decimals() {
        let r = compile("old.round(2)").unwrap().eval(&Value::Number(3.14159)).unwrap();
        assert_eq!(r, Value::Number(3.14));
    }

    #[test]
    fn to_number_parse_fails() {
        let err = compile("old.to_number()").unwrap().eval(&Value::String("xx".into())).unwrap_err();
        assert!(err.0.contains("Cannot parse"));
    }

    #[test]
    fn template_with_replace() {
        // Strip the leading `1.` from a "1.4.7" version string.
        let r = compile("`0.${old.replace(\"1.\", \"\")}`").unwrap()
            .eval(&Value::String("1.4.7".into())).unwrap();
        assert_eq!(r, Value::String("0.4.7".into()));
    }

    #[test]
    fn equality_across_types() {
        let r = compile("old == 1").unwrap().eval(&Value::String("1".into())).unwrap();
        // strict: string "1" != number 1
        assert_eq!(r, Value::Bool(false));
    }
}
