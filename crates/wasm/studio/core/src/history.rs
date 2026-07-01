//! `core::history` — the snapshot-stack undo/redo engine lifted from the
//! 5 per-format backends (blueprint §2.1). This is the single most
//! behavior-sensitive generic in the whole extraction, so the lift is
//! deliberately conservative: it captures **every** semantic the five
//! hand-rolled `record_history` functions had, parameterising the one
//! genuine divergence rather than forcing all formats through one shape.
//!
//! ## What every format agreed on (the generic core)
//!
//! * A `Vec<T>` snapshot stack + a `pos` cursor (`stack[pos]` is live).
//! * `record_text` (textarea typing) **coalesces** into the current
//!   entry when *armed* and within `window` (500ms); else it pushes a
//!   new entry.
//! * `record_struct` (tree mutation) **never** coalesces — it always
//!   pushes and dis-arms, so a following text edit can't fold into a
//!   structural entry.
//! * Recording always drops the redo tail (everything after `pos`).
//! * Overflow past `cap` drains from the front and keeps `pos` valid.
//! * `undo`/`redo` move the cursor and dis-arm (so typing right after a
//!   redo opens a fresh coalesce run instead of clobbering the redo
//!   target).
//!
//! ## The one real divergence: no-op suppression (`dedup`)
//!
//! RON and `.properties` skipped recording entirely when the new
//! snapshot equalled the current one (a replayed no-op mutation must not
//! pollute history). JSON/TOML/YAML did **not** dedup. To preserve each
//! format's exact undo granularity this is a constructor flag
//! ([`History::new`] defaults it off — the JSON/TOML/YAML shape; the RON
//! and `.properties` backends opt in via [`History::with_window`]'s
//! `dedup` parameter through [`History::new_dedup`]).
//!
//! Note: the other apparent JSON↔RON divergence — RON setting
//! `coalesce_armed = can_coalesce` unconditionally vs JSON setting it
//! only on the push branch — is **not observable**: a coalesce only
//! happens when `can_coalesce` is already `true` AND `armed` was already
//! `true`, so `armed = can_coalesce` and "leave armed alone" agree. Both
//! converge to the same generic.

use std::time::{Duration, Instant};

/// Default coalesce window — rapid `record_text` calls within this of the
/// previous push merge into one undo step. Matches all 5 backends (500ms).
pub const DEFAULT_WINDOW: Duration = Duration::from_millis(500);

/// Snapshot-stack undo/redo history, generic over the snapshot type.
///
/// Every studio format snapshots `String` (the full document text), but
/// keeping it generic lets a future format snapshot a richer value.
pub struct History<T> {
    stack:   Vec<T>,
    /// Cursor: `stack[pos]` is the live state. Always in `0..stack.len()`.
    pos:     usize,
    /// Coalesce gate. Set by `record_text`, cleared by `record_struct`
    /// and by `undo`/`redo`.
    armed:   bool,
    last_at: Instant,
    /// Max entries; overflow drains from the front. 200 for
    /// JSON/TOML/YAML/.properties, 128 for RON. Passed at ctor.
    cap:     usize,
    window:  Duration,
    /// Skip recording when the new snapshot equals the current one
    /// (RON / `.properties` semantics). Off by default.
    dedup:   bool,
}

impl<T: Clone + PartialEq> History<T> {
    /// New history seeded with `initial`, window = [`DEFAULT_WINDOW`],
    /// no-op suppression OFF (the JSON/TOML/YAML shape).
    pub fn new(initial: T, cap: usize) -> Self {
        Self::build(initial, cap, DEFAULT_WINDOW, false)
    }

    /// New history with no-op suppression ON (the RON / `.properties`
    /// shape — a replayed no-op mutation does not pollute history).
    pub fn new_dedup(initial: T, cap: usize) -> Self {
        Self::build(initial, cap, DEFAULT_WINDOW, true)
    }

    /// Full constructor — explicit window + dedup flag.
    pub fn with_window(initial: T, cap: usize, window: Duration, dedup: bool) -> Self {
        Self::build(initial, cap, window, dedup)
    }

    fn build(initial: T, cap: usize, window: Duration, dedup: bool) -> Self {
        Self {
            stack:   vec![initial],
            pos:     0,
            armed:   false,
            last_at: Instant::now(),
            cap:     cap.max(1),
            window,
            dedup,
        }
    }

    /// Text-level edit (textarea typing). Coalesces into the current
    /// entry when armed AND within `window`; otherwise pushes a new
    /// entry. Always drops the redo tail (everything after `pos`).
    /// Re-arms so a following text edit can coalesce.
    pub fn record_text(&mut self, snap: T) {
        self.record(snap, true);
    }

    /// Structured edit (tree mutation). NEVER coalesces. Pushes, drops
    /// the redo tail, drains oldest on overflow, and dis-arms (so the
    /// next text edit can't fold into a structural entry).
    pub fn record_struct(&mut self, snap: T) {
        self.record(snap, false);
    }

    fn record(&mut self, snap: T, can_coalesce: bool) {
        // No-op suppression (RON / .properties): a replayed identical
        // snapshot must not create an undo step. Note we still leave
        // `armed`/`last_at` untouched, matching the early-return in the
        // hand-rolled `record_history`.
        if self.dedup && self.stack[self.pos] == snap {
            return;
        }

        // Drop the redo tail before recording — the standard editor
        // pattern: editing after an undo loses the redo branch.
        if self.pos + 1 < self.stack.len() {
            self.stack.truncate(self.pos + 1);
        }

        let within = self.last_at.elapsed() < self.window;
        let coalesce = can_coalesce && self.armed && within && !self.stack.is_empty();

        if coalesce {
            // Overwrite the top entry so undo jumps past the whole typing
            // burst, not just the last keystroke.
            let last = self.stack.len() - 1;
            self.stack[last] = snap;
        } else {
            self.stack.push(snap);
            if self.stack.len() > self.cap {
                let drop = self.stack.len() - self.cap;
                self.stack.drain(0..drop);
            }
            self.pos = self.stack.len() - 1;
        }

        self.armed   = can_coalesce;
        self.last_at = Instant::now();
    }

    /// Move backward one step. Dis-arms the coalesce gate. Returns the
    /// now-live snapshot, or `None` when already at the oldest entry.
    pub fn undo(&mut self) -> Option<&T> {
        if self.pos == 0 {
            return None;
        }
        self.pos -= 1;
        self.armed = false;
        self.last_at = Instant::now();
        Some(&self.stack[self.pos])
    }

    /// Move forward one step. Dis-arms the coalesce gate. Returns the
    /// now-live snapshot, or `None` when already at the newest entry.
    pub fn redo(&mut self) -> Option<&T> {
        if self.pos + 1 >= self.stack.len() {
            return None;
        }
        self.pos += 1;
        self.armed = false;
        self.last_at = Instant::now();
        Some(&self.stack[self.pos])
    }

    /// The live snapshot (`stack[pos]`).
    pub fn current(&self) -> &T {
        &self.stack[self.pos]
    }

    pub fn can_undo(&self) -> bool {
        self.pos > 0
    }

    pub fn can_redo(&self) -> bool {
        self.pos + 1 < self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A short window so "outside the window" tests don't actually sleep.
    fn fresh() -> History<String> {
        History::with_window("0".into(), 200, Duration::from_millis(500), false)
    }

    #[test]
    fn fresh_doc_boundaries() {
        let h = fresh();
        assert_eq!(h.current(), "0");
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn coalesce_within_window_is_one_step() {
        let mut h = fresh();
        // First text edit pushes (armed was false → no coalesce). The
        // rest land within the window AND armed → coalesce into it.
        h.record_text("a".into());
        h.record_text("ab".into());
        h.record_text("abc".into());
        h.record_text("abcd".into());
        // initial "0" + one coalesced text entry = exactly one undo step.
        assert_eq!(h.current(), "abcd");
        assert!(h.can_undo());
        assert_eq!(h.undo().map(String::as_str), Some("0"));
        assert!(!h.can_undo());
    }

    #[test]
    fn outside_window_is_n_steps() {
        // Zero-length window → every record_text pushes a new entry.
        let mut h = History::with_window("0".into(), 200, Duration::ZERO, false);
        h.record_text("a".into());
        h.record_text("ab".into());
        h.record_text("abc".into());
        // "0","a","ab","abc" → 3 undo steps.
        assert_eq!(h.current(), "abc");
        assert_eq!(h.undo().map(String::as_str), Some("ab"));
        assert_eq!(h.undo().map(String::as_str), Some("a"));
        assert_eq!(h.undo().map(String::as_str), Some("0"));
        assert!(!h.can_undo());
    }

    #[test]
    fn struct_breaks_coalesce_chain() {
        let mut h = fresh();
        // text, struct, text — within the window, but struct dis-arms so
        // the following text can't fold into it: 3 distinct undo steps.
        h.record_text("t1".into());
        h.record_struct("s1".into());
        h.record_text("t2".into());
        assert_eq!(h.current(), "t2");
        assert_eq!(h.undo().map(String::as_str), Some("s1"));
        assert_eq!(h.undo().map(String::as_str), Some("t1"));
        assert_eq!(h.undo().map(String::as_str), Some("0"));
        assert!(!h.can_undo());
    }

    #[test]
    fn two_struct_edits_never_coalesce() {
        let mut h = fresh();
        h.record_struct("s1".into());
        h.record_struct("s2".into());
        assert_eq!(h.undo().map(String::as_str), Some("s1"));
        assert_eq!(h.undo().map(String::as_str), Some("0"));
    }

    #[test]
    fn undo_then_record_drops_redo_tail() {
        let mut h = fresh();
        h.record_struct("s1".into());
        h.record_struct("s2".into());
        assert!(h.can_redo() == false); // at the tip
        h.undo(); // back to s1
        assert!(h.can_redo());
        // A new edit after undo must drop the redo branch (s2 is gone).
        h.record_struct("s3".into());
        assert!(!h.can_redo());
        assert_eq!(h.current(), "s3");
        assert_eq!(h.undo().map(String::as_str), Some("s1"));
    }

    #[test]
    fn redo_walks_forward() {
        let mut h = fresh();
        h.record_struct("s1".into());
        h.record_struct("s2".into());
        h.undo();
        h.undo();
        assert_eq!(h.current(), "0");
        assert_eq!(h.redo().map(String::as_str), Some("s1"));
        assert_eq!(h.redo().map(String::as_str), Some("s2"));
        assert!(h.redo().is_none());
        assert!(!h.can_redo());
    }

    #[test]
    fn overflow_drains_oldest_keeps_cursor_valid() {
        // cap 3: after enough struct pushes, the oldest entries drain.
        let mut h = History::with_window("0".into(), 3, Duration::ZERO, false);
        h.record_struct("a".into()); // [0,a]
        h.record_struct("b".into()); // [0,a,b]
        h.record_struct("c".into()); // [0,a,b,c] → drain → [a,b,c]
        h.record_struct("d".into()); // [a,b,c,d] → drain → [b,c,d]
        assert_eq!(h.current(), "d");
        assert!(h.can_undo());
        // Only 2 undos remain (stack is [b,c,d]); "0" and "a" are gone.
        assert_eq!(h.undo().map(String::as_str), Some("c"));
        assert_eq!(h.undo().map(String::as_str), Some("b"));
        assert!(!h.can_undo());
    }

    #[test]
    fn dedup_suppresses_noop_record() {
        // RON / .properties shape: a replayed identical snapshot is a no-op.
        let mut h = History::with_window("0".into(), 200, Duration::ZERO, true);
        h.record_struct("a".into());
        h.record_struct("a".into()); // identical → suppressed
        h.record_struct("a".into()); // identical → suppressed
        assert_eq!(h.current(), "a");
        // Only one real step beyond the seed.
        assert_eq!(h.undo().map(String::as_str), Some("0"));
        assert!(!h.can_undo());
    }

    #[test]
    fn no_dedup_keeps_identical_pushes() {
        // JSON/TOML/YAML shape: identical pushes each create a step.
        let mut h = History::with_window("0".into(), 200, Duration::ZERO, false);
        h.record_struct("a".into());
        h.record_struct("a".into());
        // Two distinct entries despite identical text.
        assert_eq!(h.undo().map(String::as_str), Some("a"));
        assert_eq!(h.undo().map(String::as_str), Some("0"));
    }

    #[test]
    fn fully_undone_then_redone_boundaries() {
        let mut h = fresh();
        h.record_struct("s1".into());
        h.record_struct("s2".into());
        h.undo();
        h.undo();
        assert!(!h.can_undo());
        assert!(h.can_redo());
        h.redo();
        h.redo();
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }
}
