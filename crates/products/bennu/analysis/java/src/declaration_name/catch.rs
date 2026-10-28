//! The variable of a `catch` clause — `catch (IOException ` is followed by `e`.
//!
//! ## Why this is not [`super::declaration_name_at`]
//!
//! That one proposes the name a TYPE suggests (`OrderRepository` → `orderRepository`), and for a
//! caught exception that is the wrong answer: nobody writes `catch (IOException ioException)`. The
//! convention is `e` — IntelliJ's, the JDK's own documentation's, and what every reader expects —
//! so a catch parameter is its own rule rather than one more position in the general one.
//!
//! ## Why it is a fact
//!
//! After the type of a catch parameter the next token can only be the parameter's name, and the name
//! is a convention, so the proposal is derived rather than predicted. It still refuses wherever the
//! position is not exactly that: something already typed, the caret touching the type, a `catch`
//! that is not in the buffer, a type that is not written like a class.
//!
//! A multi-catch (`A | B `) names its variable the same way. A name already declared in scope gets
//! a digit (`e1`) — a catch parameter may not shadow a local, so unlike a field `e` really would not
//! compile there.

use super::tokens::{self, is, starts_uppercase, text, Kind, Token};
use super::{taken, unclaimed, DeclarationKind, DeclarationName, NameContext};

/// The conventional name of a caught exception, before it is spelled the project's way.
const CAUGHT: &str = "e";

/// The name for the catch parameter whose type ends just before the caret at `offset`, or `None` —
/// the ordinary answer everywhere a catch parameter's name is not what comes next.
///
/// `spell` turns `e` into the project's spelling (`|name, _| name` keeps it), exactly as for
/// [`super::declaration_name_at`].
pub fn catch_variable_at(
    source: &str,
    offset: usize,
    spell: impl FnOnce(String, &NameContext<'_>) -> String,
) -> Option<DeclarationName> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    // A caret inside a word is editing that word, not starting the next one.
    if source.as_bytes().get(offset).copied().is_some_and(tokens::is_identifier_byte) {
        return None;
    }
    let all = tokens::tokens(source, offset)?;
    let after = all.partition_point(|t| t.end <= offset);
    if all.get(after).is_some_and(|t| t.start < offset) {
        return None;
    }
    let last = after.checked_sub(1)?;
    // Nothing typed yet, and the type is finished: the caret is in a gap of spaces after it. A caret
    // touching the type is still writing the type.
    let gap = &source[all[last].end..offset];
    if gap.is_empty() || !gap.bytes().all(|b| b == b' ' || b == b'\t') {
        return None;
    }
    if !ends_a_caught_type(&all, source, last) {
        return None;
    }
    let in_file = taken::declared_in_file(&all, source);
    let spelled = spell(CAUGHT.to_string(), &NameContext { kind: DeclarationKind::Local, declared_in_file: &in_file });
    let name = unclaimed(spelled, &taken::declared_names(&all, source, after, after));
    Some(DeclarationName { name, typed_start: offset, typed_end: offset })
}

/// Whether the tokens up to `last` end in the type of a catch parameter: one or more `|`-separated
/// (qualified) class names, an optional `final`, the `(`, and `catch`.
fn ends_a_caught_type(all: &[Token], source: &str, last: usize) -> bool {
    let mut end = last;
    loop {
        if all[end].kind != Kind::Word || !starts_uppercase(text(source, &all[end])) {
            return false;
        }
        // `java.io.IOException`: walk back over the qualifier.
        let mut first = end;
        while first >= 2 && is(all, first - 1, b'.') && all[first - 2].kind == Kind::Word {
            first -= 2;
        }
        let Some(before) = first.checked_sub(1) else { return false };
        // `A | B`: this was the second alternative; read the first the same way.
        if is(all, before, b'|') {
            match before.checked_sub(1) {
                Some(previous) => {
                    end = previous;
                    continue;
                }
                None => return false,
            }
        }
        let open = if all[before].kind == Kind::Word && text(source, &all[before]) == "final" {
            match before.checked_sub(1) {
                Some(open) => open,
                None => return false,
            }
        } else {
            before
        };
        return is(all, open, b'(')
            && open >= 1
            && all[open - 1].kind == Kind::Word
            && text(source, &all[open - 1]) == "catch";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(src: &str) -> Option<String> {
        catch_variable_at(src, src.len(), |name, _| name).map(|d| d.name)
    }

    const TRY: &str = "class A { void m() { try { run(); } ";

    #[test]
    fn a_caught_type_is_followed_by_the_conventional_name() {
        assert_eq!(name(&format!("{TRY}catch (IOException ")).as_deref(), Some("e"));
    }

    #[test]
    fn a_qualified_type_a_final_modifier_and_a_tab_gap_all_read_the_same() {
        assert_eq!(name(&format!("{TRY}catch (java.io.IOException ")).as_deref(), Some("e"));
        assert_eq!(name(&format!("{TRY}catch (final IOException ")).as_deref(), Some("e"));
        assert_eq!(name(&format!("{TRY}catch (IOException\t")).as_deref(), Some("e"));
    }

    #[test]
    fn a_multi_catch_names_its_one_variable_after_the_last_alternative() {
        assert_eq!(name(&format!("{TRY}catch (IOException | SQLException ")).as_deref(), Some("e"));
        assert_eq!(name(&format!("{TRY}catch (java.io.IOException | java.sql.SQLException ")).as_deref(), Some("e"));
    }

    /// A catch parameter may not shadow a local or a parameter — `e` there would not compile.
    #[test]
    fn a_name_already_in_scope_gets_a_digit() {
        assert_eq!(name("class A { void m(String e) { try { } catch (IOException ").as_deref(), Some("e1"));
        assert_eq!(name("class A { void m() { int e = 1; try { } catch (IOException ").as_deref(), Some("e1"));
    }

    #[test]
    fn a_nested_catch_inside_a_catch_body_does_not_reuse_the_outer_name() {
        let src = format!("{TRY}catch (IOException e) {{ try {{ }} catch (Exception ");
        assert_eq!(name(&src).as_deref(), Some("e1"));
    }

    /// A field named `e` is shadowed legally, so it is not a clash.
    #[test]
    fn a_field_of_that_name_is_not_a_clash() {
        assert_eq!(name("class A { int e; void m() { try { } catch (IOException ").as_deref(), Some("e"));
    }

    #[test]
    fn something_already_typed_or_a_caret_touching_the_type_gets_nothing() {
        assert_eq!(name(&format!("{TRY}catch (IOException e")), None);
        assert_eq!(name(&format!("{TRY}catch (IOException")), None);
        assert_eq!(name(&format!("{TRY}catch (IOException | ")), None);
        assert_eq!(name(&format!("{TRY}catch (IOException\n")), None);
    }

    #[test]
    fn only_a_catch_parameter_is_read() {
        assert_eq!(name("class A { void m() { foo(IOException "), None);
        assert_eq!(name("class A { void m() { if (IOException "), None);
        assert_eq!(name("class A { void m() { try (Resource "), None);
        assert_eq!(name("class A { void m() { IOException "), None);
    }

    #[test]
    fn a_type_that_is_not_written_like_a_class_gets_nothing() {
        assert_eq!(name(&format!("{TRY}catch (foo ")), None);
    }

    #[test]
    fn a_caret_in_a_comment_or_a_string_or_a_word_gets_nothing() {
        assert_eq!(name("// catch (IOException "), None);
        assert_eq!(name("String s = \"catch (IOException "), None);
        let src = format!("{TRY}catch (IOException ex");
        assert_eq!(catch_variable_at(&src, src.len() - 1, |n, _| n), None);
    }

    #[test]
    fn the_name_is_handed_to_the_projects_spelling() {
        let src = format!("{TRY}catch (IOException ");
        let spelled = catch_variable_at(&src, src.len(), |name, ctx| {
            assert_eq!(ctx.kind, DeclarationKind::Local);
            name.to_uppercase()
        });
        assert_eq!(spelled.map(|d| d.name).as_deref(), Some("E"));
    }

    #[test]
    fn the_replaced_range_is_empty_at_the_caret() {
        let src = format!("{TRY}catch (IOException ");
        let found = catch_variable_at(&src, src.len(), |name, _| name).expect("a name");
        assert_eq!((found.typed_start, found.typed_end), (src.len(), src.len()));
    }

    #[test]
    fn an_offset_past_the_end_or_inside_a_character_is_refused() {
        assert_eq!(catch_variable_at("catch (IOException ", 999, |n, _| n), None);
        // `Ü` spans bytes 7..9, so 8 is in the middle of it.
        let src = "catch (Ünicode ";
        assert_eq!(catch_variable_at(src, 8, |n, _| n), None);
    }
}
