//! Hook namespacing mechanism — decisions **D9** and **D10**.
//!
//! ## Why this module exists
//!
//! Hook names used to live in one flat space (`on_commit`, `on_note_saved`, …)
//! while every other part of the Lua surface was already namespaced
//! (`arbor.repo.*`, `arbor.notes.*`, `arbor.job.*`). That flat space collided
//! the moment a second product started firing hooks: `on_note_saved` meant
//! "git note written" in Corvus and "vault note written" in Garrulus. A name is
//! now `<namespace>:<event>` — the same separator and the same shape
//! `arbor.events.emit` has always used for plugin-defined events, so a plugin
//! author sees one rule instead of two (**D9**).
//!
//! ## Why the names are constants and not literals
//!
//! `fire_hook("on_note_savd", …)` compiles, fires nothing, and nobody ever
//! finds out — the failure mode is a plugin that silently does nothing. The
//! same is true in the other direction: a mistyped catalog entry documents a
//! hook that will never arrive. Building every name from a constant at compile
//! time turns half of that hole into a compile error, and lets the catalog and
//! the fire sites read from the *same* constant so they cannot drift (**D10**).
//!
//! The namespace string itself is written **exactly once per namespace** — in
//! the `NS` constant that [`declare_hook_names!`] generates. Everything else derives.
//!
//! ## What lives here vs. in the catalog
//!
//! This module is pure mechanism: joining, splitting, qualifying, and a
//! nearest-name search over an arbitrary candidate list. It knows nothing about
//! which hooks exist. The policy that consults the catalog (which unqualified
//! name resolves to which namespace, what counts as a typo) lives in
//! [`crate::hook_catalog`], which owns the list.

use std::borrow::Cow;

/// The separator between a hook's namespace and its event.
///
/// Deliberately the same character `arbor.events.emit` uses for plugin-defined
/// events: a plugin author who has seen `my-plugin:build_done` already knows
/// how `corvus:commit` is put together.
pub const HOOK_NS_SEP: char = ':';

// ── Canonical product ids ────────────────────────────────────────────────────
//
// These are the ids a backend binds its plugin host to (`App::plugin_host(id, …)`)
// and the ids a manifest's `targets` list names. Most of them double as a hook
// namespace; the ones that do are referenced from `crate::hook_names`.

/// The host runtime itself, not any one product.
///
/// Hooks in this namespace are fired by the plugin runtime or by shell-level
/// plumbing that every product links (plugin lifecycle, main-area views, theme,
/// active-project lifecycle). They are the reason resolution cannot simply be
/// "prefix with the host product": the *same* `main.lua` line loaded under two
/// products must resolve to the *same* lifecycle hook, so these names have one
/// namespace shared by every host.
pub const PRODUCT_ARBOR: &str = "arbor";

/// The Git product (`corvus-be`).
pub const PRODUCT_CORVUS: &str = "corvus";

/// The note-vault product (`garrulus-be`).
pub const PRODUCT_GARRULUS: &str = "garrulus";

/// The standalone file-explorer product (`sitta-be`).
pub const PRODUCT_SITTA: &str = "sitta";

/// The live-coding DAW product (`merula-be`).
pub const PRODUCT_MERULA: &str = "merula";

/// The capture / recorder product (`tyto-be`).
pub const PRODUCT_TYTO: &str = "tyto";

/// The code-intelligence product (`bennu-be`).
pub const PRODUCT_BENNU: &str = "bennu";

/// The container product (`picus-be`).
pub const PRODUCT_PICUS: &str = "picus";

/// The launcher shell's own in-process plugin host.
pub const PRODUCT_LAUNCHER: &str = "launcher";

/// Every product that can host plugins.
///
/// A list rather than a derived set because it has one job: when the user installs a package
/// **for one product**, the others need an explicit "not here" written for them. Deriving that
/// from whichever backends happen to be running would make the answer depend on what was open
/// at the time, which is the opposite of a recorded decision.
///
/// `arbor` is absent on purpose — it is the namespace shared by every host, not a product a
/// plugin can be installed for.
pub const HOSTING_PRODUCTS: &[&str] = &[
    PRODUCT_CORVUS,
    PRODUCT_BENNU,
    PRODUCT_GARRULUS,
    PRODUCT_SITTA,
    PRODUCT_MERULA,
    PRODUCT_TYTO,
    PRODUCT_PICUS,
    PRODUCT_LAUNCHER,
];

/// Copy `ns`, a separator and `event` into a fixed-size byte buffer.
///
/// The `N` mismatch is the point: [`hook_name!`] computes `N` from the two
/// operands, so a buffer that does not fit fails const evaluation instead of
/// silently truncating. Not meant to be called directly — use the macro.
pub const fn join_ns_bytes<const N: usize>(ns: &str, event: &str) -> [u8; N] {
    let ns = ns.as_bytes();
    let event = event.as_bytes();
    let mut out = [0u8; N];

    let mut i = 0;
    while i < ns.len() {
        out[i] = ns[i];
        i += 1;
    }
    out[i] = HOOK_NS_SEP as u8;
    i += 1;

    let mut j = 0;
    while j < event.len() {
        out[i + j] = event[j];
        j += 1;
    }
    out
}

/// Build a `"<ns>:<event>"` hook name at compile time.
///
/// `ns` is an expression (typically the `NS` constant of a namespace module),
/// which is why `concat!` cannot do this job — it only accepts literals.
///
/// ```ignore
/// pub const NS: &str = PRODUCT_GARRULUS;
/// pub const NOTE_SAVED: &str = hook_name!(NS, "note_saved"); // "garrulus:note_saved"
/// ```
#[macro_export]
macro_rules! hook_name {
    ($ns:expr, $event:expr $(,)?) => {{
        const __HOOK_NS: &str = $ns;
        const __HOOK_EVENT: &str = $event;
        const __HOOK_LEN: usize = __HOOK_NS.len() + 1 + __HOOK_EVENT.len();
        const __HOOK_BYTES: [u8; __HOOK_LEN] =
            $crate::hook_ns::join_ns_bytes::<__HOOK_LEN>(__HOOK_NS, __HOOK_EVENT);
        const __HOOK_NAME: &str = match ::core::str::from_utf8(&__HOOK_BYTES) {
            Ok(name) => name,
            Err(_) => panic!("hook_name!: namespace + event is not valid UTF-8"),
        };
        __HOOK_NAME
    }};
}

/// Declare a whole namespace of hook names in one block.
///
/// Generates `NS`, one `pub const` per event, and an `ALL` slice in declaration
/// order. `ALL` is what lets a product contribute *its* hooks to a dispatcher
/// without re-listing them, and what makes "is this name still fired anywhere"
/// answerable by grep on a single file.
///
/// ```ignore
/// declare_hook_names! {
///     ns = PRODUCT_GARRULUS;
///     /// Fired after a vault note is written to disk.
///     NOTE_SAVED = "note_saved";
///     SYNC_DONE  = "sync_done";
/// }
/// ```
#[macro_export]
macro_rules! declare_hook_names {
    (
        ns = $ns:expr;
        $(
            $(#[$meta:meta])*
            $konst:ident = $event:literal;
        )+
    ) => {
        /// The namespace every name below is built from — the single place
        /// this namespace's string is written.
        pub const NS: &str = $ns;

        $(
            $(#[$meta])*
            pub const $konst: &str = $crate::hook_name!(NS, $event);
        )+

        /// Every hook name in this namespace, in declaration order.
        pub const ALL: &[&str] = &[$($konst),+];
    };
}

/// Split a hook name into `(namespace, event)`.
///
/// Returns `None` for an unqualified name. Splits on the *first* separator, so
/// an event that itself contains a colon stays intact in the second half.
pub fn split_ns(name: &str) -> Option<(&str, &str)> {
    name.split_once(HOOK_NS_SEP)
}

/// The namespace half of a hook name, or `None` when it is unqualified.
pub fn namespace_of(name: &str) -> Option<&str> {
    split_ns(name).map(|(ns, _)| ns)
}

/// The event half of a hook name — the whole name when it is unqualified.
pub fn event_of(name: &str) -> &str {
    split_ns(name).map_or(name, |(_, event)| event)
}

/// True when `name` belongs to `ns`.
pub fn is_in_ns(name: &str, ns: &str) -> bool {
    namespace_of(name) == Some(ns)
}

/// Apply the D9 rule: an unqualified name gets `ns` as its namespace, an
/// already-qualified name is returned untouched.
///
/// Names containing `*` are returned untouched as well — a wildcard is a
/// subscription *pattern*, and silently rewriting `*` into `corvus:*` would
/// turn "everything" into "everything from one product".
pub fn qualify<'a>(name: &'a str, ns: &str) -> Cow<'a, str> {
    if name.contains(HOOK_NS_SEP) || name.contains('*') {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{ns}{HOOK_NS_SEP}{name}"))
    }
}

/// True when the name is a subscription pattern rather than a concrete hook.
///
/// Patterns are never namespace-resolved and never validated: `garrulus:*` is
/// legal, and so is a pattern for a hook that does not exist yet.
pub fn is_pattern(name: &str) -> bool {
    name.contains('*')
}

/// Levenshtein edit distance, iterative two-row variant.
///
/// Only ever runs on a failed subscription, over a catalog of well under a
/// thousand short strings, so the allocation-per-call is irrelevant next to
/// the value of the message it produces.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur: Vec<usize> = vec![0; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// How far `candidate` is from `name`, for the purpose of "did you mean".
///
/// A candidate whose *event* half matches exactly scores 0 regardless of edit
/// distance: the overwhelmingly common mistake is reaching for the right event
/// under the wrong namespace (`corvus:note_saved` when you wanted the vault
/// one), and plain edit distance buries that answer under near-miss typos.
fn suggestion_score(name: &str, candidate: &str) -> usize {
    if event_of(name) == event_of(candidate) {
        return 0;
    }
    edit_distance(name, candidate)
}

/// The `max` closest catalog entries to `name`, nearest first.
///
/// Candidates further than [`SUGGESTION_CUTOFF`] edits away are dropped rather
/// than padded in — a suggestion list full of unrelated names is worse than a
/// short one, because it invites the reader to distrust all of it.
pub fn nearest_names<'a, I>(name: &str, candidates: I, max: usize) -> Vec<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut scored: Vec<(usize, &'a str)> = candidates
        .into_iter()
        .map(|c| (suggestion_score(name, c), c))
        .filter(|(score, _)| *score <= SUGGESTION_CUTOFF)
        .collect();
    // Stable secondary key on the name keeps the message deterministic for two
    // candidates that tie — otherwise the same typo suggests a different list
    // run to run and looks like a flapping bug.
    scored.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    scored.truncate(max);
    scored.into_iter().map(|(_, name)| name).collect()
}

/// Edit-distance ceiling for a "did you mean" suggestion.
pub const SUGGESTION_CUTOFF: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    // A miniature namespace, declared through the same macro the real ones use,
    // so the macro is exercised by the tests rather than only by the catalog.
    mod sample {
        crate::declare_hook_names! {
            ns = "sample";
            ALPHA = "alpha";
            /// Doc comments must survive the expansion.
            BETA  = "beta_two";
        }
    }

    #[test]
    fn macro_builds_qualified_names_at_compile_time() {
        assert_eq!(sample::NS, "sample");
        assert_eq!(sample::ALPHA, "sample:alpha");
        assert_eq!(sample::BETA, "sample:beta_two");
    }

    #[test]
    fn macro_collects_every_name_in_declaration_order() {
        assert_eq!(sample::ALL, &["sample:alpha", "sample:beta_two"]);
    }

    #[test]
    fn hook_name_is_usable_in_const_position() {
        // The point of the macro: the result is a `&'static str` constant, so a
        // fire site can use it where only a constant is accepted.
        const NAME: &str = crate::hook_name!(PRODUCT_CORVUS, "commit");
        assert_eq!(NAME, "corvus:commit");
    }

    #[test]
    fn splits_on_the_first_separator_only() {
        assert_eq!(split_ns("corvus:commit"), Some(("corvus", "commit")));
        assert_eq!(split_ns("a:b:c"), Some(("a", "b:c")));
        assert_eq!(split_ns("commit"), None);
        assert_eq!(namespace_of("commit"), None);
        assert_eq!(event_of("commit"), "commit");
        assert_eq!(event_of("corvus:commit"), "commit");
        assert!(is_in_ns("corvus:commit", "corvus"));
        assert!(!is_in_ns("corvus:commit", "garrulus"));
    }

    #[test]
    fn qualify_only_touches_unqualified_names() {
        assert_eq!(qualify("commit", "corvus"), "corvus:commit");
        assert_eq!(qualify("garrulus:note_saved", "corvus"), "garrulus:note_saved");
    }

    #[test]
    fn qualify_leaves_patterns_alone() {
        // `*` must keep meaning "every hook", not "every corvus hook".
        assert_eq!(qualify("*", "corvus"), "*");
        assert_eq!(qualify("garrulus:*", "corvus"), "garrulus:*");
        assert!(is_pattern("*"));
        assert!(!is_pattern("corvus:commit"));
    }

    #[test]
    fn nearest_ranks_a_typo_above_unrelated_names() {
        let catalog = ["corvus:note_saved", "corvus:commit", "garrulus:sync_done"];
        let got = nearest_names("corvus:note_savd", catalog, 2);
        assert_eq!(got.first(), Some(&"corvus:note_saved"));
    }

    #[test]
    fn nearest_prefers_the_same_event_in_another_namespace() {
        // The collision this whole change exists to fix: right event, wrong side.
        let catalog = ["corvus:note_saved", "garrulus:note_saved", "corvus:commit"];
        let got = nearest_names("sitta:note_saved", catalog, 2);
        assert_eq!(got.len(), 2);
        assert!(got.contains(&"corvus:note_saved"));
        assert!(got.contains(&"garrulus:note_saved"));
    }

    #[test]
    fn nearest_returns_nothing_when_everything_is_far_away() {
        let catalog = ["corvus:commit", "garrulus:sync_done"];
        assert!(nearest_names("arbor:completely_unrelated_name", catalog, 3).is_empty());
    }

    #[test]
    fn nearest_is_deterministic_across_ties() {
        let catalog = ["corvus:aaa", "corvus:aab", "corvus:aac"];
        let first = nearest_names("corvus:aad", catalog, 3);
        let second = nearest_names("corvus:aad", catalog, 3);
        assert_eq!(first, second);
    }

    #[test]
    fn edit_distance_handles_empty_operands() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }
}
