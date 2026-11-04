//! Bennu's own verdict on a model suggestion: would the line, written out, break in a way that means the model
//! **invented** something?
//!
//! A suggestion can be right and still not compile: it names `HttpStatus` and the file has not imported it yet, it is
//! the start of a statement and the `;` is not there, it throws a checked exception nobody catches. All of that is
//! the user's next keystroke, not the model's mistake, so none of it counts. What counts is what the code checker
//! says is *wrong rather than missing*: a method the receiver does not have, the wrong number of arguments, an
//! argument of the wrong type, a non-static member from a static place, a variable that exists nowhere. Only those
//! errors, and only the ones that are **new** on that line once the suggestion is in it.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::index_service::IndexService;

/// The kinds of error that say "this does not exist / does not fit", as opposed to "this is not finished" or "this
/// needs an import".
const INVENTED: &[&str] = &[
    "unknown-member",
    "unknown-field",
    "wrong-argument-count",
    "argument-type",
    "unresolved-call",
    "unresolved-super-method",
    "wrong-type-argument-count",
    "incompatible-type",
    "non-boolean-condition",
    "static-context-access",
    "inaccessible-member",
    "unknown-enum-case-label",
    "lambda-arity",
    "duplicate-annotation-value",
    "unknown-annotation-element",
];

/// Errors that mean "unknown name": invented unless it is a class that only needs importing.
const UNKNOWN_NAME: &[&str] = &["unresolved-type", "unresolved-symbol"];

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Switches the check off or on — for the quality harness, which measures with and without it.
#[cfg(test)]
pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Above this many bytes a file is not validated per suggestion: the check would cost more than the suggestion is worth.
const MAX_FILE_BYTES: usize = 150_000;

/// Whether a class named `name` exists in the project or on its classpath (so an unresolved `HttpStatus` is a missing
/// import, not an invention).
fn is_real_class(file: &str, name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        && IndexService::global().type_name_matches(file, name, 30).iter().any(|n| n == name)
}

/// `source` with the body of every method and constructor except the one holding `offset` blanked out (spaces, newlines
/// kept, so every offset still points at the same place). The checker then validates one method, not the whole file;
/// the signatures stay, so the types around still resolve.
fn slice_to_method(source: &str, offset: usize) -> String {
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return source.to_string() };
    let mut bytes = source.as_bytes().to_vec();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "method_declaration" | "constructor_declaration") {
                if let Some(body) = child.child_by_field_name("body") {
                    let (from, to) = (body.start_byte() + 1, body.end_byte().saturating_sub(1));
                    if !(body.start_byte() <= offset && offset <= body.end_byte()) && from < to {
                        for b in &mut bytes[from..to] {
                            if *b != b'\n' {
                                *b = b' ';
                            }
                        }
                    }
                }
            } else {
                stack.push(child);
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| source.to_string())
}

/// The variable a line declares, when it is a declaration with an initializer (`val x = …`, `Foo x = …`,
/// `final var x = …`): the identifier just before the first `=`, after at least one type word.
fn declared_name(line_prefix: &str) -> Option<String> {
    let head = line_prefix.trim_start();
    let eq = head.find('=')?;
    if head[eq..].starts_with("==") || head[..eq].contains(['(', ')', '.']) {
        return None;
    }
    let before = head[..eq].trim_end();
    let name: String = before.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect::<Vec<_>>().into_iter().rev().collect();
    let rest = before[..before.len() - name.len()].trim_end();
    (!name.is_empty() && !rest.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then_some(name)
}

/// Whether `text` holds `name` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let b = text.as_bytes();
        let before = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'$');
        let after = i + name.len() >= b.len() || !(b[i + name.len()].is_ascii_alphanumeric() || b[i + name.len()] == b'_' || b[i + name.len()] == b'$');
        before && after
    })
}

/// Errors that land on a **later use** of the variable the line declares: what the suggestion gave it as a type does
/// not fit how the rest of the method uses it (an `Object` where an `if` wants a boolean). The line itself is
/// fine, which is why the line-only check cannot see it.
fn errors_at_uses(file: &str, source: &str, name: &str, after: usize) -> usize {
    IndexService::global()
        .validate_java_scratch(file, source)
        .iter()
        .filter(|d| d.severity == "error" && d.start >= after)
        .filter(|d| INVENTED.contains(&d.code.as_str()))
        .filter(|d| source.get(d.start..d.end).is_some_and(|t| mentions(t, name)))
        .count()
}

/// The number of invented-kind errors that start inside `[from, to]` of `source`.
fn invented_errors(file: &str, source: &str, from: usize, to: usize) -> usize {
    IndexService::global()
        .validate_java_scratch(file, source)
        .iter()
        .filter(|d| d.severity == "error" && d.start >= from && d.start <= to)
        .filter(|d| {
            let code = d.code.as_str();
            if INVENTED.contains(&code) {
                return true;
            }
            UNKNOWN_NAME.contains(&code) && !is_real_class(file, source.get(d.start..d.end).unwrap_or("").trim())
        })
        .count()
}

/// Whether writing `text` at `offset` of `source` (the buffer as the editor holds it, nothing after the caret on the
/// line) puts a new invented-kind error on that line.
pub(crate) fn invents(file: &str, source: &str, offset: usize, text: &str) -> bool {
    if !ENABLED.load(Ordering::Relaxed) || source.len() > MAX_FILE_BYTES || offset > source.len() || !source.is_char_boundary(offset) {
        return false;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let eol_before = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    // One validation of the file, not two: with the suggestion in, most lines have no such error and that is the
    // answer. Only when one shows up is the line validated as it was, to tell a new error from one already there.
    let new_invention = |written: &str| -> bool {
        let with = slice_to_method(&format!("{}{}{}", &source[..offset], written, &source[offset..]), offset);
        let eol_after = eol_before + written.len();
        let after = invented_errors(file, &with, line_start, eol_after);
        after > 0 && after > invented_errors(file, &slice_to_method(source, offset), line_start, eol_before)
    };
    if new_invention(text) {
        return true;
    }
    // A suggestion that stops short of its `;` leaves the file with a syntax error, and the checker says nothing else about
    // a file it cannot parse: `Foo x = nobody.call()` would pass for want of the semicolon. The same line is judged as the
    // finished statement it will become.
    if needs_semicolon(&source[line_start..offset], text) && new_invention(&format!("{text};")) {
        return true;
    }
    let with = slice_to_method(&format!("{}{}{}", &source[..offset], text, &source[offset..]), offset);
    let eol_after = eol_before + text.len();
    // a variable the line declares and the rest of the method uses: its new type has to fit those uses
    declared_name(&source[line_start..offset]).is_some_and(|name| errors_at_uses(file, &with, &name, eol_after) > 0)
}

/// Whether the line `before` + `text` is a statement that only lacks its `;`: a finished expression with its brackets closed.
fn needs_semicolon(before: &str, text: &str) -> bool {
    let line = format!("{before}{text}");
    // only a finished expression: one ending in a closing bracket or a literal. A bare name may be the start of a longer
    // one (`holder.get`), and judging it as complete would drop a good suggestion for a method it has not finished writing.
    if !line.trim_end().ends_with([')', ']', '"', '\'']) {
        return false;
    }
    let balanced = |open: char, close: char| line.chars().filter(|c| *c == open).count() == line.chars().filter(|c| *c == close).count();
    balanced('(', ')') && balanced('[', ']') && balanced('{', '}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_variable_is_the_name_before_the_equals() {
        assert_eq!(declared_name("        val mandatory_username = ").as_deref(), Some("mandatory_username"));
        assert_eq!(declared_name("    final Optional<String> name =").as_deref(), Some("name"));
        assert_eq!(declared_name("        x = ").as_deref(), None, "an assignment declares nothing");
        assert_eq!(declared_name("        if (a == b").as_deref(), None);
        assert_eq!(declared_name("        foo.bar = 3").as_deref(), None);
    }

    #[test]
    fn a_name_is_mentioned_only_as_a_whole_word() {
        assert!(mentions("if (flag) go()", "flag"));
        assert!(!mentions("if (flags) go()", "flag"));
        assert!(!mentions("isflag", "flag"));
    }

    #[test]
    fn only_the_method_around_the_caret_keeps_its_body() {
        let src = "class A {\n    int f() {\n        return 1;\n    }\n    int g() {\n        return 2;\n    }\n}\n";
        let at = src.find("return 2").unwrap() + 3;
        let cut = slice_to_method(src, at);
        assert_eq!(cut.len(), src.len());
        assert!(cut.contains("return 2") && !cut.contains("return 1"));
        assert_eq!(cut.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn annotation_element_errors_count_as_invention() {
        // the same code strings the checker emits (analysis/check check_id.rs)
        assert!(INVENTED.contains(&"duplicate-annotation-value") && INVENTED.contains(&"unknown-annotation-element"));
    }

    #[test]
    fn a_line_only_missing_its_semicolon_is_a_statement_to_judge() {
        assert!(needs_semicolon("        Object e = ", "holder.getEncryption()"));
        assert!(!needs_semicolon("        Object e = ", "holder.get"));
        assert!(!needs_semicolon("        call(", "a, b"));
        assert!(!needs_semicolon("        Object e = ", "holder.getEncryption();"));
        assert!(!needs_semicolon("        if (", "x == y"));
    }

    #[test]
    fn the_kinds_are_disjoint() {
        assert!(INVENTED.iter().all(|c| !UNKNOWN_NAME.contains(c)));
    }
}
