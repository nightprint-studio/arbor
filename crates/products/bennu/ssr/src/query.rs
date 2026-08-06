//! The query language: what you type, and what it parses to.
//!
//! ## The shape
//!
//! ```text
//! <pattern>
//! or <pattern>          zero or more
//! in <scope>[, <scope>] optional
//! group <key>           optional
//! ```
//!
//! A pattern is **source text with holes in it** — see [`arbor_syntax::prelude::Pattern`]. Its
//! placeholders may carry a constraint after a colon:
//!
//! ```text
//! $x$                   one node, no constraint
//! $xs...$               a run of consecutive siblings, possibly empty
//! $x: com.acme.Order$   the node's static type IS that
//! $x: Order+$           ...or a subtype of it
//! $x: *Dao$             a glob on the simple type name
//! $x: #string_literal$  a node of the GRAMMAR's kind, not a type
//! $x: ~get.*$           a regex on the node's own text
//! $x: @type$            the node NAMES a type — a static access
//! $x: @value$           the node names a value — an instance access
//! $x: !equals$          the negation of any of the above
//! $x: @type & Files$    all of them at once
//! ```
//!
//! ## Why `@type` / `@value` exists
//!
//! `foo.bar()` and `Foo.bar()` are the same shape: tree-sitter reads both as a call whose object
//! is an `identifier`, and no pattern can tell them apart, because the difference is not in the
//! syntax — it is in what the name **denotes**. Answering it needs the resolver, which is exactly
//! what a [`crate::TypeOracle`] is for, so it is a constraint like any other and it is honest
//! about not knowing: an unresolvable receiver comes back undecided, never as a "no".
//!
//! ## Why `&`
//!
//! Once a placeholder can be constrained in five ways, wanting two of them at once stops being
//! exotic — "a static access **on `Files`**" is `@type & Files`, and the alternative would have
//! been a second placeholder for the same node. `&` binds looser than `!`, so `!a & b` reads as
//! "not a, and b"; there are no parentheses, because a constraint that needed them would be a
//! query better written as two alternatives.
//!
//! ## Why the clauses are keywords on their own lines
//!
//! `or`, `in` and `group` are the three words that begin a clause, and Java has no construct
//! that begins with any of them — so a line starting with one is never a pattern, and the split
//! needs no punctuation the user has to remember. That is the whole parser: split the lines,
//! peel the clauses, everything left is the pattern.
//!
//! ## `use of` is a different question
//!
//! `use of $m$ on <Type>` is not a pattern. Java spells "a use of a method" six ways — a call, a
//! call through `this`, a bare call inside the class, a `super.` call, and two shapes of method
//! reference — and asking someone to enumerate them is asking them to undercount, which is the
//! worst way to be wrong because it looks like an answer. So it is its own query kind, answered
//! by the resolver rather than by the pattern engine, and the panel shows what it expands to.

use std::fmt;

/// What a name **denotes** — the distinction the syntax does not carry.
///
/// `orders.total()` and `Orders.total()` parse identically. Which one you are looking at depends
/// on whether `Orders` resolves to a variable in scope or to a class, and only the resolver can
/// say. See the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denotes {
    /// The node names a type — `Files.copy(…)`, `Order.SIZE`. A **static** access.
    Type,
    /// The node names a value — a local, a parameter, a field. An **instance** access.
    Value,
}

impl fmt::Display for Denotes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Denotes::Type => "@type",
            Denotes::Value => "@value",
        })
    }
}

/// What a placeholder's `:` constraint demands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// `#string_literal` — the tree-sitter node kind. The grammar's vocabulary, not Java's.
    Kind(String),
    /// `com.acme.Order` / `Order` / `*Dao` — the node's static type. `subtypes` is the `+`.
    /// Resolving this needs a [`crate::TypeOracle`]; without one it never matches.
    ///
    /// Deliberately blind to [`Denotes`]: `$x: Order$` admits both `order.f()` and `Order.f()`,
    /// because in both the type in play is `Order`. Narrow it with `@value & Order`.
    Type { name: String, subtypes: bool },
    /// `~get.*` — a regex over the node's own text. Deliberately over the TEXT: a regex over the
    /// structure would be the second syntax this language exists to avoid.
    Text(String),
    /// `@type` / `@value` — what the node names. Needs a [`crate::TypeOracle`].
    Denotes(Denotes),
    /// `!<inner>` — the negation of any of the above.
    Not(Box<Constraint>),
    /// `a & b` — every one of them. Never nested by the parser (`&` is split once, at the top),
    /// but the type allows it so a caller building a constraint by hand cannot hit a wall.
    All(Vec<Constraint>),
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Kind(k) => write!(f, "#{k}"),
            Constraint::Type { name, subtypes } => {
                write!(f, "{name}{}", if *subtypes { "+" } else { "" })
            }
            Constraint::Text(r) => write!(f, "~{r}"),
            Constraint::Denotes(d) => write!(f, "{d}"),
            Constraint::Not(inner) => write!(f, "!{inner}"),
            Constraint::All(parts) => {
                let joined: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
                f.write_str(&joined.join(" & "))
            }
        }
    }
}

/// A named constraint, lifted out of the pattern text before it is compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedConstraint {
    pub name: String,
    pub constraint: Constraint,
}

/// What to count by. Four, and deliberately not more — each is a question people actually
/// arrive with, and a fifth would be a menu rather than an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupBy {
    /// `group $m$` — by what a capture matched. "Which methods, and how often each."
    Capture(String),
    /// `group file`.
    File,
    /// `group module` — the Maven module a file belongs to.
    Module,
    /// `group enclosing` — the method or class the match sits **inside**.
    ///
    /// The one that cannot be expressed by capturing, and the reason it exists: the enclosing
    /// declaration is not part of the pattern, so there is nothing to name — yet "which of MY
    /// methods use this" is the question asked most often.
    Enclosing,
}

impl fmt::Display for GroupBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupBy::Capture(n) => write!(f, "${n}$"),
            GroupBy::File => write!(f, "file"),
            GroupBy::Module => write!(f, "module"),
            GroupBy::Enclosing => write!(f, "enclosing"),
        }
    }
}

/// One alternative: the pattern text (placeholders stripped of their constraints, ready for
/// [`arbor_syntax`]) plus the constraints that were on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    /// The pattern as `arbor-syntax` will compile it — `$x$`, `$xs...$`, nothing else.
    pub pattern: String,
    /// The constraints peeled off, by capture name.
    pub constraints: Vec<NamedConstraint>,
}

impl Alternative {
    pub fn constraint(&self, name: &str) -> Option<&Constraint> {
        self.constraints.iter().find(|c| c.name == name).map(|c| &c.constraint)
    }
}

/// What a query asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// One or more patterns, matched syntactically. More than one means `or`.
    Patterns(Vec<Alternative>),
    /// `use of $m$ on <Type>` — every *use* of a member, in every shape the language spells it.
    /// Answered by the resolver, not by the pattern engine.
    UseOf {
        /// The member name, or `None` for `$m$` (any member — "which of its methods are used").
        member: Option<String>,
        /// The name the capture takes in `group $m$`.
        member_capture: String,
        /// The owning type, as written.
        owner: String,
        /// `+` — include uses through a subtype.
        subtypes: bool,
    },
}

/// A parsed query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub ask: Ask,
    /// `in a, b` — path prefixes, project-relative and forward-slashed. Empty is "everywhere".
    pub scopes: Vec<String>,
    pub group: Option<GroupBy>,
}

/// Why a query could not be read. Carries the line so the editor can point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub message: String,
    /// 1-based line of the query text, or 0 when it is about the query as a whole.
    pub line: usize,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn err(message: impl Into<String>, line: usize) -> QueryError {
    QueryError { message: message.into(), line }
}

/// Parse a query.
///
/// Line-oriented: a line whose first word is a clause keyword is that clause, everything else
/// belongs to the pattern being built. Blank lines and `--` comments are dropped, so a saved
/// query can carry a note about what it is for.
pub fn parse(text: &str) -> Result<Query, QueryError> {
    let mut alternatives: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut scopes: Vec<String> = Vec::new();
    let mut group: Option<GroupBy> = None;
    let mut use_of: Option<(Ask, usize)> = None;
    let mut group_line = 0usize;

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }

        match clause_of(trimmed) {
            Some(("or", rest)) => {
                if !current.trim().is_empty() {
                    alternatives.push(std::mem::take(&mut current));
                }
                current.push_str(rest);
                current.push('\n');
            }
            Some(("in", rest)) => {
                for scope in rest.split(',') {
                    let scope = scope.trim().trim_matches('"').replace('\\', "/");
                    if !scope.is_empty() {
                        scopes.push(scope);
                    }
                }
            }
            Some(("group", rest)) => {
                if group.is_some() {
                    return Err(err("a query groups by one thing, not two", line));
                }
                group = Some(parse_group(rest.trim(), line)?);
                group_line = line;
            }
            Some(("use", rest)) => {
                use_of = Some((parse_use_of(rest.trim(), line)?, line));
            }
            _ => {
                current.push_str(trimmed);
                current.push('\n');
            }
        }
    }

    if !current.trim().is_empty() {
        alternatives.push(current);
    }

    let ask = match use_of {
        Some((ask, line)) => {
            if !alternatives.is_empty() {
                return Err(err(
                    "`use of` is a whole query on its own — it already covers every shape a use \
                     takes, so there is nothing for a pattern beside it to add",
                    line,
                ));
            }
            ask
        }
        None => {
            if alternatives.is_empty() {
                return Err(err(
                    "write the code to look for, with $name$ where it may differ",
                    0,
                ));
            }
            Ask::Patterns(
                alternatives.iter().map(|a| peel(a)).collect::<Result<Vec<_>, _>>()?,
            )
        }
    };

    let query = Query { ask, scopes, group };
    check_group(&query, group_line)?;
    Ok(query)
}

/// `("or", "the rest")` when `line` begins with a clause keyword followed by a space.
///
/// The space matters: `orders.total()` is a pattern, `or ders.total()` is a clause. Java has no
/// construct beginning with a bare `or` / `in` / `group` / `use`, so this can never misread real
/// code as a clause.
fn clause_of(line: &str) -> Option<(&'static str, &str)> {
    for word in ["or", "in", "group", "use"] {
        if let Some(rest) = line.strip_prefix(word) {
            if rest.starts_with(char::is_whitespace) {
                return Some((word, rest.trim_start()));
            }
        }
    }
    None
}

fn parse_group(rest: &str, line: usize) -> Result<GroupBy, QueryError> {
    match rest {
        "file" => Ok(GroupBy::File),
        "module" => Ok(GroupBy::Module),
        "enclosing" => Ok(GroupBy::Enclosing),
        other => {
            let name = other.trim().trim_start_matches('$').trim_end_matches('$');
            if name.is_empty() || !is_name(name) {
                return Err(err(
                    format!(
                        "group by a capture (`group $m$`) or by `file`, `module` or \
                         `enclosing` — `{other}` is none of those"
                    ),
                    line,
                ));
            }
            Ok(GroupBy::Capture(name.to_string()))
        }
    }
}

/// `use of $m$ on com.acme.OrderService+` → the [`Ask::UseOf`].
fn parse_use_of(rest: &str, line: usize) -> Result<Ask, QueryError> {
    let body = rest.strip_prefix("of ").ok_or_else(|| {
        err("write `use of $m$ on <Type>` — the `of` is what says which member", line)
    })?;
    let (member_part, owner_part) = body.split_once(" on ").ok_or_else(|| {
        err("write `use of $m$ on <Type>` — the `on` is what says which class", line)
    })?;

    let member_text = member_part.trim();
    let (member, member_capture) = if let Some(name) = placeholder_name(member_text) {
        // `$m$` — any member, captured under that name so `group $m$` can count them.
        (None, name)
    } else if is_name(member_text) {
        (Some(member_text.to_string()), "m".to_string())
    } else {
        return Err(err(
            format!("`{member_text}` is neither a member name nor a $capture$"),
            line,
        ));
    };

    let owner_text = owner_part.trim();
    let subtypes = owner_text.ends_with('+');
    let owner = owner_text.trim_end_matches('+').trim().to_string();
    if owner.is_empty() {
        return Err(err("`on` needs a type — the class whose member you are counting", line));
    }
    Ok(Ask::UseOf { member, member_capture, owner, subtypes })
}

/// `$m$` → `m`.
fn placeholder_name(text: &str) -> Option<String> {
    let inner = text.strip_prefix('$')?.strip_suffix('$')?;
    is_name(inner).then(|| inner.to_string())
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// **The rule that makes `or` safe with `group`.**
///
/// A capture used to group must be bound by *every* alternative, or the table would have rows
/// with nothing in the column — and a hole in an aggregate reads as "none" rather than as "this
/// branch cannot answer". Caught here, when the query is read, rather than shown as a gap.
fn check_group(query: &Query, line: usize) -> Result<(), QueryError> {
    let Some(GroupBy::Capture(name)) = &query.group else { return Ok(()) };
    match &query.ask {
        Ask::UseOf { member_capture, .. } => {
            if member_capture != name {
                return Err(err(
                    format!("`use of` binds only ${member_capture}$, so it cannot group by ${name}$"),
                    line,
                ));
            }
        }
        Ask::Patterns(alts) => {
            for (i, alt) in alts.iter().enumerate() {
                if !binds(&alt.pattern, name) {
                    return Err(err(
                        format!(
                            "every alternative has to bind ${name}$ to group by it — the {} one \
                             does not",
                            ordinal(i)
                        ),
                        line,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn ordinal(i: usize) -> &'static str {
    match i {
        0 => "first",
        1 => "second",
        2 => "third",
        _ => "last",
    }
}

/// Whether `pattern` contains `$name$` or `$name...$`.
fn binds(pattern: &str, name: &str) -> bool {
    let one = format!("${name}$");
    let many = format!("${name}...$");
    pattern.contains(&one) || pattern.contains(&many)
}

/// Split a raw alternative into the pattern `arbor-syntax` can compile and the constraints that
/// were written on its placeholders.
///
/// `$x: com.acme.Order$` becomes `$x$` plus `x → Type(com.acme.Order)`. The engine below never
/// sees a constraint inside a pattern, and the pattern crate never has to learn about types.
fn peel(raw: &str) -> Result<Alternative, QueryError> {
    let mut pattern = String::with_capacity(raw.len());
    let mut constraints = Vec::new();
    let bytes: Vec<char> = raw.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != '$' {
            pattern.push(bytes[i]);
            i += 1;
            continue;
        }
        // A placeholder runs to the next `$`. An unclosed one is the user mid-typing, not an
        // error worth stopping on — it is left as written and the pattern compiler reports it.
        let Some(close) = (i + 1..bytes.len()).find(|&j| bytes[j] == '$') else {
            pattern.extend(&bytes[i..]);
            break;
        };
        let inner: String = bytes[i + 1..close].iter().collect();
        match inner.split_once(':') {
            Some((name_part, constraint_part)) => {
                let name = name_part.trim();
                let many = name.ends_with("...");
                let bare = name.trim_end_matches("...").trim();
                if !is_name(bare) {
                    return Err(err(format!("`{bare}` is not a usable capture name"), 0));
                }
                constraints.push(NamedConstraint {
                    name: bare.to_string(),
                    constraint: parse_constraint(constraint_part.trim())?,
                });
                pattern.push('$');
                pattern.push_str(bare);
                if many {
                    pattern.push_str("...");
                }
                pattern.push('$');
            }
            None => {
                pattern.push('$');
                pattern.push_str(&inner);
                pattern.push('$');
            }
        }
        i = close + 1;
    }

    Ok(Alternative { pattern, constraints })
}

/// Read a constraint, splitting a conjunction first.
///
/// `&` is peeled at the top and once only, which is what makes it bind looser than `!`: each side
/// is then an ordinary constraint, and `!a & b` is "not a, and b" rather than "not (a and b)".
fn parse_constraint(text: &str) -> Result<Constraint, QueryError> {
    if text.contains('&') {
        let parts: Vec<&str> = text.split('&').map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(err("a `&` joins two constraints — there is nothing on either side", 0));
        }
        if parts.len() > 1 {
            return Ok(Constraint::All(
                parts.iter().map(|p| one_constraint(p)).collect::<Result<Vec<_>, _>>()?,
            ));
        }
        return one_constraint(parts[0]);
    }
    one_constraint(text)
}

fn one_constraint(text: &str) -> Result<Constraint, QueryError> {
    if let Some(rest) = text.strip_prefix('!') {
        return Ok(Constraint::Not(Box::new(one_constraint(rest.trim())?)));
    }
    if let Some(word) = text.strip_prefix('@') {
        return match word.trim() {
            "type" => Ok(Constraint::Denotes(Denotes::Type)),
            "value" => Ok(Constraint::Denotes(Denotes::Value)),
            other => Err(err(
                format!(
                    "`@{other}` is not a denotation — `@type` for a static access, `@value` for \
                     an instance one"
                ),
                0,
            )),
        };
    }
    if let Some(kind) = text.strip_prefix('#') {
        let kind = kind.trim();
        if kind.is_empty() {
            return Err(err("`#` needs the name of a grammar node, like `#string_literal`", 0));
        }
        return Ok(Constraint::Kind(kind.to_string()));
    }
    if let Some(regex) = text.strip_prefix('~') {
        let regex = regex.trim();
        if regex.is_empty() {
            return Err(err("`~` needs a pattern to match the node's text against", 0));
        }
        return Ok(Constraint::Text(regex.to_string()));
    }
    if text.is_empty() {
        return Err(err("a `:` needs a constraint after it", 0));
    }
    let subtypes = text.ends_with('+');
    Ok(Constraint::Type {
        name: text.trim_end_matches('+').trim().to_string(),
        subtypes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> Query {
        parse(text).expect("parses")
    }

    fn patterns(query: &Query) -> Vec<String> {
        match &query.ask {
            Ask::Patterns(alts) => alts.iter().map(|a| a.pattern.trim().to_string()).collect(),
            _ => panic!("not a pattern query"),
        }
    }

    #[test]
    fn a_bare_pattern_is_a_query() {
        let query = q("log.debug($x$)");
        assert_eq!(patterns(&query), ["log.debug($x$)"]);
        assert!(query.scopes.is_empty());
        assert!(query.group.is_none());
    }

    #[test]
    fn or_starts_a_second_alternative() {
        let query = q("$o$.$m$($a...$)\nor $o$::$m$\ngroup $m$");
        assert_eq!(patterns(&query), ["$o$.$m$($a...$)", "$o$::$m$"]);
        assert_eq!(query.group, Some(GroupBy::Capture("m".into())));
    }

    /// The point of the keyword rule: real code that merely *starts* with those letters is not
    /// a clause. `orders`, `input`, `grouping` — all ordinary receivers.
    #[test]
    fn a_word_that_only_begins_with_a_keyword_is_still_code() {
        assert_eq!(patterns(&q("orders.total()")), ["orders.total()"]);
        assert_eq!(patterns(&q("input.read()")), ["input.read()"]);
        assert_eq!(patterns(&q("grouping.apply()")), ["grouping.apply()"]);
        assert_eq!(patterns(&q("used.clear()")), ["used.clear()"]);
    }

    #[test]
    fn a_multiline_pattern_stays_one_alternative() {
        let query = q("class $c$ extends $b$ {\n  $body...$\n}");
        assert_eq!(patterns(&query).len(), 1);
        assert!(patterns(&query)[0].contains("$body...$"));
    }

    #[test]
    fn scopes_are_a_list_and_slashes_are_normalised() {
        let query = q("$x$.close()\nin modules/core, modules\\web");
        assert_eq!(query.scopes, ["modules/core", "modules/web"]);
    }

    #[test]
    fn comments_and_blank_lines_are_not_the_pattern() {
        let query = q("-- every JDBC statement\n\n$c$.createStatement()\n");
        assert_eq!(patterns(&query), ["$c$.createStatement()"]);
    }

    // ── constraints ─────────────────────────────────────────────────────────────

    #[test]
    fn a_constraint_is_peeled_off_so_the_pattern_stays_compilable() {
        let query = q("$o: com.acme.OrderService$.$m$($a...$)");
        // What the pattern engine sees has no constraint in it at all.
        assert_eq!(patterns(&query), ["$o$.$m$($a...$)"]);
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        assert_eq!(
            alts[0].constraint("o"),
            Some(&Constraint::Type { name: "com.acme.OrderService".into(), subtypes: false }),
        );
    }

    #[test]
    fn every_constraint_form_reads_back() {
        let query = q("f($a: Order+$, $b: #string_literal$, $c: ~get.*$, $d: !equals$)");
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        let alt = &alts[0];
        assert_eq!(
            alt.constraint("a"),
            Some(&Constraint::Type { name: "Order".into(), subtypes: true }),
        );
        assert_eq!(alt.constraint("b"), Some(&Constraint::Kind("string_literal".into())));
        assert_eq!(alt.constraint("c"), Some(&Constraint::Text("get.*".into())));
        assert_eq!(
            alt.constraint("d"),
            Some(&Constraint::Not(Box::new(Constraint::Type {
                name: "equals".into(),
                subtypes: false
            }))),
        );
        assert_eq!(patterns(&query), ["f($a$, $b$, $c$, $d$)"]);
    }

    /// The whole point of the axis: the same pattern, narrowed to one side of a distinction the
    /// syntax does not carry.
    #[test]
    fn a_denotation_reads_both_ways() {
        let query = q("$o: @type$.$m$($a...$)");
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        assert_eq!(alts[0].constraint("o"), Some(&Constraint::Denotes(Denotes::Type)));
        assert_eq!(patterns(&query), ["$o$.$m$($a...$)"], "the pattern is untouched");

        let query = q("$o: @value$.close()");
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        assert_eq!(alts[0].constraint("o"), Some(&Constraint::Denotes(Denotes::Value)));
    }

    #[test]
    fn a_misspelt_denotation_says_which_two_there_are() {
        let e = parse("$o: @static$.f()").expect_err("refused");
        assert!(e.message.contains("@type"), "{}", e.message);
        assert!(e.message.contains("@value"), "{}", e.message);
    }

    /// `&` is the whole reason the axis is usable: "a static call **on Files**" is one
    /// placeholder, not two.
    #[test]
    fn an_ampersand_asks_for_both() {
        let query = q("$o: @type & java.nio.file.Files$.$m$($a...$)");
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        assert_eq!(
            alts[0].constraint("o"),
            Some(&Constraint::All(vec![
                Constraint::Denotes(Denotes::Type),
                Constraint::Type { name: "java.nio.file.Files".into(), subtypes: false },
            ])),
        );
    }

    /// `&` binds looser than `!`, so this is "not a getter, and a value" — not "not (a getter and
    /// a value)", which would be a different and much less useful question.
    #[test]
    fn negation_binds_tighter_than_the_conjunction() {
        let query = q("$o: !~get* & @value$.$m$()");
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        let Some(Constraint::All(parts)) = alts[0].constraint("o") else { panic!("a conjunction") };
        assert!(matches!(parts[0], Constraint::Not(_)));
        assert_eq!(parts[1], Constraint::Denotes(Denotes::Value));
    }

    /// It has to read back as what was written, or the panel's echo of a query would be a
    /// different query.
    #[test]
    fn a_conjunction_prints_as_it_was_written() {
        let c = one_constraint("@type").unwrap();
        assert_eq!(c.to_string(), "@type");
        assert_eq!(parse_constraint("@value & Order+").unwrap().to_string(), "@value & Order+");
        assert_eq!(parse_constraint("!#string_literal").unwrap().to_string(), "!#string_literal");
    }

    #[test]
    fn a_constrained_run_keeps_its_ellipsis() {
        let query = q("f($xs...: #string_literal$)");
        assert_eq!(patterns(&query), ["f($xs...$)"]);
        let Ask::Patterns(alts) = &query.ask else { panic!() };
        assert!(alts[0].constraint("xs").is_some());
    }

    // ── group, and the rule that makes `or` safe ────────────────────────────────

    #[test]
    fn grouping_by_a_capture_one_branch_does_not_bind_is_refused() {
        let e = parse("$o$.$m$()\nor $o$.close()\ngroup $m$").expect_err("refused");
        assert!(e.message.contains("second"), "it says WHICH branch: {}", e.message);
    }

    #[test]
    fn grouping_by_a_capture_every_branch_binds_is_accepted() {
        assert!(parse("$o$.$m$()\nor $o$::$m$\ngroup $m$").is_ok());
    }

    #[test]
    fn the_fixed_group_keys_parse() {
        for (text, want) in [
            ("file", GroupBy::File),
            ("module", GroupBy::Module),
            ("enclosing", GroupBy::Enclosing),
        ] {
            assert_eq!(q(&format!("$x$.f()\ngroup {text}")).group, Some(want));
        }
    }

    #[test]
    fn grouping_by_something_that_is_neither_says_so() {
        let e = parse("$x$.f()\ngroup nonsense").expect_err("refused");
        assert!(e.message.contains("enclosing"), "it lists the options: {}", e.message);
    }

    #[test]
    fn two_groups_are_refused_rather_than_the_last_one_winning() {
        assert!(parse("$x$.f()\ngroup file\ngroup module").is_err());
    }

    #[test]
    fn an_empty_query_asks_for_the_pattern() {
        let e = parse("\n-- nothing here\n").expect_err("refused");
        assert!(e.message.contains("$name$"));
    }

    // ── use of ──────────────────────────────────────────────────────────────────

    #[test]
    fn use_of_a_capture_counts_every_member() {
        let query = q("use of $m$ on com.acme.OrderService+\ngroup $m$");
        assert_eq!(
            query.ask,
            Ask::UseOf {
                member: None,
                member_capture: "m".into(),
                owner: "com.acme.OrderService".into(),
                subtypes: true,
            },
        );
    }

    #[test]
    fn use_of_a_named_member_pins_it() {
        let query = q("use of place on com.acme.OrderService");
        let Ask::UseOf { member, owner, subtypes, .. } = query.ask else { panic!() };
        assert_eq!(member.as_deref(), Some("place"));
        assert_eq!(owner, "com.acme.OrderService");
        assert!(!subtypes);
    }

    #[test]
    fn use_of_without_its_on_says_what_is_missing() {
        let e = parse("use of place").expect_err("refused");
        assert!(e.message.contains("on"), "{}", e.message);
    }

    /// `use of` already covers every shape a use takes; a pattern beside it would be a second,
    /// overlapping answer to the same question — and the counts would double.
    #[test]
    fn use_of_beside_a_pattern_is_refused() {
        let e = parse("$x$.f()\nuse of f on com.acme.X").expect_err("refused");
        assert!(e.message.contains("on its own"), "{}", e.message);
    }
}
