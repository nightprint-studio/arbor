//! What you picked last time — the completion memory.
//!
//! ## What "frecency" is
//!
//! Frequency plus recency, in one number. Every other ranking signal is a fact about the code:
//! what the receiver is, how deep the member was inherited from, what type the position wants.
//! This one is a fact about **you** — that on a `List` you reach for `stream` and not for
//! `listIterator`, and that whichever of `getText` and `getValue` you chose a minute ago is
//! almost certainly the one you want again.
//!
//! It is the term that makes an IDE feel like it learned the codebase, and it needs no analysis
//! at all: the editor already knows which row was accepted, and remembering it is the whole
//! feature. What makes it *frecency* rather than a use count is the second half — a name picked
//! forty times last month should not outrank the one picked twice in the last minute, because
//! what you are doing now is a better predictor than what you did then.
//!
//! ## Keyed by the DECLARING type
//!
//! Not by the receiver, and not globally. Globally it would be a popularity contest that puts
//! `toString` on top of everything. By the receiver it would learn `ArrayList` and `List`
//! separately, which are the same reach. The declaring type is where the member actually lives,
//! so what is learned is "reaching into `java.util.List`, you pick `stream`" — and it transfers
//! to every receiver that inherits it.
//!
//! ## Session-scoped, and deliberately
//!
//! It lives for as long as the process and is never written to disk. Two reasons, and the second
//! is the one that decides it: a persisted ranking that has drifted is invisible — the list is
//! simply subtly wrong and there is nothing to look at — and no user would ever think to clear a
//! cache to fix a completion order. A memory that starts empty every session cannot rot.
//!
//! ## Why a global
//!
//! Because that is what it is: one memory per editor process, belonging to no query. Threading it
//! through [`crate::completion`] and [`crate::scope_completion`] as a parameter would put it in
//! every signature and every test that calls one, to describe something none of them own.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// How many contexts are remembered before the least recently used one is dropped.
const MAX_CONTEXTS: usize = 256;
/// How many distinct picks are remembered per context.
const MAX_PER_CONTEXT: usize = 64;
/// Picks past this stop improving an item's standing — the signal has said what it has to say,
/// and letting it climb forever would freeze the first thing you ever accepted at the top.
const MAX_COUNTED: u32 = 4;
/// What each counted pick is worth.
const PER_PICK: i32 = 7;
/// What being the MOST RECENT pick in this context is worth on top. The recency half: it is worth
/// more than two ordinary picks, so a fresh choice can overtake an old habit, and less than four,
/// so one stray acceptance cannot bury a name you use constantly.
const MOST_RECENT: i32 = 18;

#[derive(Default)]
struct Pick {
    count: u32,
    /// The tick this was last accepted at — a monotonic counter, not a clock. Wall time would
    /// make the ordering depend on how long you left the editor open.
    at: u64,
}

#[derive(Default)]
struct Memory {
    tick: u64,
    by_context: HashMap<String, HashMap<String, Pick>>,
    /// Last touch per context, for the eviction below.
    touched: HashMap<String, u64>,
}

static MEMORY: LazyLock<RwLock<Memory>> = LazyLock::new(|| RwLock::new(Memory::default()));

/// The context an **annotation** pick is remembered under.
///
/// A member's context is the type that declares it, which is exactly what the next list in the
/// same place is built from. A type name has no such context — its declaring type is itself — so
/// annotations are filed under the one thing every `@` position has in common. Written by the
/// seam's `bennu_completion_accepted`, read by the provider's annotation ranking.
pub const ANNOTATION_CONTEXT: &str = "@";

/// Remember that `label` was accepted from `context` (the declaring type's binary name, or `""`
/// when the candidate has no owner).
pub fn record(context: &str, label: &str) {
    let Ok(mut m) = MEMORY.write() else {
        return; // a poisoned ranking memory is not worth propagating a panic for
    };
    m.tick += 1;
    let tick = m.tick;
    m.touched.insert(context.to_string(), tick);
    let entry = m.by_context.entry(context.to_string()).or_default();
    let pick = entry.entry(label.to_string()).or_default();
    pick.count = pick.count.saturating_add(1);
    pick.at = tick;

    if entry.len() > MAX_PER_CONTEXT {
        // The oldest pick, which is the one whose absence changes the least.
        if let Some(oldest) = entry
            .iter()
            .min_by_key(|(_, p)| p.at)
            .map(|(k, _)| k.clone())
        {
            entry.remove(&oldest);
        }
    }
    if m.by_context.len() > MAX_CONTEXTS {
        if let Some(cold) = m
            .touched
            .iter()
            .min_by_key(|(_, t)| **t)
            .map(|(k, _)| k.clone())
        {
            m.by_context.remove(&cold);
            m.touched.remove(&cold);
        }
    }
}

/// What `label` has earned in `context` — `0` when it has never been picked there.
pub fn weight(context: &str, label: &str) -> i32 {
    let Ok(m) = MEMORY.read() else {
        return 0;
    };
    let Some(entry) = m.by_context.get(context) else {
        return 0;
    };
    let Some(pick) = entry.get(label) else {
        return 0;
    };
    let mut w = PER_PICK * pick.count.min(MAX_COUNTED) as i32;
    // The most recent pick in this context — the recency half of the name.
    if entry.values().all(|p| p.at <= pick.at) {
        w += MOST_RECENT;
    }
    w
}

/// Forget everything. Tests only: the memory is global, so one test's picks would otherwise
/// decide another's ordering depending on which ran first.
#[doc(hidden)]
pub fn forget_all() {
    if let Ok(mut m) = MEMORY.write() {
        *m = Memory::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests share one global memory, so they run under one lock rather than in parallel —
    /// which is also the honest shape: what is being tested IS a single shared thing.
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        forget_all();
        g
    }

    #[test]
    fn a_name_never_picked_is_worth_nothing() {
        let _g = guard();
        assert_eq!(weight("java/util/List", "stream"), 0);
    }

    #[test]
    fn picking_a_name_raises_it_and_repeating_raises_it_further() {
        let _g = guard();
        record("java/util/List", "stream");
        let once = weight("java/util/List", "stream");
        record("java/util/List", "stream");
        assert!(weight("java/util/List", "stream") > once);
    }

    /// The recency half. `add` has been picked more often, and `stream` was picked LAST — which
    /// is what a completion list should open on.
    #[test]
    fn the_most_recent_pick_can_overtake_a_more_frequent_one() {
        let _g = guard();
        for _ in 0..2 {
            record("java/util/List", "add");
        }
        record("java/util/List", "stream");
        assert!(
            weight("java/util/List", "stream") > weight("java/util/List", "add"),
            "stream {} vs add {}",
            weight("java/util/List", "stream"),
            weight("java/util/List", "add"),
        );
    }

    /// …but not forever. A name picked constantly outweighs one stray acceptance.
    #[test]
    fn a_habit_outweighs_one_stray_pick() {
        let _g = guard();
        for _ in 0..6 {
            record("java/util/List", "add");
        }
        record("java/util/List", "listIterator");
        assert!(weight("java/util/List", "add") > weight("java/util/List", "listIterator"));
    }

    /// What you pick on a `List` says nothing about what you pick on a `Path`.
    #[test]
    fn contexts_are_kept_apart() {
        let _g = guard();
        record("java/util/List", "stream");
        assert_eq!(weight("java/nio/file/Path", "stream"), 0);
    }

    /// Unbounded growth in a process that stays open for days is a leak with a ranking attached.
    #[test]
    fn a_context_remembers_a_bounded_number_of_picks() {
        let _g = guard();
        for i in 0..(MAX_PER_CONTEXT + 20) {
            record("ctx", &format!("m{i}"));
        }
        let m = MEMORY.read().unwrap();
        assert!(m.by_context["ctx"].len() <= MAX_PER_CONTEXT);
    }
}
