//! A RON document with its **byte spans** kept.
//!
//! `arbor-studio-ron`'s tree has none — it is built to be diffed and written back, and a position
//! would be dead weight in both. Pointing at "this field name" in the editor needs exactly that
//! position, so this is a second, smaller reading of the same text.
//!
//! It is only ever run on text the real parser already accepted, which is why it can afford to be
//! strict: anything it does not understand makes the whole scan return `None`, the validator says
//! nothing, and the syntax diagnostics (which are the real parser's) stay the only voice. A
//! scanner that guessed its way past something odd would produce findings about a tree that is
//! not the document's.

/// A scalar's kind — all the validator needs of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    Int,
    Float,
    Str,
    Char,
    Bool,
}

impl Scalar {
    pub fn describe(self) -> &'static str {
        match self {
            Scalar::Int => "an integer",
            Scalar::Float => "a float",
            Scalar::Str => "a string",
            Scalar::Char => "a char",
            Scalar::Bool => "a bool",
        }
    }
}

/// What opens a struct or a tuple: its name when it has one, else the `(`. The span a "missing
/// field" is reported on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub name: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// One `name: value` of a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub value: Node,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Scalar(Scalar),
    /// `()` with no name.
    Unit,
    /// A bare identifier: a unit variant, `None`, a unit struct.
    Ident(String),
    List(Vec<Node>),
    Map(Vec<(Node, Node)>),
    /// `(a: 1)` / `Name(a: 1)`. An empty `Name()` is read as this too, with no fields.
    Struct { head: Head, fields: Vec<Field> },
    /// `(1, 2)` / `Name(1, 2)` / `Some(1)`.
    Tuple { head: Head, items: Vec<Node> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

/// Nesting past this is not a document anyone wrote; it is a guard against recursing off the stack.
const MAX_DEPTH: usize = 128;

/// Read `src` into a span-carrying tree. `None` for anything this does not understand.
pub fn scan(src: &str) -> Option<Node> {
    let mut s = Scanner { b: src.as_bytes(), src, pos: 0 };
    s.skip_bom();
    s.skip_trivia();
    s.skip_extensions();
    let root = s.value(0)?;
    s.skip_trivia();
    (s.pos == s.b.len()).then_some(root)
}

struct Scanner<'a> {
    b: &'a [u8],
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    /// Whether the text at the cursor starts with `pat` — safe past the end, unlike slicing.
    fn at(&self, pat: &[u8]) -> bool {
        self.b.get(self.pos..).is_some_and(|rest| rest.starts_with(pat))
    }

    fn skip_bom(&mut self) {
        if self.src.starts_with('\u{FEFF}') {
            self.pos = '\u{FEFF}'.len_utf8();
        }
    }

    /// Whitespace, `// line` and nested `/* block */` comments.
    fn skip_trivia(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.pos += 1;
            }
            if self.at(b"//") {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.pos += 1;
                }
            } else if self.at(b"/*") {
                let mut depth = 0usize;
                while self.pos < self.b.len() {
                    if self.at(b"/*") {
                        depth += 1;
                        self.pos += 2;
                    } else if self.at(b"*/") {
                        depth -= 1;
                        self.pos += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        self.pos += 1;
                    }
                }
            } else {
                return;
            }
        }
    }

    /// `#![enable(…)]` headers: skipped whole, they change how a value is read, not its shape.
    fn skip_extensions(&mut self) {
        while self.at(b"#!") || self.at(b"#[") {
            match self.src[self.pos..].find(']') {
                Some(i) => self.pos += i + 1,
                None => return,
            }
            self.skip_trivia();
        }
    }

    fn value(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_trivia();
        let start = self.pos;
        let c = self.peek()?;
        let kind = match c {
            b'(' => return self.paren(None, start, depth),
            b'[' => self.list(depth)?,
            b'{' => self.map(depth)?,
            b'"' => {
                self.string()?;
                Kind::Scalar(Scalar::Str)
            }
            b'\'' => {
                self.char_lit()?;
                Kind::Scalar(Scalar::Char)
            }
            b'r' if matches!(self.b.get(self.pos + 1), Some(b'"') | Some(b'#')) && self.raw_string() => {
                Kind::Scalar(Scalar::Str)
            }
            b'-' | b'+' | b'0'..=b'9' => self.number()?,
            c if is_ident_start(c) => return self.ident_or_call(start, depth),
            _ => return None,
        };
        Some(Node { start, end: self.pos, kind })
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.pos;
        // `r#raw` identifiers.
        if self.at(b"r#") && self.b.get(self.pos + 2).is_some_and(|c| is_ident_start(*c)) {
            self.pos += 2;
        }
        let name_start = self.pos;
        if !self.peek().is_some_and(is_ident_start) {
            self.pos = start;
            return None;
        }
        while self.peek().is_some_and(is_ident_byte) {
            self.pos += 1;
        }
        Some(self.src[name_start..self.pos].to_string())
    }

    fn ident_or_call(&mut self, start: usize, depth: usize) -> Option<Node> {
        let name = self.ident()?;
        let name_end = self.pos;
        let save = self.pos;
        self.skip_trivia();
        if self.peek() == Some(b'(') {
            return self.paren(Some((name, start, name_end)), start, depth);
        }
        self.pos = save;
        let kind = match name.as_str() {
            "true" | "false" => Kind::Scalar(Scalar::Bool),
            "inf" | "NaN" => Kind::Scalar(Scalar::Float),
            _ => Kind::Ident(name),
        };
        Some(Node { start, end: name_end, kind })
    }

    /// Everything from the `(`: a struct if the first thing inside is `ident :`, else a tuple.
    fn paren(&mut self, name: Option<(String, usize, usize)>, start: usize, depth: usize) -> Option<Node> {
        let open = self.pos;
        self.pos += 1; // `(`
        let head = match name {
            Some((n, s, e)) => Head { name: Some(n), start: s, end: e },
            None => Head { name: None, start: open, end: open + 1 },
        };
        self.skip_trivia();
        if self.peek() == Some(b')') {
            self.pos += 1;
            let kind = if head.name.is_some() {
                Kind::Struct { head, fields: Vec::new() }
            } else {
                Kind::Unit
            };
            return Some(Node { start, end: self.pos, kind });
        }
        let kind = if self.at_field_name() {
            Kind::Struct { fields: self.fields(depth)?, head }
        } else {
            Kind::Tuple { items: self.items(depth)?, head }
        };
        Some(Node { start, end: self.pos, kind })
    }

    /// `ident` then `:` (and not `::`) — the shape of a struct's first field.
    fn at_field_name(&mut self) -> bool {
        let save = self.pos;
        let ok = self.ident().is_some() && {
            self.skip_trivia();
            self.peek() == Some(b':') && self.b.get(self.pos + 1) != Some(&b':')
        };
        self.pos = save;
        ok
    }

    fn fields(&mut self, depth: usize) -> Option<Vec<Field>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(b')') {
                self.pos += 1;
                return Some(out);
            }
            let start = self.pos;
            let name = self.ident()?;
            let end = self.pos;
            self.skip_trivia();
            if self.peek() != Some(b':') {
                return None;
            }
            self.pos += 1;
            let value = self.value(depth + 1)?;
            out.push(Field { name, start, end, value });
            self.comma();
        }
    }

    fn items(&mut self, depth: usize) -> Option<Vec<Node>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(b')') {
                self.pos += 1;
                return Some(out);
            }
            out.push(self.value(depth + 1)?);
            self.comma();
        }
    }

    fn list(&mut self, depth: usize) -> Option<Kind> {
        self.pos += 1; // `[`
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Some(Kind::List(out));
            }
            out.push(self.value(depth + 1)?);
            self.comma();
        }
    }

    fn map(&mut self, depth: usize) -> Option<Kind> {
        self.pos += 1; // `{`
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Some(Kind::Map(out));
            }
            let key = self.value(depth + 1)?;
            self.skip_trivia();
            if self.peek() != Some(b':') {
                return None;
            }
            self.pos += 1;
            let value = self.value(depth + 1)?;
            out.push((key, value));
            self.comma();
        }
    }

    fn comma(&mut self) {
        self.skip_trivia();
        if self.peek() == Some(b',') {
            self.pos += 1;
        }
    }

    fn string(&mut self) -> Option<()> {
        self.pos += 1;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'\\' => self.pos += 1,
                b'"' => return Some(()),
                _ => {}
            }
        }
        None
    }

    /// `r"…"` / `r#"…"#`. `false` (position untouched) when it was an identifier starting with `r`.
    fn raw_string(&mut self) -> bool {
        let save = self.pos;
        let mut p = self.pos + 1;
        let mut hashes = 0usize;
        while self.b.get(p) == Some(&b'#') {
            hashes += 1;
            p += 1;
        }
        if self.b.get(p) != Some(&b'"') {
            return false;
        }
        p += 1;
        while p < self.b.len() {
            if self.b[p] == b'"' && self.b[p + 1..].iter().take(hashes).filter(|c| **c == b'#').count() == hashes {
                self.pos = p + 1 + hashes;
                return true;
            }
            p += 1;
        }
        self.pos = save;
        false
    }

    fn char_lit(&mut self) -> Option<()> {
        self.pos += 1;
        if self.peek() == Some(b'\\') {
            self.pos += 2;
        } else {
            let c = self.src[self.pos..].chars().next()?;
            self.pos += c.len_utf8();
        }
        // `\u{1F600}` runs to its own closing `}` before the quote.
        while self.peek().is_some_and(|c| c != b'\'') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\'') {
            self.pos += 1;
            return Some(());
        }
        None
    }

    fn number(&mut self) -> Option<Kind> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'-') | Some(b'+')) {
            self.pos += 1;
        }
        if self.peek().is_some_and(is_ident_start) {
            // `-inf`, `+NaN`.
            let word = self.ident()?;
            return matches!(word.as_str(), "inf" | "NaN").then_some(Kind::Scalar(Scalar::Float));
        }
        let radix_prefixed = matches!(self.b.get(self.pos..self.pos + 2), Some(b"0x" | b"0b" | b"0o"));
        let mut float = false;
        while let Some(c) = self.peek() {
            let exponent_sign = matches!(c, b'+' | b'-')
                && !radix_prefixed
                && self.pos > start
                && matches!(self.b[self.pos - 1], b'e' | b'E');
            if c == b'.' {
                float = true;
            } else if !(c.is_ascii_alphanumeric() || c == b'_' || exponent_sign) {
                break;
            }
            if !radix_prefixed && matches!(c, b'e' | b'E') {
                float = true;
            }
            self.pos += 1;
        }
        (self.pos > start).then_some(Kind::Scalar(if float { Scalar::Float } else { Scalar::Int }))
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(src: &str) -> Node {
        scan(src).unwrap_or_else(|| panic!("did not scan: {src}"))
    }

    #[test]
    fn a_struct_keeps_the_span_of_every_field_name() {
        let src = "(port: 80, host: \"x\")";
        let Kind::Struct { fields, .. } = root(src).kind else { panic!("not a struct") };
        assert_eq!(fields.len(), 2);
        assert_eq!(&src[fields[1].start..fields[1].end], "host");
        assert_eq!(fields[1].value.kind, Kind::Scalar(Scalar::Str));
    }

    #[test]
    fn a_named_struct_and_a_variant_carry_their_name_as_the_head() {
        let src = "Server(port: 1)";
        let Kind::Struct { head, .. } = root(src).kind else { panic!() };
        assert_eq!(head.name.as_deref(), Some("Server"));
        assert_eq!(&src[head.start..head.end], "Server");
    }

    #[test]
    fn tuples_lists_maps_and_options_are_told_apart() {
        assert!(matches!(root("(1, 2)").kind, Kind::Tuple { .. }));
        assert!(matches!(root("[1, 2]").kind, Kind::List(_)));
        assert!(matches!(root("{\"a\": 1}").kind, Kind::Map(_)));
        let Kind::Tuple { head, items } = root("Some(3)").kind else { panic!() };
        assert_eq!((head.name.as_deref(), items.len()), (Some("Some"), 1));
        assert_eq!(root("None").kind, Kind::Ident("None".into()));
        assert_eq!(root("()").kind, Kind::Unit);
    }

    #[test]
    fn numbers_strings_chars_and_bools_get_their_scalar() {
        let Kind::List(items) = root("[1, -2, 3.5, 1e-3, 0x1F, \"s\", r#\"a\"b\"#, 'c', true, inf]").kind else {
            panic!()
        };
        let kinds: Vec<Kind> = items.into_iter().map(|n| n.kind).collect();
        use Scalar::*;
        let expected = [Int, Int, Float, Float, Int, Str, Str, Char, Bool, Float];
        assert_eq!(kinds, expected.map(Kind::Scalar).to_vec());
    }

    #[test]
    fn comments_and_the_extension_header_are_trivia() {
        let src = "#![enable(implicit_some)]\n// note\n(/* a /* nested */ b */ x: 1, // trailing\n)";
        let Kind::Struct { fields, .. } = root(src).kind else { panic!() };
        assert_eq!(fields[0].name, "x");
    }

    #[test]
    fn a_field_value_can_be_a_nested_struct() {
        let src = "(a: (b: 1), c: Foo(d: 2))";
        let Kind::Struct { fields, .. } = root(src).kind else { panic!() };
        assert!(matches!(fields[0].value.kind, Kind::Struct { .. }));
        assert!(matches!(&fields[1].value.kind, Kind::Struct { head, .. } if head.name.as_deref() == Some("Foo")));
    }

    #[test]
    fn something_it_does_not_understand_is_none_rather_than_a_guess() {
        assert!(scan("(a: )").is_none());
        assert!(scan("(1,").is_none());
        assert!(scan("@").is_none());
    }
}
