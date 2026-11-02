//! A last look at a suggestion before it is drawn: does the kind of thing it writes fit where it is drawn.
//!
//! Only one question, the one that was wrong in practice: a **lambda** (`x -> …`, `() -> {`) or an **anonymous class**
//! (`new Runnable() {`) written where the index knows the expected type is an ordinary class — a `return` in a method that returns
//! `Order`, an initializer of a variable declared `Order`. Neither can be a value of a class that is not a functional interface (the
//! lambda) or the expected type itself (the anonymous class), so it is dropped. Where the expected type is not known, or is an
//! interface, nothing is judged: not knowing is not a reason to remove a line.

use bennu_java::prelude::{expected_type, TypeResolver};

/// What kind of value `text` starts as.
#[derive(Debug, PartialEq, Eq)]
enum Starts {
    Lambda,
    /// `new Name() {` — the name written.
    Anonymous(String),
    Other,
}

fn starts_as(text: &str) -> Starts {
    let t = text.trim_start();
    // `x -> …`, `(a, b) -> …`, `() -> …`
    let head = t.split(['\n', ';']).next().unwrap_or(t);
    if let Some(i) = head.find("->") {
        let before = head[..i].trim();
        let simple = !before.is_empty() && before.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
        let parenthesised = before.starts_with('(') && before.ends_with(')') && !before[1..before.len() - 1].contains('(');
        if simple || parenthesised {
            return Starts::Lambda;
        }
    }
    if let Some(rest) = t.strip_prefix("new ") {
        if let Some(open) = rest.find('(') {
            let name = rest[..open].trim();
            let after = rest[open..].trim_end();
            if after.ends_with('{') && !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '<' | '>')) {
                return Starts::Anonymous(name.split('<').next().unwrap_or(name).to_string());
            }
        }
    }
    Starts::Other
}

/// Whether `text` at `offset` is a lambda or anonymous class where the expected type cannot take one.
pub(crate) fn impossible_here(file: &str, source: &str, offset: usize, text: &str) -> bool {
    let kind = starts_as(text);
    if kind == Starts::Other || !source.is_char_boundary(offset) {
        return false;
    }
    let Some(resolver) = crate::index_service::IndexService::global().caret_resolver_for(file) else { return false };
    let Some(expected) = expected_type(source, offset, &*resolver) else { return false };
    judge(&kind, &expected.binary_name, expected.dims, &*resolver)
}

fn judge(kind: &Starts, expected: &str, dims: u8, resolver: &dyn TypeResolver) -> bool {
    if dims > 0 || expected == "java/lang/Object" {
        return false;
    }
    let Some(members) = resolver.members_of(expected) else { return false };
    match kind {
        Starts::Lambda => !members.flags.is_interface,
        Starts::Anonymous(name) => {
            let simple = expected.rsplit(['/', '$']).next().unwrap_or(expected);
            name.rsplit('.').next().unwrap_or(name) != simple && !members.flags.is_interface
        }
        Starts::Other => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambdas_and_anonymous_classes_are_told_from_everything_else() {
        assert_eq!(starts_as("x -> x.name()"), Starts::Lambda);
        assert_eq!(starts_as("(a, b) -> a + b"), Starts::Lambda);
        assert_eq!(starts_as("() -> {"), Starts::Lambda);
        assert_eq!(starts_as("new Runnable() {"), Starts::Anonymous("Runnable".into()));
        assert_eq!(starts_as("new Foo<String>() {"), Starts::Anonymous("Foo".into()));
        assert_eq!(starts_as("new Order(a, b);"), Starts::Other);
        assert_eq!(starts_as("order.map(x -> x.id());"), Starts::Other, "an arrow inside a call is not the value");
        assert_eq!(starts_as("a > b ? 1 : 2"), Starts::Other);
    }
}
