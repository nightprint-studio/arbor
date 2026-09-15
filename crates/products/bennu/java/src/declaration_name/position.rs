//! Whether the tokens before the caret end in the **type of a declaration** whose name comes next.
//!
//! Read backwards: the type (brackets, type arguments, a qualified name), then the modifiers and
//! annotations in front of it, then the one token that decides — what the declaration starts after.
//! A statement or member starts after `{`, `}` or `;`; a parameter after the `(` or `,` of a
//! parameter list. Everything else — `return`, `new`, an operator, `extends`, an argument list — is a
//! place a type is *used*, not declared, and a name drawn there would be a name nobody can write.

use super::tokens::{is, starts_uppercase, text, Kind, Token};
use crate::postfix::names::KEYWORDS;

/// The modifiers a declaration's type may follow. `default` is left out: before a type it starts an
/// interface method, whose next word is a method name.
const MODIFIERS: &[&str] = &[
    "public", "protected", "private", "static", "final", "abstract", "transient", "volatile",
    "synchronized", "native", "strictfp", "sealed",
];

const PRIMITIVES: &[&str] = &["boolean", "byte", "char", "short", "int", "long", "float", "double"];

/// Words a `{` may not follow and still open a body a declaration can be written in: what follows
/// them is a value.
const VALUE_WORDS: &[&str] = &["new", "return", "throw", "case", "yield", "assert"];

/// Where the declared type starts, when `tokens` (everything before the name) end in one.
pub(super) fn declared_type(tokens: &[Token], source: &str) -> Option<usize> {
    let mut end = tokens.len();
    // Array brackets and a varargs ellipsis.
    loop {
        if end >= 2 && is(tokens, end - 1, b']') && is(tokens, end - 2, b'[') {
            end -= 2;
        } else if end >= 3 && (1..=3).all(|k| is(tokens, end - k, b'.')) {
            end -= 3;
        } else {
            break;
        }
    }
    let generic = end >= 1 && is(tokens, end - 1, b'>');
    if generic {
        end = angle_open(tokens, end - 1)?;
    }
    let name = end.checked_sub(1)?;
    if tokens[name].kind != Kind::Word {
        return None;
    }
    let mut first = name;
    while first >= 2 && is(tokens, first - 1, b'.') && tokens[first - 2].kind == Kind::Word {
        first -= 2;
    }
    // `call().Foo` is not a type, and `@Foo` is an annotation — the line above a declaration, not
    // the declaration.
    if first >= 1 && (is(tokens, first - 1, b'.') || is(tokens, first - 1, b'@')) {
        return None;
    }
    let simple = text(source, &tokens[name]);
    let names_a_type = if PRIMITIVES.contains(&simple) {
        !generic && first == name
    } else {
        starts_uppercase(simple) && !KEYWORDS.contains(&simple)
    };
    if !names_a_type {
        return None;
    }
    let prev = skip_modifiers(tokens, source, first).checked_sub(1)?;
    let declares = match tokens[prev].kind {
        Kind::Punct(b'{' | b'}' | b';' | b':') => starts_statement_or_member(tokens, source, prev),
        Kind::Punct(b'(' | b',') => starts_parameter(tokens, source, prev),
        _ => false,
    };
    declares.then(|| tokens[first].start)
}

/// `index`, moved back over the modifiers and annotations that end there.
fn skip_modifiers(tokens: &[Token], source: &str, mut index: usize) -> usize {
    loop {
        if index >= 1
            && tokens[index - 1].kind == Kind::Word
            && MODIFIERS.contains(&text(source, &tokens[index - 1]))
        {
            index -= 1;
        } else if let Some(at) = annotation_ending_at(tokens, index) {
            index = at;
        } else {
            return index;
        }
    }
}

/// The index of the `@` of an annotation whose last token is just before `end`: `@Inject`,
/// `@javax.inject.Named("x")`.
fn annotation_ending_at(tokens: &[Token], end: usize) -> Option<usize> {
    let mut name_end = end;
    if name_end >= 1 && is(tokens, name_end - 1, b')') {
        name_end = bracket_open(tokens, name_end - 1)?;
    }
    let mut name = name_end.checked_sub(1)?;
    if tokens[name].kind != Kind::Word {
        return None;
    }
    while name >= 2 && is(tokens, name - 1, b'.') && tokens[name - 2].kind == Kind::Word {
        name -= 2;
    }
    let at = name.checked_sub(1)?;
    is(tokens, at, b'@').then_some(at)
}

/// A declaration after `{`, `}`, `;` or `:` — a field, or a local in a block.
fn starts_statement_or_member(tokens: &[Token], source: &str, prev: usize) -> bool {
    let Some((open, after_semicolon)) = enclosing_opener(tokens, prev + 1) else {
        return false;
    };
    match tokens[open].kind {
        // A second resource: `try (A a = x; B |`.
        Kind::Punct(b'(') => {
            is(tokens, prev, b';')
                && open >= 1
                && tokens[open - 1].kind == Kind::Word
                && text(source, &tokens[open - 1]) == "try"
        }
        Kind::Punct(b'{') => {
            if !opens_a_body(tokens, source, open) {
                return false;
            }
            let header = header_of(tokens, open);
            let in_switch = has_top_level_word(tokens, source, header.clone(), "switch");
            // `case 1: Order |` declares; after any other `:` (a ternary, a label) nothing does.
            if is(tokens, prev, b':') {
                return in_switch;
            }
            // Straight after a switch's `{` only a `case` can come.
            if in_switch && prev == open {
                return false;
            }
            // An enum body lists its constants up to the first `;`, and `RED |` is not a declaration.
            !(has_top_level_word(tokens, source, header, "enum") && !after_semicolon)
        }
        _ => false,
    }
}

/// A declaration after the `(` or `,` of a parameter list — a method's, a constructor's, a record's,
/// an enhanced `for`'s or a `try`'s resources.
fn starts_parameter(tokens: &[Token], source: &str, prev: usize) -> bool {
    let Some((open, _)) = enclosing_opener(tokens, prev + 1) else {
        return false;
    };
    if !is(tokens, open, b'(') {
        return false;
    }
    // `m(Map<String, Order |` — the comma separates type arguments, not parameters.
    if is(tokens, prev, b',') && unclosed_angles(&tokens[open + 1..prev]) {
        return false;
    }
    let Some(callee) = open.checked_sub(1) else {
        return false;
    };
    if tokens[callee].kind != Kind::Word {
        return false;
    }
    match text(source, &tokens[callee]) {
        "for" | "try" => prev == open,
        word if KEYWORDS.contains(&word) => false,
        _ => declares_parameters(tokens, source, callee),
    }
}

/// Whether the `(` after the word at `callee` opens a declaration's parameters rather than a call's
/// arguments — decided by what the word follows. A method name follows its return type; a
/// constructor's follows a modifier, an annotation or the start of a member. A call follows a `.`,
/// an operator, a `(` or a keyword like `new`.
fn declares_parameters(tokens: &[Token], source: &str, callee: usize) -> bool {
    let capitalised = starts_uppercase(text(source, &tokens[callee]));
    let Some(before) = callee.checked_sub(1) else {
        return false;
    };
    match tokens[before].kind {
        Kind::Word => match text(source, &tokens[before]) {
            "record" | "void" => true,
            word if PRIMITIVES.contains(&word) => true,
            word if MODIFIERS.contains(&word) => capitalised,
            word => !KEYWORDS.contains(&word),
        },
        // `List<Order> find(` — but not an explicit type-argument call, `this.<T>find(`.
        Kind::Punct(b'>') => match angle_open(tokens, before) {
            Some(lt) => lt == 0 || !is(tokens, lt - 1, b'.'),
            None => false,
        },
        Kind::Punct(b']') => true,
        Kind::Punct(b')') => capitalised && annotation_ending_at(tokens, callee).is_some(),
        Kind::Punct(b'{' | b'}' | b';') => {
            capitalised
                && enclosing_opener(tokens, before + 1)
                    .is_some_and(|(open, _)| is(tokens, open, b'{') && opens_type_body(tokens, source, open))
        }
        _ => false,
    }
}

/// Whether the `{` at `open` can start a body statements or members are written in, judged by the
/// token in front of it. After `=`, `]` or `new` it is an array initializer.
fn opens_a_body(tokens: &[Token], source: &str, open: usize) -> bool {
    let Some(before) = open.checked_sub(1) else {
        return false;
    };
    match tokens[before].kind {
        Kind::Punct(b')' | b'>' | b'{' | b'}' | b';' | b':') => true,
        Kind::Word => !VALUE_WORDS.contains(&text(source, &tokens[before])),
        _ => false,
    }
}

/// Whether the `{` at `open` opens a class, interface, enum or record body, or an anonymous class's.
pub(super) fn opens_type_body(tokens: &[Token], source: &str, open: usize) -> bool {
    let header = header_of(tokens, open);
    ["class", "interface", "enum", "record"]
        .iter()
        .any(|word| has_top_level_word(tokens, source, header.clone(), word))
        || (open >= 1 && is(tokens, open - 1, b')') && has_top_level_word(tokens, source, header, "new"))
}

/// The tokens that head the `{` at `open`: back to the previous statement boundary, or to the
/// bracket the whole thing is inside. `public class Foo extends Bar`, `if (x)`, `switch (kind)`.
fn header_of(tokens: &[Token], open: usize) -> std::ops::Range<usize> {
    let mut depth = 0usize;
    let mut start = open;
    while start > 0 {
        let i = start - 1;
        match tokens[i].kind {
            Kind::Punct(b')' | b']') => depth += 1,
            Kind::Punct(b'(' | b'[') => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Kind::Punct(b';' | b'{' | b'}') if depth == 0 => break,
            _ => {}
        }
        start = i;
    }
    start..open
}

/// Whether `word` appears in `range` outside any parentheses: `if (record != null)` does not declare
/// a record.
fn has_top_level_word(tokens: &[Token], source: &str, range: std::ops::Range<usize>, word: &str) -> bool {
    let mut depth = 0i32;
    for token in &tokens[range] {
        match token.kind {
            Kind::Punct(b'(' | b'[') => depth += 1,
            Kind::Punct(b')' | b']') => depth -= 1,
            Kind::Word if depth == 0 && text(source, token) == word => return true,
            _ => {}
        }
    }
    false
}

/// The innermost bracket still open before `end`, and whether a `;` separates it from `end` at its
/// own level. `None` at the top level of the file.
pub(super) fn enclosing_opener(tokens: &[Token], end: usize) -> Option<(usize, bool)> {
    let mut depth = 0usize;
    let mut semicolon = false;
    for i in (0..end).rev() {
        match tokens[i].kind {
            Kind::Punct(b')' | b']' | b'}') => depth += 1,
            Kind::Punct(b'(' | b'[' | b'{') => {
                if depth == 0 {
                    return Some((i, semicolon));
                }
                depth -= 1;
            }
            Kind::Punct(b';') if depth == 0 => semicolon = true,
            _ => {}
        }
    }
    None
}

/// The bracket opening the one that closes at `close`.
fn bracket_open(tokens: &[Token], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        match tokens[i].kind {
            Kind::Punct(b')' | b']' | b'}') => depth += 1,
            Kind::Punct(b'(' | b'[' | b'{') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `<` opening the type arguments that close at `close`, or `None` when what is between them
/// cannot be type arguments (`a < b > c`, `x -> y`).
fn angle_open(tokens: &[Token], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        match tokens[i].kind {
            Kind::Punct(b'>') => depth += 1,
            Kind::Punct(b'<') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            Kind::Word | Kind::Punct(b',' | b'.' | b'?' | b'[' | b']' | b'&' | b'@') => {}
            _ => return None,
        }
    }
    None
}

/// Whether more `<` than `>` are open across `tokens`.
fn unclosed_angles(tokens: &[Token]) -> bool {
    let balance: i32 = tokens
        .iter()
        .map(|t| match t.kind {
            Kind::Punct(b'<') => 1,
            Kind::Punct(b'>') => -1,
            _ => 0,
        })
        .sum();
    balance > 0
}
