//! The name a declaration is about to be given — `private final OrderRepository ` is followed by
//! `orderRepository`, drawn as ghost text for Tab to write.
//!
//! ## Where it answers
//!
//! Only where the next word **is** a declaration's name: a field after its modifiers, a local at the
//! start of a statement, a parameter in a method, constructor, record, `for` or `try` header. A type
//! followed by a space also appears after `return`, `new`, `throw`, `case`, `instanceof`, `extends`,
//! in an argument list and in type arguments — and a name drawn in any of those is a name that does
//! not compile, which is worse than no proposal. So the position is read strictly (see `position`)
//! and anything it does not recognise answers `None`.
//!
//! ## What it proposes
//!
//! The type's name as a variable, by [`crate::names::suggested_name_for_type`] — the one naming rule
//! the postfix templates use too — then **spelled** the project's way by the caller: `spell` is handed
//! the camelCase name, what kind of declaration it is and every variable name the file already
//! declares, so a file of `identity_resolver` fields gets `filter_configurator`. The spelling lives
//! with the caller because the conventions do (`bennu-naming`, and the project's config).
//!
//! A name already declared where the new one would clash gets a digit, IntelliJ's way: a second
//! `Order` in the method is `order1`. The clash is judged on the spelled name, the one written.
//!
//! With a partial name already typed (`OrderRepository ord|`), the proposal stands only if it
//! continues what was typed; the typed part is what accepting replaces.

mod position;
mod taken;
#[cfg(test)]
mod tests;
mod tokens;

use crate::names::suggested_name_for_type;
use tokens::Kind;

/// A declaration's predicted name, and the half-typed name it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationName {
    /// The name accepting writes: `myMsRestClientApi`.
    pub name: String,
    /// Byte range of what has already been typed of the name — empty (`typed_start == typed_end`, at
    /// the caret) when nothing has.
    pub typed_start: usize,
    pub typed_end: usize,
}

/// What the declaration at the caret declares — the naming rule its name answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    /// A member of a class, interface, enum or record body.
    Field,
    /// A local in a block, a `for` variable, a `try` resource.
    Local,
    /// A method, constructor or record parameter.
    Parameter,
}

/// What `spell` is told about the name it spells.
#[derive(Debug, Clone, Copy)]
pub struct NameContext<'a> {
    pub kind: DeclarationKind,
    /// Every variable name the file declares — fields, locals, parameters — whatever its scope.
    pub declared_in_file: &'a [&'a str],
}

/// The name for the declaration whose type ends just before the caret at `offset`, or `None` — the
/// ordinary answer everywhere a declaration's name is not what comes next.
///
/// `case_sensitive` is the editor's match-case setting, applied to what has been typed of the name.
/// `spell` turns the camelCase name into the one the project writes (`|name, _| name` keeps it).
pub fn declaration_name_at(
    source: &str,
    offset: usize,
    case_sensitive: bool,
    spell: impl FnOnce(String, &NameContext<'_>) -> String,
) -> Option<DeclarationName> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    // A caret inside a word is editing that word, not starting the next one.
    if source.as_bytes().get(offset).copied().is_some_and(tokens::is_identifier_byte) {
        return None;
    }
    let tokens = tokens::tokens(source, offset)?;
    let after = tokens.partition_point(|t| t.end <= offset);
    if tokens.get(after).is_some_and(|t| t.start < offset) {
        return None;
    }
    let (typed_start, name_at) = match after.checked_sub(1).map(|i| tokens[i]) {
        Some(last) if last.kind == Kind::Word && last.end == offset => (last.start, after - 1),
        _ => (offset, after),
    };
    let typed = &source[typed_start..offset];
    // A capital is a type being written (`Order|`, `OrderRepository Ord|`), not a name.
    if typed.starts_with(|c: char| !(c.is_lowercase() || c == '_')) {
        return None;
    }
    let before = &tokens[..name_at];
    let type_end = before.last()?.end;
    // A space or tab between the type and the name: none is still the type, a line break is the
    // next statement, and anything else is a comment in between.
    let gap = &source[type_end..typed_start];
    if gap.is_empty() || !gap.bytes().all(|b| b == b' ' || b == b'\t') {
        return None;
    }
    let (type_start, kind) = position::declared_type(before, source)?;
    let suggested = suggested_name_for_type(&source[type_start..type_end])?;
    let in_file = taken::declared_in_file(&tokens, source);
    let spelled = spell(suggested, &NameContext { kind, declared_in_file: &in_file });
    let name = unclaimed(spelled, &taken::declared_names(&tokens, source, name_at, after));
    continues(&name, typed, case_sensitive).then_some(DeclarationName { name, typed_start, typed_end: offset })
}

/// Every variable name `source` declares — fields, locals, parameters — whatever its scope: what a
/// proposed name's spelling is read from when the project declared none. Read off tokens, like the
/// rest of this module, so a buffer mid-edit still answers.
pub fn declared_variable_names(source: &str) -> Vec<&str> {
    // No caret to protect: past the end, nothing hides it.
    tokens::tokens(source, usize::MAX).map(|tokens| taken::declared_in_file(&tokens, source)).unwrap_or_default()
}

/// `name`, or `name1`, `name2`, … — the first that is not already declared.
fn unclaimed(name: String, taken: &[&str]) -> String {
    if !taken.contains(&name.as_str()) {
        return name;
    }
    let mut n = 1;
    loop {
        let candidate = format!("{name}{n}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether `name` is a longer continuation of `typed`: a name that is already fully typed has nothing
/// to add, and one that does not start with what was typed is not what is being written.
fn continues(name: &str, typed: &str, case_sensitive: bool) -> bool {
    if typed.len() >= name.len() {
        return false;
    }
    match name.get(..typed.len()) {
        Some(head) if case_sensitive => head == typed,
        Some(head) => head.eq_ignore_ascii_case(typed),
        None => false,
    }
}
