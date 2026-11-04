//! The space (or none) between two neighbouring tokens on a line.
//!
//! With `normalize_spacing` off every gap is [`Gap::Keep`] — the author's own whitespace, run for
//! run — which is what makes the default style safe. With it on, each pair is decided by what the
//! two tokens *are* (see `classify`), and a pair no rule covers still keeps the author's spacing:
//! the rules only ever claim what they are sure about.

use super::classify::{is_control_word, Info, Role};
use super::lex::{Kind, Tok};
use super::style::FormatStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gap {
    /// The author's whitespace, unchanged.
    Keep,
    One,
    Zero,
}

fn one(b: bool) -> Gap {
    if b { Gap::One } else { Gap::Zero }
}

/// The gap before every token (the first token of a line ignores it).
pub(crate) fn gaps(toks: &[Tok], info: &[Info], style: &FormatStyle) -> Vec<Gap> {
    (0..toks.len())
        .map(|i| if i == 0 || !style.normalize_spacing { Gap::Keep } else { gap_before(toks, info, i, style) })
        .collect()
}

fn is_atom(t: &Tok) -> bool {
    matches!(t.kind, Kind::Word | Kind::Number | Kind::Str | Kind::TextBlock)
}

/// The space a role asks for *before* its own token.
fn before_rule(toks: &[Tok], info: &[Info], i: usize, style: &FormatStyle) -> Option<bool> {
    let p = &toks[i - 1];
    match info[i].role {
        Role::Binary => Some(style.space_around_binary),
        Role::Assign => Some(style.space_around_assign),
        Role::TernaryQ | Role::TernaryColon => Some(style.space_around_ternary),
        Role::Arrow => Some(style.space_around_arrow),
        Role::ForEachColon => Some(style.space_around_foreach_colon),
        Role::LabelColon | Role::Postfix => Some(false),
        Role::GenericOpen => Some(p.is_word() && super::classify::is_modifier(&p.text) || p.is("}") || p.is(";") || p.is("{")),
        Role::GenericClose => Some(style.space_within_angles && !p.is("<")),
        _ => None,
    }
}

/// The space a role asks for *after* its own token.
fn after_rule(toks: &[Tok], info: &[Info], i: usize, style: &FormatStyle) -> Option<bool> {
    let p = i - 1;
    let t = &toks[i];
    match info[p].role {
        Role::Binary => Some(style.space_around_binary),
        Role::Assign => Some(style.space_around_assign),
        Role::TernaryQ | Role::TernaryColon => Some(style.space_around_ternary),
        Role::Arrow => Some(style.space_around_arrow),
        Role::ForEachColon => Some(style.space_around_foreach_colon),
        Role::LabelColon => Some(!t.is(";")),
        Role::Unary => Some(false),
        Role::CastClose => Some(style.space_after_cast),
        Role::Wildcard => Some(t.is_word()),
        Role::GenericOpen => Some(style.space_within_angles && !t.is(">")),
        Role::GenericClose => {
            if t.is("(") || t.is("[") || t.is(">") || t.is(",") || t.is(")") || t.is(";") {
                return Some(false);
            }
            // \`.<T>foo()\` — explicit type arguments hug the method name.
            let opener = info[p].matching;
            let explicit = opener != super::classify::NO_MATCH && opener > 0 && toks[opener - 1].is(".");
            Some(!explicit)
        }
        _ => None,
    }
}

fn gap_before(toks: &[Tok], info: &[Info], i: usize, style: &FormatStyle) -> Gap {
    let p = &toks[i - 1];
    let t = &toks[i];

    // Comments: the author decides, except that code glued to a comment it follows is left glued.
    if p.is_comment() || t.is_comment() {
        return Gap::Keep;
    }

    // ── Separators ──────────────────────────────────────────────────────────────────────
    if t.is(";") {
        return Gap::Zero;
    }
    if t.is(",") {
        return one(style.space_before_comma);
    }
    if p.is(",") {
        return if t.is(")") || t.is("]") || (t.is("}") && info[i].brace == Some(super::classify::BraceKind::Array)) {
            Gap::Zero
        } else {
            one(style.space_after_comma)
        };
    }
    if p.is(";") {
        // `;}` is the author's call (one-line bodies); only a `for` header's `;)` / `;;` is tightened.
        return if t.is(")") || t.is(";") {
            Gap::Zero
        } else if t.is("}") {
            Gap::Keep
        } else {
            Gap::One
        };
    }
    if t.is(".") || p.is(".") || t.is("::") || p.is("::") || t.is("...") {
        return Gap::Zero;
    }
    if p.is("...") {
        return Gap::One;
    }

    // `a >> b`, `a >>> b`, `a >>= b`: adjacent `>`s are one operator.
    if p.is(">") && (t.is(">") || t.is(">=")) && !t.ws_before && info[i - 1].role == Role::Binary {
        return Gap::Zero;
    }

    // ── Roles ───────────────────────────────────────────────────────────────────────────
    let before = before_rule(toks, info, i, style);
    let after = after_rule(toks, info, i, style);
    match (before, after) {
        (Some(true), _) | (_, Some(true)) => return Gap::One,
        (Some(false), _) | (_, Some(false)) => {
            // A closer or opener still decides for itself: \`foo(-1)\` is claimed by the paren below.
            if !(p.is("(") || p.is("[") || t.is(")") || t.is("]")) {
                return Gap::Zero;
            }
        }
        _ => {}
    }

    // ── Brackets ────────────────────────────────────────────────────────────────────────
    if t.is("(") {
        if p.is_word() {
            if is_control_word(&p.text) {
                return one(style.space_before_control_paren);
            }
            return if matches!(p.text.as_str(), "return" | "throw" | "new" | "case" | "else" | "assert" | "yield" | "do" | "in") {
                Gap::One
            } else {
                one(style.space_before_call_paren)
            };
        }
        if info[i - 1].role == Role::GenericClose {
            return one(style.space_before_call_paren);
        }
        if p.is("@") {
            return Gap::Zero;
        }
    }
    if p.is("(") {
        return if t.is(")") { Gap::Zero } else { one(style.space_within_parens) };
    }
    if t.is(")") {
        return one(style.space_within_parens);
    }
    if p.is("[") {
        return if t.is("]") { Gap::Zero } else { one(style.space_within_brackets) };
    }
    if t.is("]") {
        return one(style.space_within_brackets);
    }
    if t.is("[") && (p.is_word() || p.is("]") || p.is(")") || info[i - 1].role == Role::GenericClose) {
        return Gap::Zero;
    }
    if p.is("]") && (t.is_word() || t.kind == Kind::Number || t.kind == Kind::Str) {
        return Gap::One;
    }

    // ── Braces ──────────────────────────────────────────────────────────────────────────
    if t.is("{") {
        if info[i].brace == Some(super::classify::BraceKind::Array) {
            return if p.is("{") { one(style.space_within_array_braces) } else { Gap::One };
        }
        return one(style.space_before_lbrace);
    }
    if p.is("{") {
        if t.is("}") {
            return Gap::Zero;
        }
        if info[i - 1].brace == Some(super::classify::BraceKind::Array) {
            return one(style.space_within_array_braces);
        }
        return Gap::Keep;
    }
    if t.is("}") {
        if info[i].brace == Some(super::classify::BraceKind::Array) {
            return one(style.space_within_array_braces);
        }
        return Gap::Keep;
    }
    if p.is("}") && t.is_word() {
        return Gap::One;
    }

    // ── Annotations ─────────────────────────────────────────────────────────────────────
    if p.is("@") {
        return Gap::Zero;
    }
    if t.is("@") {
        return Gap::One;
    }

    // ── Words and operands ──────────────────────────────────────────────────────────────
    if is_atom(p) && is_atom(t) {
        return Gap::One;
    }
    if (p.is(")") || p.is("]")) && is_atom(t) {
        return Gap::One;
    }
    if info[i].role == Role::Unary && !matches!(t.text.as_str(), "++" | "--" | "-" | "+" | "!" | "~") {
        return Gap::Keep;
    }
    if info[i].role == Role::Unary && (is_atom(p) || p.is(")") || p.is("]")) {
        return Gap::One;
    }
    Gap::Keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::classify::classify;
    use crate::format::lex::lex;

    /// Re-join `src` using only the spacing rules (no layout), on one line.
    fn spaced(src: &str, style: &FormatStyle) -> String {
        let toks = lex(src);
        let info = classify(&toks);
        let gaps = gaps(&toks, &info, style);
        let mut out = String::new();
        for (i, t) in toks.iter().enumerate() {
            if i > 0 {
                match gaps[i] {
                    Gap::One => out.push(' '),
                    Gap::Zero => {}
                    Gap::Keep => out.push_str(&t.gap),
                }
            }
            out.push_str(&t.text);
        }
        out
    }

    fn on() -> FormatStyle {
        FormatStyle { normalize_spacing: true, ..FormatStyle::default() }
    }

    #[test]
    fn spacing_is_untouched_unless_asked_for() {
        assert_eq!(spaced("if(a  ==b){x=1;}", &FormatStyle::default()), "if(a  ==b){x=1;}");
    }

    #[test]
    fn operators_and_keywords_get_their_spaces() {
        assert_eq!(spaced("if(a==b&&c<d){x=a+b*2;}", &on()), "if (a == b && c < d) {x = a + b * 2;}");
    }

    #[test]
    fn generics_casts_and_unary_operators_are_left_tight_or_spaced_by_role() {
        assert_eq!(spaced("Map<String,List<Integer>> m=new HashMap<>();", &on()), "Map<String, List<Integer>> m = new HashMap<>();");
        assert_eq!(spaced("x=(String)o;", &on()), "x = (String) o;");
        assert_eq!(spaced("y=-a+ ++b;", &on()), "y = -a + ++b;");
        assert_eq!(spaced("f(a,b);", &on()), "f(a, b);");
    }

    #[test]
    fn each_rule_can_be_turned_around() {
        let style = FormatStyle {
            normalize_spacing: true,
            space_before_control_paren: false,
            space_around_binary: false,
            space_after_comma: false,
            space_within_parens: true,
            ..FormatStyle::default()
        };
        assert_eq!(spaced("if (a + b) f(x, y);", &style), "if( a+b ) f( x,y );");
    }

    #[test]
    fn a_shift_operator_stays_one_operator() {
        assert_eq!(spaced("x=a>>2;", &on()), "x = a >> 2;");
    }

    #[test]
    fn lambda_arrows_ternaries_and_foreach_colons() {
        assert_eq!(spaced("f(x->x+1);", &on()), "f(x -> x + 1);");
        assert_eq!(spaced("y=a?b:c;", &on()), "y = a ? b : c;");
        assert_eq!(spaced("for(String s:list){}", &on()), "for (String s : list) {}");
    }
}
