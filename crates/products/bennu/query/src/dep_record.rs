//! Per-file dependency **recording** for the incremental validation cache.
//!
//! Validating one file resolves every type / member it touches through the
//! [`IndexResolver`](crate::resolver::IndexResolver)'s two `TypeResolver` methods
//! (`members_of` / `resolve_simple_name`) — the single choke point every project lookup passes.
//! When a recording scope is active (a thread-local, so it composes with a future *parallel*
//! validation with zero shared state), those two methods report every PROJECT type they consult
//! here:
//!
//! - a **members hit** — a project type whose members were read (`binary` → hash of its
//!   members-JSON);
//! - a **simple hit** — a bare name that resolved to a project type (`simple` → resolved binary);
//! - a **miss** — a name probed against the project and found ABSENT (as a binary or a simple
//!   name). Recorded so that ADDING such a type later invalidates the file: a diagnostic that
//!   exists *because* a type was missing must be recomputed once the type appears (the "negative
//!   dependency").
//!
//! The recorded set is, by construction, a SUPERSET of everything a re-validation of the file
//! could read from the mutable project surface. JDK / library types are immutable within a
//! classpath epoch, so they are deliberately *not* recorded — the cache header's epoch guards
//! them wholesale. This superset property is what makes serving a cached diagnostic list safe: if
//! none of the recorded dependencies (nor the file's own bytes) changed, a fresh validation is
//! guaranteed to produce the identical diagnostics — no stale (false-positive) diagnostic can slip
//! through.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// FNV-1a over bytes — the fast, deterministic (cross-run stable) hash the recorder uses for a
/// project type's members-JSON. Matches the discipline in [`crate::resolver`]: record-time and
/// freshness-time both hash through this one function, so a dependency's stored hash and its
/// re-checked hash are directly comparable.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// What one file's validation read from the mutable project surface — the fingerprint inputs the
/// cache stores and later re-checks.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecordedDeps {
    /// Project types whose members were read: binary name → hash of its members-JSON.
    pub members: HashMap<String, u64>,
    /// Bare names that resolved to a project type: simple name → resolved binary name.
    pub simple_hits: HashMap<String, String>,
    /// Names probed against the project and found ABSENT (must stay absent for reuse).
    pub misses: HashSet<String>,
}

thread_local! {
    /// The active recorder for THIS thread, or `None` when not recording. Thread-local so a
    /// future parallel validation records each file's deps independently, with no shared state.
    static RECORDER: RefCell<Option<RecordedDeps>> = const { RefCell::new(None) };
}

/// Whether a recording scope is active on this thread — the cheap gate the resolver checks
/// before doing any recording work (one thread-local read on the `members_of` hot path; the
/// recording probes only run when this is `true`).
#[inline]
pub fn recording() -> bool {
    RECORDER.with(|r| r.borrow().is_some())
}

/// Record a project type whose members were read (`binary` present, with `members_hash`), or —
/// when `members_hash` is `None` — a MISS on `binary` (absent from the project). No-op when not
/// recording.
pub fn note_type(binary: &str, members_hash: Option<u64>) {
    RECORDER.with(|r| {
        if let Some(rec) = r.borrow_mut().as_mut() {
            match members_hash {
                Some(h) => {
                    rec.members.insert(binary.to_string(), h);
                }
                None => {
                    rec.misses.insert(binary.to_string());
                }
            }
        }
    });
}

/// Record a bare-name → project-type resolution (a "simple hit"). No-op when not recording.
pub fn note_simple_hit(simple: &str, binary: &str) {
    RECORDER.with(|r| {
        if let Some(rec) = r.borrow_mut().as_mut() {
            rec.simple_hits.insert(simple.to_string(), binary.to_string());
        }
    });
}

/// Record a bare name probed against the project and found ABSENT (a negative dependency). No-op
/// when not recording.
pub fn note_simple_miss(simple: &str) {
    RECORDER.with(|r| {
        if let Some(rec) = r.borrow_mut().as_mut() {
            rec.misses.insert(simple.to_string());
        }
    });
}

/// Run `f` with dependency recording active on this thread, returning its result paired with the
/// dependencies it recorded. The previous recorder (if any) is restored afterwards, so a nested
/// scope never leaks into an outer one (validation never nests, but this keeps the guard robust).
pub fn record<R>(f: impl FnOnce() -> R) -> (R, RecordedDeps) {
    let prev = RECORDER.with(|r| r.borrow_mut().replace(RecordedDeps::default()));
    let out = f();
    let deps = RECORDER.with(|r| {
        std::mem::replace(&mut *r.borrow_mut(), prev).unwrap_or_default()
    });
    (out, deps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_stable_and_distinguishes() {
        assert_eq!(fnv1a(b"abc"), fnv1a(b"abc"));
        assert_ne!(fnv1a(b"abc"), fnv1a(b"abd"));
        assert_ne!(fnv1a(b""), fnv1a(b"a"));
    }

    #[test]
    fn no_op_outside_a_recording_scope() {
        // These must be harmless when no scope is active (the resolver calls them unconditionally
        // once `recording()` is true, but a stray call outside must not panic / allocate globals).
        assert!(!recording());
        note_type("com/acme/Foo", Some(1));
        note_type("com/acme/Bar", None);
        note_simple_hit("Foo", "com/acme/Foo");
        note_simple_miss("Nope");
        assert!(!recording());
    }

    #[test]
    fn record_collects_hits_misses_and_simple_hits() {
        let (out, deps) = record(|| {
            assert!(recording(), "scope active inside the closure");
            note_type("com/acme/Order", Some(42));
            note_type("com/acme/Missing", None);
            note_simple_hit("Order", "com/acme/Order");
            note_simple_miss("Ghost");
            7
        });
        assert_eq!(out, 7);
        assert_eq!(deps.members.get("com/acme/Order"), Some(&42));
        assert!(deps.misses.contains("com/acme/Missing"));
        assert!(deps.misses.contains("Ghost"));
        assert_eq!(deps.simple_hits.get("Order").map(String::as_str), Some("com/acme/Order"));
        // Scope is torn down after `record` returns.
        assert!(!recording());
    }

    #[test]
    fn nested_scopes_restore_the_outer_recorder() {
        let (_outer_out, outer) = record(|| {
            note_type("A", Some(1));
            let (_i, inner) = record(|| {
                note_type("B", Some(2));
            });
            assert!(inner.members.contains_key("B"));
            assert!(!inner.members.contains_key("A"), "inner scope is isolated");
            // Back in the outer scope: recording is active again and sees the outer recorder.
            assert!(recording());
            note_type("C", Some(3));
        });
        assert!(outer.members.contains_key("A"));
        assert!(outer.members.contains_key("C"));
        assert!(!outer.members.contains_key("B"), "inner-scope dep never leaked out");
    }
}
