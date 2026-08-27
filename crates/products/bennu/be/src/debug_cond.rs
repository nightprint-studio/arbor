//! What a Java breakpoint condition is: **paths compared with literals**, and nothing else.
//!
//! ## Why not Java
//!
//! A condition is the same question a watch asks, one step further — and [`crate::debug_path`]
//! already argues why a watch is a path rather than an expression: reading fields and array slots
//! out of a stopped program is something JDWP does *exactly*, while `a + b`, a method call or a
//! stream chain needs a real evaluator, and half an expression language that silently fails on the
//! other half is worse than a small one whose shape is obvious.
//!
//! That argument is stronger here, not weaker. A watch that quietly answers something adjacent to
//! what you typed shows you a wrong number, which you might notice. A **condition** that does it
//! swallows the stop — the program runs past the line you were waiting for, and there is nothing on
//! screen at all. So anything outside the grammar is refused **by name**, before the debugger ever
//! runs, and the frontend checks it while you type.
//!
//! ## What it is
//!
//! ```text
//!   condition := disjunction
//!   disjunction := conjunction ( "||" conjunction )*
//!   conjunction := unary ( "&&" unary )*
//!   unary       := "!"? primary
//!   primary     := "(" condition ")" | operand ( op operand )?
//!   op          := "==" | "!=" | "<" | "<=" | ">" | ">="
//!   operand     := path | number | string | char | "true" | "false" | "null"
//!   path        := whatever a watch accepts — `i`, `order.customer.name`, `items[2].price`
//! ```
//!
//! A bare operand with no comparison must be a **boolean**: `flag`, `!order.paid`. Anything else
//! there is an error at evaluation naming what it actually was, because "stop when `order`" has no
//! meaning anyone would agree on.
//!
//! ## What it deliberately cannot do, and the answer for each
//!
//! * **No method calls.** `list.size() > 3` — calling into a suspended VM runs application code
//!   inside a paused program (see [`crate::debug_value`]'s module doc on `toString`). Compare a
//!   field instead.
//! * **No arithmetic.** `i + 1 == n` — compare `i` with the number you meant.
//! * **No enum constants as bare names.** `status == ACTIVE` would need a static field read on a
//!   class that may not be loaded. `status.name == "ACTIVE"` reads the `name` field every enum
//!   constant has, through the ordinary path walk, and works today.
//!
//! ## What happens when it goes wrong at runtime
//!
//! It **stops**, and says why. A condition that cannot be evaluated — a null halfway down the path,
//! a field that is not there in this subclass — is a bug in the condition, and the only way to see
//! it is to be standing there. Silently continuing would turn a typo into a breakpoint that never
//! fires and never explains itself, which is the exact failure this module exists to prevent.

use bennu_jdwp::prelude::{string_value, Frame, Id, Tag, Value};

use crate::debug::Session;
use crate::debug_path::{self, Step};

/// A condition, parsed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cond {
    /// Two operands and a comparison.
    Compare(Operand, Op, Operand),
    /// One operand on its own — it has to be a boolean.
    Truth(Operand),
    Not(Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Operand {
    /// Read out of the stopped frame.
    Path(Vec<Step>),
    /// Written in the condition.
    Value(Datum),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn word(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }

    /// Whether this asks which is **bigger** — the ones only numbers can answer.
    fn is_ordering(self) -> bool {
        !matches!(self, Op::Eq | Op::Ne)
    }
}

/// A value, from either side: written in the condition, or read out of the VM.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Datum {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Null,
    /// An object that is neither a string nor null — comparable only by identity.
    Object(Id),
}

impl Datum {
    /// What it is, for an error a person reads.
    fn describe(&self) -> String {
        match self {
            Datum::Int(v) => format!("the number {v}"),
            Datum::Float(v) => format!("the number {v}"),
            Datum::Bool(v) => format!("`{v}`"),
            Datum::Text(v) => format!("the string {v:?}"),
            Datum::Null => "null".to_string(),
            Datum::Object(_) => "an object".to_string(),
        }
    }
}

// ── parsing ─────────────────────────────────────────────────────────────────────

/// Parse a condition, or say what is wrong with it in one line.
///
/// An empty (or whitespace-only) condition is `None` rather than an error: that is what "no
/// condition" is written as everywhere else in the product, and refusing it would make clearing a
/// condition impossible.
pub(crate) fn parse(text: &str) -> Result<Option<Cond>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let tokens = lex(text)?;
    let mut parser = Parser { tokens: &tokens, at: 0 };
    let cond = parser.disjunction()?;
    match parser.peek() {
        None => Ok(Some(cond)),
        // Naming the leftover is what turns "syntax error" into something actionable — it is
        // almost always an operator this grammar does not have.
        Some(t) => Err(format!("`{}` is not something a condition can contain", t.text())),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    And,
    Or,
    Bang,
    Op(Op),
    Path(String),
    Lit(Datum),
}

impl Token {
    fn text(&self) -> String {
        match self {
            Token::Open => "(".into(),
            Token::Close => ")".into(),
            Token::And => "&&".into(),
            Token::Or => "||".into(),
            Token::Bang => "!".into(),
            Token::Op(op) => op.word().into(),
            Token::Path(p) => p.clone(),
            Token::Lit(d) => d.describe(),
        }
    }
}

fn lex(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            '&' | '|' => {
                // A single `&` or `|` is bitwise in Java and this grammar has no arithmetic, so
                // saying which one was meant beats reporting an unexpected character.
                if chars.get(i + 1) != Some(&c) {
                    return Err(format!("write `{c}{c}` — a condition has no bitwise operators"));
                }
                out.push(if c == '&' { Token::And } else { Token::Or });
                i += 2;
            }
            '=' => {
                if chars.get(i + 1) != Some(&'=') {
                    return Err("write `==` to compare — a condition assigns nothing".to_string());
                }
                out.push(Token::Op(Op::Eq));
                i += 2;
            }
            '!' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token::Op(Op::Ne));
                    i += 2;
                } else {
                    out.push(Token::Bang);
                    i += 1;
                }
            }
            '<' | '>' => {
                let ordering = if c == '<' { (Op::Lt, Op::Le) } else { (Op::Gt, Op::Ge) };
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token::Op(ordering.1));
                    i += 2;
                } else {
                    out.push(Token::Op(ordering.0));
                    i += 1;
                }
            }
            '"' => {
                let (text, next) = quoted(&chars, i, '"')?;
                out.push(Token::Lit(Datum::Text(text)));
                i = next;
            }
            '\'' => {
                let (text, next) = quoted(&chars, i, '\'')?;
                let mut it = text.chars();
                let (Some(ch), None) = (it.next(), it.next()) else {
                    return Err("a `'…'` literal holds exactly one character".to_string());
                };
                out.push(Token::Lit(Datum::Int(ch as i64)));
                i = next;
            }
            _ if c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) =>
            {
                let (datum, next) = number(&chars, i)?;
                out.push(Token::Lit(datum));
                i = next;
            }
            _ if is_name_start(c) => {
                let (word, next) = path_run(&chars, i);
                out.push(match word.as_str() {
                    "true" => Token::Lit(Datum::Bool(true)),
                    "false" => Token::Lit(Datum::Bool(false)),
                    "null" => Token::Lit(Datum::Null),
                    _ => Token::Path(word),
                });
                i = next;
            }
            // The commonest thing to reach for, and the one worth explaining rather than
            // reporting as an unexpected character.
            '+' | '*' | '/' | '%' | '-' => {
                return Err(format!(
                    "a condition has no arithmetic, so `{c}` cannot appear — compare a value with \
                     the number you meant"
                ))
            }
            _ => return Err(format!("`{c}` has no meaning in a condition")),
        }
    }
    if out.is_empty() {
        return Err("there is nothing to test".to_string());
    }
    Ok(out)
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_name_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// A run of path characters — a name, then any number of `.field` and `[0]` steps. Handed to
/// [`debug_path`] verbatim, so a condition's left-hand side and a watch box accept exactly the
/// same thing.
fn path_run(chars: &[char], from: usize) -> (String, usize) {
    let mut i = from;
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        if is_name_part(c) || c == '.' {
            out.push(c);
            i += 1;
        } else if c == '[' {
            // Up to the matching `]`; the path parser is what decides whether the inside is a
            // number. Bounded by the end of the text, so an unclosed bracket is its error to
            // report and not an infinite loop here.
            let close = chars[i..].iter().position(|&c| c == ']');
            let end = match close {
                Some(n) => i + n + 1,
                None => chars.len(),
            };
            out.extend(&chars[i..end]);
            i = end;
        } else {
            break;
        }
    }
    (out, i)
}

/// The contents of a quoted literal, and where it ended.
fn quoted(chars: &[char], from: usize, quote: char) -> Result<(String, usize), String> {
    let mut i = from + 1;
    let mut out = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                let Some(&esc) = chars.get(i + 1) else { break };
                out.push(match esc {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '0' => '\0',
                    other => other,
                });
                i += 2;
            }
            c if c == quote => return Ok((out, i + 1)),
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Err(format!("a {quote} is never closed"))
}

/// A number: decimal or hex, integer or floating, with Java's suffixes tolerated and ignored.
fn number(chars: &[char], from: usize) -> Result<(Datum, usize), String> {
    let mut i = from;
    let negative = chars[i] == '-';
    if negative {
        i += 1;
    }
    if chars.get(i) == Some(&'0') && matches!(chars.get(i + 1), Some('x') | Some('X')) {
        let start = i + 2;
        let mut end = start;
        while chars.get(end).is_some_and(|c| c.is_ascii_hexdigit() || *c == '_') {
            end += 1;
        }
        let digits: String = chars[start..end].iter().filter(|c| **c != '_').collect();
        let Ok(value) = i64::from_str_radix(&digits, 16) else {
            return Err(format!("`0x{digits}` is not a number this can compare"));
        };
        return Ok((Datum::Int(if negative { -value } else { value }), skip_suffix(chars, end)));
    }

    let start = i;
    let mut float = false;
    while let Some(&c) = chars.get(i) {
        if c.is_ascii_digit() || c == '_' {
            i += 1;
        } else if c == '.' && !float && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
            float = true;
            i += 1;
        } else {
            break;
        }
    }
    let digits: String = chars[start..i].iter().filter(|c| **c != '_').collect();
    let end = skip_suffix(chars, i);
    // A trailing `f`/`d` makes it floating even when it was written without a point.
    let suffixed_float = chars[i..end].iter().any(|c| matches!(c, 'f' | 'F' | 'd' | 'D'));
    if float || suffixed_float {
        let Ok(value) = digits.parse::<f64>() else {
            return Err(format!("`{digits}` is not a number"));
        };
        return Ok((Datum::Float(if negative { -value } else { value }), end));
    }
    let Ok(value) = digits.parse::<i64>() else {
        return Err(format!("`{digits}` is not a whole number this can compare"));
    };
    Ok((Datum::Int(if negative { -value } else { value }), end))
}

/// Step over a Java numeric suffix (`10L`, `1.5f`) — the type it names is the VM's business.
fn skip_suffix(chars: &[char], at: usize) -> usize {
    match chars.get(at) {
        Some('L' | 'l' | 'f' | 'F' | 'd' | 'D') => at + 1,
        _ => at,
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, want: &Token) -> bool {
        if self.peek() == Some(want) {
            self.at += 1;
            return true;
        }
        false
    }

    fn disjunction(&mut self) -> Result<Cond, String> {
        let mut left = self.conjunction()?;
        while self.eat(&Token::Or) {
            let right = self.conjunction()?;
            left = Cond::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn conjunction(&mut self) -> Result<Cond, String> {
        let mut left = self.unary()?;
        while self.eat(&Token::And) {
            let right = self.unary()?;
            left = Cond::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Cond, String> {
        if self.eat(&Token::Bang) {
            return Ok(Cond::Not(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Cond, String> {
        if self.eat(&Token::Open) {
            let inner = self.disjunction()?;
            if !self.eat(&Token::Close) {
                return Err("a `(` is never closed".to_string());
            }
            return Ok(inner);
        }
        let left = self.operand()?;
        let Some(Token::Op(op)) = self.peek().cloned() else {
            if let Operand::Value(datum) = &left {
                // A constant on its own: `true`, `1`. It either always stops or never does, and
                // either way nothing about the program could change the answer — see the
                // two-constants case below.
                return Err(format!(
                    "{} is a constant — a condition has to read something from the program",
                    datum.describe()
                ));
            }
            return Ok(Cond::Truth(left));
        };
        self.at += 1;
        let right = self.operand()?;
        if matches!((&left, &right), (Operand::Value(_), Operand::Value(_))) {
            // Two literals: the answer does not depend on the program, so it is a breakpoint that
            // either always stops or never does. Both are certainly a mistake, and neither would
            // ever explain itself.
            return Err("both sides are constants — a condition has to read something".to_string());
        }
        Ok(Cond::Compare(left, op, right))
    }

    fn operand(&mut self) -> Result<Operand, String> {
        match self.peek().cloned() {
            Some(Token::Path(text)) => {
                self.at += 1;
                // `list.size()` — by far the commonest thing to try, and the reason it cannot work
                // is not obvious: calling into a suspended VM runs application code inside a paused
                // program. Saying so beats "`(` is not something a condition can contain".
                if self.peek() == Some(&Token::Open) {
                    return Err(format!(
                        "`{text}(…)` calls a method, which a condition cannot do — it would run \
                         application code inside the paused program. Compare a field instead"
                    ));
                }
                Ok(Operand::Path(debug_path::parse(&text, debug_path::JAVA)?))
            }
            Some(Token::Lit(datum)) => {
                self.at += 1;
                Ok(Operand::Value(datum))
            }
            Some(other) => Err(format!("`{}` is not something to compare", other.text())),
            None => Err("the condition stops after an operator".to_string()),
        }
    }
}

// ── evaluating ──────────────────────────────────────────────────────────────────

/// Whether the condition holds in this frame. `Err` is a condition that could not be answered —
/// which the caller turns into a stop, not into a skip. See the module doc.
pub(crate) fn holds(
    session: &Session,
    thread: Id,
    frame: &Frame,
    cond: &Cond,
) -> Result<bool, String> {
    match cond {
        Cond::Not(inner) => Ok(!holds(session, thread, frame, inner)?),
        // Short-circuit, so `order != null && order.total > 100` is a condition anyone can write:
        // without it the right-hand side walks through a null and the whole thing errors.
        Cond::And(a, b) => {
            Ok(holds(session, thread, frame, a)? && holds(session, thread, frame, b)?)
        }
        Cond::Or(a, b) => {
            Ok(holds(session, thread, frame, a)? || holds(session, thread, frame, b)?)
        }
        Cond::Truth(operand) => match read(session, thread, frame, operand)? {
            Datum::Bool(b) => Ok(b),
            other => Err(format!(
                "that is {} — a condition on its own has to be a boolean, so compare it",
                other.describe()
            )),
        },
        Cond::Compare(left, op, right) => {
            let a = read(session, thread, frame, left)?;
            let b = read(session, thread, frame, right)?;
            compare(&a, *op, &b)
        }
    }
}

/// One side's value: read out of the frame, or the literal as written.
fn read(
    session: &Session,
    thread: Id,
    frame: &Frame,
    operand: &Operand,
) -> Result<Datum, String> {
    match operand {
        Operand::Value(datum) => Ok(datum.clone()),
        Operand::Path(steps) => {
            let value = crate::debug_value::walk_path(session, thread, frame, steps)?;
            Ok(datum_of(session, value))
        }
    }
}

/// A JDWP value as something comparable.
///
/// A `char` becomes its code unit, which is what makes `c == 'x'` and `c == 120` the same
/// question — the literal `'x'` was lexed to a number for exactly this reason.
fn datum_of(session: &Session, value: Value) -> Datum {
    match value {
        Value::Boolean(b) => Datum::Bool(b),
        Value::Byte(v) => Datum::Int(v as i64),
        Value::Short(v) => Datum::Int(v as i64),
        Value::Int(v) => Datum::Int(v as i64),
        Value::Long(v) => Datum::Int(v),
        Value::Char(c) => Datum::Int(c as i64),
        Value::Float(v) => Datum::Float(v as f64),
        Value::Double(v) => Datum::Float(v),
        Value::Object { id: 0, .. } => Datum::Null,
        // A string compares as its text, which is the only comparison anyone means by
        // `name == "admin"` — identity would answer `false` for two equal strings.
        Value::Object { tag: Tag::String, id } => match string_value(&session.client, id) {
            Ok(text) => Datum::Text(text),
            Err(_) => Datum::Object(id),
        },
        Value::Object { id, .. } => Datum::Object(id),
        // Unreachable from a path walk — a field or an array slot is never `void` — and mapping it
        // to null is the reading that cannot mislead if it ever happens.
        Value::Void => Datum::Null,
    }
}

/// Compare two data, or say why they cannot be.
fn compare(a: &Datum, op: Op, b: &Datum) -> Result<bool, String> {
    // Numbers, including a char against a number: exact while both are whole, and floating the
    // moment either side is — so `i > 3` never goes through a float and `d < 0.5` does.
    if let (Some(x), Some(y)) = (as_int(a), as_int(b)) {
        return Ok(ordered(x.cmp(&y), op));
    }
    if let (Some(x), Some(y)) = (as_float(a), as_float(b)) {
        return Ok(match op {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            Op::Ge => x >= y,
        });
    }

    if op.is_ordering() {
        return Err(format!(
            "`{}` compares numbers, and this compares {} with {}",
            op.word(),
            a.describe(),
            b.describe()
        ));
    }
    let equal = match (a, b) {
        (Datum::Bool(x), Datum::Bool(y)) => x == y,
        (Datum::Text(x), Datum::Text(y)) => x == y,
        (Datum::Null, Datum::Null) => true,
        // A string is never null and null is never a string, which is the answer both of these
        // want — `name == null` on a set string is `false`, not an error.
        (Datum::Null, Datum::Text(_)) | (Datum::Text(_), Datum::Null) => false,
        (Datum::Null, Datum::Object(_)) | (Datum::Object(_), Datum::Null) => false,
        // Identity, which is what `==` means between two objects in Java too.
        (Datum::Object(x), Datum::Object(y)) => x == y,
        _ => {
            return Err(format!(
                "{} and {} are not comparable",
                a.describe(),
                b.describe()
            ))
        }
    };
    Ok(if op == Op::Eq { equal } else { !equal })
}

/// The whole-number view of a datum, when it has one.
fn as_int(d: &Datum) -> Option<i64> {
    match d {
        Datum::Int(v) => Some(*v),
        _ => None,
    }
}

/// The floating view, for a comparison where either side is fractional.
fn as_float(d: &Datum) -> Option<f64> {
    match d {
        Datum::Int(v) => Some(*v as f64),
        Datum::Float(v) => Some(*v),
        _ => None,
    }
}

fn ordered(ordering: std::cmp::Ordering, op: Op) -> bool {
    use std::cmp::Ordering::*;
    match op {
        Op::Eq => ordering == Equal,
        Op::Ne => ordering != Equal,
        Op::Lt => ordering == Less,
        Op::Le => ordering != Greater,
        Op::Gt => ordering == Greater,
        Op::Ge => ordering != Less,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> Operand {
        Operand::Path(debug_path::parse(name, debug_path::JAVA).expect("a path"))
    }

    fn parsed(text: &str) -> Cond {
        parse(text).expect("parses").expect("not empty")
    }

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
    fn a_comparison_with_a_number() {
        assert_eq!(parsed("i > 5"), Cond::Compare(path("i"), Op::Gt, Operand::Value(Datum::Int(5))));
        assert_eq!(
            parsed("count <= -3"),
            Cond::Compare(path("count"), Op::Le, Operand::Value(Datum::Int(-3)))
        );
        assert_eq!(
            parsed("ratio < 0.5"),
            Cond::Compare(path("ratio"), Op::Lt, Operand::Value(Datum::Float(0.5)))
        );
    }

    #[test]
    fn java_number_suffixes_are_tolerated() {
        assert_eq!(parsed("n == 10L"), Cond::Compare(path("n"), Op::Eq, Operand::Value(Datum::Int(10))));
        assert_eq!(
            parsed("x == 2f"),
            Cond::Compare(path("x"), Op::Eq, Operand::Value(Datum::Float(2.0)))
        );
        assert_eq!(
            parsed("mask == 0xFF"),
            Cond::Compare(path("mask"), Op::Eq, Operand::Value(Datum::Int(255)))
        );
    }

    #[test]
    fn the_left_hand_side_is_the_watch_grammar() {
        assert_eq!(
            parsed("order.customer.name == \"acme\""),
            Cond::Compare(
                path("order.customer.name"),
                Op::Eq,
                Operand::Value(Datum::Text("acme".into()))
            )
        );
        assert_eq!(
            parsed("items[2].price > 0"),
            Cond::Compare(path("items[2].price"), Op::Gt, Operand::Value(Datum::Int(0)))
        );
    }

    #[test]
    fn a_char_literal_is_its_code_unit_so_it_compares_with_a_char() {
        assert_eq!(parsed("c == 'x'"), Cond::Compare(path("c"), Op::Eq, Operand::Value(Datum::Int(120))));
        assert_eq!(parsed("c == '\\n'"), Cond::Compare(path("c"), Op::Eq, Operand::Value(Datum::Int(10))));
    }

    #[test]
    fn either_side_may_be_the_literal() {
        assert_eq!(parsed("0 < i"), Cond::Compare(Operand::Value(Datum::Int(0)), Op::Lt, path("i")));
    }

    #[test]
    fn a_bare_path_is_a_boolean_test_and_negation_works() {
        assert_eq!(parsed("done"), Cond::Truth(path("done")));
        assert_eq!(parsed("!order.paid"), Cond::Not(Box::new(Cond::Truth(path("order.paid")))));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // `a || b && c` is `a || (b && c)`.
        let Cond::Or(left, right) = parsed("a || b && c") else { panic!("an or at the top") };
        assert_eq!(*left, Cond::Truth(path("a")));
        assert!(matches!(*right, Cond::And(..)));
    }

    #[test]
    fn parentheses_override_the_precedence() {
        let Cond::And(left, _) = parsed("(a || b) && c") else { panic!("an and at the top") };
        assert!(matches!(*left, Cond::Or(..)));
    }

    #[test]
    fn null_and_the_booleans_are_literals_not_paths() {
        assert_eq!(
            parsed("order != null"),
            Cond::Compare(path("order"), Op::Ne, Operand::Value(Datum::Null))
        );
        assert_eq!(
            parsed("flag == true"),
            Cond::Compare(path("flag"), Op::Eq, Operand::Value(Datum::Bool(true)))
        );
    }

    #[test]
    fn an_enum_is_compared_through_the_name_field_every_constant_has() {
        assert_eq!(
            parsed("status.name == \"ACTIVE\""),
            Cond::Compare(path("status.name"), Op::Eq, Operand::Value(Datum::Text("ACTIVE".into())))
        );
    }

    #[test]
    fn what_it_refuses_it_refuses_by_name() {
        // Each of these is refused with the reason, not with "syntax error" — the message IS the
        // feature, because the alternative to understanding it is a breakpoint that never fires.
        assert!(refused("list.size() > 3").contains("calls a method"));
        assert!(refused("i + 1 == n").contains("arithmetic"));
        assert!(refused("i = 5").contains("=="));
        assert!(refused("a & b").contains("&&"));
        assert!(refused("i >").contains("stops after an operator"));
        assert!(refused("(i > 1").contains("never closed"));
        assert!(refused("name == \"unclosed").contains("never closed"));
    }

    #[test]
    fn a_constant_condition_is_refused_because_the_program_could_not_change_the_answer() {
        // Both shapes: a comparison of two literals, and a literal on its own. Either always stops
        // or never does, and neither would ever explain itself.
        assert!(refused("1 == 1").contains("read something"));
        assert!(refused("true").contains("constant"));
    }

    #[test]
    fn numbers_compare_as_numbers_whichever_width_they_came_from() {
        assert_eq!(compare(&Datum::Int(5), Op::Gt, &Datum::Int(3)), Ok(true));
        assert_eq!(compare(&Datum::Int(3), Op::Ge, &Datum::Float(3.0)), Ok(true));
        assert_eq!(compare(&Datum::Float(0.25), Op::Lt, &Datum::Int(1)), Ok(true));
        assert_eq!(compare(&Datum::Int(120), Op::Eq, &Datum::Int(120)), Ok(true));
    }

    #[test]
    fn null_compares_with_anything_that_could_be_null_and_never_orders() {
        assert_eq!(compare(&Datum::Null, Op::Eq, &Datum::Null), Ok(true));
        assert_eq!(compare(&Datum::Object(7), Op::Ne, &Datum::Null), Ok(true));
        assert_eq!(compare(&Datum::Text("x".into()), Op::Eq, &Datum::Null), Ok(false));
        assert!(compare(&Datum::Object(7), Op::Lt, &Datum::Null).is_err());
    }

    #[test]
    fn a_string_compares_by_its_text_and_an_object_by_identity() {
        assert_eq!(compare(&Datum::Text("a".into()), Op::Eq, &Datum::Text("a".into())), Ok(true));
        assert_eq!(compare(&Datum::Object(3), Op::Eq, &Datum::Object(3)), Ok(true));
        assert_eq!(compare(&Datum::Object(3), Op::Ne, &Datum::Object(4)), Ok(true));
        // Ordering two objects is the mistake worth naming rather than answering.
        assert!(compare(&Datum::Text("a".into()), Op::Lt, &Datum::Text("b".into())).is_err());
    }

    #[test]
    fn comparing_things_of_different_kinds_says_which_two() {
        let err = compare(&Datum::Bool(true), Op::Eq, &Datum::Int(1)).unwrap_err();
        assert!(err.contains("not comparable"), "{err}");
    }
}
