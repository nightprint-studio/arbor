//! Running a query: compiling the alternatives, matching, filtering, de-duplicating.
//!
//! ## Two parses, and only when they are needed
//!
//! [`arbor_syntax::prelude::Pattern::find_all`] parses the subject itself. A `#kind` constraint
//! and `group enclosing` need the tree as well, and the pattern crate hands back ranges rather
//! than nodes — so this module parses a second time **only for the queries that ask for it**,
//! which is a small minority. A plain `log.debug($x$)` never pays for it.
//!
//! ## De-duplication is not an optimisation
//!
//! With `or`, two alternatives can match the same bytes: `$o$.$m$()` and `$o$.close()` both
//! match `x.close()`. Counting that twice produces a number that is *plausible and wrong*, which
//! is the worst kind — so a hit is identified by its file and range, and the first alternative to
//! claim it keeps it.
//!
//! ## `~` is a glob, not a regex
//!
//! The `regex` crate is not a dependency of this workspace's Bennu crates, and adding one to
//! spell `get.*` when `get*` says the same thing would be a poor trade. So `~` and the type-name
//! constraint share one matcher: `*` stands for any run, `|` separates alternatives, everything
//! else is literal, and the whole thing is anchored. `~place|cancel`, `~get*`, `*Dao`.

use std::collections::HashSet;

use arbor_syntax::prelude::{ByteRange, HoleForms, Pattern, SyntaxError};
use tree_sitter::{Language, Node, Parser, Tree};

use crate::query::{Alternative, Ask, Constraint, DeclKind, Denotes, Enclosure, GroupBy, Query};

/// What an expression turned out to be: a type, or a value of a type.
///
/// **One answer, not two**, because the two questions share all their work. Resolving `Files` in
/// `Files.copy(a, b)` means asking "is there a local, a parameter, a field called `Files`?" and
/// then "is there a class called `Files`?" — and the answer to a `$x: Files$` constraint and to a
/// `$x: @type$` one falls out of the same lookup. Splitting them into two oracle calls would have
/// resolved the same name twice and let the two answers disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denotation {
    /// It names a **type** — a static access. Carries that type's own dotted name.
    Type(String),
    /// It names a **value** — a local, a parameter, a field. Carries the value's dotted static
    /// type, which is what a `$x: Order$` constraint compares against.
    Value(String),
}

impl Denotation {
    /// The type in play either way: the type itself, or the value's type.
    pub fn type_name(&self) -> &str {
        match self {
            Denotation::Type(name) | Denotation::Value(name) => name,
        }
    }

    /// Which side of the `@type` / `@value` distinction this is.
    pub fn denotes(&self) -> Denotes {
        match self {
            Denotation::Type(_) => Denotes::Type,
            Denotation::Value(_) => Denotes::Value,
        }
    }
}

/// Resolving a name is the one thing this crate cannot do for itself.
///
/// It needs the project's classpath, its imports and its inference — all of which live behind
/// the index. So the caller supplies them, and this crate stays pure and testable: the unit
/// tests hand it a two-line fake and the backend hands it the real resolver.
pub trait TypeOracle {
    /// What the expression at `range` in `file` denotes, or `None` when it cannot be told.
    ///
    /// **`None` must mean "unknown", never "no"** — the difference is what keeps an incomplete
    /// classpath from silently shrinking a count. See [`Hit::unresolved`].
    ///
    /// An implementation must try the **value** reading first: in Java a local shadows a type of
    /// the same name, so a variable called `Order` is a value, not the class.
    fn denotation_at(&self, file: &str, source: &str, range: ByteRange) -> Option<Denotation>;

    /// Whether `candidate` is `wanted` or a subtype of it. Both fully qualified.
    fn is_subtype_of(&self, candidate: &str, wanted: &str) -> bool;
}

/// A [`TypeOracle`] that knows nothing — every resolved constraint comes back unresolved.
///
/// Not a null object for convenience: it is what a query runs against before the index has
/// landed, and its answers are honestly marked rather than absent.
pub struct NoTypes;

impl TypeOracle for NoTypes {
    fn denotation_at(&self, _file: &str, _source: &str, _range: ByteRange) -> Option<Denotation> {
        None
    }
    fn is_subtype_of(&self, candidate: &str, wanted: &str) -> bool {
        candidate == wanted
    }
}

/// One thing a capture matched, carried through to the report and the replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitCapture {
    pub name: String,
    pub range: ByteRange,
    /// The subject's own bytes. What a `group $m$` counts by, and what a template substitutes.
    pub text: String,
}

/// One match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub range: ByteRange,
    /// 1-based.
    pub line: usize,
    /// The matched source, trimmed for a list row.
    pub preview: String,
    pub captures: Vec<HitCapture>,
    /// The method or class this sits inside, when the query asked for it (`group enclosing`).
    pub enclosing: Option<String>,
    /// A type constraint on this match could not be decided — the classpath does not reach that
    /// far. Kept and **flagged** rather than dropped: a count that quietly excludes what it could
    /// not resolve is a count that lies about being complete.
    pub unresolved: bool,
}

impl Hit {
    pub fn capture(&self, name: &str) -> Option<&HitCapture> {
        self.captures.iter().find(|c| c.name == name)
    }
}

/// One file to search.
pub struct Subject<'a> {
    /// Project-relative, forward-slashed — what `in` filters and what a row shows.
    pub path: &'a str,
    pub source: &'a str,
}

/// Compile a query's alternatives.
///
/// ## Why this takes a *list* of contexts
///
/// A pattern is a fragment, and a grammar accepts a fragment only where it is legal. The user
/// does not think about that — they write the code they are looking for — so the compiler tries
/// each context in turn and keeps the first that parses cleanly.
///
/// For Java that is four, and leaving any of them out breaks a whole class of query:
///
/// | Context | What it admits | Written as |
/// |---|---|---|
/// | none | a compilation unit | `class $c$ extends $b$ { $body...$ }` |
/// | a class body | a member declaration | `void $m$($args...$) { $body...$ }` |
/// | a method body | a statement | `if ($c$) { $b...$ }`, `return $x$;` |
/// | a method body, with a `;` appended | an **expression** | `log.debug($x$)`, `$a$.$b$($c$)` |
///
/// The last one is not an optimisation, it is the common case: `$a$.$b$($c$, $d$)` is an
/// expression, Java requires a semicolon to make it a statement, and nobody writing a pattern
/// types one. Without that context every method-call pattern fails to compile and the query
/// silently finds nothing — which reads as "there is none of that in this project".
///
/// `compile_in` reports the fragment's own range, so the appended `;` is outside it and the
/// matched node is the expression rather than the statement wrapping it. That is what makes
/// `log.debug($x$)` match a call anywhere, including inside a larger expression.
pub fn compile(
    language: &Language,
    query: &Query,
    contexts: &[(&str, &str)],
) -> Result<Vec<Pattern>, SyntaxError> {
    compile_with(language, query, contexts, &[HoleForms::PLAIN])
}

/// The ways a Java hole may be written where a bare name would not parse, **most ordinary first**.
///
/// A hole alone in a block is a statement (`{};`) or, in a class body, a member (`int {};`); alone in a
/// parenthesised list it is a parameter (`int {}`); in front of a declaration it is a run of modifiers and
/// annotations (`@{}`). The compiler tries these in order — and the bare form first, in every context,
/// before any other — so a pattern that compiled before compiles exactly as it did.
pub const JAVA_FORMS: &[HoleForms] = &[
    HoleForms::PLAIN,
    HoleForms { block: "{};", item: "{}", mods: "{}" },
    HoleForms { block: "int {};", item: "{}", mods: "{}" },
    HoleForms { block: "{}", item: "int {}", mods: "{}" },
    HoleForms { block: "{}", item: "{}", mods: "@{}" },
    HoleForms { block: "{};", item: "{}", mods: "@{}" },
    HoleForms { block: "int {};", item: "{}", mods: "@{}" },
    HoleForms { block: "{}", item: "int {}", mods: "@{}" },
    HoleForms { block: "{};", item: "int {}", mods: "{}" },
    HoleForms { block: "int {};", item: "int {}", mods: "{}" },
    HoleForms { block: "{};", item: "int {}", mods: "@{}" },
    HoleForms { block: "int {};", item: "int {}", mods: "@{}" },
];

/// [`compile`] with the forms a hole may take in a position a bare name cannot fill (see [`JAVA_FORMS`]).
///
/// An alternative with an `as` kind is compiled in that kind's wrappers **only**: it is the user saying how
/// the text is to be read, and falling back to another reading is how one pattern came to mean two things.
pub fn compile_with(
    language: &Language,
    query: &Query,
    contexts: &[(&str, &str)],
    forms: &[HoleForms],
) -> Result<Vec<Pattern>, SyntaxError> {
    let Ask::Patterns(alternatives) = &query.ask else { return Ok(Vec::new()) };
    alternatives.iter().map(|alt| compile_one(language, alt, contexts, forms)).collect()
}

fn compile_one(
    language: &Language,
    alt: &Alternative,
    contexts: &[(&str, &str)],
    forms: &[HoleForms],
) -> Result<Pattern, SyntaxError> {
    let pattern = alt.pattern.as_str();
    // Where the fragment may sit: the one reading it was declared as, or bare and then each context.
    // The guess is about Java: a grammar compiled with no wrapper contexts (a page) is left alone.
    let reading = alt.decl.or_else(|| if contexts.is_empty() { None } else { DeclKind::guess(pattern) });
    let attempts: Vec<(&str, &str)> = match reading {
        Some(kind) => kind.wrappers().to_vec(),
        None => std::iter::once(("", "")).chain(contexts.iter().copied()).collect(),
    };
    let mut first_error: Option<SyntaxError> = None;
    for (index, form) in forms.iter().enumerate() {
        let mut last = None;
        for (prefix, suffix) in &attempts {
            match Pattern::compile_in_with(language, pattern, prefix, suffix, form) {
                Ok(compiled) => return Ok(compiled),
                // Kept so the error the caller sees is a real one about their pattern, rather than
                // whichever context happened to be tried first.
                Err(e) => last = Some(e),
            }
        }
        // The error to report is the bare form's: a complaint about `int ARBOR_PLACEHOLDER_3` would be
        // about text the user never wrote.
        if index == 0 {
            first_error = last;
        }
    }
    Err(first_error.unwrap_or_else(|| SyntaxError::Pattern {
        reason: "this pattern has no body".to_string(),
        at: None,
    }))
}

/// Run the compiled alternatives over one file.
///
/// The alternatives are tried in the order they were written and a range already claimed is
/// skipped, so `or` never double-counts.
pub fn search_file(
    language: &Language,
    query: &Query,
    compiled: &[Pattern],
    subject: &Subject<'_>,
    types: &dyn TypeOracle,
) -> Result<Vec<Hit>, SyntaxError> {
    let Ask::Patterns(alternatives) = &query.ask else { return Ok(Vec::new()) };

    // Only parsed when something actually needs the tree — see the module doc.
    let tree = needs_tree(query).then(|| parse(language, subject.source)).flatten();

    let mut claimed: HashSet<(usize, usize)> = HashSet::new();
    let mut hits = Vec::new();

    for (alt, pattern) in alternatives.iter().zip(compiled) {
        for found in pattern.find_all(language, subject.source)? {
            if !claimed.insert((found.range.start, found.range.end)) {
                continue; // another alternative already reported these bytes
            }
            let captures: Vec<HitCapture> = found
                .captures
                .iter()
                .map(|c| HitCapture {
                    name: c.name.clone(),
                    range: c.range,
                    text: c.range.slice(subject.source).unwrap_or_default().to_string(),
                })
                .collect();

            let mut verdict = admits(alt, &captures, subject, types, tree.as_ref());
            // Where the match lives. Without a tree the question cannot be answered, and that is
            // "unknown", not "no" — the same rule a type constraint follows.
            if verdict != Verdict::Refused && !query.inside.is_empty() {
                verdict = all_of([verdict, lives_inside(&query.inside, tree.as_ref(), found.range)]);
            }
            if verdict == Verdict::Refused {
                claimed.remove(&(found.range.start, found.range.end));
                continue;
            }

            hits.push(Hit {
                file: subject.path.to_string(),
                range: found.range,
                line: line_of(subject.source, found.range.start),
                preview: preview_of(subject.source, found.range),
                enclosing: matches!(query.group, Some(GroupBy::Enclosing))
                    .then(|| enclosing_of(tree.as_ref(), subject.source, found.range))
                    .flatten(),
                captures,
                unresolved: verdict == Verdict::Unresolved,
            });
        }
    }

    hits.sort_by_key(|h| h.range.start);
    Ok(hits)
}

/// Whether a query needs its own parse of the subject.
fn needs_tree(query: &Query) -> bool {
    if matches!(query.group, Some(GroupBy::Enclosing)) || !query.inside.is_empty() {
        return true;
    }
    let Ask::Patterns(alts) = &query.ask else { return false };
    alts.iter().any(|a| a.constraints.iter().any(|c| mentions_kind(&c.constraint)))
}

fn mentions_kind(c: &Constraint) -> bool {
    match c {
        Constraint::Kind(_) => true,
        Constraint::Not(inner) => mentions_kind(inner),
        Constraint::All(parts) => parts.iter().any(mentions_kind),
        _ => false,
    }
}

/// The three answers a constraint check can give. `Unresolved` is not `Refused`: it is the
/// oracle saying it does not know, and the hit survives carrying that fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Admitted,
    Unresolved,
    Refused,
}

/// Fold several verdicts into one: any refusal decides it, otherwise any unknown taints it.
///
/// The rule for a whole alternative and for a single `a & b` are the same one, which is why it is
/// written once — the alternative that got this wrong would report a hit whose type constraint
/// could not be read as if it had been.
fn all_of(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
    let mut out = Verdict::Admitted;
    for verdict in verdicts {
        match verdict {
            Verdict::Refused => return Verdict::Refused,
            Verdict::Unresolved => out = Verdict::Unresolved,
            Verdict::Admitted => {}
        }
    }
    out
}

fn admits(
    alt: &Alternative,
    captures: &[HitCapture],
    subject: &Subject<'_>,
    types: &dyn TypeOracle,
    tree: Option<&Tree>,
) -> Verdict {
    all_of(alt.constraints.iter().filter_map(|named| {
        // A `...` that matched nothing has nothing to constrain.
        let capture = captures.iter().find(|c| c.name == named.name)?;
        Some(check(&named.constraint, capture, subject, types, tree))
    }))
}

fn check(
    constraint: &Constraint,
    capture: &HitCapture,
    subject: &Subject<'_>,
    types: &dyn TypeOracle,
    tree: Option<&Tree>,
) -> Verdict {
    match constraint {
        Constraint::Text(glob) => {
            if glob_matches(glob, &capture.text) { Verdict::Admitted } else { Verdict::Refused }
        }
        Constraint::Kind(kind) => {
            let Some(node) = tree.and_then(|t| node_at(t, capture.range)) else {
                return Verdict::Unresolved;
            };
            if node.kind() == kind { Verdict::Admitted } else { Verdict::Refused }
        }
        Constraint::Type { name, subtypes } => {
            let Some(actual) = types.denotation_at(subject.path, subject.source, capture.range)
            else {
                return Verdict::Unresolved;
            };
            // The type in play, whether the node named the type itself or a value of it — the
            // `@type` / `@value` half is a separate constraint, on purpose.
            let actual = actual.type_name();
            let ok = if *subtypes {
                types.is_subtype_of(actual, name)
            } else {
                // Written unqualified (`Order`) matches on the simple name; a glob matches
                // either form — which is what `*Dao` is for.
                type_names_agree(actual, name)
            };
            if ok { Verdict::Admitted } else { Verdict::Refused }
        }
        Constraint::Denotes(wanted) => {
            match types.denotation_at(subject.path, subject.source, capture.range) {
                Some(actual) if actual.denotes() == *wanted => Verdict::Admitted,
                Some(_) => Verdict::Refused,
                None => Verdict::Unresolved,
            }
        }
        // Unknown stays unknown under negation: "not a thing I could not determine" is not a
        // fact, and turning it into one is how an unresolved classpath starts inventing hits.
        Constraint::Not(inner) => match check(inner, capture, subject, types, tree) {
            Verdict::Admitted => Verdict::Refused,
            Verdict::Refused => Verdict::Admitted,
            Verdict::Unresolved => Verdict::Unresolved,
        },
        Constraint::All(parts) => {
            all_of(parts.iter().map(|p| check(p, capture, subject, types, tree)))
        }
    }
}

/// The declaration the match is **nearest inside** — a method, a lambda, a class…
///
/// The match's own node is not a candidate: a method matched by `as member` is inside its class, not
/// inside itself. A block that is a direct child of a class body is an instance initializer, and a class
/// body under `new X() { … }` or an enum constant is an anonymous class, which is why neither is read off
/// the node's own kind.
fn nearest_enclosure(node: Node<'_>) -> Option<Enclosure> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let grand = parent.parent().map(|g| g.kind());
        let found = match parent.kind() {
            "method_declaration" => Some(Enclosure::Method),
            "constructor_declaration" | "compact_constructor_declaration" => Some(Enclosure::Constructor),
            "static_initializer" => Some(Enclosure::Initializer),
            "block" if grand == Some("class_body") => Some(Enclosure::Initializer),
            "lambda_expression" => Some(Enclosure::Lambda),
            "class_declaration" => Some(Enclosure::Class),
            "interface_declaration" | "annotation_type_declaration" => Some(Enclosure::Interface),
            "enum_declaration" => Some(Enclosure::Enum),
            "record_declaration" => Some(Enclosure::Record),
            "class_body" if matches!(grand, Some("object_creation_expression" | "enum_constant")) => {
                Some(Enclosure::Anonymous)
            }
            _ => None,
        };
        if found.is_some() {
            return found;
        }
        current = parent;
    }
    None
}

/// Whether the match at `range` lives in one of `wanted`. No tree, or a match that sits in none of them
/// (top level of a file), is undecided rather than refused.
fn lives_inside(wanted: &[Enclosure], tree: Option<&Tree>, range: ByteRange) -> Verdict {
    let Some(node) = tree.and_then(|t| node_at(t, range)) else { return Verdict::Unresolved };
    match nearest_enclosure(node) {
        Some(found) if wanted.contains(&found) => Verdict::Admitted,
        Some(_) => Verdict::Refused,
        None => Verdict::Refused,
    }
}

/// Whether a resolved fully-qualified type answers to `wanted`, which may be qualified, simple,
/// or a glob over either.
fn type_names_agree(actual: &str, wanted: &str) -> bool {
    if glob_matches(wanted, actual) {
        return true;
    }
    let simple = actual.rsplit('.').next().unwrap_or(actual);
    glob_matches(wanted, simple)
}

/// `*` = any run, `|` = alternatives, everything else literal, whole-string. See the module doc
/// for why this is not a regex.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    pattern.split('|').any(|alt| one_glob(alt.trim(), text))
}

fn one_glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    // The first part is anchored at the start, the last at the end, the middle ones float.
    if let Some(first) = parts.first() {
        let Some(stripped) = rest.strip_prefix(first) else { return false };
        rest = stripped;
    }
    if let Some(last) = parts.last() {
        if parts.len() > 1 {
            if rest.len() < last.len() || !rest.ends_with(last) {
                return false;
            }
            rest = &rest[..rest.len() - last.len()];
        }
    }
    for middle in &parts[1..parts.len().saturating_sub(1)] {
        if middle.is_empty() {
            continue;
        }
        let Some(at) = rest.find(middle) else { return false };
        rest = &rest[at + middle.len()..];
    }
    true
}

fn parse(language: &Language, source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    parser.parse(source, None)
}

/// The smallest node covering exactly `range`, or `None` when the range names no single node.
fn node_at(tree: &Tree, range: ByteRange) -> Option<Node<'_>> {
    tree.root_node().descendant_for_byte_range(range.start, range.end)
}

/// The name of the method or class the match sits in — `OrderDao.findAll`, or `OrderDao` for a
/// match in a field initialiser.
///
/// Best effort: a match in a construct the walk does not recognise reports `None`, and the report
/// buckets those together rather than inventing a name for them.
fn enclosing_of(tree: Option<&Tree>, source: &str, range: ByteRange) -> Option<String> {
    let node = node_at(tree?, range)?;
    let mut method: Option<String> = None;
    let mut owner: Option<String> = None;
    let mut current = Some(node);
    while let Some(n) = current {
        match n.kind() {
            "method_declaration" | "constructor_declaration" if method.is_none() => {
                method = named_child_text(n, source);
            }
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" => {
                if owner.is_none() {
                    owner = named_child_text(n, source);
                }
            }
            _ => {}
        }
        current = n.parent();
    }
    match (owner, method) {
        (Some(o), Some(m)) => Some(format!("{o}.{m}")),
        (Some(o), None) => Some(o),
        (None, Some(m)) => Some(m),
        (None, None) => None,
    }
}

fn named_child_text(node: Node<'_>, source: &str) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    source.get(name.start_byte()..name.end_byte()).map(str::to_string)
}

/// The 1-based line an offset sits on.
///
/// Public because a caller that produces hits from a **fragment** — a `<% … %>` body cut out of a
/// page and parsed as Java — has to re-express them against the file the fragment came from, and
/// two implementations of "which line is this" would drift into two different previews for the
/// same match depending on which dialect found it.
pub fn line_of(source: &str, at: usize) -> usize {
    source.get(..at).map(|head| head.lines().count().max(1)).unwrap_or(1)
}

/// The matched text as a row shows it: one line, collapsed whitespace, bounded.
///
/// Public for the same reason as [`line_of`].
pub fn preview_of(source: &str, range: ByteRange) -> String {
    const MAX: usize = 200;
    let text = range.slice(source).unwrap_or_default();
    let mut out = String::with_capacity(text.len().min(MAX));
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
        if out.chars().count() >= MAX {
            out.push('…');
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse as parse_query;

    /// The contexts a Java fragment can be parsed in, most permissive last. The  one is what
    /// makes an EXPRESSION pattern — the common case — compile at all.
    const WRAP: &[(&str, &str)] = &[
        ("class __Q {", "}"),
        ("class __Q { void __m() {", "} }"),
        ("class __Q { void __m() {", ";} }"),
        // A `catch` clause is only legal after a `try`.
        ("class __Q { void __m() { try {} ", "} }"),
    ];

    fn language() -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn run(query_text: &str, source: &str, types: &dyn TypeOracle) -> Vec<Hit> {
        let query = parse_query(query_text).expect("query parses");
        // Named in the message: a table-driven test that fails on `.expect("pattern compiles")`
        // reports a byte range into text nobody wrote, and finding out *which row* meant doing
        // the arithmetic by hand.
        let compiled = compile_with(&language(), &query, WRAP, JAVA_FORMS)
            .unwrap_or_else(|e| panic!("`{query_text}` does not compile — {e}"));
        search_file(
            &language(),
            &query,
            &compiled,
            &Subject { path: "src/main/java/com/acme/OrderDao.java", source },
            types,
        )
        .expect("search runs")
    }

    /// **The one that was broken.** `$a$.$b$($c$, $d$)` is an *expression*; Java needs a `;` to
    /// make it a statement, and nobody writing a pattern types one. Without the trailing-`;`
    /// context it did not compile, and a query that does not compile finds nothing — which reads
    /// as "there are no two-argument calls in this project".
    #[test]
    fn a_bare_expression_pattern_compiles_and_matches() {
        let src = "class A { void m() { a.f(1, 2); b.g(3); c.h(4, 5); } }";
        let hits = run("$a$.$b$($c$, $d$)", src, &NoTypes);
        let found: Vec<&str> = hits.iter().map(|h| h.preview.as_str()).collect();
        assert_eq!(found, ["a.f(1, 2)", "c.h(4, 5)"], "every two-argument call, and only those");
    }

    /// Each context admits a different shape, and dropping any of them silently breaks a whole
    /// class of query. One assertion per row of the table in `compile`'s doc.
    #[test]
    fn every_parse_context_admits_its_shape() {
        for (pattern, source) in [
            // a compilation unit
            ("import $p$;", "import com.acme.Foo;"),
            // a statement
            ("return $x$;", "class A { int m() { return 7; } }"),
            // an expression — the case that needs the appended `;`
            ("log.debug($x$)", "class A { void m() { log.debug(\"a\"); } }"),
            // an argument list, which is where a run of siblings actually works
            ("f($args...$)", "class A { void m() { f(1, 2); } }"),
        ] {
            let hits = run(pattern, source, &NoTypes);
            assert_eq!(hits.len(), 1, "`{pattern}` should match in `{source}`");
        }
    }

    /// A bare hole is an **identifier**, so on its own it can only sit where a name is legal: a run
    /// of arguments works, a run of members, statements or parameters does not. That is what plain
    /// [`compile`] still does — and it is what the *forms* ([`JAVA_FORMS`]) exist to lift, which the
    /// next tests pin: the same patterns compile once the hole may be written as a statement, a
    /// member, a parameter or a modifier.
    #[test]
    fn a_bare_hole_only_works_where_a_name_is_legal() {
        let compiles = |text: &str| {
            compile(&language(), &parse_query(text).expect("parses"), WRAP).is_ok()
        };
        assert!(compiles("f($args...$)"), "arguments are expressions");
        assert!(!compiles("class $c$ extends $b$ { $members...$ }"), "members are not names");
        assert!(!compiles("void $m$($params...$) { $body...$ }"), "parameters are not names either");
    }

    fn compiles_with_forms(text: &str) -> bool {
        compile_with(&language(), &parse_query(text).expect("parses"), WRAP, JAVA_FORMS).is_ok()
    }

    /// The shapes that were impossible to write, now compiling. One row each, named by what the
    /// user was after.
    #[test]
    fn declaration_shaped_patterns_compile() {
        for text in [
            "for (int $i$ = 0; $i$ < $l$.size(); $i$++) { $body...$ }",
            "catch ($t$ $e$) { }",
            "@Autowired private $t$ $n$;",
            "class $c$ extends $b$ { $members...$ }",
            "$mods...$ class $c$ extends $b$ implements $i$ { $members...$ }",
            "void $m$($params...$) { $body...$ }",
            "$mods...$ $r$ $m$($params...$) throws $e$ { $body...$ }",
            "if ($c$) { $stmts...$ }",
            "try { $stmts...$ } catch ($t$ $e$) { $handler...$ }",
            "$mods...$ interface $i$ { $members...$ }",
            // An enum's members come after the constants and a `;`.
            "$mods...$ enum $e$ { $constants...$ ; $members...$ }",
            "$mods...$ record $r$($params...$) { $members...$ }",
            "($a$, $b$) -> { $stmts...$ }",
        ] {
            assert!(compiles_with_forms(text), "`{text}` should compile");
        }
    }

    #[test]
    fn a_statement_hole_matches_the_body_of_a_loop() {
        let src = "class A { void m() { for (int i = 0; i < xs.size(); i++) { a(); b(); } } }";
        let hits = run("for (int $i$ = 0; $i$ < $l$.size(); $i$++) { $body...$ }", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture("body").map(|c| c.text.as_str()), Some("a(); b();"));
    }

    #[test]
    fn a_catch_clause_is_a_pattern() {
        let src = "class A { void m() { try { f(); } catch (IOException e) { } catch (Exception x) { g(); } } }";
        let hits = run("catch ($t$ $e$) { }", src, &NoTypes);
        assert_eq!(hits.len(), 1, "only the catch with an empty body");
        assert_eq!(hits[0].capture("t").map(|c| c.text.as_str()), Some("IOException"));
    }

    #[test]
    fn a_class_pattern_ignores_what_is_in_the_body() {
        let src = "public class A extends B { int x; void f() {} } class C extends B {} class D {}";
        let hits = run("$mods...$ class $c$ extends $b$ { $members...$ }", src, &NoTypes);
        let names: Vec<&str> = hits.iter().filter_map(|h| h.capture("c")).map(|c| c.text.as_str()).collect();
        assert_eq!(names, ["A", "C"]);
    }

    #[test]
    fn an_annotated_field_is_found_by_its_annotation() {
        let src = "class A { @Autowired private Svc s; private Svc t; }";
        let hits = run("@Autowired private $t$ $n$;", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture("n").map(|c| c.text.as_str()), Some("s"));
    }

    // ── `as`: what the text is ──────────────────────────────────────────────────

    const FINALS: &str =
        "class A { final int f = 1; void m(final int p) { final int l = 2; int k = 3; } }";

    #[test]
    fn as_picks_one_reading_of_the_same_text() {
        let locals = run("final $t$ $n$ = $v$;\nas local", FINALS, &NoTypes);
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].preview, "final int l = 2;");

        let fields = run("final $t$ $n$ = $v$;\nas field", FINALS, &NoTypes);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].preview, "final int f = 1;");

        let params = run("final $t$ $n$\nas param", FINALS, &NoTypes);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].preview, "final int p");
    }

    /// Two readings in one query: the `as` belongs to the alternative it follows.
    #[test]
    fn as_belongs_to_its_own_alternative() {
        let hits = run("final $t$ $n$ = $v$;\nas local\nor final $t$ $n$ = $v$;\nas field", FINALS, &NoTypes);
        assert_eq!(hits.len(), 2, "a local and a field, each found once");
    }

    #[test]
    fn a_modifier_hole_lets_one_pattern_cover_every_declaration() {
        let hits = run("$mods...$ int $n$ = $v$;\nas local", FINALS, &NoTypes);
        let found: Vec<&str> = hits.iter().map(|h| h.preview.as_str()).collect();
        assert_eq!(found, ["final int l = 2;", "int k = 3;"]);
        assert_eq!(hits[0].capture("mods").map(|c| c.text.as_str()), Some("final"));
        assert_eq!(hits[1].capture("mods").map(|c| c.text.as_str()), Some(""));
    }

    /// The case that started this: take `final` off locals and parameters, keep it on fields, leave
    /// the other modifiers and annotations exactly where they were.
    #[test]
    fn removing_final_from_locals_and_parameters_leaves_the_fields() {
        use crate::replace::{apply, edits_for};
        let src = "class A { final int f = 1; void m(final int p, String q) { final int l = 2; @Deprecated final int k = 3; } }";
        let rewrite = |query: &str, template: &str, source: &str| {
            let hits = run(query, source, &NoTypes);
            apply(source, &edits_for(template, &hits))
        };
        let locals = rewrite(
            "$pre...$ final $post...$ $t$ $n$ = $v$;\nas local",
            "$pre$ $post$ $t$ $n$ = $v$;",
            src,
        );
        let both = rewrite("$pre...$ final $post...$ $t$ $n$\nas param", "$pre$ $post$ $t$ $n$", &locals);
        assert_eq!(
            both,
            "class A { final int f = 1; void m(int p, String q) { int l = 2; @Deprecated int k = 3; } }",
        );
    }

    // ── `inside`: where it lives ────────────────────────────────────────────────

    #[test]
    fn inside_asks_for_the_nearest_enclosing_declaration() {
        let src = "class A { int f = g(); void m() { g(); Runnable r = () -> g(); } A() { g(); } }";
        let count = |q: &str| run(q, src, &NoTypes).len();
        assert_eq!(count("g()"), 4);
        assert_eq!(count("g()\ninside method"), 1);
        assert_eq!(count("g()\ninside lambda"), 1);
        assert_eq!(count("g()\ninside constructor"), 1);
        assert_eq!(count("g()\ninside class"), 1, "the field initialiser");
        assert_eq!(count("g()\ninside body"), 3);
        assert_eq!(count("g()\ninside method, lambda"), 2);
    }

    #[test]
    fn inside_tells_anonymous_classes_from_named_ones() {
        let src = "class A { void m() { new R() { void run() { g(); } int x = g(); }; } }";
        assert_eq!(run("g()\ninside anonymous", src, &NoTypes).len(), 1, "the field of the anonymous class");
        assert_eq!(run("g()\ninside method", src, &NoTypes).len(), 1, "the method of the anonymous class");
    }

    /// The appended `;` must stay outside the matched range, or the pattern would match the
    /// STATEMENT and never a call nested inside a larger expression.
    #[test]
    fn an_expression_pattern_matches_a_call_that_is_not_a_statement() {
        let src = "class A { void m() { int n = list.size(); } }";
        let hits = run("$o$.size()", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].preview, "list.size()", "the call, not the declaration around it");
    }

    #[test]
    fn a_pattern_finds_what_it_names_and_leaves_the_rest() {
        let src = "class A { void m() { log.debug(\"a\" + x); log.info(\"b\"); } }";
        let hits = run("log.debug($x$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].preview, "log.debug(\"a\" + x)");
        assert_eq!(hits[0].capture("x").map(|c| c.text.as_str()), Some("\"a\" + x"));
    }

    /// Structural, not textual: the whitespace and the line break are not nodes.
    #[test]
    fn formatting_between_the_tokens_does_not_matter() {
        let src = "class A { void m() {\n  log\n     .debug(  \"a\"  );\n} }";
        assert_eq!(run("log.debug($x$)", src, &NoTypes).len(), 1);
    }

    #[test]
    fn a_run_captures_the_original_bytes_separators_and_all() {
        let src = "class A { void m() { f(1, 2, 3); } }";
        let hits = run("f($args...$)", src, &NoTypes);
        assert_eq!(hits[0].capture("args").map(|c| c.text.as_str()), Some("1, 2, 3"));
    }

    // ── or, and the double-count it would otherwise cause ───────────────────────

    #[test]
    fn or_finds_both_shapes() {
        let src = "class A { void m() { svc.place(o); Runnable r = svc::cancel; } }";
        let hits = run("$o$.$m$($a...$)\nor $o$::$m$\ngroup $m$", src, &NoTypes);
        let mut names: Vec<&str> =
            hits.iter().filter_map(|h| h.capture("m")).map(|c| c.text.as_str()).collect();
        names.sort();
        assert_eq!(names, ["cancel", "place"]);
    }

    /// The trap: two alternatives that both describe the same bytes must produce ONE hit, or
    /// every count built on them is quietly inflated.
    #[test]
    fn two_alternatives_matching_the_same_bytes_count_once() {
        let src = "class A { void m() { stream.close(); } }";
        let hits = run("$o$.$m$()\nor $o$.close()", src, &NoTypes);
        assert_eq!(hits.len(), 1, "one place in the file is one hit");
    }

    // ── constraints ─────────────────────────────────────────────────────────────

    #[test]
    fn a_text_glob_narrows_by_name() {
        let src = "class A { void m() { a.getName(); a.setName(x); a.getAge(); } }";
        let hits = run("$o$.$m: ~get*$($a...$)", src, &NoTypes);
        let names: Vec<&str> =
            hits.iter().filter_map(|h| h.capture("m")).map(|c| c.text.as_str()).collect();
        assert_eq!(names, ["getName", "getAge"]);
    }

    #[test]
    fn an_alternation_glob_takes_either() {
        let src = "class A { void m() { a.place(); a.cancel(); a.other(); } }";
        assert_eq!(run("$o$.$m: ~place|cancel$()", src, &NoTypes).len(), 2);
    }

    #[test]
    fn a_negated_constraint_takes_the_complement() {
        let src = "class A { void m() { a.place(); a.cancel(); } }";
        let hits = run("$o$.$m: !~place$()", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture("m").map(|c| c.text.as_str()), Some("cancel"));
    }

    #[test]
    fn a_kind_constraint_asks_the_grammar() {
        let src = "class A { void m() { f(\"lit\"); f(x); } }";
        let hits = run("f($a: #string_literal$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture("a").map(|c| c.text.as_str()), Some("\"lit\""));
    }

    // ── types, and the honesty about not knowing ────────────────────────────────

    struct Fake(&'static str);
    impl TypeOracle for Fake {
        fn denotation_at(&self, _f: &str, source: &str, range: ByteRange) -> Option<Denotation> {
            // Only the receiver we planted resolves; everything else is genuinely unknown, which
            // is the case that matters. `svc` is a variable, `Svc` is the class — the pair the
            // whole denotation axis exists for.
            match range.slice(source) {
                Some("svc") => Some(Denotation::Value(self.0.to_string())),
                Some("Svc") => Some(Denotation::Type(self.0.to_string())),
                _ => None,
            }
        }
        fn is_subtype_of(&self, candidate: &str, wanted: &str) -> bool {
            candidate == wanted || wanted == "com.acme.Service"
        }
    }

    /// The constraint decides the receiver it can decide, and says so about the one it cannot.
    ///
    /// `other` is not a *non*-match — the oracle answered `None`, which is "I do not know" and
    /// never "no". So it is kept and flagged, exactly like
    /// [`a_type_that_cannot_be_resolved_is_flagged_rather_than_dropped`] says. Asserting one hit
    /// here asserted the opposite of the crate's contract.
    #[test]
    fn a_type_constraint_keeps_the_receiver_it_names() {
        let src = "class A { void m() { svc.place(o); other.place(o); } }";
        let hits = run("$o: com.acme.OrderService$.place($a$)", src, &Fake("com.acme.OrderService"));
        assert_eq!(hits.len(), 2, "the one it named, and the one it could not decide");
        assert_eq!(hits[0].capture("o").map(|c| c.text.as_str()), Some("svc"));
        assert!(!hits[0].unresolved, "decided, and it is the type asked for");
        assert_eq!(hits[1].capture("o").map(|c| c.text.as_str()), Some("other"));
        assert!(hits[1].unresolved, "undecided — kept and marked, never silently dropped");
    }

    #[test]
    fn a_type_written_unqualified_still_matches() {
        let src = "class A { void m() { svc.place(o); } }";
        assert_eq!(run("$o: OrderService$.place($a$)", src, &Fake("com.acme.OrderService")).len(), 1);
    }

    #[test]
    fn a_subtype_constraint_walks_the_hierarchy() {
        let src = "class A { void m() { svc.place(o); } }";
        assert_eq!(run("$o: com.acme.Service+$.place($a$)", src, &Fake("com.acme.OrderService")).len(), 1);
    }

    /// The one that keeps a count honest on a legacy project: a receiver whose type the
    /// classpath cannot reach is **kept and flagged**, not dropped. Dropping it would make the
    /// total look complete while being short by however much did not resolve.
    #[test]
    fn a_type_that_cannot_be_resolved_is_flagged_rather_than_dropped() {
        let src = "class A { void m() { mystery.place(o); } }";
        let hits = run("$o: com.acme.OrderService$.place($a$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].unresolved, "kept, and marked as undecided");
    }

    /// "Not something I could not determine" is not a fact. Negation must not turn an unknown
    /// into a yes — that is how an incomplete classpath starts inventing hits.
    #[test]
    fn negating_an_unknown_stays_unknown() {
        let src = "class A { void m() { mystery.place(o); } }";
        let hits = run("$o: !com.acme.OrderService$.place($a$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].unresolved);
    }

    // ── what a name denotes ─────────────────────────────────────────────────────

    /// **The distinction the syntax cannot make.** Both lines are `method_invocation` with an
    /// `identifier` object; only the resolver knows one is a class and the other a variable.
    #[test]
    fn a_denotation_constraint_separates_a_static_call_from_an_instance_one() {
        let src = "class A { void m() { svc.place(o); Svc.place(o); } }";
        let statics = run("$o: @type$.place($a$)", src, &Fake("com.acme.OrderService"));
        assert_eq!(statics.len(), 1);
        assert_eq!(statics[0].capture("o").map(|c| c.text.as_str()), Some("Svc"));

        let instances = run("$o: @value$.place($a$)", src, &Fake("com.acme.OrderService"));
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].capture("o").map(|c| c.text.as_str()), Some("svc"));
    }

    /// A plain type constraint stays blind to the distinction: in both lines the type in play is
    /// `OrderService`, and quietly dropping one of them would make `$o: OrderService$` a
    /// different question than it reads as.
    #[test]
    fn a_plain_type_constraint_admits_both_readings() {
        let src = "class A { void m() { svc.place(o); Svc.place(o); } }";
        assert_eq!(run("$o: com.acme.OrderService$.place($a$)", src, &Fake("com.acme.OrderService")).len(), 2);
    }

    #[test]
    fn a_conjunction_narrows_to_one_type_and_one_denotation() {
        let src = "class A { void m() { svc.place(o); Svc.place(o); } }";
        let hits = run("$o: @type & OrderService$.place($a$)", src, &Fake("com.acme.OrderService"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].capture("o").map(|c| c.text.as_str()), Some("Svc"));
    }

    /// A conjunction is only as decided as its least decided part — a hit whose type could not be
    /// read must not be reported as if it had been.
    #[test]
    fn a_conjunction_with_an_unresolvable_part_stays_undecided() {
        let src = "class A { void m() { mystery.place(o); } }";
        let hits = run("$o: @value & com.acme.OrderService$.place($a$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].unresolved);
    }

    /// The same honesty the type constraint has: a receiver nobody can resolve is neither a type
    /// nor a value, and answering "not a type" would invent a fact.
    #[test]
    fn an_unresolvable_receiver_is_undecided_rather_than_a_value() {
        let src = "class A { void m() { mystery.place(o); } }";
        let hits = run("$o: @type$.place($a$)", src, &NoTypes);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].unresolved);
    }

    // ── enclosing ───────────────────────────────────────────────────────────────

    #[test]
    fn enclosing_names_the_method_and_its_class() {
        let src = "class OrderDao { void findAll() { log.debug(\"x\"); } }";
        let hits = run("log.debug($x$)\ngroup enclosing", src, &NoTypes);
        assert_eq!(hits[0].enclosing.as_deref(), Some("OrderDao.findAll"));
    }

    #[test]
    fn enclosing_falls_back_to_the_class_outside_a_method() {
        let src = "class OrderDao { String s = f(\"x\"); }";
        let hits = run("f($x$)\ngroup enclosing", src, &NoTypes);
        assert_eq!(hits[0].enclosing.as_deref(), Some("OrderDao"));
    }

    /// It costs a second parse, so it must not happen for the queries that do not ask.
    #[test]
    fn a_query_that_needs_no_tree_says_so() {
        assert!(!needs_tree(&parse_query("log.debug($x$)").unwrap()));
        assert!(needs_tree(&parse_query("log.debug($x$)\ngroup enclosing").unwrap()));
        assert!(needs_tree(&parse_query("f($a: #string_literal$)").unwrap()));
        // ...including when the kind is buried in a conjunction, which is where it is easy to
        // stop looking — and a missing tree makes a `#kind` silently undecided.
        assert!(needs_tree(&parse_query("f($a: @value & #identifier$)").unwrap()));
    }

    // ── the glob ────────────────────────────────────────────────────────────────

    #[test]
    fn the_glob_anchors_and_alternates() {
        assert!(glob_matches("get*", "getName"));
        assert!(!glob_matches("get*", "forgetName"));
        assert!(glob_matches("*Dao", "OrderDao"));
        assert!(glob_matches("*Order*", "com.acme.OrderService"));
        assert!(glob_matches("place|cancel", "cancel"));
        assert!(!glob_matches("place|cancel", "cancelled"));
        assert!(glob_matches("exact", "exact"));
        assert!(!glob_matches("exact", "exactly"));
    }

    #[test]
    fn a_preview_is_one_bounded_line() {
        let src = "class A { void m() { f(\n   1,\n   2\n); } }";
        let hits = run("f($a...$)", src, &NoTypes);
        assert_eq!(hits[0].preview, "f( 1, 2 )");
    }
}
