//! The one parser.
//!
//! It never fails. It records a [`SyntaxError`] and stops describing structure,
//! but it always returns a [`Parsed`] with a slot at every position it managed to
//! reach — because the *other* caller of this function is a completion handler
//! running on a half-typed line, and a parser that returned `Err` for `s#ordini>`
//! would force a second, sloppier parser into existence to answer it. Two parsers
//! that disagree about where the caret is, is the failure this design exists to
//! prevent.

use crate::span::{Slot, Span};
use crate::syntax::{Block, ChainLink, ColItem, Parsed, PredItem, RawValue, SyntaxError};

/// Parse an abbreviation. Always succeeds; look at [`Parsed::error`].
pub fn parse(input: &str) -> Parsed {
    let mut sc = Scanner::new(input);
    let mut error: Option<SyntaxError> = None;

    sc.skip_ws();
    let verb = sc.read_name();
    sc.skip_ws();
    let hash = sc.eat('#').then(|| sc.pos - 1);

    if hash.is_none() {
        // Nothing after the verb is meaningful without the separator, and
        // pretending otherwise would put the caret in a table slot that the user
        // has not opened yet.
        if !sc.at_end() {
            error = Some(sc.unexpected("expected `#` after the verb"));
        }
        return Parsed { verb, hash, table: Slot::empty(sc.pos), chain: Vec::new(), cols: None, preds: None, mult: None, error };
    }

    sc.skip_ws();
    let table = sc.read_name();
    let chain = sc.read_chain();

    sc.skip_ws();
    let cols = (sc.peek() == Some('(')).then(|| sc.read_block(')', &mut error, Scanner::read_col_item));
    sc.skip_ws();
    let preds = (sc.peek() == Some('[')).then(|| sc.read_block(']', &mut error, Scanner::read_pred_item));
    sc.skip_ws();
    let mult = sc.eat('*').then(|| sc.read_digits());

    sc.skip_ws();
    if error.is_none() && !sc.at_end() {
        error = Some(sc.unexpected("expected the end of the abbreviation"));
    }
    if error.is_none() {
        // The string is asked about first, and the order is the message: an
        // unquoted quote swallows the rest of the line, so the bracket it was in
        // is *also* unclosed — and "you left a string open" is the fact that
        // explains both.
        error = unterminated_string(&preds, &cols)
            .or_else(|| unterminated(&cols, '('))
            .or_else(|| unterminated(&preds, '['));
    }

    Parsed { verb, hash, table, chain, cols, preds, mult, error }
}

fn unterminated<T>(block: &Option<Block<T>>, open: char) -> Option<SyntaxError> {
    let block = block.as_ref()?;
    (!block.closed).then(|| SyntaxError {
        at: block.span.start,
        message: format!("`{open}` is never closed"),
    })
}

/// A string the user opened and did not close. Harmless while typing, fatal for
/// an expansion — the value would silently swallow the rest of the line.
fn unterminated_string(preds: &Option<Block<PredItem>>, cols: &Option<Block<ColItem>>) -> Option<SyntaxError> {
    let from_preds = preds.iter().flat_map(|b| b.items.iter()).map(|i| &i.value);
    let from_cols = cols.iter().flat_map(|b| b.items.iter()).filter_map(|i| i.value.as_ref());
    from_preds
        .chain(from_cols)
        .find(|v| v.quoted && !v.terminated)
        .map(|v| SyntaxError { at: v.slot.span.start, message: "a quoted value is never closed".to_string() })
}

/// Characters that make up an identifier. Unicode-aware because these schemas are
/// not all ASCII, and `$` because Oracle-era names use it.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, want: char) -> bool {
        if self.peek() == Some(want) {
            self.pos += want.len_utf8();
            return true;
        }
        false
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.bump();
        }
    }

    fn slice(&self, start: usize) -> Slot {
        Slot::new(&self.src[start..self.pos], Span::new(start, self.pos))
    }

    fn unexpected(&self, expected: &str) -> SyntaxError {
        match self.peek() {
            Some(c) => SyntaxError { at: self.pos, message: format!("unexpected `{c}` — {expected}") },
            None => SyntaxError { at: self.pos, message: expected.to_string() },
        }
    }

    fn read_name(&mut self) -> Slot {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_name_char(c)) {
            self.bump();
        }
        self.slice(start)
    }

    /// A column reference, optionally qualified: `keycode` or `localstrings.keycode`.
    fn read_qualified(&mut self) -> Slot {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_name_char(c) || c == '.') {
            self.bump();
        }
        self.slice(start)
    }

    fn read_digits(&mut self) -> Slot {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.bump();
        }
        self.slice(start)
    }

    fn read_chain(&mut self) -> Vec<ChainLink> {
        let mut links = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some('>') {
                return links;
            }
            let arrow = self.pos;
            self.bump();
            self.skip_ws();
            let table = self.read_name();
            self.skip_ws();
            let column = self.eat(':').then(|| {
                self.skip_ws();
                self.read_name()
            });
            links.push(ChainLink { arrow, table, column });
        }
    }

    /// A value, stopping at whitespace or at whatever closes the list it is in.
    ///
    /// A quoted string is taken whole, doubled quotes included, and an unclosed
    /// one stops at the end of the input rather than consuming forever.
    ///
    /// A **bare** one tracks parentheses, so `now()` survives inside `(...)` —
    /// where `)` would otherwise close the column list on the user's most obvious
    /// possible default and hand them a parse error. Inside the parentheses
    /// nothing terminates, so `coalesce(a, b)` keeps its comma and its space. An
    /// unbalanced `(` runs to the end and is reported by the unclosed-bracket
    /// check, which is the right message for it anyway.
    fn read_value(&mut self, stops: &[char]) -> RawValue {
        self.skip_ws();
        let start = self.pos;
        if self.peek() != Some('\'') {
            let mut depth = 0usize;
            while let Some(c) = self.peek() {
                if depth == 0 && (c.is_whitespace() || c == ')' || stops.contains(&c)) {
                    break;
                }
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                self.bump();
            }
            return RawValue { slot: self.slice(start), quoted: false, terminated: true };
        }

        self.bump();
        let mut terminated = false;
        while let Some(c) = self.bump() {
            if c == '\'' {
                // `''` is an escaped quote, not the end.
                if self.peek() == Some('\'') {
                    self.bump();
                    continue;
                }
                terminated = true;
                break;
            }
        }
        RawValue { slot: self.slice(start), quoted: true, terminated }
    }

    fn read_col_item(&mut self) -> ColItem {
        self.skip_ws();
        let name = self.read_qualified();
        self.skip_ws();
        if self.peek() != Some('=') {
            return ColItem { name, eq: None, value: None };
        }
        let eq = self.pos;
        self.bump();
        let value = self.read_value(&[',', ')']);
        ColItem { name, eq: Some(eq), value: Some(value) }
    }

    fn read_pred_item(&mut self) -> PredItem {
        self.skip_ws();
        let name = self.read_qualified();
        self.skip_ws();
        let op = self.read_operator();
        let value = self.read_value(&[',', ']']);
        PredItem { name, op, value }
    }

    /// One or two characters out of the comparison set. Inside `[...]` a `>` is
    /// unambiguously an operator, which is why the chain is parsed before we ever
    /// get here.
    fn read_operator(&mut self) -> Slot {
        let start = self.pos;
        while self.pos - start < 2 && matches!(self.peek(), Some('=' | '<' | '>' | '!' | '~')) {
            self.bump();
        }
        self.slice(start)
    }

    /// The shared bracket-list loop.
    ///
    /// An item is pushed on **every** turn, including the turn that finds an
    /// empty one — `i#t(a,` and `i#t()` both have to leave a slot where the caret
    /// is, or completion inside them has nothing to answer with. Expansion
    /// discards the blanks.
    fn read_block<T>(
        &mut self,
        close: char,
        error: &mut Option<SyntaxError>,
        mut item: impl FnMut(&mut Self) -> T,
    ) -> Block<T> {
        let start = self.pos;
        self.bump();
        let mut items = Vec::new();
        let mut closed = false;
        loop {
            items.push(item(self));
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(c) if c == close => {
                    self.bump();
                    closed = true;
                    break;
                }
                None => break,
                Some(_) => {
                    if error.is_none() {
                        *error = Some(self.unexpected(&format!("expected `,` or `{close}`")));
                    }
                    break;
                }
            }
        }
        Block { span: Span::new(start, self.pos), items, closed }
    }
}
