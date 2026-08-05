//! The extension point: what a rule is, and how a set of them is assembled.
//!
//! A rule is asked one question — *does something start here?* — at every **token start** of
//! a line (the beginning, or just after a boundary character). It answers with a [`Hit`]
//! saying how far it consumed and which parts of that it wants annotated, or `None`.
//!
//! Rules are tried in order and the **first hit wins**, so a set is a priority list. That is
//! why [`RuleSet::java`] puts the stack-frame rule before the qualified-name rule: both
//! match at `com.acme.Foo`, and only one of them knows the frame it is part of.
//!
//! Adding one is [`RuleSet::with`] and a closure:
//!
//! ```
//! use arbor_logscan::prelude::*;
//!
//! let rules = RuleSet::common().with(FnRule::new("ticket", |text: &str, at: usize| {
//!     let end = text[at..].find(' ').map(|i| at + i).unwrap_or(text.len());
//!     let word = &text[at..end];
//!     (word.starts_with("JIRA-")).then(|| Hit::one(at, end, Token::Package))
//! }));
//! let line = interpret(&rules, "fixed in JIRA-4412 finally");
//! assert_eq!(line.spans.len(), 1);
//! ```

use std::sync::Arc;

use crate::model::{Link, Token};

/// One annotated range a rule produced. Parts of a single [`Hit`] need not be contiguous
/// (a stack frame annotates the method reference and the `(File.java:42)` and leaves the
/// `at ` between them plain) but must be in order and must not overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub start: usize,
    pub end: usize,
    pub token: Token,
    pub link: Option<Link>,
}

/// What a rule matched: how far it consumed, and what of that is worth annotating.
///
/// `end` is where scanning resumes, and it must be strictly greater than the position the
/// rule was asked about — a rule that consumes nothing is ignored rather than trusted to
/// terminate the scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub end: usize,
    pub parts: Vec<Part>,
}

impl Hit {
    /// A hit whose consumed range is exactly one annotated part.
    pub fn one(start: usize, end: usize, token: Token) -> Self {
        Hit { end, parts: vec![Part { start, end, token, link: None }] }
    }

    /// The same, with somewhere to go.
    pub fn linked(start: usize, end: usize, token: Token, link: Link) -> Self {
        Hit { end, parts: vec![Part { start, end, token, link: Some(link) }] }
    }

    /// An empty hit consuming up to `end`, for a rule that builds its parts up.
    pub fn spanning(end: usize) -> Self {
        Hit { end, parts: Vec::new() }
    }

    /// Add a part. Fluent, because the rules that need several read better as a chain.
    pub fn part(mut self, start: usize, end: usize, token: Token, link: Option<Link>) -> Self {
        self.parts.push(Part { start, end, token, link });
        self
    }
}

/// Something that recognises a construct at a token start.
///
/// `Send + Sync` because a [`RuleSet`] is shared: one set is typically built once and used
/// by every reader in the process, including readers on different threads (one per stream
/// of one process, say).
pub trait Rule: Send + Sync {
    /// For diagnostics — which rule claimed a piece of text.
    fn name(&self) -> &'static str;

    /// Does something start at `at`? `at` is always a char boundary and always a token
    /// start. Implementations may slice `text` from it freely.
    fn match_at(&self, text: &str, at: usize) -> Option<Hit>;
}

/// A rule from a function or closure — the ordinary way to write one.
pub struct FnRule<F> {
    name: &'static str,
    matcher: F,
}

impl<F> FnRule<F>
where
    F: Fn(&str, usize) -> Option<Hit> + Send + Sync,
{
    pub fn new(name: &'static str, matcher: F) -> Self {
        FnRule { name, matcher }
    }
}

impl<F> Rule for FnRule<F>
where
    F: Fn(&str, usize) -> Option<Hit> + Send + Sync,
{
    fn name(&self) -> &'static str {
        self.name
    }

    fn match_at(&self, text: &str, at: usize) -> Option<Hit> {
        (self.matcher)(text, at)
    }
}

/// An ordered set of rules, plus how to tell a continuation line.
///
/// Cheap to clone (the rules are shared), so a host holds one and hands it to as many
/// readers as it has streams.
#[derive(Clone)]
pub struct RuleSet {
    rules: Vec<Arc<dyn Rule>>,
    continues: fn(&str) -> bool,
}

impl RuleSet {
    /// No rules at all — ANSI is still honoured, nothing else is recognised. The base for a
    /// host that wants only its own rules.
    pub fn empty() -> Self {
        RuleSet { rules: Vec::new(), continues: indented }
    }

    /// Append a rule. Later rules are lower priority.
    pub fn with(mut self, rule: impl Rule + 'static) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    /// Insert a rule at the FRONT, ahead of everything already in the set — for a host
    /// whose dialect must win over a built-in (its own timestamp format, say).
    pub fn first(mut self, rule: impl Rule + 'static) -> Self {
        self.rules.insert(0, Arc::new(rule));
        self
    }

    /// Replace the "is this line a continuation of the one before it" test.
    ///
    /// A continuation inherits the previous line's level, which is what makes a whole stack
    /// trace read as one error instead of as one red line followed by twenty grey ones.
    pub fn continued_by(mut self, f: fn(&str) -> bool) -> Self {
        self.continues = f;
        self
    }

    pub fn rules(&self) -> &[Arc<dyn Rule>] {
        &self.rules
    }

    /// Whether `text` continues the previous line rather than starting something.
    pub fn is_continuation(&self, text: &str) -> bool {
        (self.continues)(text)
    }
}

impl std::fmt::Debug for RuleSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleSet")
            .field("rules", &self.rules.iter().map(|r| r.name()).collect::<Vec<_>>())
            .finish()
    }
}

/// The default continuation test: a line that starts with whitespace is a continuation of
/// the one above it. True of stack traces, of `javac`'s carets, and of most multi-line
/// messages anyone writes on purpose.
pub fn indented(text: &str) -> bool {
    text.starts_with(' ') || text.starts_with('\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_reports_its_rules_by_name() {
        let set = RuleSet::empty()
            .with(FnRule::new("a", |_: &str, _: usize| None))
            .with(FnRule::new("b", |_: &str, _: usize| None));
        assert_eq!(set.rules().iter().map(|r| r.name()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn first_puts_a_rule_ahead_of_the_existing_ones() {
        let set = RuleSet::empty()
            .with(FnRule::new("a", |_: &str, _: usize| None))
            .first(FnRule::new("z", |_: &str, _: usize| None));
        assert_eq!(set.rules()[0].name(), "z");
    }

    #[test]
    fn the_default_continuation_is_indentation() {
        let set = RuleSet::empty();
        assert!(set.is_continuation("\tat com.acme.Foo.bar"));
        assert!(!set.is_continuation("ERROR something"));
    }
}
