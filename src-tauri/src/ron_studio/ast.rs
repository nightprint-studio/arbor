//! Tag-preserving RON parser.
//!
//! The `ron` crate's `Value` type officially does NOT support enum variants
//! (per its own docs: "This does not support enums (because Value does not
//! store them)"). When parsing `element: Dark` or `variant: Action((..))`
//! through `ron::from_str::<ron::Value>`, the variant tag is dropped — the
//! tree shows `()` or a bare `Seq([..])` with no indication of which variant
//! the user wrote.
//!
//! For RON Studio that's a real loss: half the value of a typed game-config
//! schema is seeing "this is `Dark`, not just any unit value". So we run a
//! tiny recursive-descent parser of our own that builds a `RonAst` carrying
//! variant tags as first-class data.
//!
//! Scope: the syntax produced by `ron::to_string_pretty` plus the usual
//! hand-written forms (comments, raw strings, char literals, extension
//! attributes, suffix-less numbers). Anything more exotic falls through to
//! a parse error and the modal can decide whether to fall back to the
//! lossy `ron::Value` path.

use std::fmt::Write as _;

/// Tag-preserving RON value tree.
///
/// `PartialEq` is derived so the structural-diff walker can compare
/// subtrees in O(n) without re-serialising. `Float(f64)` makes `Eq`
/// impossible (NaN != NaN) so we stop at `PartialEq` — diff treats
/// any NaN-vs-anything comparison as "different", which is correct
/// for the user's intent (they explicitly wrote that value).
#[derive(Debug, Clone, PartialEq)]
pub enum RonAst {
    Unit,
    Bool(bool),
    Char(char),
    Int(i64),
    Float(f64),
    String(String),
    Option(Option<Box<RonAst>>),
    List(Vec<RonAst>),
    /// `{ k: v, ... }`. Keys can be any value; we store them as RonAst.
    Map(Vec<(RonAst, RonAst)>),
    /// `(field: v, ...)` or `Name(field: v, ...)`. `name` is `Some` for
    /// named structs and enum struct-variants.
    Struct {
        name:   Option<String>,
        fields: Vec<(String, RonAst)>,
    },
    /// `(v, v, ...)` or `Name(v, v)` or `Name(v)`. `name` is `Some` for
    /// named tuples and enum tuple-variants (including newtype variants).
    Tuple {
        name:  Option<String>,
        items: Vec<RonAst>,
    },
    /// A bare identifier — typically a unit enum variant like `Dark`,
    /// `North`, etc. Parsed as `Ident` so the consumer can render the
    /// actual name instead of a generic `()`.
    UnitVariant(String),
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub col:  usize,
    pub msg:  String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, col {}: {}", self.line, self.col, self.msg)
    }
}

pub fn parse(src: &str) -> Result<RonAst, ParseError> {
    // Strip a leading BOM if present so the cursor lands on real content.
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    let mut p = Parser { src, pos: 0 };
    p.skip_extensions();
    let v = p.parse_value()?;
    p.skip_ws_and_comments();
    if p.pos != p.src.len() {
        return Err(p.error_here("trailing input after value"));
    }
    Ok(v)
}

// ── Parser ──────────────────────────────────────────────────────────────────

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_value(&mut self) -> Result<RonAst, ParseError> {
        self.skip_ws_and_comments();
        let c = self.peek_char().ok_or_else(|| self.error_here("unexpected end of input"))?;
        match c {
            '(' => { self.advance(c.len_utf8()); self.parse_paren_value(None) }
            '[' => { self.advance(1); self.parse_list() }
            '{' => { self.advance(1); self.parse_map() }
            '"' => self.parse_string(),
            '\'' => self.parse_char(),
            '-' | '+' => self.parse_number(),
            '0'..='9' => self.parse_number(),
            'r' if self.starts_with(b"r\"") || self.starts_with(b"r#") => self.parse_raw_string(),
            c if is_ident_start(c) => self.parse_ident_or_typed_value(),
            _ => Err(self.error_here(&format!("unexpected character '{c}'"))),
        }
    }

    /// We've just consumed the opening `(`; figure out whether this is an
    /// anonymous struct (`(field: val, ..)`) or a tuple (`(val, val)`).
    /// Empty `()` resolves to a zero-field Tuple (which also covers the
    /// unit type).
    fn parse_paren_value(&mut self, name: Option<String>) -> Result<RonAst, ParseError> {
        self.skip_ws_and_comments();
        if self.peek_char() == Some(')') {
            self.advance(1);
            // For an unnamed empty `()` the canonical interpretation is Unit;
            // for `Name()` it's still a tuple-like variant with no payload.
            return Ok(match name {
                None       => RonAst::Unit,
                Some(name) => RonAst::Tuple { name: Some(name), items: vec![] },
            });
        }
        if self.looks_like_struct_start() {
            let mut fields = Vec::<(String, RonAst)>::new();
            loop {
                self.skip_ws_and_comments();
                let field = self.consume_ident()?;
                self.skip_ws_and_comments();
                self.expect(':')?;
                let value = self.parse_value()?;
                fields.push((field, value));
                self.skip_ws_and_comments();
                match self.peek_char() {
                    Some(',') => { self.advance(1); self.skip_ws_and_comments();
                        if self.peek_char() == Some(')') { self.advance(1); break; }
                    }
                    Some(')') => { self.advance(1); break; }
                    Some(other) => return Err(self.error_here(&format!("expected ',' or ')' inside struct, got '{other}'"))),
                    None => return Err(self.error_here("unexpected EOF inside struct literal")),
                }
            }
            Ok(RonAst::Struct { name, fields })
        } else {
            let mut items = Vec::<RonAst>::new();
            loop {
                let value = self.parse_value()?;
                items.push(value);
                self.skip_ws_and_comments();
                match self.peek_char() {
                    Some(',') => { self.advance(1); self.skip_ws_and_comments();
                        if self.peek_char() == Some(')') { self.advance(1); break; }
                    }
                    Some(')') => { self.advance(1); break; }
                    Some(other) => return Err(self.error_here(&format!("expected ',' or ')' inside tuple, got '{other}'"))),
                    None => return Err(self.error_here("unexpected EOF inside tuple literal")),
                }
            }
            Ok(RonAst::Tuple { name, items })
        }
    }

    fn parse_list(&mut self) -> Result<RonAst, ParseError> {
        let mut items = Vec::<RonAst>::new();
        self.skip_ws_and_comments();
        if self.peek_char() == Some(']') { self.advance(1); return Ok(RonAst::List(items)); }
        loop {
            let v = self.parse_value()?;
            items.push(v);
            self.skip_ws_and_comments();
            match self.peek_char() {
                Some(',') => { self.advance(1); self.skip_ws_and_comments();
                    if self.peek_char() == Some(']') { self.advance(1); break; }
                }
                Some(']') => { self.advance(1); break; }
                Some(other) => return Err(self.error_here(&format!("expected ',' or ']' inside list, got '{other}'"))),
                None => return Err(self.error_here("unexpected EOF inside list literal")),
            }
        }
        Ok(RonAst::List(items))
    }

    fn parse_map(&mut self) -> Result<RonAst, ParseError> {
        let mut pairs = Vec::<(RonAst, RonAst)>::new();
        self.skip_ws_and_comments();
        if self.peek_char() == Some('}') { self.advance(1); return Ok(RonAst::Map(pairs)); }
        loop {
            let k = self.parse_value()?;
            self.skip_ws_and_comments();
            self.expect(':')?;
            let v = self.parse_value()?;
            pairs.push((k, v));
            self.skip_ws_and_comments();
            match self.peek_char() {
                Some(',') => { self.advance(1); self.skip_ws_and_comments();
                    if self.peek_char() == Some('}') { self.advance(1); break; }
                }
                Some('}') => { self.advance(1); break; }
                Some(other) => return Err(self.error_here(&format!("expected ',' or '}}' inside map, got '{other}'"))),
                None => return Err(self.error_here("unexpected EOF inside map literal")),
            }
        }
        Ok(RonAst::Map(pairs))
    }

    fn parse_ident_or_typed_value(&mut self) -> Result<RonAst, ParseError> {
        let ident = self.consume_ident()?;
        // Reserved literals first.
        match ident.as_str() {
            "true"  => return Ok(RonAst::Bool(true)),
            "false" => return Ok(RonAst::Bool(false)),
            "None"  => return Ok(RonAst::Option(None)),
            "Some"  => {
                self.skip_ws_and_comments();
                self.expect('(')?;
                let inner = self.parse_value()?;
                self.skip_ws_and_comments();
                self.expect(')')?;
                return Ok(RonAst::Option(Some(Box::new(inner))));
            }
            _ => {}
        }
        // After an identifier, `(` opens a tagged tuple/struct.
        self.skip_ws_and_comments();
        if self.peek_char() == Some('(') {
            self.advance(1);
            self.parse_paren_value(Some(ident))
        } else {
            Ok(RonAst::UnitVariant(ident))
        }
    }

    fn parse_string(&mut self) -> Result<RonAst, ParseError> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.peek_char() {
                Some('"') => { self.advance(1); break; }
                Some('\\') => {
                    self.advance(1);
                    let esc = self.peek_char().ok_or_else(|| self.error_here("unterminated escape"))?;
                    self.advance(esc.len_utf8());
                    match esc {
                        '"'  => s.push('"'),
                        '\\' => s.push('\\'),
                        '\'' => s.push('\''),
                        'n'  => s.push('\n'),
                        'r'  => s.push('\r'),
                        't'  => s.push('\t'),
                        '0'  => s.push('\0'),
                        'x'  => {
                            let h1 = self.consume_hex_digit()?;
                            let h2 = self.consume_hex_digit()?;
                            let v = ((h1 as u32) << 4) | (h2 as u32);
                            if let Some(c) = char::from_u32(v) { s.push(c); }
                        }
                        'u'  => {
                            self.expect('{')?;
                            let mut v: u32 = 0;
                            let mut n = 0;
                            while let Some(c) = self.peek_char() {
                                if c == '}' { break; }
                                let d = c.to_digit(16).ok_or_else(|| self.error_here("invalid hex digit in unicode escape"))?;
                                v = (v << 4) | d;
                                self.advance(c.len_utf8());
                                n += 1;
                                if n > 6 { return Err(self.error_here("unicode escape too long")); }
                            }
                            self.expect('}')?;
                            if let Some(c) = char::from_u32(v) { s.push(c); }
                        }
                        other => return Err(self.error_here(&format!("unknown escape sequence \\{other}"))),
                    }
                }
                Some(c) => { s.push(c); self.advance(c.len_utf8()); }
                None => return Err(self.error_here("unterminated string literal")),
            }
        }
        Ok(RonAst::String(s))
    }

    fn parse_raw_string(&mut self) -> Result<RonAst, ParseError> {
        self.expect('r')?;
        let mut hashes = 0;
        while self.peek_char() == Some('#') { self.advance(1); hashes += 1; }
        self.expect('"')?;
        let start = self.pos;
        let needle: String = std::iter::once('"').chain(std::iter::repeat('#').take(hashes)).collect();
        let end = self.src[self.pos..].find(&needle)
            .ok_or_else(|| self.error_here("unterminated raw string"))?;
        let s = self.src[start..start + end].to_string();
        self.pos = start + end + needle.len();
        Ok(RonAst::String(s))
    }

    fn parse_char(&mut self) -> Result<RonAst, ParseError> {
        self.expect('\'')?;
        let c = if self.peek_char() == Some('\\') {
            self.advance(1);
            let esc = self.peek_char().ok_or_else(|| self.error_here("unterminated escape"))?;
            self.advance(esc.len_utf8());
            match esc {
                '\'' => '\'',
                '\\' => '\\',
                '"'  => '"',
                'n'  => '\n',
                'r'  => '\r',
                't'  => '\t',
                '0'  => '\0',
                'u'  => {
                    self.expect('{')?;
                    let mut v: u32 = 0;
                    while let Some(c) = self.peek_char() {
                        if c == '}' { break; }
                        let d = c.to_digit(16).ok_or_else(|| self.error_here("invalid hex digit"))?;
                        v = (v << 4) | d;
                        self.advance(c.len_utf8());
                    }
                    self.expect('}')?;
                    char::from_u32(v).unwrap_or('\u{FFFD}')
                }
                other => return Err(self.error_here(&format!("unknown char escape \\{other}"))),
            }
        } else {
            let c = self.peek_char().ok_or_else(|| self.error_here("empty char literal"))?;
            self.advance(c.len_utf8());
            c
        };
        self.expect('\'')?;
        Ok(RonAst::Char(c))
    }

    fn parse_number(&mut self) -> Result<RonAst, ParseError> {
        let start = self.pos;
        if self.peek_char() == Some('-') || self.peek_char() == Some('+') { self.advance(1); }
        // Hex / binary / octal — only valid as integers.
        if self.peek_char() == Some('0') && matches!(self.peek_char_at(1), Some('x') | Some('o') | Some('b')) {
            self.advance(1);
            let base_marker = self.peek_char().unwrap();
            self.advance(1);
            let radix = match base_marker { 'x' => 16, 'o' => 8, 'b' => 2, _ => unreachable!() };
            let mut digits = String::new();
            while let Some(c) = self.peek_char() {
                if c == '_' { self.advance(1); continue; }
                if c.to_digit(radix).is_some() { digits.push(c); self.advance(c.len_utf8()); }
                else { break; }
            }
            // Strip optional integer-type suffix like `_i32`, `i32`, `u64` etc.
            self.skip_number_suffix();
            let sign = self.src[start..].starts_with('-');
            let raw  = i128::from_str_radix(&digits, radix).map_err(|_| self.error_here("invalid integer"))?;
            let v: i64 = (if sign { -raw } else { raw }).try_into().map_err(|_| self.error_here("integer out of i64 range"))?;
            return Ok(RonAst::Int(v));
        }
        let mut is_float = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '_' { self.advance(c.len_utf8()); }
            else if c == '.' && !is_float {
                // Make sure this is a decimal point, not the start of a `..` range
                if self.peek_char_at(1) == Some('.') { break; }
                is_float = true;
                self.advance(1);
            }
            else if (c == 'e' || c == 'E') && is_float_compat(self) {
                is_float = true;
                self.advance(1);
                if matches!(self.peek_char(), Some('+') | Some('-')) { self.advance(1); }
            }
            else { break; }
        }
        // Optional type suffix.
        self.skip_number_suffix();
        let raw = &self.src[start..self.pos];
        // Trim a trailing suffix the loop above didn't consume (shouldn't happen,
        // but be defensive).
        let raw = raw.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '_');
        let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
        if is_float {
            let f: f64 = cleaned.parse().map_err(|_| self.error_here("invalid float"))?;
            Ok(RonAst::Float(f))
        } else {
            // Special-case: leading + needs stripping for i64::from_str_radix
            let s = if let Some(rest) = cleaned.strip_prefix('+') { rest.to_string() } else { cleaned };
            let v: i64 = s.parse().map_err(|_| self.error_here("invalid integer"))?;
            Ok(RonAst::Int(v))
        }
    }

    fn skip_number_suffix(&mut self) {
        // `42i32`, `42_u8`, `3.14f64`, etc.
        let snap = self.pos;
        if self.peek_char() == Some('_') { self.advance(1); }
        let after_underscore = self.pos;
        let mut saw_letter = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() && (c.is_ascii_alphabetic() || saw_letter) {
                if c.is_ascii_alphabetic() { saw_letter = true; }
                self.advance(c.len_utf8());
            } else { break; }
        }
        if !saw_letter {
            // It wasn't really a suffix; rewind.
            self.pos = snap;
            return;
        }
        // Successfully consumed a suffix (with optional leading underscore).
        let _ = after_underscore; // borrow appeasement
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn looks_like_struct_start(&self) -> bool {
        // Peek-only: identifier (start with letter/_) followed by optional
        // whitespace then ':' (and NOT `::`, which is a path separator).
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        while i < bytes.len() && (bytes[i] as char).is_whitespace() { i += 1; }
        let ident_start = i;
        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_alphanumeric() || b == b'_' { i += 1; } else { break; }
        }
        if i == ident_start { return false; }
        // Skip ws
        while i < bytes.len() && (bytes[i] as char).is_whitespace() { i += 1; }
        // Must be a single ':' (not '::')
        if i >= bytes.len() || bytes[i] != b':' { return false; }
        if i + 1 < bytes.len() && bytes[i + 1] == b':' { return false; }
        true
    }

    fn skip_extensions(&mut self) {
        // `#![enable(...)]` lines at the very top of the file. Skip them
        // without trying to interpret. Multiple consecutive ones allowed.
        loop {
            self.skip_ws_and_comments();
            if !self.starts_with(b"#!") && !self.starts_with(b"#[") { break; }
            let close = self.src[self.pos..].find(']').map(|i| self.pos + i + 1);
            match close {
                Some(end) => self.pos = end,
                None => break,
            }
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let snap = self.pos;
            while let Some(c) = self.peek_char() {
                if c.is_whitespace() { self.advance(c.len_utf8()); } else { break; }
            }
            if self.starts_with(b"//") {
                while let Some(c) = self.peek_char() {
                    if c == '\n' { break; }
                    self.advance(c.len_utf8());
                }
                continue;
            }
            if self.starts_with(b"/*") {
                self.advance(2);
                let mut depth = 1;
                while depth > 0 {
                    if self.starts_with(b"/*") { self.advance(2); depth += 1; }
                    else if self.starts_with(b"*/") { self.advance(2); depth -= 1; }
                    else if let Some(c) = self.peek_char() { self.advance(c.len_utf8()); }
                    else { break; }
                }
                continue;
            }
            if self.pos == snap { break; }
        }
    }

    fn peek_char(&self) -> Option<char> { self.src[self.pos..].chars().next() }
    fn peek_char_at(&self, offset: usize) -> Option<char> {
        let mut chars = self.src[self.pos..].chars();
        for _ in 0..offset { chars.next()?; }
        chars.next()
    }
    fn starts_with(&self, bytes: &[u8]) -> bool {
        self.src.as_bytes().get(self.pos..self.pos + bytes.len()) == Some(bytes)
    }
    fn advance(&mut self, n: usize) { self.pos += n; }

    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        if self.peek_char() == Some(c) { self.advance(c.len_utf8()); Ok(()) }
        else {
            let got = self.peek_char()
                .map(|ch| format!("'{ch}'"))
                .unwrap_or_else(|| "EOF".to_string());
            Err(self.error_here(&format!("expected '{c}', got {got}")))
        }
    }

    fn consume_ident(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let first = self.peek_char().ok_or_else(|| self.error_here("expected identifier"))?;
        if !is_ident_start(first) {
            return Err(self.error_here(&format!("expected identifier, got '{first}'")));
        }
        self.advance(first.len_utf8());
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' { self.advance(c.len_utf8()); }
            else { break; }
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn consume_hex_digit(&mut self) -> Result<u8, ParseError> {
        let c = self.peek_char().ok_or_else(|| self.error_here("expected hex digit"))?;
        let v = c.to_digit(16).ok_or_else(|| self.error_here("invalid hex digit"))?;
        self.advance(c.len_utf8());
        Ok(v as u8)
    }

    fn error_here(&self, msg: &str) -> ParseError {
        let mut line = 1;
        let mut col = 1;
        for c in self.src[..self.pos].chars() {
            if c == '\n' { line += 1; col = 1; } else { col += 1; }
        }
        ParseError { line, col, msg: msg.to_string() }
    }
}

fn is_ident_start(c: char) -> bool { c.is_ascii_alphabetic() || c == '_' }

fn is_float_compat(p: &Parser) -> bool {
    // We're sitting on 'e' or 'E'. The exponent form only makes sense
    // inside a number that already has digits — caller already verified
    // we read at least one digit.
    let _ = p;
    true
}

// ── Pretty-print (canonical reserialisation) ────────────────────────────────

pub fn to_pretty_string(v: &RonAst) -> String {
    to_pretty_string_with(v, "  ")
}

/// Like `to_pretty_string` but lets the caller pick the indent unit
/// (`"  "`, `"    "`, `"\t"`, …). Anything goes — we don't validate the
/// argument, it's just repeated per depth level.
pub fn to_pretty_string_with(v: &RonAst, indent: &str) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0, indent);
    out
}

fn write_value(out: &mut String, v: &RonAst, depth: usize, indent: &str) {
    match v {
        RonAst::Unit          => out.push_str("()"),
        RonAst::Bool(b)       => write!(out, "{b}").unwrap(),
        RonAst::Char(c)       => write!(out, "'{}'", c.escape_default()).unwrap(),
        RonAst::Int(i)        => write!(out, "{i}").unwrap(),
        RonAst::Float(f)      => write_float(out, *f),
        RonAst::String(s)     => write_quoted(out, s),
        RonAst::Option(None)  => out.push_str("None"),
        RonAst::Option(Some(inner)) => { out.push_str("Some("); write_value(out, inner, depth, indent); out.push(')'); }
        RonAst::UnitVariant(name) => out.push_str(name),
        RonAst::List(items) => write_list(out, items, depth, indent),
        RonAst::Map(pairs)  => write_map(out, pairs, depth, indent),
        RonAst::Struct { name, fields } => write_struct(out, name.as_deref(), fields, depth, indent),
        RonAst::Tuple { name, items }   => write_tuple(out, name.as_deref(), items, depth, indent),
    }
}

fn write_float(out: &mut String, f: f64) {
    if f.is_nan() { out.push_str("NaN"); return; }
    if f.is_infinite() { out.push_str(if f > 0.0 { "inf" } else { "-inf" }); return; }
    let s = format!("{f}");
    out.push_str(&s);
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        // Force float disambiguator so parsers don't read it back as int.
        out.push_str(".0");
    }
}

fn write_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => write!(out, "\\u{{{:x}}}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn indent_str(out: &mut String, depth: usize, indent: &str) {
    for _ in 0..depth { out.push_str(indent); }
}

fn write_list(out: &mut String, items: &[RonAst], depth: usize, indent: &str) {
    if items.is_empty() { out.push_str("[]"); return; }
    out.push('[');
    out.push('\n');
    for (i, it) in items.iter().enumerate() {
        indent_str(out, depth + 1, indent);
        write_value(out, it, depth + 1, indent);
        if i + 1 < items.len() { out.push(','); }
        out.push('\n');
    }
    indent_str(out, depth, indent);
    out.push(']');
}

fn write_map(out: &mut String, pairs: &[(RonAst, RonAst)], depth: usize, indent: &str) {
    if pairs.is_empty() { out.push_str("{}"); return; }
    out.push('{');
    out.push('\n');
    for (i, (k, v)) in pairs.iter().enumerate() {
        indent_str(out, depth + 1, indent);
        write_value(out, k, depth + 1, indent);
        out.push_str(": ");
        write_value(out, v, depth + 1, indent);
        if i + 1 < pairs.len() { out.push(','); }
        out.push('\n');
    }
    indent_str(out, depth, indent);
    out.push('}');
}

fn write_struct(out: &mut String, name: Option<&str>, fields: &[(String, RonAst)], depth: usize, indent: &str) {
    if let Some(n) = name { out.push_str(n); }
    if fields.is_empty() {
        // Empty struct/struct-variant — keep parens to disambiguate from
        // a unit variant.
        out.push_str("()");
        return;
    }
    out.push('(');
    out.push('\n');
    for (i, (k, v)) in fields.iter().enumerate() {
        indent_str(out, depth + 1, indent);
        out.push_str(k);
        out.push_str(": ");
        write_value(out, v, depth + 1, indent);
        if i + 1 < fields.len() { out.push(','); }
        out.push('\n');
    }
    indent_str(out, depth, indent);
    out.push(')');
}

fn write_tuple(out: &mut String, name: Option<&str>, items: &[RonAst], depth: usize, indent: &str) {
    if let Some(n) = name { out.push_str(n); }
    if items.is_empty() {
        out.push_str("()");
        return;
    }
    if items.len() == 1 && depth < 6 {
        // Single-arg tuples (often newtype enum variants) read better
        // inline so `Some(x)`-style values don't blow up vertically.
        out.push('(');
        write_value(out, &items[0], depth, indent);
        out.push(')');
        return;
    }
    out.push('(');
    out.push('\n');
    for (i, it) in items.iter().enumerate() {
        indent_str(out, depth + 1, indent);
        write_value(out, it, depth + 1, indent);
        if i + 1 < items.len() { out.push(','); }
        out.push('\n');
    }
    indent_str(out, depth, indent);
    out.push(')');
}

// ── Conversion to serde_json::Value (for the RON → JSON action) ─────────────

pub fn to_json(v: &RonAst) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        RonAst::Unit          => J::Null,
        RonAst::Bool(b)       => J::Bool(*b),
        RonAst::Char(c)       => J::String(c.to_string()),
        RonAst::Int(i)        => J::Number((*i).into()),
        RonAst::Float(f)      => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        RonAst::String(s)     => J::String(s.clone()),
        RonAst::Option(None)  => J::Null,
        RonAst::Option(Some(inner)) => to_json(inner),
        RonAst::UnitVariant(name) => J::String(name.clone()),
        RonAst::List(items)   => J::Array(items.iter().map(to_json).collect()),
        RonAst::Map(pairs)    => {
            let mut obj = serde_json::Map::new();
            for (k, v) in pairs {
                obj.insert(key_to_string(k), to_json(v));
            }
            J::Object(obj)
        }
        RonAst::Struct { name, fields } => {
            let mut obj = serde_json::Map::new();
            if let Some(n) = name { obj.insert("$type".into(), J::String(n.clone())); }
            for (k, v) in fields { obj.insert(k.clone(), to_json(v)); }
            J::Object(obj)
        }
        RonAst::Tuple { name, items } => {
            if let Some(n) = name {
                // Tagged tuple — render as `{ "$tag": "Foo", "$items": [...] }`
                // so JSON consumers can read the variant. Keeps round-trip lossy
                // but informative.
                let mut obj = serde_json::Map::new();
                obj.insert("$tag".into(), J::String(n.clone()));
                obj.insert("$items".into(), J::Array(items.iter().map(to_json).collect()));
                J::Object(obj)
            } else {
                J::Array(items.iter().map(to_json).collect())
            }
        }
    }
}

fn key_to_string(v: &RonAst) -> String {
    match v {
        RonAst::String(s) => s.clone(),
        RonAst::Char(c)   => c.to_string(),
        RonAst::Bool(b)   => b.to_string(),
        RonAst::Int(i)    => i.to_string(),
        RonAst::Float(f)  => f.to_string(),
        RonAst::UnitVariant(n) => n.clone(),
        _ => format!("{:?}", v),
    }
}
