//! What stands between the model and the editor: places where it must stay silent, and a check of the line it
//! wrote.
//!
//! A small model completes *text*, and text is not always what the caret needs. It will happily continue an
//! annotation that is already complete, write after a closing brace, call a method on a type name the user is only
//! declaring a variable of, or invent a class that was never imported. None of that is the model's fault to fix —
//! the host knows the caret's context and the file's names, so the host says no. Pure text in, text out: the
//! host passes what it knows as a closure.

use std::collections::HashSet;

const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do", "double",
    "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
    "interface", "long", "native", "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false",
    "null", "var", "val", "record", "yield",
];

/// Words that take a space before whatever follows them: `if (`, `return x`, `new Foo`.
const SPACE_AFTER: &[&str] = &[
    "if", "for", "while", "switch", "catch", "synchronized", "return", "new", "throw", "else", "case", "instanceof", "extends",
    "implements", "throws", "final", "static", "public", "private", "protected", "abstract", "class", "interface", "enum", "import",
    "package", "void", "var", "val", "int", "long", "double", "float", "boolean", "char", "byte", "short",
];

const MODIFIERS: &[&str] = &["private", "protected", "public", "static", "final", "volatile", "transient"];

/// What the caret's line says about whether the model should speak.
#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    /// Ask the model.
    Model,
    /// Nothing useful can follow on this line.
    Silent,
    /// The line is a type awaiting its variable name: offer the name, never a call.
    Name(String),
}

pub fn gate(line_prefix: &str) -> Gate {
    let whole = line_prefix.trim();
    let t = strip_annotations(whole);
    // a complete annotation on its own, a finished statement, a block opened or closed: what comes next is on the next line
    if t.is_empty() || (paren_depth(t) <= 0 && (t.ends_with(';') || t.ends_with('{') || t.ends_with('}'))) {
        return Gate::Silent;
    }
    match declared_type(t) {
        Some(name) => Gate::Name(match name {
            Some(n) if line_prefix.ends_with(char::is_whitespace) => n,
            Some(n) => format!(" {n}"),
            None => return Gate::Silent,
        }),
        None => Gate::Model,
    }
}

/// Whether a new, empty line continues an unfinished call chain — `Foo.builder()` or `    .name("x")` right above it,
/// no `;` yet — which is where a builder's next `.step(...)` is worth offering before a single character is typed.
pub fn continues_chain(text_before_line: &str) -> bool {
    let Some(prev) = text_before_line.lines().rev().map(str::trim).find(|l| !l.is_empty()) else {
        return false;
    };
    if !prev.ends_with(')') || paren_depth(prev) != 0 || prev.starts_with('@') {
        return false;
    }
    let first = leading_word(prev);
    !["if", "for", "while", "switch", "catch", "synchronized", "else", "try"].contains(&first) && (prev.starts_with('.') || prev.contains('.'))
}

fn paren_depth(t: &str) -> i32 {
    t.bytes().fold(0, |d, b| match b {
        b'(' => d + 1,
        b')' => d - 1,
        _ => d,
    })
}

/// The line without the annotations at its start: `@Autowired private Foo` → `private Foo`; an annotation still
/// open (`@Foo(a = `) is returned whole, because the caret is inside it.
fn strip_annotations(mut t: &str) -> &str {
    while let Some(rest) = t.strip_prefix('@') {
        let name = leading_word(rest);
        let mut after = &rest[name.len()..];
        if after.starts_with('(') {
            let mut depth = 0;
            let close = after.char_indices().find(|(_, c)| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth == 0
            });
            match close {
                Some((i, _)) => after = &after[i + 1..],
                None => return t,
            }
        }
        t = after.trim_start();
    }
    t
}

/// `Some(Some(name))` when the line is only a type (modifiers allowed) — `Order`, `final List<Item>` — with the
/// variable name that type suggests; `Some(None)` for `val`/`var`, which have no type to name after; `None` when the
/// line is anything else.
fn declared_type(t: &str) -> Option<Option<String>> {
    let rest = after_modifiers(t);
    if rest == "val" || rest == "var" {
        return Some(None);
    }
    let (base, args) = type_parts(rest)?;
    let mut name = base.clone();
    let first = base.chars().next()?;
    name.replace_range(..1, &first.to_ascii_lowercase().to_string());
    if args == "[]" || (args.starts_with('<') && matches!(base.as_str(), "List" | "Set" | "Collection" | "Iterable")) {
        let element: String = args.trim_matches(|c| c == '<' || c == '>' || c == '[' || c == ']').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if let Some(e) = element.chars().next().filter(|c| c.is_ascii_uppercase()) {
            name = format!("{}{}s", e.to_ascii_lowercase(), &element[1..]);
        }
    }
    Some(Some(name))
}

/// `t` without the modifiers at its start (`private final List<Item>` → `List<Item>`).
fn after_modifiers(t: &str) -> &str {
    let mut rest = t;
    while let Some((word, tail)) = rest.split_once(char::is_whitespace) {
        if !MODIFIERS.contains(&word) {
            break;
        }
        rest = tail.trim_start();
    }
    rest
}

/// The base name and the generic/array tail of `rest` when it is only a type: a capitalised name that is not all
/// capitals, followed by nothing, `<…>` or `[]`.
fn type_parts(rest: &str) -> Option<(String, &str)> {
    let base: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let args = &rest[base.len()..];
    let generic_or_array = args.is_empty() || (args.starts_with('<') && args.ends_with('>')) || args == "[]";
    let first = base.chars().next()?;
    if !first.is_ascii_uppercase() || !generic_or_array || base.chars().all(|c| !c.is_ascii_lowercase()) {
        return None;
    }
    Some((base, args))
}

/// The type text when the line so far is only modifiers and a type — `Order`, `final List<Item>` → `List<Item>` — the
/// place a variable name is due. `None` for anything else, `val` and `var` included.
pub fn bare_type(line_prefix: &str) -> Option<String> {
    let rest = after_modifiers(strip_annotations(line_prefix.trim()));
    let (base, args) = type_parts(rest)?;
    Some(format!("{base}{args}"))
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn is_keyword(w: &str) -> bool {
    KEYWORDS.contains(&w)
}

fn trailing_word(s: &str) -> &str {
    let start = s.char_indices().rev().take_while(|(_, c)| is_ident(*c)).last().map_or(s.len(), |(i, _)| i);
    &s[start..]
}

/// The word the caret is at the end of — what is being typed, empty when the caret follows a space or a symbol.
pub fn typed_word(line_prefix: &str) -> &str {
    trailing_word(line_prefix)
}

fn leading_word(s: &str) -> &str {
    let end = s.char_indices().find(|(_, c)| !is_ident(*c)).map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// The identifiers of a source text, for the `known` check.
pub fn words_of(source: &str) -> HashSet<&str> {
    let mut out = HashSet::new();
    let mut start = None;
    for (i, c) in source.char_indices() {
        match (is_ident(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.insert(&source[s..i]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.insert(&source[s..]);
    }
    out
}

struct Tok {
    word: String,
    after_dot: bool,
    /// What stands before it: an identifier that is not a keyword (a type), `>`/`]` closing one, or a type keyword.
    after_type: bool,
    next: Option<char>,
    before_arrow: bool,
}

/// `before` is what precedes `text` on the line: the first word of a continuation may follow a dot or a type.
fn tokens(before: &str, text: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let arrow = text.find("->").map(|b| text[..b].chars().count());
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    let before = before.trim_end();
    let mut prev: Option<char> = before.chars().last();
    let mut prev_word: Option<String> = Some(trailing_word(before).to_string()).filter(|w| !w.is_empty());
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
            prev = Some(c);
            prev_word = None;
        } else if c == '-' && chars.get(i + 1) == Some(&'>') {
            i += 2;
            prev = Some('~');
            prev_word = None;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        } else if c.is_ascii_digit() {
            while i < chars.len() && (is_ident(chars[i]) || chars[i] == '.') {
                i += 1;
            }
            prev = Some('0');
            prev_word = None;
        } else if is_ident(c) {
            let start = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let after_type = matches!(prev, Some('>') | Some(']')) || prev_word.as_ref().is_some_and(|w| !is_keyword(w) || matches!(w.as_str(), "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short" | "var" | "val"));
            let next = chars[i..].iter().find(|c| !c.is_whitespace()).copied();
            out.push(Tok { word: word.clone(), after_dot: prev == Some('.'), after_type, next, before_arrow: arrow.is_some_and(|a| start < a) });
            prev = Some('a');
            prev_word = Some(word);
        } else {
            if !c.is_whitespace() {
                prev = Some(c);
                prev_word = None;
            }
            i += 1;
        }
    }
    out
}

/// `String name = ...` offered where an expression goes — an argument, the right side of `=`, an operand: a declaration
/// is a statement. `for (` and `try (` open a statement position of their own.
fn declaration_in_expression(prefix: &str, text: &str) -> bool {
    let before = prefix.trim_end();
    let Some(last) = before.chars().last() else { return false };
    if !"(,=&|?:+*/%".contains(last) || ["for (", "try (", "for(", "try("].iter().any(|k| before.ends_with(k)) {
        return false;
    }
    let t = text.trim_start();
    let t = t.strip_prefix("final ").map_or(t, str::trim_start);
    let first = leading_word(t);
    let mut rest = t[first.len()..].trim_start();
    if first.is_empty() || is_keyword(first) && !matches!(first, "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short" | "var" | "val" | "final") {
        return false;
    }
    if rest.starts_with('<') {
        let Some(close) = rest.find('>') else { return false };
        rest = rest[close + 1..].trim_start();
    }
    let name = leading_word(rest);
    let after = rest[name.len()..].trim_start();
    !name.is_empty() && !is_keyword(name) && after.starts_with('=') && !after.starts_with("==")
}

/// The comma-separated pieces of `text` at the top level, as `(start, end)` byte ranges: commas inside brackets,
/// generics and string literals do not split.
fn top_level_parts(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let (mut parts, mut start, mut depth, mut i) = (Vec::new(), 0usize, 0i32, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'<' if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') => depth += 1,
            b'>' if depth > 0 && i > 0 && b[i - 1] != b'-' => depth -= 1,
            b',' if depth <= 0 => {
                parts.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push((start, b.len()));
    parts
}

/// The name a parameter declaration ends with (`@A("x") String name` → `name`), `None` for anything that is not
/// `Type name`.
fn declared_param_name(part: &str) -> Option<&str> {
    // the closing `)` of the list is not part of the last parameter
    let t = part.trim().trim_end_matches([')', ';']).trim_end();
    let name = trailing_word(t);
    let before = t[..t.len() - name.len()].trim_end();
    (!name.is_empty() && !before.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && !t.contains(['=', '.'].as_ref()) || t.starts_with('@') && !name.is_empty() && !before.is_empty()).then_some(name)
}

/// The names written as `name =` in `text` (an annotation's elements), skipping `==`, `!=`, `<=`, `>=`.
fn element_names(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for (i, _) in text.match_indices('=') {
        let next = b.get(i + 1).copied();
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        if next == Some(b'=') || matches!(prev, b'=' | b'!' | b'<' | b'>') {
            continue;
        }
        let name = trailing_word(text[..i].trim_end());
        if !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) {
            out.push(name);
        }
    }
    out
}

/// An annotation element written twice (`produces = X, produces = X`): the suggestion is cut before the element the
/// line already has, or one it has already written itself.
fn cut_repeated_element(prefix: &str, text: &str) -> String {
    if !prefix.trim_start().starts_with('@') {
        return text.to_string();
    }
    let mut names: Vec<&str> = element_names(prefix.trim_start());
    let base = usize::from(text.starts_with('('));
    for (from, to) in top_level_parts(&text[base..]).into_iter().map(|(f, t)| (f + base, t + base)) {
        let piece = &text[from..to];
        let mut found = element_names(piece);
        found.truncate(1);
        if let Some(name) = found.first().copied().filter(|n| piece.trim_start().starts_with(n)) {
            if names.contains(&name) {
                return text[..from.saturating_sub(1)].trim_end().to_string();
            }
            names.push(name);
        }
    }
    text.to_string()
}

/// The suggestion up to its first `)` that closes nothing: with `open` parentheses still open at the caret, a `)` past
/// them (`.build()` + `).get(…)`) is the model closing something that is not there. Strings and char literals are skipped.
pub fn cut_unbalanced(text: &str, open: i32) -> String {
    let b = text.as_bytes();
    let (mut depth, mut i) = (open, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => depth += 1,
            b')' => {
                if depth <= 0 {
                    return text[..i].trim_end().to_string();
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    text.to_string()
}

/// Where `text` chains a member onto a `)` that closes a parenthesis opened *before* the suggestion
/// (`foo(Bar.builder().a(1)` + `).get()`): the receiver is the result of the outer call, which a caret deep in
/// that call has not got typed, so the member is the model guessing. Byte offset of each such `.`.
pub fn dots_after_prefix_close(text: &str) -> Vec<usize> {
    let b = text.as_bytes();
    let (mut depth, mut i, mut found) = (0i32, 0usize, Vec::new());
    let mut closed_prefix_at: Option<usize> = None;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                closed_prefix_at = None;
            }
            b'(' => {
                depth += 1;
                closed_prefix_at = None;
            }
            b')' => {
                depth -= 1;
                closed_prefix_at = (depth < 0).then_some(i);
            }
            b'.' if closed_prefix_at.is_some() && b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') => {
                found.push(i);
                closed_prefix_at = None;
            }
            c if c.is_ascii_whitespace() => {}
            _ => closed_prefix_at = None,
        }
        i += 1;
    }
    found
}

/// A model that loops writes the same piece again and again (`String a, String a, String a, …`). The text is cut
/// before the first piece that repeats an earlier one, or that declares a name an earlier parameter already took.
fn cut_repetition(text: &str) -> String {
    // a list that opens right here (`(String a, String a`) is split inside its own parenthesis
    let base = usize::from(text.starts_with('('));
    let parts: Vec<(usize, usize)> = top_level_parts(&text[base..]).into_iter().map(|(f, t)| (f + base, t + base)).collect();
    let mut seen: Vec<&str> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for &(from, to) in &parts {
        let piece = text[from..to].trim();
        let name = declared_param_name(piece);
        let again = (piece.len() >= 8 && seen.contains(&piece)) || name.is_some_and(|n| names.contains(&n));
        if again {
            return text[..from.saturating_sub(1)].trim_end().to_string();
        }
        seen.push(piece);
        names.extend(name);
    }
    text.to_string()
}

/// Shortest and longest repeating unit, in characters, that counts as a loop. Under six it is punctuation and short numbers
/// (`0, 0, 0`, `))))`), which real code repeats; over eighty the line is not a line any more.
const LOOP_UNIT: std::ops::RangeInclusive<usize> = 6..=80;

/// Where to cut a loop that the first copy of ends at `at`: the copy may begin mid-expression, so the cut backs up to the
/// last place a piece of code is whole — after a closing bracket, a quote or a `;` — and only then to the end of a word.
fn loop_end(c: &[char], at: usize) -> usize {
    let whole = |k: usize| k > 0 && matches!(c[k - 1], ')' | ']' | '}' | '"' | '\'' | ';');
    (1..=at).rev().find(|&k| whole(k)).or_else(|| (1..=at).rev().find(|&k| c[k - 1].is_alphanumeric() && c.get(k).is_none_or(|n| !n.is_alphanumeric()))).unwrap_or(at)
}

/// A model that loops, in the general case: some stretch of the text written three times in a row
/// (`; rel="2023"; rel="2023"; rel="2023"`, ` || i <= 'Z' || i <= 'Z' || i <= 'Z'`, ` + " " + str + " " + str + " " + str`).
/// The text is cut after the first copy: that one is what the model meant, the rest is it failing to stop.
pub fn cut_loop(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    // earliest loop wins; for that start the shortest unit, so `ab` is not read as `abab`
    for start in 0..n {
        for unit in LOOP_UNIT {
            if start + 3 * unit > n {
                break;
            }
            if c[start..start + unit] == c[start + unit..start + 2 * unit] && c[start..start + unit] == c[start + 2 * unit..start + 3 * unit] {
                // a unit that is only blanks and punctuation is a rule or a row of dashes, not a loop of words
                if c[start..start + unit].iter().any(|ch| ch.is_alphanumeric()) {
                    return c[..loop_end(&c, start + unit)].iter().collect::<String>().trim_end().to_string();
                }
            }
        }
    }
    text.to_string()
}

/// A ternary the model wrote out of nothing: `cond ? a : b` as the continuation of a call chain.
fn is_ternary(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with('?') || (t.contains(" ? ") && t.contains(" : "))
}

fn wants_space(prefix: &str, last: char, first: char) -> bool {
    let word_like = is_ident(first) || first == '"' || first == '\'';
    if is_ident(last) {
        // `if(`, `return"x"`, `else{`: a keyword glued to what follows (a word after a word is the caller's case)
        return SPACE_AFTER.contains(&trailing_word(prefix)) && (word_like || first == '(' || first == '{');
    }
    match last {
        '=' | ',' | ':' | '?' | '&' | '|' | '*' | '/' | '%' | ';' => word_like || first == '(',
        '>' | ')' => word_like,
        _ => false,
    }
}

/// The model's line, made fit to show — or `None` when it must not be shown.
///
/// `known(word)` says whether the file or the project has that name: a local, a field, a method, an imported type.
/// A line that names something nobody has is the model inventing, and an invented name is worse than no ghost text.
pub fn vet(prefix: &str, text: &str, known: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut text = cut_repeated_element(prefix, &cut_repetition(&cut_loop(text.trim_end())));
    if text.trim().is_empty() || is_ternary(&text) || declaration_in_expression(prefix, &text) {
        return None;
    }
    let last = prefix.chars().last()?;
    let first = text.chars().next()?;
    let mut joined = false;
    if is_ident(last) && is_ident(first) {
        let tail = trailing_word(prefix);
        let combined = format!("{tail}{}", leading_word(&text));
        let member = prefix[..prefix.len() - tail.len()].trim_end().ends_with('.');
        if member || is_keyword(&combined) || known(&combined) {
            joined = true;
        } else if is_keyword(tail) || known(tail) {
            text.insert(0, ' ');
        } else {
            return None;
        }
    } else if wants_space(prefix, last, first) {
        text.insert(0, ' ');
    }
    // names the line declares itself — a local, a lambda parameter — are not inventions, here or later in the line
    let mut declared: HashSet<String> = HashSet::new();
    for (i, t) in tokens(prefix, &text).into_iter().enumerate() {
        let declares = (t.after_type && matches!(t.next, None | Some('=') | Some(';') | Some(':') | Some(',') | Some(')')))
            || (t.before_arrow && t.next != Some('(') && t.word.starts_with(|c: char| c.is_ascii_lowercase()));
        if declares && !t.after_dot && !is_keyword(&t.word) {
            declared.insert(t.word.clone());
            continue;
        }
        let skip = (i == 0 && joined) || t.after_dot || is_keyword(&t.word) || declared.contains(&t.word);
        if !skip && !known(&t.word) {
            return None;
        }
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known_in(source: &'static str) -> impl Fn(&str) -> bool {
        let words = words_of(source);
        move |w| words.contains(w)
    }

    #[test]
    fn a_type_awaiting_its_variable_gets_a_name_not_a_call() {
        assert_eq!(gate("        Order"), Gate::Name(" order".into()));
        assert_eq!(gate("        final Order "), Gate::Name("order".into()));
        assert_eq!(gate("    private List<Item>"), Gate::Name(" items".into()));
        assert_eq!(gate("        val"), Gate::Silent);
        assert_eq!(gate("        var "), Gate::Silent);
        assert_eq!(gate("        return Order"), Gate::Model);
        assert_eq!(gate("        order.get"), Gate::Model);
    }

    #[test]
    fn the_type_of_a_line_that_is_only_a_type() {
        assert_eq!(bare_type("        Order").as_deref(), Some("Order"));
        assert_eq!(bare_type("    private final List<Item> ").as_deref(), Some("List<Item>"));
        assert_eq!(bare_type("    @Autowired private OrderService").as_deref(), Some("OrderService"));
        assert_eq!(bare_type("        Item[]").as_deref(), Some("Item[]"));
        for line in ["        val", "        return Order", "        order.get", "        MAX_SIZE", "        Order order"] {
            assert_eq!(bare_type(line), None, "{line}");
        }
    }

    #[test]
    fn nothing_is_said_after_a_finished_annotation_statement_or_block() {
        for line in ["@Slf4j", "    @PostMapping", "    @PostMapping(\"/x\")", "        foo();", "    void f() {", "    }", "        });"] {
            assert_eq!(gate(line), Gate::Silent, "{line}");
        }
        assert_eq!(gate("    @PostMapping(produces = "), Gate::Model);
        assert_eq!(gate("        for (int i = 0; "), Gate::Model);
    }

    #[test]
    fn an_empty_line_below_an_open_chain_is_worth_a_suggestion() {
        assert!(continues_chain("        Order o = Order.builder()\n            .id(1)\n"));
        assert!(continues_chain("    return Foo.builder()\n"));
        assert!(!continues_chain("        foo();\n"));
        assert!(!continues_chain("        if (x.isEmpty())\n"));
        assert!(!continues_chain("    @Override\n"));
        assert!(!continues_chain("    void f() {\n"));
        assert!(!continues_chain(""));
    }

    #[test]
    fn a_declaration_is_not_an_argument() {
        let k = |_: &str| true;
        assert_eq!(vet("        foo(", "String name = bar();", &k), None);
        assert_eq!(vet("        foo(a, ", "final int n = 3;", &k), None);
        assert_eq!(vet("        for (", "int i = 0; i < n; i++) {", &k).as_deref(), Some("int i = 0; i < n; i++) {"));
        assert_eq!(vet("        foo(", "name == other)", &k).as_deref(), Some("name == other)"));
    }

    #[test]
    fn a_looping_parameter_list_is_cut_before_its_second_copy() {
        let looped = "(@RequestParam(\"something\") String something, @RequestParam(\"something\") String something, @RequestParam(\"something\") String something";
        assert_eq!(cut_repetition(looped), "(@RequestParam(\"something\") String something");
        // two different parameters stay, a repeated name does not
        assert_eq!(cut_repetition("(String a, int b)"), "(String a, int b)");
        assert_eq!(cut_repetition("(String a, int a)"), "(String a");
        // a call may pass the same value twice
        assert_eq!(cut_repetition("assertEquals(expected, expected)"), "assertEquals(expected, expected)");
        // commas inside generics and strings do not split
        assert_eq!(cut_repetition("(Map<String, Integer> m, List<String> l)"), "(Map<String, Integer> m, List<String> l)");
    }

    #[test]
    fn an_annotation_element_is_not_written_twice() {
        let line = "    @PostMapping(value = \"/something\", produces = MediaType.APPLICATION_JSON_VALUE";
        assert_eq!(cut_repeated_element(line, ", produces = MediaType.APPLICATION_JSON_VALUE)"), "");
        // a new element is fine
        assert_eq!(cut_repeated_element(line, ", consumes = MediaType.APPLICATION_JSON_VALUE)"), ", consumes = MediaType.APPLICATION_JSON_VALUE)");
        // an element the suggestion writes twice by itself
        assert_eq!(cut_repeated_element("    @GetMapping(value = \"/x\"", ", produces = A, produces = A)"), ", produces = A");
        // not an annotation: nothing to do
        assert_eq!(cut_repeated_element("    foo(a = 1", ", a = 2)"), ", a = 2)");
    }

    #[test]
    fn a_closer_that_closes_nothing_ends_the_suggestion() {
        assert_eq!(dots_after_prefix_close(").get()"), vec![1]);
        assert_eq!(dots_after_prefix_close(".x(1)).get()"), vec![6]);
        assert!(dots_after_prefix_close(".x(1).get()").is_empty(), "closes only what it opened");
        assert!(dots_after_prefix_close("\")).get\"").is_empty());
        assert_eq!(cut_unbalanced(").get(x)", 0), "");
        assert_eq!(cut_unbalanced(".build()).get()", 0), ".build()");
        assert_eq!(cut_unbalanced("x).get()", 1), "x).get()");
        assert_eq!(cut_unbalanced("foo(\")\", a)", 0), "foo(\")\", a)");
        assert_eq!(cut_unbalanced("foo(a, b);", 0), "foo(a, b);");
    }

    #[test]
    fn a_ternary_is_dropped() {
        let k = |_: &str| true;
        assert_eq!(vet("x.filter(a)", " ? a : b", &k), None);
        assert_eq!(vet("int r =", " c ? 1 : 2;", &k), None);
    }

    #[test]
    fn a_missing_space_is_added_and_a_word_continued_is_not() {
        let k = known_in("class A { int count; int size() { return count; } }");
        assert_eq!(vet("        int n =", "count;", &k).as_deref(), Some(" count;"));
        assert_eq!(vet("        return", "count;", &k).as_deref(), Some(" count;"));
        assert_eq!(vet("        ret", "urn count;", &k).as_deref(), Some("urn count;"));
        assert_eq!(vet("        if", "(count > 0) {", &k).as_deref(), Some(" (count > 0) {"));
        assert_eq!(vet("        new ArrayList<>", "();", &|w| w == "ArrayList").as_deref(), Some("();"));
    }

    #[test]
    fn a_name_nobody_has_is_dropped() {
        let k = known_in("import org.springframework.http.ResponseEntity; class A { ResponseEntity<String> go() { return null; } }");
        assert_eq!(vet("        return ResponseEntity.", "ok(\"x\");", &k).as_deref(), Some("ok(\"x\");"));
        assert_eq!(vet("        return", "ResponseEntity.status(HttpStatus.UNAUTHORIZED).build();", &k), None);
        assert_eq!(vet("        int n =", "invented(3);", &k), None);
        // a continuation that is no word of the file, after a word that is
        assert_eq!(vet("    @Slf4j", "ysic(1)", &known_in("@Slf4j class A {}")), None);
    }

    #[test]
    fn new_local_names_and_lambda_parameters_are_not_inventions() {
        let k = known_in("class A { List<Order> orders; Order find() { return null; } }");
        assert_eq!(vet("        ", "Order found = find();", &k).as_deref(), Some("Order found = find();"));
        assert_eq!(vet("        orders.forEach(", "o -> o.touch());", &k).as_deref(), Some("o -> o.touch());"));
        assert_eq!(vet("        for (", "Order o : orders) {", &k).as_deref(), Some("Order o : orders) {"));
    }

    #[test]
    fn a_stretch_written_three_times_is_cut_after_its_first_copy() {
        assert_eq!(
            cut_loop("\"\\\"\" + sunsetDate + \">; rel=\\\"2023\\\"; rel=\\\"2023\\\"; rel=\\\"2023\\\"; rel"),
            "\"\\\"\" + sunsetDate + \">; rel=\\\"2023\\\""
        );
        assert_eq!(cut_loop("<= 'Z' || i <= 'Z' || i <= 'Z' || i <= 'Z' || i"), "<= 'Z'");
        // the copy may begin anywhere in the unit; the cut lands where a piece is whole, here after the first `" "`
        assert_eq!(cut_loop("str, delimiters) + \" \" + str + \" \" + str + \" \" + str + \" \""), "str, delimiters) + \" \"");
    }

    #[test]
    fn real_repetition_is_left_alone() {
        for ok in ["0, 0, 0, 0, 0, 0", "foo(a, b) + bar(c, d)", "))))));", "-----------------------", "a.b().c().d().e()", "\"x\" + \"y\" + \"z\""] {
            assert_eq!(cut_loop(ok), ok);
        }
        // twice is a pattern in code, three times is the model not stopping
        assert_eq!(cut_loop("rel=\"2023\"; rel=\"2023\""), "rel=\"2023\"; rel=\"2023\"");
    }

    /// A probe, not a test: `FLC_LOOP_PROBE=<dump.tsv> cargo test -p bennu-flc --lib probe_loops -- --ignored --nocapture`
    /// on a `FLC_DUMP` of the lab's evaluator (suggestion in column 6, expected text in column 7). Numbers only.
    #[test]
    #[ignore]
    fn probe_loops() {
        let Ok(path) = std::env::var("FLC_LOOP_PROBE") else { return };
        let share = |got: &str, want: &str| {
            let total = want.chars().count().max(1);
            got.chars().zip(want.chars()).take_while(|(a, b)| a == b).count() as f32 / total as f32
        };
        let (mut n, mut looped, mut before, mut after, mut hit_before, mut hit_after) = (0usize, 0usize, 0f32, 0f32, 0usize, 0usize);
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 7 { continue }
            n += 1;
            let cut = cut_loop(c[5]);
            if cut != c[5] {
                looped += 1;
                before += share(c[5], c[6]);
                after += share(&cut, c[6]);
                hit_before += usize::from(c[5] == c[6]);
                hit_after += usize::from(cut == c[6]);
            }
        }
        println!("suggestions {n}, with a loop {looped} ({:.1}%); on those: exact {hit_before} -> {hit_after}, prefix share {:.2} -> {:.2}",
            100.0 * looped as f32 / n.max(1) as f32, before / looped.max(1) as f32, after / looped.max(1) as f32);
    }
}
