//! **What this project actually calls**, counted — the ranking terms a type system cannot supply.
//!
//! Two facts about a codebase, both visible in its own text:
//!
//! * **How often each method name is called.** `stream` is called four hundred times, `listIterator`
//!   never; a project is its habits, and the member list after `items.` should open on the habit.
//! * **What is called right after what.** `open()` is followed by `close()`, `builder()` by
//!   `build()`, `lock()` by `unlock()`. A sequence is a far more specific fact than a popularity, and
//!   it is exactly the question completion asks mid-chain: "given the call I just wrote, what next?"
//!
//! ## By name, not by declaring type
//!
//! Attributing a call site to the type that declares the method is a type inference per site — the
//! price of a full validation, paid to break a tie in a popup. Counting by simple name is wrong
//! where two unrelated types share a method name (`get`, `add`), and right where it matters: the
//! ranking only asks about names the receiver's own member list already contains, so a name shared
//! by another type costs a few points of a tie-breaker, never a candidate.
//!
//! ## Same philosophy as the import census
//!
//! Derived data, a pure function of the sources, counted in the pass that already reads every file;
//! **not persisted** (a stale copy would keep recommending what the project stopped doing) and
//! **not live** (a statistic about the shape of a project does not change because a file was saved).
//! One use is a coincidence and not evidence, so [`UsageCensus::finalize`] drops everything seen
//! once — which is also what keeps the table small on a large project.
//!
//! ## What counts as "after"
//!
//! * **Within one chain**: `a.b().c()` records `(b, c)`.
//! * **Across statements on the same receiver**: `x.open(); x.read(); x.close();` records
//!   `(open, read)` and `(read, close)`. A statement that is not a call, or is a call on something
//!   else, ends the run — `x.open(); y.close();` says nothing about `open` and `close`.

use std::collections::HashMap;

use bennu_java::prelude::parse_java;
use bennu_query::prelude::ProjectUsage;
use tree_sitter::Node;

/// Where a count stops climbing. The ranking turns a count into a band that saturates long before
/// this; the bound is there so a generated file with a million calls cannot overflow a counter.
const SATURATE_AT: u32 = 4096;

/// How many distinct method names are kept. A real project has a few thousand; this bounds a
/// pathological one. Reached, the census stops learning new names and keeps counting known ones.
const MAX_NAMES: usize = 50_000;

/// How many distinct `(previous, next)` pairs are kept, bounded for the same reason.
const MAX_PAIRS: usize = 100_000;

/// Method-call counts and call-sequence counts for a project (or, before merging, one file).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageCensus {
    /// Simple method name → call sites.
    uses: HashMap<String, u32>,
    /// `previous` → `next` → how many times `next` followed `previous`. Nested rather than keyed by
    /// a tuple so a lookup by `&str` allocates nothing — it runs once per candidate per keystroke.
    after: HashMap<String, HashMap<String, u32>>,
    /// How many `(previous, next)` pairs `after` holds, for the cap.
    pairs: usize,
}

impl UsageCensus {
    /// Count one source file. A file that does not parse at all counts nothing.
    pub fn of_source(source: &str) -> Self {
        let mut census = Self::default();
        let Some(tree) = parse_java(source) else {
            return census;
        };
        census.collect(tree.root_node(), source.as_bytes());
        census
    }

    /// Fold another census into this one — how per-file results become the project's.
    pub fn merge(&mut self, other: UsageCensus) {
        for (name, n) in other.uses {
            self.add_use(&name, n);
        }
        for (previous, nexts) in other.after {
            for (next, n) in nexts {
                self.add_pair(&previous, &next, n);
            }
        }
    }

    /// Drop everything seen once. One call is a coincidence, and `usage_points(1)` is zero anyway,
    /// so this loses nothing the ranking could use and shrinks the table to what carries evidence.
    /// Call it once, after the last [`merge`](Self::merge).
    pub fn finalize(mut self) -> Self {
        self.uses.retain(|_, n| *n >= 2);
        for nexts in self.after.values_mut() {
            nexts.retain(|_, n| *n >= 2);
        }
        self.after.retain(|_, nexts| !nexts.is_empty());
        self.pairs = self.after.values().map(HashMap::len).sum();
        self.uses.shrink_to_fit();
        self.after.shrink_to_fit();
        self
    }

    /// How many distinct names and pairs are held — zero means the census was never built, which
    /// the caller reads as "no opinion".
    pub fn len(&self) -> usize {
        self.uses.len() + self.pairs
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn add_use(&mut self, name: &str, n: u32) {
        if let Some(count) = self.uses.get_mut(name) {
            *count = count.saturating_add(n).min(SATURATE_AT);
            return;
        }
        if self.uses.len() < MAX_NAMES {
            self.uses.insert(name.to_string(), n.min(SATURATE_AT));
        }
    }

    fn add_pair(&mut self, previous: &str, next: &str, n: u32) {
        if let Some(count) = self.after.get_mut(previous).and_then(|nexts| nexts.get_mut(next)) {
            *count = count.saturating_add(n).min(SATURATE_AT);
            return;
        }
        if self.pairs >= MAX_PAIRS {
            return;
        }
        self.after
            .entry(previous.to_string())
            .or_default()
            .insert(next.to_string(), n.min(SATURATE_AT));
        self.pairs += 1;
    }

    /// Walk the whole tree: every invocation is a use (and, when its receiver is itself an
    /// invocation, a chain pair); every block is a run of statements to read sequences from.
    /// Iterative, so a deeply nested expression cannot overflow the stack.
    fn collect(&mut self, root: Node, source: &[u8]) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "method_invocation" => self.count_invocation(node, source),
                "block" | "constructor_body" => self.count_sequence(node, source),
                _ => {}
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
    }

    fn count_invocation(&mut self, call: Node, source: &[u8]) {
        let Some(name) = call_name(call, source) else { return };
        self.add_use(name, 1);
        let chained = call
            .child_by_field_name("object")
            .filter(|object| object.kind() == "method_invocation")
            .and_then(|object| call_name(object, source));
        if let Some(previous) = chained {
            self.add_pair(previous, name, 1);
        }
    }

    /// `x.open(); x.read(); x.close();` — consecutive statements calling on the same receiver.
    fn count_sequence(&mut self, block: Node, source: &[u8]) {
        let mut cursor = block.walk();
        let mut last: Option<(String, String)> = None;
        for statement in block.named_children(&mut cursor) {
            let this = statement_call(statement)
                .and_then(|call| Some((receiver_root(call, source)?, call_name(call, source)?)));
            if let (Some((last_receiver, last_call)), Some((receiver, call))) = (last.as_ref(), this) {
                if last_receiver == receiver {
                    self.add_pair(last_call, call, 1);
                }
            }
            // A statement that is not a call on a receiver ends the run.
            last = this.map(|(receiver, call)| (receiver.to_string(), call.to_string()));
        }
    }
}

impl ProjectUsage for UsageCensus {
    fn member_uses(&self, name: &str) -> u32 {
        self.uses.get(name).copied().unwrap_or(0)
    }

    fn follows(&self, previous: &str, next: &str) -> u32 {
        self.after
            .get(previous)
            .and_then(|nexts| nexts.get(next))
            .copied()
            .unwrap_or(0)
    }
}

/// The method name of a `method_invocation`.
fn call_name<'a>(call: Node, source: &'a [u8]) -> Option<&'a str> {
    call.child_by_field_name("name")?.utf8_text(source).ok()
}

/// The method call a statement is made of: `x.open();`, `y = x.open();`, `Reader r = x.open();`.
fn statement_call(statement: Node) -> Option<Node> {
    match statement.kind() {
        "expression_statement" => invocation_of(statement.named_child(0)?),
        "local_variable_declaration" => {
            let mut cursor = statement.walk();
            let found = statement
                .children_by_field_name("declarator", &mut cursor)
                .find_map(|declarator| {
                    declarator.child_by_field_name("value").and_then(invocation_of)
                });
            found
        }
        _ => None,
    }
}

/// `expression` itself when it is a call, or the call on the right of an assignment.
fn invocation_of(expression: Node) -> Option<Node> {
    match expression.kind() {
        "method_invocation" => Some(expression),
        "assignment_expression" => expression
            .child_by_field_name("right")
            .filter(|right| right.kind() == "method_invocation"),
        _ => None,
    }
}

/// What a call is made on, as text: `x` in `x.open()`, `this.items` in `this.items.add(y)`. The
/// root of a chain, so `x.a().b()` is still "on `x`". `None` for a call with no receiver or one
/// that is not a plain name (`new Foo().bar()`, `(cast).baz()`).
fn receiver_root<'a>(call: Node, source: &'a [u8]) -> Option<&'a str> {
    let mut node = call.child_by_field_name("object")?;
    loop {
        match node.kind() {
            "identifier" | "this" | "field_access" => return node.utf8_text(source).ok(),
            "method_invocation" => node = node.child_by_field_name("object")?,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn census(body: &str) -> UsageCensus {
        UsageCensus::of_source(&format!("class A {{ void m() {{ {body} }} }}"))
    }

    #[test]
    fn every_call_is_counted_by_its_simple_name() {
        let c = census("x.a(); x.a(); y.b(); foo();");
        assert_eq!(c.member_uses("a"), 2);
        assert_eq!(c.member_uses("b"), 1);
        assert_eq!(c.member_uses("foo"), 1, "a call with no receiver is still a use");
        assert_eq!(c.member_uses("never"), 0);
    }

    #[test]
    fn a_chained_call_records_the_pair() {
        let c = census("b.build().make().go();");
        assert_eq!(c.follows("build", "make"), 1);
        assert_eq!(c.follows("make", "go"), 1);
        assert_eq!(c.follows("build", "go"), 0, "only adjacent calls are a pair");
    }

    #[test]
    fn consecutive_statements_on_one_receiver_record_the_pairs() {
        let c = census("x.open(); x.read(); x.close();");
        assert_eq!(c.follows("open", "read"), 1);
        assert_eq!(c.follows("read", "close"), 1);
        assert_eq!(c.follows("open", "close"), 0);
    }

    #[test]
    fn calls_on_different_receivers_say_nothing_about_each_other() {
        let c = census("x.open(); y.close();");
        assert_eq!(c.follows("open", "close"), 0);
    }

    #[test]
    fn a_statement_that_is_not_a_call_ends_the_run() {
        let c = census("x.open(); int i = 0; x.close();");
        assert_eq!(c.follows("open", "close"), 0);
    }

    #[test]
    fn a_declaration_or_assignment_holding_the_call_continues_the_run() {
        let c = census("Reader r = x.open(); x.close();");
        assert_eq!(c.follows("open", "close"), 1);
        let d = census("r = x.open(); x.close();");
        assert_eq!(d.follows("open", "close"), 1);
    }

    #[test]
    fn a_call_on_a_chain_belongs_to_the_chains_root_receiver() {
        let c = census("x.a().b(); x.c();");
        assert_eq!(c.follows("b", "c"), 1);
    }

    #[test]
    fn a_field_receiver_is_kept_whole() {
        let c = census("this.items.add(1); this.other.size();");
        assert_eq!(c.follows("add", "size"), 0, "different fields are different receivers");
        let d = census("this.items.add(1); this.items.size();");
        assert_eq!(d.follows("add", "size"), 1);
    }

    #[test]
    fn merging_adds_the_counts_of_both_files() {
        let mut a = census("x.open(); x.close();");
        a.merge(census("y.open(); y.close();"));
        assert_eq!(a.member_uses("open"), 2);
        assert_eq!(a.follows("open", "close"), 2);
    }

    #[test]
    fn finalize_drops_what_was_seen_once() {
        let mut c = census("x.open(); x.close(); x.rare();");
        c.merge(census("y.open(); y.close();"));
        let c = c.finalize();
        assert_eq!(c.member_uses("open"), 2);
        assert_eq!(c.member_uses("rare"), 0, "a single use is a coincidence");
        assert_eq!(c.follows("open", "close"), 2);
        assert_eq!(c.follows("close", "rare"), 0);
    }

    #[test]
    fn a_census_of_one_coincidence_is_empty_once_finalized() {
        assert!(census("x.once();").finalize().is_empty());
        assert!(!census("x.twice(); x.twice();").finalize().is_empty());
    }

    #[test]
    fn counts_saturate_rather_than_wrapping() {
        let mut c = UsageCensus::default();
        c.add_use("hot", u32::MAX);
        c.add_use("hot", 5);
        assert_eq!(c.member_uses("hot"), SATURATE_AT);
    }

    #[test]
    fn learning_stops_at_the_name_ceiling_and_known_names_keep_counting() {
        let mut c = UsageCensus::default();
        c.uses = (0..MAX_NAMES).map(|i| (format!("m{i}"), 2)).collect();
        c.add_use("m0", 1);
        c.add_use("newcomer", 3);
        assert_eq!(c.member_uses("m0"), 3);
        assert_eq!(c.member_uses("newcomer"), 0);
    }

    #[test]
    fn empty_and_unparsable_sources_count_nothing_and_do_not_panic() {
        assert!(UsageCensus::of_source("").is_empty());
        let _ = UsageCensus::of_source("}}} class ((( x.y( ;;");
        let _ = UsageCensus::of_source("\u{feff}");
    }

    #[test]
    fn the_trait_answers_zero_for_an_empty_census() {
        let c = UsageCensus::default();
        assert_eq!(c.member_uses("anything"), 0);
        assert_eq!(c.follows("a", "b"), 0);
    }
}
