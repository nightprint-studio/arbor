//! The names a new declaration would clash with — so a second `Order` in the same method is
//! `order1`, not a second `order` that does not compile.
//!
//! Read off the tokens, the same ones the position was, and only as far as a clash is an error in
//! Java: the locals and parameters of the scopes the caret is in, or the fields of the class when the
//! caret is writing one. A field is **not** a clash for a local or a parameter — shadowing it is legal,
//! and a constructor parameter named after the field it sets is the most ordinary code there is.
//!
//! A heuristic over tokens, and it may miss a name (`int a, b` counts `a` only) or count one that is
//! not a declaration. Either costs a digit, never a program: the proposal is ghost text, not an edit.

use super::position::opens_type_body;
use super::tokens::{is, starts_uppercase, text, Kind, Token};
use crate::postfix::names::KEYWORDS;

/// The names declared where a declaration ending at token `name_at` would clash, reading the tokens
/// before it and those from `after` on (the token after the caret).
pub(super) fn declared_names<'s>(tokens: &[Token], source: &'s str, name_at: usize, after: usize) -> Vec<&'s str> {
    let mut names = declared_before(tokens, source, name_at);
    names.extend(declared_after(tokens, source, after));
    names
}

/// A brace-delimited scope and the names declared directly in it so far.
struct Scope<'s> {
    /// A class, interface, enum, record or anonymous-class body: its names are fields.
    type_body: bool,
    names: Vec<&'s str>,
}

fn declared_before<'s>(tokens: &[Token], source: &'s str, end: usize) -> Vec<&'s str> {
    // The file itself stands in for a type body: nothing outside a class is a clash for a local.
    let mut scopes = vec![Scope { type_body: true, names: Vec::new() }];
    // Parameters, `for` variables, resources and pattern variables: declared inside parentheses, in
    // scope in the block that follows them.
    let mut pending: Vec<&'s str> = Vec::new();
    let mut paren = 0usize;
    let mut outer_parens: Vec<usize> = Vec::new();
    for i in 0..end {
        match tokens[i].kind {
            Kind::Punct(b'{') => {
                outer_parens.push(paren);
                paren = 0;
                let type_body = opens_type_body(tokens, source, i);
                scopes.push(Scope { type_body, names: std::mem::take(&mut pending) });
            }
            Kind::Punct(b'}') => {
                if scopes.len() > 1 {
                    scopes.pop();
                }
                paren = outer_parens.pop().unwrap_or(0);
                pending.clear();
            }
            Kind::Punct(b'(') => paren += 1,
            Kind::Punct(b')') => paren = paren.saturating_sub(1),
            // An abstract method's parameters, or a braceless `for` body's variable, end here.
            Kind::Punct(b';') if paren == 0 => pending.clear(),
            Kind::Word if is_declared_name(tokens, source, i) => {
                let name = text(source, &tokens[i]);
                match scopes.last_mut() {
                    Some(scope) if paren == 0 => scope.names.push(name),
                    _ => pending.push(name),
                }
            }
            _ => {}
        }
    }
    // The innermost type body bounds what can clash. Its own names count only when the caret is
    // writing a member of it, not a parameter list inside it.
    let innermost_type = scopes.iter().rposition(|s| s.type_body).unwrap_or(0);
    let writing_a_member = innermost_type == scopes.len() - 1 && paren == 0;
    let from = if writing_a_member { innermost_type } else { innermost_type + 1 };
    scopes.drain(from..).flat_map(|s| s.names).chain(pending).collect()
}

/// The names declared later in the caret's own scope: a field further down the class, a local further
/// down the block, a parameter later in the list. Stops where that scope — or that list — closes.
fn declared_after<'s>(tokens: &[Token], source: &'s str, from: usize) -> Vec<&'s str> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut paren = 0usize;
    for i in from..tokens.len() {
        match tokens[i].kind {
            Kind::Punct(b'{') => depth += 1,
            Kind::Punct(b'}') if depth == 0 => break,
            Kind::Punct(b'}') => depth -= 1,
            Kind::Punct(b'(') => paren += 1,
            Kind::Punct(b')') if paren == 0 => break,
            Kind::Punct(b')') => paren -= 1,
            Kind::Word if depth == 0 && paren == 0 && is_declared_name(tokens, source, i) => {
                names.push(text(source, &tokens[i]));
            }
            _ => {}
        }
    }
    names
}

/// Whether the word at `index` is the name in a declaration: it follows a type and is followed by
/// what ends a declarator — `Order order;`, `Order order = …`, `(Order order)`, `Order o : orders`.
fn is_declared_name(tokens: &[Token], source: &str, index: usize) -> bool {
    if KEYWORDS.contains(&text(source, &tokens[index])) {
        return false;
    }
    let ends_a_declarator =
        matches!(tokens.get(index + 1).map(|t| t.kind), Some(Kind::Punct(b'=' | b';' | b',' | b')' | b':')));
    ends_a_declarator && index >= 1 && ends_a_type(tokens, source, index - 1)
}

/// Whether the token at `index` can be the last token of a type: `Order`, `int`, `var`, `[]`, or the
/// `>` of type arguments.
fn ends_a_type(tokens: &[Token], source: &str, index: usize) -> bool {
    let token = &tokens[index];
    match token.kind {
        Kind::Word => {
            let word = text(source, token);
            matches!(word, "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double" | "var")
                || (starts_uppercase(word) && !KEYWORDS.contains(&word))
        }
        Kind::Punct(b']') => index >= 1 && is(tokens, index - 1, b'['),
        // `Map<String, Order> orders` — and not `a > b)`, whose `>` follows a value.
        Kind::Punct(b'>') => {
            index >= 1
                && match tokens[index - 1].kind {
                    Kind::Punct(b'>' | b'?' | b']') => true,
                    Kind::Word => starts_uppercase(text(source, &tokens[index - 1])),
                    _ => false,
                }
        }
        _ => false,
    }
}
