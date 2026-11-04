//! What each token *is*, to the extent a formatter needs to know: which `<` opens a generic, which
//! `-` is binary, which `(` starts a cast, what kind of block a `{` opens.
//!
//! Heuristic by design — there is no parse tree — and every heuristic errs towards "leave it alone":
//! an operator that might be binary or unary is classified by what precedes it, a `<` is a generic
//! only if a closing `>` is reachable through nothing but type syntax, and a token that fits no rule
//! gets [`Role::None`], which the spacing pass treats as "keep the author's spacing".

use super::lex::{Kind, Tok};

/// What a `{` opens (and its `}` closes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BraceKind {
    /// `class` / `interface` / `enum` / `record` / `@interface` body.
    Type,
    /// A method, constructor or initialiser body.
    Method,
    /// `if` / `for` / `while` / `do` / `try` / `catch` / `finally` / `synchronized` / bare block.
    Block,
    /// A `switch` body.
    Switch,
    /// `{1, 2}` — an array initialiser or annotation array.
    Array,
    /// A lambda or arrow-case body.
    Lambda,
    /// An anonymous class or enum-constant body.
    Anon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    None,
    Binary,
    Assign,
    /// Prefix `+ - ! ~ ++ --`.
    Unary,
    /// Postfix `++ --`.
    Postfix,
    TernaryQ,
    TernaryColon,
    ForEachColon,
    /// `case 1:`, `label:`.
    LabelColon,
    Arrow,
    GenericOpen,
    GenericClose,
    /// `?` in `List<?>`.
    Wildcard,
    CastOpen,
    CastClose,
}

#[derive(Clone, Debug)]
pub(crate) struct Info {
    pub role: Role,
    /// For `{` and `}`: what they open / close.
    pub brace: Option<BraceKind>,
    /// Index of the matching bracket, or `usize::MAX`.
    pub matching: usize,
    /// The innermost enclosing brace kind *around* this token (for a `{`/`}`: outside it).
    pub scope: Option<BraceKind>,
}

pub(crate) const NO_MATCH: usize = usize::MAX;

const PRIMITIVES: [&str; 8] = ["int", "long", "short", "byte", "char", "float", "double", "boolean"];
const MODIFIERS: [&str; 10] =
    ["public", "private", "protected", "static", "final", "abstract", "default", "synchronized", "native", "strictfp"];
/// Words after which an operator is a prefix one: they end no operand.
const NON_VALUE_WORDS: [&str; 17] = [
    "return", "case", "throw", "else", "new", "instanceof", "assert", "yield", "do", "import", "package",
    "throws", "implements", "extends", "default", "in", "goto",
];
const CONTROL: [&str; 7] = ["if", "for", "while", "switch", "catch", "synchronized", "try"];

pub(crate) fn is_primitive(w: &str) -> bool {
    PRIMITIVES.contains(&w)
}

pub(crate) fn is_modifier(w: &str) -> bool {
    MODIFIERS.contains(&w)
}

pub(crate) fn is_control_word(w: &str) -> bool {
    CONTROL.contains(&w)
}

/// A word that ends an operand (a name, `this`, a literal) — so an operator after it is binary.
fn word_is_value(w: &str) -> bool {
    !NON_VALUE_WORDS.contains(&w)
}

fn starts_upper(w: &str) -> bool {
    w.chars().next().is_some_and(char::is_uppercase)
}

pub(crate) fn classify(toks: &[Tok]) -> Vec<Info> {
    let n = toks.len();
    let mut info: Vec<Info> =
        (0..n).map(|_| Info { role: Role::None, brace: None, matching: NO_MATCH, scope: None }).collect();

    match_brackets(toks, &mut info);
    mark_generics(toks, &mut info);
    mark_casts(toks, &mut info);
    mark_roles_and_braces(toks, &mut info);
    info
}

/// The previous token that is not a comment.
pub(crate) fn prev_sig(toks: &[Tok], i: usize) -> Option<usize> {
    (0..i).rev().find(|&j| !toks[j].is_comment())
}

/// The next token that is not a comment.
pub(crate) fn next_sig(toks: &[Tok], i: usize) -> Option<usize> {
    (i + 1..toks.len()).find(|&j| !toks[j].is_comment())
}

fn match_brackets(toks: &[Tok], info: &mut [Info]) {
    let mut stack: Vec<usize> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => stack.push(i),
            ")" | "]" | "}" => {
                let want = match t.text.as_str() {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                if let Some(&open) = stack.last() {
                    if toks[open].text == want {
                        stack.pop();
                        info[open].matching = i;
                        info[i].matching = open;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Mark the `<` / `>` pairs that are type arguments or type parameters.
fn mark_generics(toks: &[Tok], info: &mut [Info]) {
    const LIMIT: usize = 80;
    for i in 0..toks.len() {
        if !toks[i].is("<") || info[i].role != Role::None {
            continue;
        }
        let Some(p) = prev_sig(toks, i) else { continue };
        let prev = &toks[p];
        let plausible = prev.is(".")
            || (prev.is(">") && info[p].role == Role::GenericClose)
            || (prev.is_word() && (starts_upper(&prev.text) || is_modifier(&prev.text) || prev.text == "new"));
        // `<T> void m()` — a modifier or the start of a member precedes the parameters.
        let at_member_start = prev.is("}") || prev.is(";") || prev.is("{");
        if !plausible && !at_member_start {
            continue;
        }
        let mut depth = 1usize;
        let mut open_stack = vec![i];
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut j = i + 1;
        let mut ok = false;
        while j < toks.len() && j - i < LIMIT {
            let t = &toks[j];
            if t.is_comment() {
                j += 1;
                continue;
            }
            match (t.kind, t.text.as_str()) {
                (Kind::Word, _) => {}
                (Kind::Punct, "." | "," | "?" | "&" | "[" | "]" | "@") => {}
                (Kind::Punct, "<") => {
                    depth += 1;
                    open_stack.push(j);
                }
                (Kind::Punct, ">") => {
                    depth -= 1;
                    if let Some(o) = open_stack.pop() {
                        pairs.push((o, j));
                    }
                    if depth == 0 {
                        ok = true;
                        break;
                    }
                }
                _ => break,
            }
            j += 1;
        }
        if !ok {
            continue;
        }
        // `a < b > 3` — a literal right after the close means it was a comparison.
        if let Some(after) = next_sig(toks, j) {
            if matches!(toks[after].kind, Kind::Number | Kind::Str | Kind::TextBlock) {
                continue;
            }
        }
        for (o, c) in pairs {
            info[o].role = Role::GenericOpen;
            info[c].role = Role::GenericClose;
            info[o].matching = c;
            info[c].matching = o;
        }
    }
}

/// Whether the token before an operator ends an operand.
fn operand_end(toks: &[Tok], info: &[Info], p: usize) -> bool {
    let t = &toks[p];
    match t.kind {
        Kind::Word => word_is_value(&t.text),
        Kind::Number | Kind::Str | Kind::TextBlock => true,
        Kind::Punct => match t.text.as_str() {
            ")" => info[p].role != Role::CastClose,
            "]" => true,
            "++" | "--" => info[p].role == Role::Postfix,
            ">" => false,
            _ => false,
        },
        _ => false,
    }
}

/// `(Type) operand` — mark the pair as a cast.
fn mark_casts(toks: &[Tok], info: &mut [Info]) {
    for i in 0..toks.len() {
        if !toks[i].is("(") || info[i].matching == NO_MATCH {
            continue;
        }
        let close = info[i].matching;
        if close <= i + 1 {
            continue;
        }
        // A call or a grouping after a value is not a cast.
        if let Some(p) = prev_sig(toks, i) {
            let pt = &toks[p];
            let after_value = match pt.kind {
                Kind::Word => word_is_value(&pt.text) && !is_control_word(&pt.text),
                Kind::Number | Kind::Str | Kind::TextBlock => true,
                Kind::Punct => matches!(pt.text.as_str(), "]" | ")") || info[p].role == Role::GenericClose,
                _ => false,
            };
            if after_value {
                continue;
            }
        }
        // The inside must be a bare type: words, dots, generics, arrays, `&` for intersections.
        let mut words = 0usize;
        let mut prev_word = false;
        let mut first_word: Option<&str> = None;
        let mut qualified = false;
        let mut ok = true;
        let mut generic_depth = 0usize;
        for j in i + 1..close {
            let t = &toks[j];
            match (t.kind, t.text.as_str()) {
                (Kind::Word, w) => {
                    if prev_word && generic_depth == 0 {
                        ok = false;
                        break;
                    }
                    words += 1;
                    prev_word = true;
                    first_word.get_or_insert(w);
                }
                (Kind::Punct, ".") => {
                    qualified = true;
                    prev_word = false;
                }
                (Kind::Punct, "[" | "]" | "&" | ",") => prev_word = false,
                (Kind::Punct, "<") if info[j].role == Role::GenericOpen => {
                    generic_depth += 1;
                    prev_word = false;
                }
                (Kind::Punct, ">") if info[j].role == Role::GenericClose => {
                    generic_depth = generic_depth.saturating_sub(1);
                    prev_word = false;
                }
                (Kind::Punct, "?") => prev_word = false,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || words == 0 {
            continue;
        }
        let first = first_word.unwrap_or("");
        let primitive = words == 1 && is_primitive(first);
        if !primitive && !starts_upper(first) && !qualified {
            continue;
        }
        let Some(nx) = next_sig(toks, close) else { continue };
        let nt = &toks[nx];
        let castable = match nt.kind {
            Kind::Word => !matches!(nt.text.as_str(), "instanceof" | "else" | "extends" | "implements" | "throws" | "case"),
            Kind::Number | Kind::Str | Kind::TextBlock => true,
            Kind::Punct => match nt.text.as_str() {
                "(" | "!" | "~" => true,
                "-" | "+" | "++" | "--" => primitive,
                _ => false,
            },
            _ => false,
        };
        if castable {
            info[i].role = Role::CastOpen;
            info[close].role = Role::CastClose;
        }
    }
}

/// One frame per open bracket while walking: ternary `?` pending count, and whether it is a `for (`.
struct Frame {
    questions: usize,
    is_for: bool,
}

fn mark_roles_and_braces(toks: &[Tok], info: &mut [Info]) {
    let mut frames: Vec<Frame> = vec![Frame { questions: 0, is_for: false }];
    let mut scope_stack: Vec<BraceKind> = Vec::new();
    let mut angle_depth = 0usize;

    for i in 0..toks.len() {
        info[i].scope = scope_stack.last().copied();
        let t = &toks[i];
        if t.kind != Kind::Punct {
            continue;
        }
        let prev = prev_sig(toks, i);
        let after_operand = prev.is_some_and(|p| operand_end(toks, info, p));

        match t.text.as_str() {
            "(" | "[" => {
                let is_for = t.text == "(" && prev.is_some_and(|p| toks[p].text == "for" && toks[p].is_word());
                frames.push(Frame { questions: 0, is_for });
            }
            ")" | "]" => {
                if frames.len() > 1 {
                    frames.pop();
                }
            }
            "{" => {
                let kind = brace_kind(toks, info, i);
                info[i].brace = Some(kind);
                frames.push(Frame { questions: 0, is_for: false });
                scope_stack.push(kind);
            }
            "}" => {
                let open = info[i].matching;
                if frames.len() > 1 {
                    frames.pop();
                }
                scope_stack.pop();
                info[i].scope = scope_stack.last().copied();
                info[i].brace = if open != NO_MATCH { info[open].brace } else { None };
            }
            "<" => match info[i].role {
                Role::GenericOpen => angle_depth += 1,
                _ => info[i].role = Role::Binary,
            },
            ">" => match info[i].role {
                Role::GenericClose => angle_depth = angle_depth.saturating_sub(1),
                _ => info[i].role = Role::Binary,
            },
            "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" => info[i].role = Role::Assign,
            "==" | "!=" | "<=" | ">=" | "&&" | "||" | "/" | "%" | "|" | "^" | "<<" | "&" => {
                info[i].role = Role::Binary;
            }
            "+" | "-" => info[i].role = if after_operand { Role::Binary } else { Role::Unary },
            "*" => info[i].role = if after_operand { Role::Binary } else { Role::None },
            "!" | "~" => info[i].role = Role::Unary,
            "++" | "--" => info[i].role = if after_operand { Role::Postfix } else { Role::Unary },
            "->" => info[i].role = Role::Arrow,
            "?" => {
                if angle_depth > 0 {
                    info[i].role = Role::Wildcard;
                } else {
                    info[i].role = Role::TernaryQ;
                    if let Some(f) = frames.last_mut() {
                        f.questions += 1;
                    }
                }
            }
            ":" => {
                let top = frames.last_mut();
                match top {
                    Some(f) if f.questions > 0 => {
                        f.questions -= 1;
                        info[i].role = Role::TernaryColon;
                    }
                    Some(f) if f.is_for => info[i].role = Role::ForEachColon,
                    _ => info[i].role = Role::LabelColon,
                }
            }
            _ => {}
        }
    }
}

/// Scan back from the `{` at `i` to the start of its header: the first token after a `;`, `{` or `}`
/// that is not inside brackets.
fn header_start(toks: &[Tok], i: usize) -> usize {
    let mut depth = 0usize;
    let mut j = i;
    while j > 0 {
        let t = &toks[j - 1];
        if t.kind == Kind::Punct {
            match t.text.as_str() {
                ")" | "]" => depth += 1,
                "(" | "[" => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                ";" | "{" | "}" if depth == 0 => break,
                _ => {}
            }
        }
        j -= 1;
    }
    j
}

fn brace_kind(toks: &[Tok], info: &[Info], i: usize) -> BraceKind {
    let scope = info.get(i).and_then(|x| x.scope);
    let Some(p) = prev_sig(toks, i) else { return BraceKind::Block };
    let pt = &toks[p];

    if pt.is("->") {
        return BraceKind::Lambda;
    }
    if pt.kind == Kind::Punct && matches!(pt.text.as_str(), "=" | "," | "(" | "[") {
        return BraceKind::Array;
    }
    if pt.is("{") && info[p].brace == Some(BraceKind::Array) {
        return BraceKind::Array;
    }
    if pt.is("]") {
        return BraceKind::Array;
    }

    let start = header_start(toks, i);
    // Header words at bracket depth 0, annotations' parentheses skipped.
    let mut depth = 0usize;
    let mut has_new = false;
    let mut type_kw = false;
    let mut has_paren = false;
    let mut first_word: Option<&str> = None;
    let mut words_at_zero = 0usize;
    for j in start..i {
        let t = &toks[j];
        if t.is_comment() {
            continue;
        }
        if t.kind == Kind::Punct {
            match t.text.as_str() {
                "(" | "[" => {
                    if depth == 0 && t.text == "(" {
                        has_paren = true;
                    }
                    depth += 1;
                }
                ")" | "]" => depth = depth.saturating_sub(1),
                _ => {}
            }
            continue;
        }
        if depth == 0 && t.is_word() {
            let w = t.text.as_str();
            // The word after `@` is an annotation name, not a keyword.
            let after_at = j > start && toks[j - 1].is("@");
            if after_at {
                continue;
            }
            if first_word.is_none() && !is_modifier(w) {
                first_word = Some(w);
            }
            words_at_zero += 1;
            match w {
                "new" => has_new = true,
                "class" | "interface" | "enum" => type_kw = true,
                "record" => {
                    if toks.get(j + 1).is_some_and(|n| n.is_word()) {
                        type_kw = true;
                    }
                }
                _ => {}
            }
        }
    }
    if type_kw && !has_new {
        return BraceKind::Type;
    }
    if has_new {
        return BraceKind::Anon;
    }
    match first_word {
        Some("switch") => return BraceKind::Switch,
        Some(w) if is_control_word(w) || matches!(w, "else" | "do" | "finally") => return BraceKind::Block,
        _ => {}
    }
    if pt.is_word() && matches!(pt.text.as_str(), "else" | "try" | "finally" | "do") {
        return BraceKind::Block;
    }
    if has_paren {
        return BraceKind::Method;
    }
    if words_at_zero == 0 || toks[start..i].iter().all(|t| t.is_comment() || (t.is_word() && is_modifier(&t.text))) {
        return match scope {
            Some(BraceKind::Type) | Some(BraceKind::Anon) => BraceKind::Method,
            _ => BraceKind::Block,
        };
    }
    // `A {` in an enum body; anything else word-only is a plain block.
    match scope {
        Some(BraceKind::Type) | Some(BraceKind::Anon) if words_at_zero == 1 => BraceKind::Anon,
        _ => BraceKind::Block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::lex::lex;

    fn roles(src: &str) -> Vec<(String, Role)> {
        let toks = lex(src);
        let info = classify(&toks);
        toks.into_iter().zip(info).map(|(t, i)| (t.text, i.role)).collect()
    }

    fn role_of(src: &str, text: &str) -> Role {
        roles(src).into_iter().find(|(t, _)| t == text).map(|(_, r)| r).unwrap()
    }

    fn brace_kinds(src: &str) -> Vec<BraceKind> {
        let toks = lex(src);
        let info = classify(&toks);
        toks.iter().zip(info).filter(|(t, _)| t.is("{")).filter_map(|(_, i)| i.brace).collect()
    }

    #[test]
    fn generics_are_told_from_comparisons() {
        assert_eq!(role_of("List<String> x;", "<"), Role::GenericOpen);
        assert_eq!(role_of("Map<String, List<Integer>> m;", ">"), Role::GenericClose);
        assert_eq!(role_of("if (a < b && c > d) {}", "<"), Role::Binary);
    }

    #[test]
    fn minus_is_binary_after_an_operand_and_unary_otherwise() {
        assert_eq!(role_of("x = a - b;", "-"), Role::Binary);
        assert_eq!(role_of("x = -b;", "-"), Role::Unary);
        assert_eq!(role_of("return -1;", "-"), Role::Unary);
    }

    #[test]
    fn a_cast_is_recognised_and_a_call_is_not() {
        assert_eq!(role_of("x = (String) o;", "("), Role::CastOpen);
        assert_eq!(role_of("x = (int) -y;", "("), Role::CastOpen);
        assert_eq!(role_of("x = foo(bar);", "("), Role::None);
        assert_eq!(role_of("x = (a) + b;", "("), Role::None);
    }

    #[test]
    fn ternary_and_foreach_and_label_colons() {
        assert_eq!(role_of("x = a ? b : c;", ":"), Role::TernaryColon);
        assert_eq!(role_of("for (String s : list) {}", ":"), Role::ForEachColon);
        assert_eq!(role_of("case 1: break;", ":"), Role::LabelColon);
    }

    #[test]
    fn braces_know_what_they_open() {
        use BraceKind::*;
        assert_eq!(brace_kinds("class A { void m() { if (x) { } } }"), [Type, Method, Block]);
        assert_eq!(brace_kinds("int[] a = {1, 2};"), [Array]);
        assert_eq!(brace_kinds("f(x -> { y(); });"), [Lambda]);
        assert_eq!(brace_kinds("class A { void m() { switch (x) { } } }"), [Type, Method, Switch]);
        assert_eq!(brace_kinds("void m() { new Runnable() { }; }"), [Method, Anon]);
        assert_eq!(brace_kinds("class A { static { } }"), [Type, Method]);
    }
}
