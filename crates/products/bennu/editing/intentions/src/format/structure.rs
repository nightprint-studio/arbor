//! Edits to the *shape* of the code — where braces sit, which statements get braces, how annotations
//! and members are separated — made by adjusting the line-break flag of the affected tokens (or, for
//! forced braces, inserting two tokens). Nothing here touches indentation or spacing; those are the
//! later passes' job, which is why these can be expressed as "break here / do not break here".

use super::classify::{next_sig, prev_sig, BraceKind, Info, NO_MATCH};
use super::lex::{Kind, Tok};
use super::style::{AnnotationPlacement, BraceStyle, FormatStyle};

/// A run of tokens ending in `;`, `{` or `}` — one statement, declaration or block header.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Unit {
    pub start: usize,
    pub end: usize,
}

/// Cut the token stream into units. Trailing comments stay with the unit they trail.
pub(crate) fn units(toks: &[Tok], info: &[Info]) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    let mut start: Option<usize> = None;
    // Bracket depth per brace scope, so a `;` inside a lambda body is a boundary even though the
    // lambda sits inside parentheses.
    let mut depth: Vec<i32> = vec![0];

    for i in 0..toks.len() {
        let t = &toks[i];
        if start.is_none() && t.is_comment() && t.nl_before == 0 {
            if let Some(last) = out.last_mut() {
                last.end = i;
                continue;
            }
        }
        let first = *start.get_or_insert(i);
        if t.kind != Kind::Punct {
            continue;
        }
        let top = depth.len() - 1;
        match t.text.as_str() {
            "(" | "[" => depth[top] += 1,
            ")" | "]" => depth[top] -= 1,
            ";" if depth[top] <= 0 => {
                out.push(Unit { start: first, end: i });
                start = None;
            }
            "{" => {
                if info[i].brace == Some(BraceKind::Array) {
                    depth[top] += 1;
                } else {
                    out.push(Unit { start: first, end: i });
                    start = None;
                    depth.push(0);
                }
            }
            "}" => {
                if info[i].brace == Some(BraceKind::Array) {
                    depth[top] -= 1;
                } else {
                    if first < i {
                        out.push(Unit { start: first, end: i - 1 });
                    }
                    out.push(Unit { start: i, end: i });
                    start = None;
                    if depth.len() > 1 {
                        depth.pop();
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        out.push(Unit { start: first, end: toks.len() - 1 });
    }
    out
}

// ── Braces ──────────────────────────────────────────────────────────────────────────────

/// Whether everything from the `{` at `open` to its `}` is on one line.
fn is_one_line(toks: &[Tok], info: &[Info], open: usize) -> bool {
    let close = info[open].matching;
    close != NO_MATCH && toks[open + 1..=close].iter().all(|t| t.nl_before == 0)
}

pub(crate) fn apply_braces(toks: &mut [Tok], info: &[Info], style: &FormatStyle) {
    for i in 1..toks.len() {
        if toks[i - 1].is_comment() {
            continue;
        }
        if toks[i].is("{") {
            let want = match info[i].brace {
                Some(BraceKind::Type) => style.brace_type,
                Some(BraceKind::Method) => style.brace_method,
                Some(BraceKind::Block) | Some(BraceKind::Switch) => style.brace_block,
                _ => BraceStyle::Preserve,
            };
            // A block with nothing before it on the statement (`{ … }` on its own, an initialiser
            // after a member) has no header to attach to.
            let attached = !(toks[i - 1].is("{") || toks[i - 1].is("}") || toks[i - 1].is(";"));
            match want {
                BraceStyle::SameLine if attached && toks[i].nl_before > 0 => toks[i].nl_before = 0,
                BraceStyle::NextLine if attached && toks[i].nl_before == 0 && !is_one_line(toks, info, i) => {
                    toks[i].nl_before = 1;
                }
                _ => {}
            }
        } else if toks[i].is_word() && toks[i - 1].is("}") && info[i - 1].brace != Some(BraceKind::Array) {
            let continues = match toks[i].text.as_str() {
                "else" | "catch" | "finally" => true,
                // The `while` of a `do { } while (…);`.
                "while" => {
                    let open = info[i - 1].matching;
                    open != NO_MATCH && prev_sig(toks, open).is_some_and(|d| toks[d].is("do"))
                }
                _ => false,
            };
            if !continues {
                continue;
            }
            match style.brace_continuation {
                BraceStyle::SameLine => toks[i].nl_before = 0,
                BraceStyle::NextLine => toks[i].nl_before = toks[i].nl_before.max(1),
                BraceStyle::Preserve => {}
            }
        }
    }
}

// ── Forced braces ───────────────────────────────────────────────────────────────────────

fn synthetic(text: &str, nl_before: usize) -> Tok {
    Tok { kind: Kind::Punct, text: text.to_string(), nl_before, ws_before: true, gap: " ".to_string() }
}

/// Wrap the single-statement body of every `if` / `else` / `for` / `while` that has no braces.
/// Returns whether anything changed (the caller then re-classifies).
///
/// Only bodies that are a simple statement ending in `;` are touched. A body that is itself a control
/// statement (`if (a) if (b) x();`, a dangling `else`) is left alone: where its braces belong is a
/// question a text-level tool should not answer.
pub(crate) fn force_braces(toks: &mut Vec<Tok>, info: &[Info]) -> bool {
    let mut sites: Vec<(usize, usize)> = Vec::new();
    for i in 0..toks.len() {
        if !toks[i].is_word() {
            continue;
        }
        let body = match toks[i].text.as_str() {
            "if" | "for" | "while" => {
                let Some(open) = next_sig(toks, i) else { continue };
                if !toks[open].is("(") || info[open].matching == NO_MATCH {
                    continue;
                }
                next_sig(toks, info[open].matching)
            }
            "else" => match next_sig(toks, i) {
                Some(n) if toks[n].is("if") => continue,
                other => other,
            },
            _ => continue,
        };
        let Some(b) = body else { continue };
        let bt = &toks[b];
        if bt.is("{") || bt.is(";") {
            continue;
        }
        if bt.is_word()
            && matches!(bt.text.as_str(), "if" | "for" | "while" | "do" | "try" | "switch" | "synchronized" | "else")
        {
            continue;
        }
        let mut depth = 0i32;
        let mut end = None;
        for (j, t) in toks.iter().enumerate().skip(b) {
            if t.kind != Kind::Punct {
                continue;
            }
            match t.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => {
                    depth -= 1;
                    if depth < 0 {
                        break;
                    }
                }
                ";" if depth == 0 => {
                    end = Some(j);
                    break;
                }
                _ => {}
            }
        }
        if let Some(e) = end {
            sites.push((b, e));
        }
    }
    if sites.is_empty() {
        return false;
    }

    // Insertions, applied in one rebuild so nested sites cannot shift each other's indices.
    // (index to insert before, order at the same index, token)
    let mut inserts: Vec<(usize, u8, Tok)> = Vec::new();
    for &(b, e) in &sites {
        toks[b].nl_before = toks[b].nl_before.max(1);
        inserts.push((b, 1, synthetic("{", 0)));
        inserts.push((e + 1, 0, synthetic("}", 1)));
        if let Some(next) = toks.get_mut(e + 1) {
            if !next.is_word() || next.text != "else" {
                if !(next.is_comment() && next.nl_before == 0) {
                    next.nl_before = next.nl_before.max(1);
                }
            }
        }
    }
    inserts.sort_by_key(|(at, order, _)| (*at, *order));

    let mut out: Vec<Tok> = Vec::with_capacity(toks.len() + inserts.len());
    let mut pending = inserts.into_iter().peekable();
    for (i, t) in toks.drain(..).enumerate() {
        while pending.peek().is_some_and(|(at, _, _)| *at == i) {
            out.push(pending.next().map(|(_, _, tok)| tok).unwrap());
        }
        out.push(t);
    }
    out.extend(pending.map(|(_, _, tok)| tok));
    *toks = out;
    true
}

// ── Annotations ─────────────────────────────────────────────────────────────────────────

pub(crate) fn apply_annotations(toks: &mut [Tok], info: &[Info], units: &[Unit], style: &FormatStyle) {
    if style.annotations_type == AnnotationPlacement::Preserve
        && style.annotations_method == AnnotationPlacement::Preserve
        && style.annotations_field == AnnotationPlacement::Preserve
    {
        return;
    }
    for u in units {
        // Declarations only: a member of a type, or a top-level type.
        match info[u.start].scope {
            None | Some(BraceKind::Type) | Some(BraceKind::Anon) => {}
            _ => continue,
        }
        let policy = match unit_kind(toks, info, u) {
            DeclKind::Type => style.annotations_type,
            DeclKind::Method => style.annotations_method,
            DeclKind::Field => style.annotations_field,
            DeclKind::Other => continue,
        };
        if policy == AnnotationPlacement::Preserve {
            continue;
        }
        let mut i = u.start;
        while i <= u.end && toks[i].is_comment() {
            i += 1;
        }
        while i < u.end && toks[i].is("@") && toks.get(i + 1).is_some_and(|n| n.is_word() && n.text != "interface") {
            // `@a.b.Name`
            let mut j = i + 2;
            while j + 1 <= u.end && toks[j].is(".") && toks[j + 1].is_word() {
                j += 2;
            }
            let marker = !(j <= u.end && toks[j].is("("));
            if !marker {
                let close = info[j].matching;
                if close == NO_MATCH || close >= u.end {
                    break;
                }
                j = close + 1;
            }
            if j > u.end || toks[j].is_comment() {
                break;
            }
            match policy {
                AnnotationPlacement::OwnLine => toks[j].nl_before = toks[j].nl_before.max(1),
                AnnotationPlacement::SameLine if marker && toks[j].nl_before > 0 => toks[j].nl_before = 0,
                _ => {}
            }
            i = j;
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum DeclKind {
    Type,
    Method,
    Field,
    Other,
}

fn unit_kind(toks: &[Tok], info: &[Info], u: &Unit) -> DeclKind {
    let end = &toks[u.end];
    if end.is("{") {
        return match info[u.end].brace {
            Some(BraceKind::Type) => DeclKind::Type,
            Some(BraceKind::Method) => DeclKind::Method,
            _ => DeclKind::Other,
        };
    }
    if !end.is(";") {
        return DeclKind::Other;
    }
    let mut paren = false;
    for (j, t) in toks.iter().enumerate().take(u.end + 1).skip(u.start) {
        if t.is("=") {
            return DeclKind::Field;
        }
        if t.is("(") && info[j].role == super::classify::Role::None {
            paren = true;
            break;
        }
    }
    if paren { DeclKind::Method } else { DeclKind::Field }
}

// ── Blank lines ─────────────────────────────────────────────────────────────────────────

fn force_blank_lines(tok: &mut Tok, blank_lines: usize) {
    tok.nl_before = tok.nl_before.max(blank_lines + 1);
}

pub(crate) fn apply_blank_lines(toks: &mut [Tok], info: &[Info], units: &[Unit], style: &FormatStyle) {
    if style.blank_lines_after_header > 0 {
        let mut last_header: Option<&Unit> = None;
        for u in units {
            let first = toks.iter().skip(u.start).take(u.end + 1 - u.start).find(|t| !t.is_comment());
            match first {
                Some(t) if t.is("package") || t.is("import") => last_header = Some(u),
                _ => {}
            }
        }
        if let Some(u) = last_header {
            let end = u.end;
            if let Some(next) = toks.get_mut(end + 1) {
                if next.nl_before > 0 {
                    force_blank_lines(next, style.blank_lines_after_header);
                }
            }
        }
    }

    if style.blank_lines_before_member > 0 {
        for u in units {
            if !matches!(info[u.start].scope, Some(BraceKind::Type) | Some(BraceKind::Anon)) {
                continue;
            }
            if !matches!(unit_kind(toks, info, u), DeclKind::Method | DeclKind::Type) {
                continue;
            }
            // Not the first member: something other than the opening brace precedes it.
            let Some(p) = prev_sig(toks, u.start) else { continue };
            if toks[p].is("{") || toks[u.start].nl_before == 0 {
                continue;
            }
            force_blank_lines(&mut toks[u.start], style.blank_lines_before_member);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::classify::classify;
    use crate::format::lex::lex;

    #[test]
    fn units_end_at_semicolons_and_braces() {
        let toks = lex("class A { int x; void m() { f(); } }");
        let info = classify(&toks);
        let us = units(&toks, &info);
        let texts: Vec<String> = us
            .iter()
            .map(|u| toks[u.start..=u.end].iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(texts, ["class A {", "int x ;", "void m ( ) {", "f ( ) ;", "}", "}"]);
    }

    #[test]
    fn a_lambda_block_inside_a_call_does_not_swallow_the_statement() {
        let toks = lex("run(() -> { a(); b(); });");
        let info = classify(&toks);
        let us = units(&toks, &info);
        assert_eq!(us.len(), 5, "{us:?}");
    }

    #[test]
    fn force_braces_wraps_a_simple_body_only() {
        let mut toks = lex("if (a) x(); else y();");
        let info = classify(&toks);
        assert!(force_braces(&mut toks, &info));
        let joined: String = toks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined, "if ( a ) { x ( ) ; } else { y ( ) ; }");

        // The outer `if`'s body is itself an `if`: left alone. The inner one is simple and is wrapped.
        let mut nested = lex("if (a) if (b) x();");
        let info = classify(&nested);
        assert!(force_braces(&mut nested, &info));
        let joined: String = nested.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(joined, "if ( a ) if ( b ) { x ( ) ; }");
    }
}
