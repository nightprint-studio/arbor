//! Text → tokens.
//!
//! Every character this grammar has no use for is refused **here, by name**: a single `=` says to
//! write `==`, `++` says nothing is ever incremented, `->` says lambdas are not evaluated. The
//! alternative — "unexpected character" — leaves the user guessing why a perfectly ordinary Java
//! expression is not accepted, which is how a condition ends up never firing.

use crate::ast::Lit;
use crate::syntax::{Lang, Syntax};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Lit(Lit),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Comma,
    Question,
    Colon,
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
}

impl Tok {
    /// How it was written, for a message.
    pub fn text(&self) -> String {
        match self {
            Tok::Ident(n) => n.clone(),
            Tok::Lit(l) => l.to_string(),
            Tok::LParen => "(".into(),
            Tok::RParen => ")".into(),
            Tok::LBracket => "[".into(),
            Tok::RBracket => "]".into(),
            Tok::Dot => ".".into(),
            Tok::Comma => ",".into(),
            Tok::Question => "?".into(),
            Tok::Colon => ":".into(),
            Tok::Bang => "!".into(),
            Tok::Plus => "+".into(),
            Tok::Minus => "-".into(),
            Tok::Star => "*".into(),
            Tok::Slash => "/".into(),
            Tok::Percent => "%".into(),
            Tok::Lt => "<".into(),
            Tok::Le => "<=".into(),
            Tok::Gt => ">".into(),
            Tok::Ge => ">=".into(),
            Tok::EqEq => "==".into(),
            Tok::Ne => "!=".into(),
            Tok::AndAnd => "&&".into(),
            Tok::OrOr => "||".into(),
        }
    }
}

const NO_BITWISE: &str = "bitwise operators are not evaluated";
const NO_ASSIGN: &str = "assignment is never evaluated — an expression reads the program and changes nothing";

pub fn lex(text: &str, syntax: Syntax) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // The digit after a `.` is a tuple index (`t.0.1`), not the start of a float.
        if c.is_ascii_digit() && out.last() == Some(&Tok::Dot) {
            let mut end = i;
            while chars.get(end).is_some_and(char::is_ascii_digit) {
                end += 1;
            }
            out.push(Tok::Ident(chars[i..end].iter().collect()));
            i = end;
            continue;
        }
        if c.is_ascii_digit() {
            let (lit, end) = number(&chars, i, syntax.lang)?;
            out.push(Tok::Lit(lit));
            i = end;
            continue;
        }
        if is_name_start(c) {
            let mut end = i;
            while chars.get(end).is_some_and(|c| is_name_part(*c)) {
                end += 1;
            }
            let word: String = chars[i..end].iter().collect();
            out.push(keyword_or_name(word, syntax)?);
            i = end;
            continue;
        }
        let (tok, width) = match (c, next) {
            ('"', _) => {
                let (s, end) = quoted(&chars, i, '"')?;
                out.push(Tok::Lit(Lit::Str(s)));
                i = end;
                continue;
            }
            ('\'', _) => {
                let (s, end) = quoted(&chars, i, '\'')?;
                let mut it = s.chars();
                let (Some(ch), None) = (it.next(), it.next()) else {
                    return Err("a `'…'` literal holds exactly one character".to_string());
                };
                out.push(Tok::Lit(Lit::Char(ch as u32)));
                i = end;
                continue;
            }
            ('&', Some('&')) => (Tok::AndAnd, 2),
            ('|', Some('|')) => (Tok::OrOr, 2),
            ('&', _) => return Err(format!("write `&&` — {NO_BITWISE}")),
            ('|', _) => return Err(format!("write `||` — {NO_BITWISE}")),
            ('^' | '~', _) => return Err(NO_BITWISE.to_string()),
            ('=', Some('=')) => (Tok::EqEq, 2),
            ('=', Some('>')) => return Err("lambdas are not evaluated".to_string()),
            ('=', _) => return Err(format!("write `==` to compare — {NO_ASSIGN}")),
            ('!', Some('=')) => (Tok::Ne, 2),
            ('!', _) => (Tok::Bang, 1),
            ('<', Some('=')) => (Tok::Le, 2),
            ('<', Some('<')) => return Err(NO_BITWISE.to_string()),
            ('<', _) => (Tok::Lt, 1),
            ('>', Some('=')) => (Tok::Ge, 2),
            ('>', Some('>')) => return Err(NO_BITWISE.to_string()),
            ('>', _) => (Tok::Gt, 1),
            ('+', Some('+')) | ('-', Some('-')) => {
                return Err("increments and decrements are never evaluated".to_string())
            }
            ('+' | '-' | '*' | '/' | '%', Some('=')) => return Err(NO_ASSIGN.to_string()),
            ('-', Some('>')) => return Err("lambdas are not evaluated".to_string()),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('%', _) => (Tok::Percent, 1),
            (':', Some(':')) => {
                return Err(
                    "`::` paths and method references are not evaluated — read the value itself \
                     (expand it in the variables tree to see which variant it holds)"
                        .to_string(),
                )
            }
            (':', _) => (Tok::Colon, 1),
            ('?', _) => (Tok::Question, 1),
            (',', _) => (Tok::Comma, 1),
            ('.', _) => (Tok::Dot, 1),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('[', _) => (Tok::LBracket, 1),
            (']', _) => (Tok::RBracket, 1),
            (other, _) => return Err(format!("`{other}` has no meaning in an expression")),
        };
        out.push(tok);
        i += width;
    }
    if out.is_empty() {
        return Err("there is nothing to evaluate".to_string());
    }
    Ok(out)
}

fn keyword_or_name(word: String, syntax: Syntax) -> Result<Tok, String> {
    Ok(match word.as_str() {
        "true" => Tok::Lit(Lit::Bool(true)),
        "false" => Tok::Lit(Lit::Bool(false)),
        "null" if syntax.lang == Lang::Java => Tok::Lit(Lit::Null),
        "null" => {
            return Err(
                "Rust has no `null` — an `Option` or a raw pointer is read by expanding it"
                    .to_string(),
            )
        }
        _ => Tok::Ident(word),
    })
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_name_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// The contents of a quoted literal, and where it ended.
fn quoted(chars: &[char], from: usize, quote: char) -> Result<(String, usize), String> {
    let mut i = from + 1;
    let mut out = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                let Some(&esc) = chars.get(i + 1) else { break };
                if esc == 'u' {
                    let hex: String = chars[i + 2..].iter().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16)
                        .ok()
                        .filter(|_| hex.chars().count() == 4)
                        .and_then(char::from_u32)
                        .ok_or_else(|| format!("`\\u{hex}` is not a character"))?;
                    out.push(code);
                    i += 6;
                    continue;
                }
                out.push(match esc {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '0' => '\0',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
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

/// A number: decimal, hex, binary or (Java) octal; integer or floating; with the language's
/// suffixes understood.
fn number(chars: &[char], from: usize, lang: Lang) -> Result<(Lit, usize), String> {
    let radix = match (chars[from], chars.get(from + 1)) {
        ('0', Some('x' | 'X')) => 16,
        ('0', Some('b' | 'B')) => 2,
        _ => 10,
    };
    if radix != 10 {
        return radix_number(chars, from + 2, radix, lang);
    }

    let mut i = from;
    let mut floating = false;
    while let Some(&c) = chars.get(i) {
        if c.is_ascii_digit() || c == '_' {
            i += 1;
        } else if c == '.' && !floating && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
            floating = true;
            i += 1;
        } else if matches!(c, 'e' | 'E') && exponent_follows(chars, i) {
            floating = true;
            i += 1;
            if matches!(chars.get(i), Some('+' | '-')) {
                i += 1;
            }
        } else {
            break;
        }
    }
    let digits: String = chars[from..i].iter().filter(|c| **c != '_').collect();
    let (suffix, end) = suffix(chars, i);
    let kind = suffix_kind(&suffix, lang, &digits)?;

    if floating || kind == Suffix::Float {
        let value: f64 = digits.parse().map_err(|_| format!("`{digits}` is not a number"))?;
        return Ok((Lit::Double(value), end));
    }
    // Java octal: a leading zero and nothing but 0-7.
    let value = if lang == Lang::Java && digits.len() > 1 && digits.starts_with('0') {
        i64::from_str_radix(&digits[1..], 8)
            .map_err(|_| format!("`{digits}` is not an octal number"))?
    } else {
        digits.parse::<i64>().map_err(|_| format!("`{digits}` is too large a whole number"))?
    };
    Ok((integer(value, kind, lang)?, end))
}

/// Whether the `e` at `at` begins an exponent (`1e5`, `2E-3`) rather than a stray letter.
fn exponent_follows(chars: &[char], at: usize) -> bool {
    match chars.get(at + 1) {
        Some(c) if c.is_ascii_digit() => true,
        Some('+' | '-') => chars.get(at + 2).is_some_and(char::is_ascii_digit),
        _ => false,
    }
}

fn radix_number(chars: &[char], start: usize, radix: u32, lang: Lang) -> Result<(Lit, usize), String> {
    let mut end = start;
    while chars.get(end).is_some_and(|c| c.is_digit(radix) || *c == '_') {
        end += 1;
    }
    let digits: String = chars[start..end].iter().filter(|c| **c != '_').collect();
    let (suffix, after) = suffix(chars, end);
    let kind = suffix_kind(&suffix, lang, &digits)?;
    let prefix = if radix == 16 { "0x" } else { "0b" };
    let unsigned = u64::from_str_radix(&digits, radix)
        .map_err(|_| format!("`{prefix}{digits}` is not a whole number this can evaluate"))?;
    // A hex literal is a bit pattern: Java's `0xFFFFFFFF` is -1 as an `int`.
    let value = match (lang, kind) {
        (Lang::Java, Suffix::Long) => unsigned as i64,
        (Lang::Java, _) if unsigned <= u64::from(u32::MAX) => i64::from(unsigned as u32 as i32),
        _ => i64::try_from(unsigned)
            .map_err(|_| format!("`{prefix}{digits}` is too large a whole number"))?,
    };
    Ok((integer(value, kind, lang)?, after))
}

/// What an integer literal becomes: Java's width follows its suffix (and, unsuffixed, its size);
/// Rust's integers are all read as `i64`.
fn integer(value: i64, kind: Suffix, lang: Lang) -> Result<Lit, String> {
    Ok(match (lang, kind) {
        (Lang::Java, Suffix::Long) | (Lang::Rust, _) => Lit::Long(value),
        (Lang::Java, _) => match i32::try_from(value) {
            Ok(v) => Lit::Int(v),
            // `3000000000` is an error in Java without its `L`; here it is read as the long it
            // obviously means rather than refused over a missing letter.
            Err(_) => Lit::Long(value),
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suffix {
    None,
    Long,
    Float,
    /// Rust's `u8`, `i32`, … — the width is not modelled.
    Width,
}

/// The letters glued to the end of a number, and where they stop.
fn suffix(chars: &[char], at: usize) -> (String, usize) {
    let mut end = at;
    if chars.get(end) == Some(&'_') {
        end += 1;
    }
    while chars.get(end).is_some_and(|c| c.is_ascii_alphanumeric()) {
        end += 1;
    }
    (chars[at..end].iter().collect::<String>().trim_start_matches('_').to_string(), end)
}

fn suffix_kind(suffix: &str, lang: Lang, digits: &str) -> Result<Suffix, String> {
    Ok(match (lang, suffix) {
        (_, "") => Suffix::None,
        (Lang::Java, "L" | "l") => Suffix::Long,
        (Lang::Java, "f" | "F" | "d" | "D") => Suffix::Float,
        (Lang::Rust, "f32" | "f64") => Suffix::Float,
        (Lang::Rust, s)
            if s.strip_prefix(['i', 'u']).is_some_and(|w| {
                matches!(w, "8" | "16" | "32" | "64" | "128" | "size")
            }) =>
        {
            Suffix::Width
        }
        _ => return Err(format!("`{digits}{suffix}` is not a number")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{JAVA, RUST};

    fn java(text: &str) -> Vec<Tok> {
        lex(text, JAVA).expect("lexes")
    }

    fn refused(text: &str, syntax: Syntax) -> String {
        lex(text, syntax).expect_err("refused")
    }

    #[test]
    fn numbers_take_the_width_their_suffix_names() {
        assert_eq!(java("5"), vec![Tok::Lit(Lit::Int(5))]);
        assert_eq!(java("10L"), vec![Tok::Lit(Lit::Long(10))]);
        assert_eq!(java("3000000000"), vec![Tok::Lit(Lit::Long(3_000_000_000))]);
        assert_eq!(java("2f"), vec![Tok::Lit(Lit::Double(2.0))]);
        assert_eq!(java("0.5"), vec![Tok::Lit(Lit::Double(0.5))]);
        assert_eq!(java("1e3"), vec![Tok::Lit(Lit::Double(1000.0))]);
        assert_eq!(java("1_000"), vec![Tok::Lit(Lit::Int(1000))]);
        assert_eq!(java("010"), vec![Tok::Lit(Lit::Int(8))], "a leading zero is octal in Java");
    }

    #[test]
    fn a_hex_literal_is_a_bit_pattern() {
        assert_eq!(java("0xFF"), vec![Tok::Lit(Lit::Int(255))]);
        assert_eq!(java("0xFFFFFFFF"), vec![Tok::Lit(Lit::Int(-1))]);
        assert_eq!(java("0xFFFFFFFFL"), vec![Tok::Lit(Lit::Long(4_294_967_295))]);
        assert_eq!(java("0b101"), vec![Tok::Lit(Lit::Int(5))]);
    }

    #[test]
    fn every_integer_in_rust_is_a_long() {
        assert_eq!(lex("42u8", RUST).unwrap(), vec![Tok::Lit(Lit::Long(42))]);
        assert_eq!(lex("1_000_i64", RUST).unwrap(), vec![Tok::Lit(Lit::Long(1000))]);
        assert_eq!(lex("2.5f32", RUST).unwrap(), vec![Tok::Lit(Lit::Double(2.5))]);
        assert!(lex("5L", RUST).is_err(), "`L` is Java's suffix");
    }

    #[test]
    fn a_digit_after_a_dot_is_a_tuple_index_not_a_float() {
        assert_eq!(
            lex("t.0.1", RUST).unwrap(),
            vec![
                Tok::Ident("t".into()),
                Tok::Dot,
                Tok::Ident("0".into()),
                Tok::Dot,
                Tok::Ident("1".into()),
            ]
        );
    }

    #[test]
    fn a_char_and_a_string_unescape() {
        assert_eq!(java("'x'"), vec![Tok::Lit(Lit::Char(120))]);
        assert_eq!(java("'\\n'"), vec![Tok::Lit(Lit::Char(10))]);
        assert_eq!(java("\"a\\tb\""), vec![Tok::Lit(Lit::Str("a\tb".into()))]);
        assert_eq!(java("'\\u0041'"), vec![Tok::Lit(Lit::Char(65))]);
        assert!(refused("'ab'", JAVA).contains("exactly one character"));
        assert!(refused("\"open", JAVA).contains("never closed"));
    }

    #[test]
    fn what_it_refuses_it_refuses_by_name() {
        assert!(refused("a = 1", JAVA).contains("=="));
        assert!(refused("a += 1", JAVA).contains("assignment"));
        assert!(refused("a++", JAVA).contains("increments"));
        assert!(refused("a & b", JAVA).contains("&&"));
        assert!(refused("a | b", JAVA).contains("||"));
        assert!(refused("a << 2", JAVA).contains("bitwise"));
        assert!(refused("x -> x", JAVA).contains("lambda"));
        assert!(refused("|x| x", RUST).contains("||"), "a Rust closure is a bar, refused as one");
        assert!(refused("A::b", RUST).contains("::"));
        assert!(refused("a;", JAVA).contains(';'));
    }

    #[test]
    fn null_is_java_only() {
        assert_eq!(java("null"), vec![Tok::Lit(Lit::Null)]);
        assert!(refused("null", RUST).contains("no `null`"));
    }
}
