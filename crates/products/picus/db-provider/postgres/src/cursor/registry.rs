//! The held results of one session, and the policy that stops them leaking.
//!
//! A `WITH HOLD` cursor is server-side storage on somebody's database. Every one
//! that is opened is eventually closed — the only question is by what. Four things
//! close one, and between them they cover every way a caller can go away:
//!
//! | What closes it | When |
//! |---|---|
//! | an explicit close | the result the grid was showing is dismissed |
//! | eviction | a session already holds [`MAX_OPEN`] results and opens another |
//! | expiry | nothing has touched it for [`IDLE_TTL`] |
//! | the session closing | disconnect, reconnect, or the backend exiting |
//!
//! Eviction and expiry exist because the first row of that table cannot be relied
//! on. A window closed by killing it, a frontend that crashed, a `picus-be` whose
//! caller forgot — none of them send a close, and the server would keep the
//! tuplestore regardless. The policy is deliberately dull and stated rather than
//! adaptive: a rule you can read is one you can predict at three in the morning.
//!
//! This registry only *decides*. Issuing the `CLOSE` is the session's job, because
//! that needs the connection and this type is a lock — and a lock guard must never
//! be held across an `await`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio_postgres::types::Type;

/// How many results one session may hold at once.
///
/// Generous on purpose — several query tabs and several table tabs on one
/// connection is an ordinary afternoon, and evicting a result the user is still
/// scrolling would be worse than the storage it saves. It is a backstop against a
/// caller that never closes anything, not a budget anyone should feel.
pub const MAX_OPEN: usize = 16;

/// How long a result survives with nobody asking it anything.
///
/// Half an hour: a grid being scrolled is touched every few seconds, so thirty
/// minutes of silence means the tab was abandoned rather than paused. Short enough
/// that a forgotten result is not still pinning server storage tomorrow morning.
pub const IDLE_TTL: Duration = Duration::from_secs(30 * 60);

/// What the session needs to serve a window over a held result.
#[derive(Debug, Clone)]
pub struct CursorHandle {
    /// The **server-side** cursor name. Generated here, never received: the id the
    /// caller sends is only ever a key into this map, so no string from the wire
    /// reaches a `DECLARE`.
    pub name: String,
    /// Column names and types from the declaring `prepare`, kept so a later window
    /// maps its values the same way the first one did. `None` when the statement
    /// was not preparable — then the columns are simply untyped, as they were on
    /// the first window.
    pub types: Option<Vec<(String, Type)>>,
}

struct Entry {
    handle: CursorHandle,
    last_used: Instant,
}

/// Every result one session is holding.
pub struct CursorRegistry {
    next: AtomicU64,
    open: Mutex<HashMap<String, Entry>>,
    max_open: usize,
    idle_ttl: Duration,
}

impl Default for CursorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorRegistry {
    pub fn new() -> Self {
        Self::with_policy(MAX_OPEN, IDLE_TTL)
    }

    /// The same registry with a policy of the caller's choosing — the tests', in
    /// practice, which must not wait half an hour to prove expiry works.
    pub fn with_policy(max_open: usize, idle_ttl: Duration) -> Self {
        Self {
            next: AtomicU64::new(0),
            open: Mutex::new(HashMap::new()),
            max_open,
            idle_ttl,
        }
    }

    /// A fresh result id, unique within this session.
    ///
    /// Also the cursor's name on the server: one identity rather than two, so a
    /// `CLOSE` that fires from a sweep and one that fires from the caller cannot
    /// possibly name different things. Fixed prefix and a counter — nothing derived
    /// from the SQL, so no user text ever reaches an identifier.
    pub fn next_id(&self) -> String {
        format!("picus_cur_{}", self.next.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// Record a cursor the server has just accepted.
    ///
    /// Returns the names of any results evicted to stay inside the budget — the
    /// caller must `CLOSE` them. Least recently used goes first, which is the one
    /// the user is least likely to scroll next.
    pub fn register(&self, id: &str, handle: CursorHandle, now: Instant) -> Vec<String> {
        let mut open = self.lock();
        open.insert(id.to_string(), Entry { handle, last_used: now });

        let mut evicted = Vec::new();
        while open.len() > self.max_open {
            let Some(oldest) = open
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(entry) = open.remove(&oldest) {
                evicted.push(entry.handle.name);
            }
        }
        evicted
    }

    /// The handle for a result, marking it used so it does not expire under a user
    /// who is still reading it. `None` when the id names nothing — which is what a
    /// window arriving after a close looks like, and is an error worth reporting
    /// rather than an empty page pretending the result ended.
    pub fn touch(&self, id: &str, now: Instant) -> Option<CursorHandle> {
        let mut open = self.lock();
        let entry = open.get_mut(id)?;
        entry.last_used = now;
        Some(entry.handle.clone())
    }

    /// Forget a result, returning its cursor name so the caller can `CLOSE` it.
    /// `None` on an id that is already gone — which is exactly what makes closing
    /// idempotent.
    pub fn remove(&self, id: &str) -> Option<String> {
        self.lock().remove(id).map(|e| e.handle.name)
    }

    /// Forget everything untouched for longer than the TTL, returning their cursor
    /// names to close.
    pub fn expired(&self, now: Instant) -> Vec<String> {
        let ttl = self.idle_ttl;
        let mut open = self.lock();
        let stale: Vec<String> = open
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_used) >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        stale.iter().filter_map(|k| open.remove(k)).map(|e| e.handle.name).collect()
    }

    /// Forget everything, returning their cursor names. The session is closing.
    pub fn drain(&self) -> Vec<String> {
        self.lock().drain().map(|(_, e)| e.handle.name).collect()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A poisoned lock here means another thread panicked mid-bookkeeping; the map
    /// is still a map, and refusing to serve any further window over it would turn
    /// one panic into a dead connection.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(name: &str) -> CursorHandle {
        CursorHandle { name: name.to_string(), types: None }
    }

    #[test]
    fn ids_are_unique_and_carry_no_user_text() {
        let reg = CursorRegistry::new();
        let a = reg.next_id();
        let b = reg.next_id();
        assert_ne!(a, b);
        assert!(a.starts_with("picus_cur_"), "{a}");
        // A generated identifier is what lets `DECLARE` interpolate a name at all.
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{a}");
    }

    #[test]
    fn a_registered_result_can_be_found_and_closed_once() {
        let reg = CursorRegistry::new();
        let now = Instant::now();
        let id = reg.next_id();
        assert!(reg.register(&id, handle(&id), now).is_empty());

        assert_eq!(reg.touch(&id, now).map(|h| h.name), Some(id.clone()));
        assert_eq!(reg.remove(&id).as_deref(), Some(id.as_str()));
        // The second close is the idempotent one: nothing to do, and not an error.
        assert_eq!(reg.remove(&id), None);
        assert!(reg.touch(&id, now).is_none());
        assert!(reg.is_empty());
    }

    #[test]
    fn several_results_coexist() {
        // Query tabs share a connection; a second result must not disturb the first.
        let reg = CursorRegistry::new();
        let now = Instant::now();
        let ids: Vec<String> = (0..4).map(|_| reg.next_id()).collect();
        for id in &ids {
            reg.register(id, handle(id), now);
        }
        assert_eq!(reg.len(), 4);
        for id in &ids {
            assert!(reg.touch(id, now).is_some(), "{id} was disturbed");
        }
    }

    #[test]
    fn the_budget_evicts_the_least_recently_used() {
        let reg = CursorRegistry::with_policy(2, IDLE_TTL);
        let t0 = Instant::now();

        reg.register("a", handle("a"), t0);
        reg.register("b", handle("b"), t0 + Duration::from_secs(1));
        // `a` is the oldest — until it is read again.
        reg.touch("a", t0 + Duration::from_secs(2));

        let evicted = reg.register("c", handle("c"), t0 + Duration::from_secs(3));
        assert_eq!(evicted, vec!["b".to_string()], "the one nobody was scrolling");
        assert!(reg.touch("a", t0 + Duration::from_secs(4)).is_some());
        assert!(reg.touch("c", t0 + Duration::from_secs(4)).is_some());
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn disuse_expires_a_result_and_use_keeps_it() {
        let ttl = Duration::from_secs(60);
        let reg = CursorRegistry::with_policy(MAX_OPEN, ttl);
        let t0 = Instant::now();

        reg.register("idle", handle("idle"), t0);
        reg.register("busy", handle("busy"), t0);

        let later = t0 + Duration::from_secs(59);
        assert!(reg.expired(later).is_empty(), "not yet");

        reg.touch("busy", later);
        let much_later = t0 + Duration::from_secs(61);
        assert_eq!(reg.expired(much_later), vec!["idle".to_string()]);
        assert!(reg.touch("busy", much_later).is_some(), "still being read");
    }

    #[test]
    fn closing_the_session_drains_everything() {
        let reg = CursorRegistry::new();
        let now = Instant::now();
        reg.register("a", handle("a"), now);
        reg.register("b", handle("b"), now);

        let mut names = reg.drain();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
        assert!(reg.is_empty());
        assert!(reg.drain().is_empty(), "draining twice is not an error either");
    }
}
