//! The constants of the **enum an argument slot takes** — offered first, the likeliest one on top.
//!
//! `Device.builder().type(|)` wants a `DeviceType`, and the type-name index cannot say so: it lists classes, and
//! an enum constant is not one. The call already knows — when `type` has one overload at that position its
//! parameter is the enum — so the constants are written out as `DeviceType.PIN`, each with the import it needs.
//!
//! Which constant leads is decided by the **words around the caret**, not by a model: a field the author has just
//! called `pin`, a method named `connectPin`, a local `pinCode` all say `PIN`, and `TIPO_PIN` is split into
//! `tipo` and `pin` so the part that is not the enum's own name does the matching. Nearer words count for more.
//! With nothing in common the declaration order is kept.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::TypeResolver;
use bennu_proto::prelude::CompletionItem;

use crate::completion::split_prefix;
use crate::hints::argument_slot_types;
use crate::resolver::IndexResolver;

/// How far around the caret a word still counts as "right here", and as "in this method or so".
const NEAR: (usize, usize) = (300, 150);
const AROUND: (usize, usize) = (1500, 500);

/// `TIPO_PIN`, `pinCode`, `HTTPServer` → lower-case words.
pub(crate) fn words_of(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let boundary = c.is_uppercase()
            && i > 0
            && chars[i - 1].is_alphanumeric()
            && (chars[i - 1].is_lowercase() || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Two words that mean the same for matching: equal, or one is the other plus a plural `s`.
pub(crate) fn same_word(a: &str, b: &str) -> bool {
    a == b || a.strip_suffix('s') == Some(b) || b.strip_suffix('s') == Some(a)
}

fn window_words(source: &str, caret: usize, (before, after): (usize, usize)) -> HashSet<String> {
    let mut from = caret.saturating_sub(before);
    let mut to = (caret + after).min(source.len());
    while from > 0 && !source.is_char_boundary(from) {
        from -= 1;
    }
    while to < source.len() && !source.is_char_boundary(to) {
        to += 1;
    }
    source[from..to]
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|w| w.len() >= 3)
        .flat_map(words_of)
        .filter(|w| w.len() >= 3)
        .collect()
}

/// How well each constant fits the words around the caret, in the same order as `constants`: the sum, over the
/// constant's own words (the enum's name left out), of 3 for a word right at the caret, 2 for one in the method
/// around it, 1 for one anywhere in the file.
pub(crate) fn fit_scores(constants: &[String], enum_name: &str, source: &str, caret: usize) -> Vec<u32> {
    let near = window_words(source, caret, NEAR);
    let around = window_words(source, caret, AROUND);
    let file = window_words(source, caret, (source.len(), source.len()));
    let own: HashSet<String> = words_of(enum_name).into_iter().collect();
    constants
        .iter()
        .map(|c| {
            words_of(c)
                .iter()
                .filter(|w| !own.contains(*w))
                .map(|w| {
                    let hit = |set: &HashSet<String>| set.iter().any(|x| same_word(x, w));
                    if hit(&near) {
                        3
                    } else if hit(&around) {
                        2
                    } else if hit(&file) {
                        1
                    } else {
                        0
                    }
                })
                .sum()
        })
        .collect()
}

/// The one constant of the enum `slot` that fits the names around `caret` clearly better than all the others, written
/// `Enum.CONSTANT`; `None` when the slot is not an enum, or no constant stands out.
pub(crate) fn clear_constant(source: &str, caret: usize, resolver: &dyn TypeResolver, slot: &bennu_java::prelude::TypeRef) -> Option<String> {
    if slot.dims > 0 {
        return None;
    }
    let members = resolver.members_of(&slot.binary_name)?;
    if !members.flags.is_enum {
        return None;
    }
    let simple = slot.binary_name.rsplit(['/', '$']).next().unwrap_or(&slot.binary_name).to_string();
    let names: Vec<String> = members
        .fields
        .iter()
        .filter(|f| f.is_static && f.return_type.dims == 0 && f.return_type.binary_name == slot.binary_name && !f.name.starts_with('$'))
        .map(|f| f.name.clone())
        .collect();
    let scores = fit_scores(&names, &simple, source, caret);
    let (best, top) = scores.iter().enumerate().max_by_key(|(_, s)| **s)?;
    (*top > 0 && scores.iter().filter(|s| *s == top).count() == 1).then(|| format!("{simple}.{}", names[best]))
}

/// Whether the caret is inside the label list of a `case`: after the keyword, before the `:` or `->`.
pub(crate) fn in_case_label(head: &str) -> bool {
    let Some(at) = head.rmatch_indices("case").map(|(i, _)| i).find(|&i| {
        let before_ok = i == 0 || !head.as_bytes()[i - 1].is_ascii_alphanumeric() && head.as_bytes()[i - 1] != b'_';
        let after_ok = head.as_bytes().get(i + 4).is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        before_ok && after_ok
    }) else {
        return false;
    };
    !head[at + 4..].contains([':', '{', '}', ';', '(']) && !head[at + 4..].contains("->")
}

/// The constants named by the `case` labels of the `switch` the caret is in — the ones a new label has no use for.
pub(crate) fn labels_already_used(source: &str, caret: usize) -> HashSet<String> {
    let b = source.as_bytes();
    // the `{` that opens the block the caret is directly in
    let mut depth = 0i32;
    let mut open = None;
    for i in (0..caret.min(b.len())).rev() {
        match b[i] {
            b'}' => depth += 1,
            b'{' if depth == 0 => {
                open = Some(i);
                break;
            }
            b'{' => depth -= 1,
            _ => {}
        }
    }
    let Some(open) = open else { return HashSet::new() };
    let block = &source[open + 1..source.len().min(find_block_end(b, open))];
    let mut used = HashSet::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < block.len() {
        match block.as_bytes()[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'c' if depth == 0 && block[i..].starts_with("case ") && (i == 0 || !block.as_bytes()[i - 1].is_ascii_alphanumeric()) => {
                let labels: String = block[i + 5..].chars().take_while(|c| !matches!(c, ':' | '-' | '\n' | '{')).collect();
                used.extend(labels.split(',').map(|l| l.trim().rsplit('.').next().unwrap_or("").to_string()).filter(|l| !l.is_empty()));
                i += 5;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    used
}

fn find_block_end(b: &[u8], open: usize) -> usize {
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate().skip(open) {
        match c {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    b.len()
}

/// The constants of the enum the caret is writing a value of — an argument slot of an unambiguous call, a `case`
/// label, the other side of a comparison, a branch of a ternary, an element of an array or `List.of(…)`, a declared
/// variable's initializer — as completions: best fit first, the clear winner preselected.
///
/// Written `Enum.CONSTANT` with the enum's import everywhere except a `case` label, where the language wants the bare
/// name, nothing is imported, and the constants an earlier label of the same `switch` already names are left out.
/// Empty unless the position's type is one enum and the word under the caret (when there is one) starts the constant's
/// name or the enum's.
pub fn enum_argument_constants<M: CpMemberIndex>(
    source: &str,
    caret: usize,
    resolver: &IndexResolver<M>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    let mut caret = caret.min(source.len());
    while caret > 0 && !source.is_char_boundary(caret) {
        caret -= 1;
    }
    let (word_start, prefix) = split_prefix(source, caret);
    let head = source[..word_start].trim_end();
    if head.ends_with('.') || head.ends_with("::") {
        return Vec::new();
    }
    let bare = in_case_label(head);
    // a label is never an argument, even in a switch that sits inside a call
    let slots = if bare { Vec::new() } else { argument_slot_types(source, caret, resolver as &dyn TypeResolver) };
    let slot = match slots.as_slice() {
        [one] => one.clone(),
        [] => match bennu_java::prelude::expected_type(source, caret, resolver as &dyn TypeResolver) {
            Some(t) => t,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    // a parameter that is a type variable (`<T> Matcher<T> helper(T value)`) takes what the call around it makes of it
    let slot = if !bare && crate::slot_inference::is_variable(&slot) {
        match crate::slot_inference::inferred_slot_type(source, caret, resolver as &dyn TypeResolver) {
            Some(t) => t,
            None => return Vec::new(),
        }
    } else {
        slot
    };
    if slot.dims > 0 {
        return Vec::new();
    }
    let Some(members) = resolver.members_of(&slot.binary_name) else { return Vec::new() };
    if !members.flags.is_enum {
        return Vec::new();
    }
    let fqn = slot.binary_name.replace(['/', '$'], ".");
    let simple = slot.binary_name.rsplit(['/', '$']).next().unwrap_or(&slot.binary_name).to_string();
    let used = if bare { labels_already_used(source, caret) } else { HashSet::new() };
    let names: Vec<String> = members
        .fields
        .iter()
        .filter(|f| f.is_static && f.return_type.dims == 0 && f.return_type.binary_name == slot.binary_name && !f.name.starts_with('$'))
        .map(|f| f.name.clone())
        .filter(|n| !used.contains(n))
        .collect();
    let typed = Typed::new(&prefix, case);
    let scores = fit_scores(&names, &simple, source, caret);
    let package = fqn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    let mut rows: Vec<(u32, usize, &String)> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| prefix.is_empty() || typed.matches(n) || (!bare && typed.matches(&simple)))
        .map(|(i, n)| (scores[i], i, n))
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let clear_winner = rows.first().is_some_and(|(top, _, _)| *top > 0 && rows.get(1).is_none_or(|(next, _, _)| next < top));
    rows.iter()
        .enumerate()
        .map(|(rank, (_, _, name))| CompletionItem {
            label: if bare { name.to_string() } else { format!("{simple}.{name}") },
            kind: "field".to_string(),
            detail: Some(fqn.clone()),
            auto_import: (!bare && package != "java.lang").then(|| fqn.clone()),
            preselect: rank == 0 && clear_winner,
            owner: Some(slot.binary_name.clone()),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_split_into_words() {
        assert_eq!(words_of("TIPO_PIN"), ["tipo", "pin"]);
        assert_eq!(words_of("pinCode"), ["pin", "code"]);
        assert_eq!(words_of("HTTPServer"), ["http", "server"]);
    }

    #[test]
    fn the_constant_that_matches_the_name_being_written_leads() {
        let constants: Vec<String> = ["TIPO_USB", "TIPO_BLUETOOTH", "TIPO_PIN"].iter().map(|s| s.to_string()).collect();
        let src = "class Device {\n    private Pin pin;\n    Device build() {\n        return Device.builder().type(";
        let scores = fit_scores(&constants, "Tipo", src, src.len());
        assert!(scores[2] > scores[0] && scores[2] > scores[1], "{scores:?}");
    }

    #[test]
    fn nothing_in_common_leaves_every_score_zero() {
        let constants: Vec<String> = ["USB", "BLUETOOTH"].iter().map(|s| s.to_string()).collect();
        let scores = fit_scores(&constants, "Kind", "class A { void f() { x.kind(", 28);
        assert_eq!(scores, [0, 0]);
    }

    #[test]
    fn a_nearer_word_counts_for_more_and_a_plural_matches() {
        let constants: Vec<String> = ["PIN", "USB"].iter().map(|s| s.to_string()).collect();
        let src = format!("class A {{ int usb; {} void f() {{ pins.go(", " ".repeat(2000));
        let scores = fit_scores(&constants, "Kind", &src, src.len());
        assert_eq!(scores, [3, 1]);
    }

    #[test]
    fn a_case_label_is_found_by_its_keyword() {
        assert!(in_case_label("switch (s) { case"));
        assert!(in_case_label("switch (s) { case A, "));
        assert!(!in_case_label("switch (s) { case A:"));
        assert!(!in_case_label("switch (s) { case A ->"));
        assert!(!in_case_label("int showcase = "));
        assert!(!in_case_label("x = c ? a : "));
    }

    #[test]
    fn the_labels_of_the_enclosing_switch_are_collected() {
        let src = "void f(S s) { switch (s) { case A: g(); break; case B, C -> h(); case ";
        let used = labels_already_used(src, src.len());
        let mut got: Vec<&str> = used.iter().map(String::as_str).collect();
        got.sort();
        assert_eq!(got, ["A", "B", "C"]);
        // an inner switch's labels are not the outer one's
        let src = "void f(S s) { switch (s) { case A: switch (t) { case X: break; } break; case ";
        assert_eq!(labels_already_used(src, src.len()).len(), 1);
    }
}
