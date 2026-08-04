//! Collecting candidates: de-duplicated, capped, in the order the provider offered them.
//!
//! ## Why order is insertion order
//!
//! Because the provider knows something a sort cannot: which of its vocabularies is the more
//! authoritative. A documented property beats an inferred one; an element the schema allows
//! *here* beats one it allows somewhere. Offering in that order and keeping it means ranking
//! is expressed by the code that has the knowledge, rather than by a score invented to
//! reconstruct it afterwards.
//!
//! ## Why de-duplication is free
//!
//! Because two vocabularies overlapping is the normal case, not the exception: a project whose
//! `@ConfigurationProperties` are processed into metadata has every key in *both* sources, and
//! a list that shows each one twice looks broken. Offering the authoritative source first and
//! letting the second be silently rejected is exactly the behaviour wanted, so it is the
//! default rather than something each provider tracks in a side `Vec`.

use std::collections::HashSet;

use bennu_proto::prelude::CompletionItem;

/// How many candidates a single request returns unless the provider says otherwise.
///
/// Enough that a two-letter prefix under a busy namespace is still complete, small enough that
/// the popup stays a list a person can look at.
pub const DEFAULT_CAP: usize = 300;

/// One candidate, before it becomes wire data.
///
/// Built fluently because the optional half (detail, auto-import) is genuinely optional and
/// a four-field struct literal at every call site puts `None` in front of the reader far more
/// often than it puts information there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The text inserted on accept, and the identity used for de-duplication.
    pub label: String,
    /// Kind tag the editor maps to an icon (`"property"`, `"element"`, `"value"`).
    pub kind: String,
    /// Right-aligned detail — a type, a default, the class that declares it.
    pub detail: String,
    /// The fully-qualified name to import on accept, for a type-name candidate.
    pub auto_import: Option<String>,
}

impl Proposal {
    pub fn new(label: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: kind.into(),
            detail: String::new(),
            auto_import: None,
        }
    }

    /// Attach the right-aligned detail. An empty string leaves it off.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Mark the candidate as one whose acceptance should add an `import`.
    pub fn importing(mut self, fqcn: impl Into<String>) -> Self {
        self.auto_import = Some(fqcn.into());
        self
    }
}

impl From<Proposal> for CompletionItem {
    fn from(p: Proposal) -> Self {
        CompletionItem {
            label: p.label,
            kind: p.kind,
            detail: Some(p.detail).filter(|d| !d.is_empty()),
            auto_import: p.auto_import,
        }
    }
}

/// A candidate list being built: keeps insertion order, rejects repeats, stops at a ceiling.
#[derive(Debug, Clone)]
pub struct Proposals {
    items: Vec<Proposal>,
    seen: HashSet<String>,
    cap: usize,
}

impl Default for Proposals {
    fn default() -> Self {
        Self::new(DEFAULT_CAP)
    }
}

impl Proposals {
    pub fn new(cap: usize) -> Self {
        Self { items: Vec::new(), seen: HashSet::new(), cap }
    }

    /// Whether the ceiling has been reached — the condition to break a loop on, so a provider
    /// with several vocabularies stops walking the rest of them.
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.cap
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether a label has already been offered — for a provider that wants to skip the *work*
    /// of building a proposal, not just its insertion.
    pub fn has(&self, label: &str) -> bool {
        self.seen.contains(label)
    }

    /// Offer a candidate. Returns whether it was taken: `false` means the list is full or the
    /// label was already offered by a source that got there first.
    pub fn offer(&mut self, proposal: Proposal) -> bool {
        if self.is_full() || !self.seen.insert(proposal.label.clone()) {
            return false;
        }
        self.items.push(proposal);
        true
    }

    /// The labels offered so far, in order. What [`crate::prefix::unique_continuation`] takes
    /// when ghost text is derived from the same list the popup shows.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|p| p.label.as_str())
    }

    pub fn into_items(self) -> Vec<CompletionItem> {
        self.items.into_iter().map(CompletionItem::from).collect()
    }
}

impl Extend<Proposal> for Proposals {
    fn extend<I: IntoIterator<Item = Proposal>>(&mut self, iter: I) {
        for p in iter {
            if !self.offer(p) && self.is_full() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(label: &str) -> Proposal {
        Proposal::new(label, "property")
    }

    #[test]
    fn the_first_source_to_offer_a_label_keeps_it() {
        let mut c = Proposals::default();
        assert!(c.offer(p("server.port").detail("documented")));
        assert!(!c.offer(p("server.port").detail("inferred")), "the second source is rejected");
        let items = c.into_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].detail.as_deref(), Some("documented"));
    }

    #[test]
    fn order_is_the_order_the_provider_offered() {
        let mut c = Proposals::default();
        c.extend([p("b"), p("a"), p("c")]);
        assert_eq!(c.labels().collect::<Vec<_>>(), ["b", "a", "c"]);
    }

    #[test]
    fn the_ceiling_stops_the_walk_rather_than_truncating_afterwards() {
        let mut c = Proposals::new(2);
        assert!(c.offer(p("a")));
        assert!(c.offer(p("b")));
        assert!(c.is_full());
        assert!(!c.offer(p("c")));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn an_empty_detail_is_absent_on_the_wire_not_an_empty_string() {
        let item: CompletionItem = p("x").into();
        assert_eq!(item.detail, None);
        assert_eq!(CompletionItem::from(p("x").detail("y")).detail.as_deref(), Some("y"));
    }
}
