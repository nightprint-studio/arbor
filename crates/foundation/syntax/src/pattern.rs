//! Structural search and replace: a pattern is **source text with holes in it**.
//!
//! ## Why the pattern is written in the target language
//!
//! There is no second syntax to learn. `INSERT INTO $t$ ($cols...$) VALUES
//! ($vals...$)` is SQL that happens to have placeholders, and somebody who can
//! write the statement can write the pattern for it. It is parsed **with the same
//! grammar as the subject**, which is what makes the match structural: a pattern
//! matches across line breaks, extra whitespace and interleaved comments, because
//! none of those are nodes it compares.
//!
//! ## The two kinds of hole
//!
//! * `$name$` matches **one** node and captures it.
//! * `$name...$` matches **any run of consecutive siblings**, including none, and
//!   captures the source from the first to the last — separators and all. That is
//!   why a captured column list comes back as `COD, VAL` and not as a list this
//!   crate had to guess how to re-join: the original bytes are the answer, and
//!   they are already correct.
//!
//! ## Where a hole can go
//!
//! A placeholder is substituted with an ordinary identifier before the pattern is
//! parsed, so it may sit anywhere an identifier is legal. That covers every
//! position these transformations actually need — table names, column names,
//! values, arguments — and it is a real limit worth stating plainly: `$x$` cannot
//! stand for a whole statement, because no grammar accepts an identifier there.
//!
//! ## Fragments
//!
//! A pattern is usually not a whole file. [`Pattern::compile`] parses the text on
//! its own, which is right for a language whose fragments are top-level constructs
//! (an SQL statement is). [`Pattern::compile_in`] takes a prefix and a suffix to
//! parse the fragment inside — `class C { void m() { … } }` — for the languages
//! where it is not.

use std::collections::HashMap;

use tree_sitter::{Language, Node, Parser, Tree};

use crate::error::SyntaxError;
use crate::range::ByteRange;

/// What a placeholder in the pattern stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// `$name$` — exactly one node.
    One,
    /// `$name...$` — a run of consecutive siblings, possibly empty.
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placeholder {
    name: String,
    arity: Arity,
    /// The identifier it was replaced with so the pattern would parse.
    token: String,
    /// Every text the hole was written as — the bare token, plus each wrapped form (`TOKEN;`, `@TOKEN`,
    /// `int TOKEN`…) it took where a bare name would not parse. A pattern node whose text is one of these
    /// **is** the hole.
    texts: Vec<String>,
}

/// How a hole is written into the pattern text where a bare identifier would not parse.
///
/// A placeholder is an identifier, and an identifier is not a statement, a member, a parameter or a
/// modifier. Each field is a template with `{}` for the identifier, applied to the holes in that
/// **position** (see [`Position`]); `"{}"` is the bare identifier, which is what a pattern always used.
///
/// The caller tries several combinations and keeps the first that parses, so a pattern is never written
/// differently from the way it was before — a form only matters where the bare one failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoleForms {
    /// A hole standing alone on a statement or member position (`{ $body...$ }`).
    pub block: &'static str,
    /// A hole standing alone between `(`/`,` and `)`/`,` (`m($params...$)`).
    pub item: &'static str,
    /// A run hole in front of a declaration (`$mods...$ final int x`).
    pub mods: &'static str,
}

impl HoleForms {
    /// Every hole is the bare identifier — the behaviour before forms existed.
    pub const PLAIN: HoleForms = HoleForms { block: "{}", item: "{}", mods: "{}" };
}

/// Where a hole sits in the pattern text, decided from the characters around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Plain,
    Block,
    Item,
    Mods,
}

/// One thing a pattern matched, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub name: String,
    pub arity: Arity,
    /// The subject's bytes. Empty for a `...` placeholder that matched nothing.
    pub range: ByteRange,
    /// The ranges of the individual siblings, so a template can address them by
    /// index. One entry for [`Arity::One`].
    pub parts: Vec<ByteRange>,
}

/// A pattern matched against one place in the subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The whole matched node — what a replacement replaces.
    pub range: ByteRange,
    pub captures: Vec<Capture>,
}

impl Match {
    pub fn capture(&self, name: &str) -> Option<&Capture> {
        self.captures.iter().find(|c| c.name == name)
    }
}

/// A compiled pattern.
#[derive(Debug)]
pub struct Pattern {
    /// The pattern text with its placeholders substituted — what was parsed, and
    /// what every pattern-side range indexes into.
    text: String,
    tree: Tree,
    /// Where the meaningful part starts: the smallest node covering the fragment,
    /// skipping whatever wrapper it had to be parsed inside.
    root: ByteRange,
    placeholders: Vec<Placeholder>,
    by_token: HashMap<String, usize>,
    /// Compare leaf text without regard to case. On for SQL, where `INSERT` and
    /// `insert` are the same keyword; off for Java, where `x` and `X` are not the
    /// same name. A caller's decision, never a guess from the grammar.
    case_insensitive: bool,
}

impl Pattern {
    /// Compile a pattern that is a valid construct of the language on its own.
    pub fn compile(language: &Language, pattern: &str) -> Result<Pattern, SyntaxError> {
        Self::compile_in(language, pattern, "", "")
    }

    /// Compile a fragment, parsed between `prefix` and `suffix`.
    ///
    /// The wrapper exists only to make the grammar accept the fragment; nothing
    /// from it takes part in the match, and no range ever escapes pointing into
    /// it.
    pub fn compile_in(
        language: &Language,
        pattern: &str,
        prefix: &str,
        suffix: &str,
    ) -> Result<Pattern, SyntaxError> {
        Self::compile_in_with(language, pattern, prefix, suffix, &HoleForms::PLAIN)
    }

    /// [`Pattern::compile_in`], with holes in statement / member / parameter / modifier position written
    /// as `forms` says instead of as a bare identifier.
    pub fn compile_in_with(
        language: &Language,
        pattern: &str,
        prefix: &str,
        suffix: &str,
        forms: &HoleForms,
    ) -> Result<Pattern, SyntaxError> {
        let (body, placeholders) = substitute_with(pattern, forms)?;
        let text = format!("{prefix}{body}{suffix}");
        let fragment = ByteRange::new(prefix.len(), prefix.len() + body.len());

        let mut parser = Parser::new();
        parser
            .set_language(language)
            .map_err(|e| SyntaxError::Language(e.to_string()))?;
        let tree = parser
            .parse(&text, None)
            .ok_or_else(|| SyntaxError::Language("the parser produced no tree".to_string()))?;

        if tree.root_node().has_error() {
            return Err(SyntaxError::Pattern {
                reason: "this pattern is not valid in the file's language — a placeholder \
                         stands in for a name, so it can go anywhere a name can, but not \
                         where a whole statement is expected"
                    .to_string(),
                at: first_error(tree.root_node()).map(|n| {
                    // Reported against the **pattern the user typed**, not the
                    // wrapped text they never saw.
                    ByteRange::new(
                        n.start_byte().saturating_sub(prefix.len()),
                        n.end_byte().saturating_sub(prefix.len()),
                    )
                }),
            });
        }

        let root = smallest_covering(tree.root_node(), fragment);
        let by_token = placeholders
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.texts.iter().map(move |t| (t.clone(), i)))
            .collect();
        Ok(Pattern { text, tree, root, placeholders, by_token, case_insensitive: false })
    }

    /// Compare leaf text case-insensitively — for the languages where case in a
    /// keyword or a name carries no meaning.
    pub fn case_insensitive(mut self, yes: bool) -> Self {
        self.case_insensitive = yes;
        self
    }

    /// The placeholder names, in the order they appear. What a template editor
    /// offers for completion.
    pub fn names(&self) -> Vec<&str> {
        self.placeholders.iter().map(|p| p.name.as_str()).collect()
    }

    /// Every place in `source` this pattern matches, outermost first and never
    /// overlapping.
    ///
    /// Nested matches are deliberately not reported: a replacement rewrites the
    /// matched range whole, so an inner match inside an outer one is an edit
    /// inside an edit, and the second would be applied to text that no longer
    /// exists.
    pub fn find_all(&self, language: &Language, source: &str) -> Result<Vec<Match>, SyntaxError> {
        let mut parser = Parser::new();
        parser
            .set_language(language)
            .map_err(|e| SyntaxError::Language(e.to_string()))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| SyntaxError::Language("the parser produced no tree".to_string()))?;

        let pattern_root = node_at(&self.tree, self.root).ok_or_else(|| {
            SyntaxError::Pattern { reason: "the pattern has no body".to_string(), at: None }
        })?;

        let mut found = vec![];
        self.search(pattern_root, tree.root_node(), source, &mut found);
        Ok(found)
    }

    fn search(&self, pattern: Node<'_>, subject: Node<'_>, source: &str, out: &mut Vec<Match>) {
        let mut captures = vec![];
        if self.node_matches(pattern, subject, source, &mut captures) {
            out.push(Match {
                range: ByteRange::new(subject.start_byte(), subject.end_byte()),
                captures,
            });
            // Not descending: see `find_all`.
            return;
        }
        let mut cursor = subject.walk();
        if !cursor.goto_first_child() {
            return;
        }
        loop {
            self.search(pattern, cursor.node(), source, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Does this pattern node describe this subject node?
    fn node_matches(
        &self,
        pattern: Node<'_>,
        subject: Node<'_>,
        source: &str,
        captures: &mut Vec<Capture>,
    ) -> bool {
        if let Some(index) = self.placeholder_of(pattern) {
            let placeholder = &self.placeholders[index];
            if placeholder.arity == Arity::Many {
                // A `...` in a position where exactly one node sits is simply a
                // run of one. Rejecting it would make `$x...$` unusable anywhere
                // a list happens to have a single element.
                capture_run(placeholder, &[subject], captures);
            } else {
                capture_run(placeholder, &[subject], captures);
            }
            return true;
        }

        if pattern.kind() != subject.kind() {
            return false;
        }

        let pattern_children = significant_children(pattern, &self.text);
        let subject_children = significant_children(subject, source);

        if pattern_children.is_empty() {
            // A leaf: the kinds agreeing is not enough. `T` and `U` are both
            // `identifier`, and a matcher that stopped here would rewrite every
            // table in the repository when asked about one.
            return self.leaf_text_matches(pattern, subject, source);
        }

        self.children_match(&pattern_children, &subject_children, source, captures)
    }

    fn leaf_text_matches(&self, pattern: Node<'_>, subject: Node<'_>, source: &str) -> bool {
        let p = &self.text[pattern.start_byte()..pattern.end_byte()];
        let Some(s) = source.get(subject.start_byte()..subject.end_byte()) else {
            return false;
        };
        if self.case_insensitive {
            p.eq_ignore_ascii_case(s)
        } else {
            p == s
        }
    }

    /// Match a pattern's children against a subject's, letting `...` placeholders
    /// swallow runs. Backtracks, because a pattern may hold more than one of them.
    fn children_match(
        &self,
        pattern: &[Node<'_>],
        subject: &[Node<'_>],
        source: &str,
        captures: &mut Vec<Capture>,
    ) -> bool {
        let Some((head, rest)) = pattern.split_first() else {
            return subject.is_empty();
        };

        if let Some(index) = self.placeholder_of(*head) {
            let placeholder = &self.placeholders[index];
            if placeholder.arity == Arity::Many {
                // Greedy, then give bytes back. Longest-first matters: `f($a...$)`
                // against `f(1, 2)` should capture `1, 2` rather than `1`, and only
                // a shorter run should be tried when the tail refuses to fit.
                for take in (0..=subject.len()).rev() {
                    let mut attempt = captures.clone();
                    capture_run(placeholder, &subject[..take], &mut attempt);
                    if self.children_match(rest, &subject[take..], source, &mut attempt) {
                        *captures = attempt;
                        return true;
                    }
                }
                return false;
            }
            let Some((first, tail)) = subject.split_first() else { return false };
            let mut attempt = captures.clone();
            capture_run(placeholder, &[*first], &mut attempt);
            if self.children_match(rest, tail, source, &mut attempt) {
                *captures = attempt;
                return true;
            }
            return false;
        }

        let Some((first, tail)) = subject.split_first() else { return false };
        let mut attempt = captures.clone();
        if !self.node_matches(*head, *first, source, &mut attempt) {
            return false;
        }
        if self.children_match(rest, tail, source, &mut attempt) {
            *captures = attempt;
            return true;
        }
        false
    }

    /// Is this pattern node nothing but a placeholder's stand-in token?
    ///
    /// By **text**, not by kind or position: the grammar may wrap the identifier
    /// in one or more nodes on the way down (`expression` → `primary` →
    /// `identifier`), and the placeholder is whichever of those the subject is
    /// being compared against at the time.
    fn placeholder_of(&self, node: Node<'_>) -> Option<usize> {
        let text = self.text.get(node.start_byte()..node.end_byte())?;
        self.by_token.get(text).copied()
    }
}

fn capture_run(placeholder: &Placeholder, nodes: &[Node<'_>], captures: &mut Vec<Capture>) {
    let parts: Vec<ByteRange> =
        nodes.iter().map(|n| ByteRange::new(n.start_byte(), n.end_byte())).collect();
    let range = match (parts.first(), parts.last()) {
        (Some(first), Some(last)) => ByteRange::new(first.start, last.end),
        // Matched nothing. An empty range rather than an absent capture: the
        // template still names it, and "matched nothing" renders as nothing.
        _ => ByteRange::new(0, 0),
    };
    captures.push(Capture {
        name: placeholder.name.clone(),
        arity: placeholder.arity,
        range,
        parts,
    });
}

/// The children that take part in a match: everything the grammar produced except
/// its **extras** — comments and whitespace. Skipping those is what makes a
/// pattern survive a comment somebody left in the middle of a statement.
fn significant_children<'a>(node: Node<'a>, text: &str) -> Vec<Node<'a>> {
    let mut out = vec![];
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return out;
    }
    loop {
        let child = cursor.node();
        if !child.is_extra() && !is_layout(child, text) {
            out.push(child);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    out
}

/// An anonymous leaf that is nothing but whitespace.
///
/// Most grammars declare whitespace as an `extra`, so it never reaches the tree
/// and this never fires. A few do not: tree-sitter-html and the JSP grammar
/// modelled on it make the space between a tag name and its attributes an
/// explicit token, because that is what keeps `<a href` and `<ahref` apart.
///
/// Either way it is **layout**, and a matcher that counted it would make
/// `<s:property value="$x$"/>` miss the same tag written across three lines —
/// which is the one thing this whole module exists to do. Restricted to
/// *anonymous* leaves so a named node that happens to hold only spaces (a run of
/// page text) stays a node: it is content that is currently blank, not absent
/// punctuation.
fn is_layout(node: Node<'_>, text: &str) -> bool {
    !node.is_named()
        && node.child_count() == 0
        && text.get(node.start_byte()..node.end_byte()).is_some_and(|t| t.trim().is_empty())
}

fn first_error(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        if let Some(found) = first_error(cursor.node()) {
            return Some(found);
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

/// The smallest node covering `range` — how a fragment is found inside the
/// wrapper it had to be parsed in.
fn smallest_covering(root: Node<'_>, range: ByteRange) -> ByteRange {
    let mut best = ByteRange::new(root.start_byte(), root.end_byte());
    let mut node = root;
    loop {
        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return best;
        }
        let mut descended = false;
        loop {
            let child = cursor.node();
            let child_range = ByteRange::new(child.start_byte(), child.end_byte());
            if child_range.contains(&range) {
                best = child_range;
                node = child;
                descended = true;
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        if !descended {
            return best;
        }
    }
}

fn node_at(tree: &Tree, range: ByteRange) -> Option<Node<'_>> {
    tree.root_node().descendant_for_byte_range(range.start, range.end)
}

/// The identifier a placeholder is replaced with so the pattern will parse.
///
/// Deliberately ugly and deliberately an ordinary identifier: it has to be legal
/// in every grammar this crate is pointed at, and it has to be something nobody
/// writes by accident.
fn token_for(index: usize) -> String {
    format!("ARBOR_PLACEHOLDER_{index}")
}

/// `$name$` and `$name...$` out, identifiers in.
///
/// `$$` is a literal `$`, which is the escape a pattern needs the moment somebody
/// writes a PostgreSQL dollar-quoted body.
fn substitute(pattern: &str) -> Result<(String, Vec<Placeholder>), SyntaxError> {
    substitute_with(pattern, &HoleForms::PLAIN)
}

/// Where the hole spanning `chars[open..=close]` sits, from the text around it.
///
/// * **Block** — it opens a statement or member (after `{`, `;` or `}`) and nothing but a `}` or the end of
///   the line follows: `{ $body...$ }`.
/// * **Item** — it is a whole element of a parenthesised list: `m($params...$)`, `f($a$, $b$)`.
/// * **Mods** — a run hole that is followed by more of a declaration: `$mods...$ final int x`.
fn position_of(chars: &[char], open: usize, close: usize, arity: Arity) -> Position {
    let prev = chars[..open].iter().rev().find(|c| !c.is_whitespace()).copied();
    let after = &chars[close + 1..];
    let next = after.iter().find(|c| !c.is_whitespace()).copied();
    let tail: String = after.iter().take_while(|c| **c != '\n').collect();
    let tail = tail.trim();
    let opens_block = matches!(prev, Some('{' | ';' | '}'));

    if opens_block && (tail.is_empty() || tail.starts_with('}')) {
        return Position::Block;
    }
    if matches!(prev, Some('(' | ',')) && matches!(next, Some(')' | ',')) {
        return Position::Item;
    }
    let line_start = chars[..open].iter().rev().take_while(|c| **c != '\n').all(|c| c.is_whitespace());
    // After a word or an annotation's `)` too: `final $rest...$ int x` — modifiers come in runs.
    let heads_a_declaration = prev.is_none()
        || opens_block
        || line_start
        || matches!(prev, Some(c) if c.is_alphanumeric() || matches!(c, '(' | ',' | '_' | ')' | '>'));
    if arity == Arity::Many
        && heads_a_declaration
        && matches!(next, Some(c) if c.is_alphabetic() || c == '@' || c == '$' || c == '_')
    {
        return Position::Mods;
    }
    Position::Plain
}

fn substitute_with(
    pattern: &str,
    forms: &HoleForms,
) -> Result<(String, Vec<Placeholder>), SyntaxError> {
    let mut out = String::with_capacity(pattern.len());
    let mut placeholders: Vec<Placeholder> = vec![];
    let bytes: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != '$' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        // `${` is never a placeholder — a name cannot begin with a brace — and it is EL, which
        // is most of what a JSP pattern is made of. Reading it as an opener made
        // `${$x$ sessionScope.$p$ $rest$}` fail with a complaint about `{` not being a usable
        // name, and left `$${` as the incantation. A language that needs an incantation for its
        // commonest construct is one nobody will write correctly twice, so the `$` is simply
        // literal here. `$$` still escapes, for the dollar-quoted bodies that wanted it.
        if bytes.get(i + 1) == Some(&'{') {
            out.push('$');
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..bytes.len()).find(|&j| bytes[j] == '$') else {
            return Err(SyntaxError::Placeholder(format!(
                "the placeholder starting at character {} is never closed — write $name$, or \
                 $$ for a literal dollar sign",
                i + 1
            )));
        };
        let inner: String = bytes[i + 1..close].iter().collect();
        let (name, arity) = match inner.strip_suffix("...") {
            Some(stem) => (stem.trim().to_string(), Arity::Many),
            None => (inner.trim().to_string(), Arity::One),
        };
        if name.is_empty() {
            return Err(SyntaxError::Placeholder(
                "a placeholder needs a name: $table$, or $columns...$ for a list".to_string(),
            ));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(SyntaxError::Placeholder(format!(
                "{name} is not a usable placeholder name — letters, digits and underscores only"
            )));
        }
        let token = match placeholders.iter().find(|p| p.name == name) {
            Some(existing) => {
                if existing.arity != arity {
                    return Err(SyntaxError::Placeholder(format!(
                        "{name} is used both as one node and as a list — pick one"
                    )));
                }
                existing.token.clone()
            }
            None => {
                let token = token_for(placeholders.len());
                placeholders.push(Placeholder {
                    name: name.clone(),
                    arity,
                    token: token.clone(),
                    texts: vec![token.clone()],
                });
                token
            }
        };
        let template = match position_of(&bytes, i, close, arity) {
            Position::Block => forms.block,
            Position::Item => forms.item,
            Position::Mods => forms.mods,
            Position::Plain => "{}",
        };
        let written = template.replace("{}", &token);
        if let Some(p) = placeholders.iter_mut().find(|p| p.name == name) {
            if !p.texts.contains(&written) {
                p.texts.push(written.clone());
            }
        }
        out.push_str(&written);
        i = close + 1;
    }

    Ok((out, placeholders))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn java() -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    /// Java fragments are not top-level constructs, so they are parsed inside a
    /// method — which is also what exercises `compile_in`.
    fn in_a_method(pattern: &str) -> Result<Pattern, SyntaxError> {
        Pattern::compile_in(&java(), pattern, "class C { void m() { ", " } }")
    }

    /// `${` is EL, not a hole — a placeholder name cannot begin with a brace, and reading the
    /// `$` as an opener made the commonest construct of a JSP pattern fail with a complaint
    /// about `{` not being a usable name.
    #[test]
    fn a_dollar_before_a_brace_is_literal() {
        let (text, holes) = substitute("${$pre...$ user.$prop$ $post...$}").expect("substitutes");
        assert!(text.starts_with("${"), "the EL delimiter survives: {text}");
        assert_eq!(
            holes.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["pre", "prop", "post"],
            "and the holes after it are still holes",
        );
    }

    /// The escape that was there first still works, for anyone who already wrote it — and for
    /// the dollar-quoted bodies it was added for.
    #[test]
    fn a_doubled_dollar_is_still_one_literal_dollar() {
        let (text, holes) = substitute("$${x$$y}").expect("substitutes");
        assert_eq!(text, "${x$y}");
        assert!(holes.is_empty());
    }

    fn matched<'a>(source: &'a str, m: &Match) -> &'a str {
        m.range.slice(source).expect("a real range")
    }

    fn captured<'a>(source: &'a str, m: &Match, name: &str) -> &'a str {
        m.capture(name).expect("that capture").range.slice(source).expect("a real range")
    }

    #[test]
    fn a_placeholder_matches_one_node_and_says_what_it_was() {
        let pattern = in_a_method("registra($what$);").expect("compiles");
        let source = "class A { void go() { registra(codice); registra(42); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 2);
        assert_eq!(captured(source, &found[0], "what"), "codice");
        assert_eq!(captured(source, &found[1], "what"), "42");
    }

    #[test]
    fn a_leaf_must_agree_on_its_text_and_not_only_on_its_kind() {
        // The bug this exists to prevent: `registra` and `cancella` are both
        // `identifier`, and a matcher comparing kinds alone rewrites every call
        // in the repository when asked about one.
        let pattern = in_a_method("registra($what$);").expect("compiles");
        let source = "class A { void go() { cancella(codice); } }";
        assert!(pattern.find_all(&java(), source).expect("searches").is_empty());
    }

    #[test]
    fn a_list_placeholder_takes_the_run_with_its_separators() {
        let pattern = in_a_method("registra($args...$);").expect("compiles");
        let source = "class A { void go() { registra(uno, due, tre); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        // The original bytes, commas included — nothing was re-joined, so nothing
        // could be re-joined wrongly.
        assert_eq!(captured(source, &found[0], "args"), "uno, due, tre");
        assert_eq!(found[0].capture("args").unwrap().parts.len(), 5, "three names and two commas");
    }

    #[test]
    fn a_list_placeholder_matches_nothing_at_all() {
        let pattern = in_a_method("registra($args...$);").expect("compiles");
        let source = "class A { void go() { registra(); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert!(found[0].capture("args").expect("still captured").range.is_empty());
    }

    #[test]
    fn two_list_placeholders_backtrack_into_place() {
        let pattern = in_a_method("registra($head...$, ultimo, $tail...$);").expect("compiles");
        let source = "class A { void go() { registra(a, b, ultimo, c); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1, "the greedy first run has to give bytes back");
        assert_eq!(captured(source, &found[0], "head"), "a, b");
        assert_eq!(captured(source, &found[0], "tail"), "c");
    }

    #[test]
    fn the_match_survives_line_breaks_and_a_comment_in_the_middle() {
        // The whole reason this is structural and not a regex.
        let pattern = in_a_method("registra($what$);").expect("compiles");
        let source = "class A { void go() { registra(\n   /* perché no */ codice\n  ); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(captured(source, &found[0], "what"), "codice");
    }

    #[test]
    fn matches_never_nest() {
        // An inner match inside an outer one would be an edit inside an edit.
        let pattern = in_a_method("registra($what$);").expect("compiles");
        let source = "class A { void go() { registra(registra(x)); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(matched(source, &found[0]), "registra(registra(x));");
    }

    #[test]
    fn the_same_name_used_twice_is_the_same_hole() {
        let pattern = in_a_method("somma($x$, $x$);").expect("compiles");
        assert_eq!(pattern.names(), vec!["x"]);
        // Both positions are the same placeholder, so both capture — the second
        // simply overwrites nothing, and the pattern still matches structurally.
        let source = "class A { void go() { somma(uno, due); } }";
        assert_eq!(pattern.find_all(&java(), source).expect("searches").len(), 1);
    }

    #[test]
    fn case_sensitivity_is_the_callers_decision() {
        let source = "class A { void go() { REGISTRA(x); } }";
        let strict = in_a_method("registra($w$);").expect("compiles");
        assert!(strict.find_all(&java(), source).expect("searches").is_empty());

        let loose = in_a_method("registra($w$);").expect("compiles").case_insensitive(true);
        assert_eq!(loose.find_all(&java(), source).expect("searches").len(), 1);
    }

    #[test]
    fn an_unclosed_placeholder_says_so_before_anything_is_parsed() {
        let err = in_a_method("registra($what);").expect_err("refused");
        assert!(matches!(err, SyntaxError::Placeholder(_)), "{err:?}");
        assert!(err.to_string().contains("never closed"), "{err}");
    }

    #[test]
    fn a_double_dollar_is_a_literal_one() {
        // PostgreSQL bodies are written `$$ … $$`, so the escape is not academic.
        let (text, holes) = substitute("valore $$ e $x$").expect("substitutes");
        assert!(text.starts_with("valore $ e "));
        assert_eq!(holes.len(), 1);
    }

    #[test]
    fn a_pattern_that_is_not_the_language_is_refused_with_its_own_offsets() {
        let err = in_a_method("registra($what$ ;;;;;)").expect_err("refused");
        match err {
            SyntaxError::Pattern { at: Some(range), .. } => {
                // Pointing into the pattern the user typed, not into the wrapper
                // they never saw.
                assert!(range.start < 40, "{range:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    const STATEMENTS: HoleForms = HoleForms { block: "{};", item: "{}", mods: "{}" };
    const MEMBERS: HoleForms = HoleForms { block: "int {};", item: "{}", mods: "{}" };

    /// A hole alone in a block is a run of statements — the position a bare identifier cannot fill.
    #[test]
    fn a_hole_in_a_block_stands_for_a_run_of_statements() {
        let pattern = Pattern::compile_in_with(
            &java(),
            "for (int $i$ = 0; $i$ < $n$; $i$++) { $body...$ }",
            "class C { void m() { ",
            " } }",
            &STATEMENTS,
        )
        .expect("compiles");
        let source = "class A { void go() { for (int k = 0; k < 3; k++) { a(); b(); } } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(captured(source, &found[0], "body"), "a(); b();");
    }

    #[test]
    fn a_hole_in_a_class_body_stands_for_a_run_of_members() {
        let pattern = Pattern::compile_in_with(
            &java(),
            "class $c$ extends $b$ { $members...$ }",
            "",
            "",
            &MEMBERS,
        )
        .expect("compiles");
        let source = "class A extends B { int x; void f() {} }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(captured(source, &found[0], "members"), "int x; void f() {}");
    }

    #[test]
    fn a_parameter_run_hole_takes_whole_parameters() {
        const PARAMS: HoleForms = HoleForms { block: "{}", item: "int {}", mods: "{}" };
        let pattern = Pattern::compile_in_with(&java(), "void $m$($params...$) { }", "class C { ", " }", &PARAMS)
            .expect("compiles");
        let source = "class A { void go(String a, int b) { } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(captured(source, &found[0], "params"), "String a, int b");
    }

    /// `$mods...$` is zero or more modifiers and annotations — including none at all, when the declaration
    /// has no `modifiers` node for it to sit in.
    #[test]
    fn a_modifier_hole_may_match_nothing() {
        const MODS: HoleForms = HoleForms { block: "{}", item: "{}", mods: "@{}" };
        let pattern = Pattern::compile_in_with(&java(), "$mods...$ int $n$ = 1;", "class C { void m() { ", " } }", &MODS)
            .expect("compiles");
        let source = "class A { void go() { final int a = 1; int b = 1; @Deprecated final int c = 1; } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        assert_eq!(found.len(), 3, "with modifiers, without, and with an annotation too");
        assert_eq!(captured(source, &found[0], "mods"), "final");
        assert!(found[1].capture("mods").expect("still captured").range.is_empty());
        assert_eq!(captured(source, &found[2], "mods"), "@Deprecated final");
    }

    /// Forms only apply where a bare identifier would not do: the plain form keeps every older pattern
    /// meaning what it meant.
    #[test]
    fn plain_forms_change_nothing() {
        let (a, _) = substitute("f($x$, { $y$ })").expect("substitutes");
        let (b, _) = substitute_with("f($x$, { $y$ })", &HoleForms::PLAIN).expect("substitutes");
        assert_eq!(a, b);
    }

    #[test]
    fn one_name_cannot_be_a_node_here_and_a_list_there() {
        let err = in_a_method("f($x$, g($x...$));").expect_err("refused");
        assert!(err.to_string().contains("pick one"), "{err}");
    }
}
