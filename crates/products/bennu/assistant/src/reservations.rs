//! Who is editing which file, across conversations.
//!
//! Two conversations can run at once on the same project, and each has a model that edits files. Left alone
//! they overwrite each other's work without either noticing. This is the small, in-memory registry that
//! prevents it: a run **reserves** a file the first time it changes it, and holds it until the run ends
//! (finished, cancelled or crashed — the host releases by run id, and a time-to-live covers a host that
//! never got to).
//!
//! The rules are deliberately few:
//!
//! * Reads never reserve. Only a change does.
//! * A set of files is reserved **all or nothing, in sorted order**: a multi-file operation either holds
//!   every file it needs or none, and two operations over overlapping sets cannot each keep half.
//! * A conflict fails fast, naming who holds the file, so the model can wait or do something else rather
//!   than block on a lock.
//! * The same conversation never conflicts with itself.
//! * A proposal waiting for the user is **not** a reservation (nothing is written until they decide); the
//!   host leaves a soft note instead.
//!
//! Pure on purpose: time is passed in, so the expiry is testable without sleeping.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a reservation lives without being renewed. A run renews a file each time it changes it again;
/// this only matters when the run vanished without releasing.
pub const DEFAULT_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
struct Held {
    run: String,
    conversation: String,
    title: String,
    at: Instant,
}

/// Who holds a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The file as it was asked for.
    pub file: String,
    pub conversation: String,
    pub title: String,
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = if self.title.trim().is_empty() { String::new() } else { format!(" ({})", self.title.trim()) };
        write!(
            f,
            "`{}` is being edited by conversation {}{title}, which has not finished. Nothing was changed. Work on something else for now, or try this file again later.",
            self.file, self.conversation
        )
    }
}

/// One spelling of a path for comparison: forward slashes, no trailing slash, case-folded on Windows.
pub fn key_of(path: &str) -> String {
    let k = path.replace('\\', "/");
    let k = k.trim_end_matches('/');
    if cfg!(windows) { k.to_lowercase() } else { k.to_string() }
}

pub struct Registry {
    ttl: Duration,
    held: Mutex<HashMap<String, Held>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(DEFAULT_TTL)
    }
}

impl Registry {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, held: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Held>> {
        self.held.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Reserve every file in `files` for `conversation`'s `run`, or none of them.
    ///
    /// Files already held by the same conversation are renewed. The first file (in sorted order) held by
    /// another conversation is the one reported.
    pub fn reserve(&self, run: &str, conversation: &str, title: &str, files: &[String], now: Instant) -> Result<(), Conflict> {
        let mut wanted: Vec<(String, &String)> = files.iter().map(|f| (key_of(f), f)).collect();
        wanted.sort_by(|a, b| a.0.cmp(&b.0));
        wanted.dedup_by(|a, b| a.0 == b.0);

        let mut held = self.lock();
        held.retain(|_, h| now.saturating_duration_since(h.at) < self.ttl);
        for (key, file) in &wanted {
            if let Some(h) = held.get(key) {
                if h.conversation != conversation {
                    return Err(Conflict { file: (*file).clone(), conversation: h.conversation.clone(), title: h.title.clone() });
                }
            }
        }
        for (key, _) in wanted {
            held.insert(key, Held { run: run.to_string(), conversation: conversation.to_string(), title: title.to_string(), at: now });
        }
        Ok(())
    }

    /// Release everything a run holds: it ended, was cancelled, or crashed. Returns how many files.
    pub fn release_run(&self, run: &str) -> usize {
        let mut held = self.lock();
        let before = held.len();
        held.retain(|_, h| h.run != run);
        before - held.len()
    }

    /// Release everything a conversation holds (it was deleted).
    pub fn release_conversation(&self, conversation: &str) -> usize {
        let mut held = self.lock();
        let before = held.len();
        held.retain(|_, h| h.conversation != conversation);
        before - held.len()
    }

    /// Who holds `file` right now, if anyone but `except` (a conversation id) does.
    pub fn holder(&self, file: &str, except: &str, now: Instant) -> Option<Conflict> {
        let held = self.lock();
        let h = held.get(&key_of(file))?;
        if h.conversation == except || now.saturating_duration_since(h.at) >= self.ttl {
            return None;
        }
        Some(Conflict { file: file.to_string(), conversation: h.conversation.clone(), title: h.title.clone() })
    }

    /// How many files are held (expired ones included until the next reserve sweeps them).
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_file_held_by_another_conversation_is_refused_with_its_name() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "Fix the cart", &files(&["/p/A.java"]), now).unwrap();
        let err = r.reserve("r2", "c-b", "Other", &files(&["/p/A.java"]), now).unwrap_err();
        assert_eq!((err.conversation.as_str(), err.title.as_str()), ("c-a", "Fix the cart"));
        assert!(err.to_string().contains("conversation c-a (Fix the cart)"));
    }

    #[test]
    fn the_same_conversation_never_conflicts_with_itself() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["/p/A.java"]), now).unwrap();
        r.reserve("r2", "c-a", "t", &files(&["/p/A.java", "/p/B.java"]), now).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn a_set_is_taken_whole_or_not_at_all() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["/p/B.java"]), now).unwrap();
        let err = r.reserve("r2", "c-b", "u", &files(&["/p/A.java", "/p/B.java", "/p/C.java"]), now).unwrap_err();
        assert_eq!(err.file, "/p/B.java");
        assert_eq!(r.len(), 1, "A and C were not kept while B was refused");
        assert!(r.holder("/p/A.java", "c-b", now).is_none());
    }

    #[test]
    fn spellings_of_one_path_are_one_file() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["C:\\work\\app\\A.java"]), now).unwrap();
        let clash = r.reserve("r2", "c-b", "u", &files(&["C:/work/app/A.java"]), now);
        assert!(clash.is_err());
    }

    #[test]
    fn ending_a_run_frees_its_files_and_only_its_files() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["/p/A.java"]), now).unwrap();
        r.reserve("r2", "c-b", "u", &files(&["/p/B.java"]), now).unwrap();
        assert_eq!(r.release_run("r1"), 1);
        assert!(r.reserve("r3", "c-b", "u", &files(&["/p/A.java"]), now).is_ok());
        assert!(r.holder("/p/B.java", "c-a", now).is_some());
    }

    #[test]
    fn a_reservation_nobody_renewed_expires() {
        let r = Registry::new(Duration::from_secs(60));
        let t0 = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["/p/A.java"]), t0).unwrap();
        let later = t0 + Duration::from_secs(61);
        assert!(r.holder("/p/A.java", "c-b", later).is_none());
        assert!(r.reserve("r2", "c-b", "u", &files(&["/p/A.java"]), later).is_ok());
    }

    #[test]
    fn deleting_a_conversation_frees_whatever_it_held() {
        let r = Registry::default();
        let now = Instant::now();
        r.reserve("r1", "c-a", "t", &files(&["/p/A.java", "/p/B.java"]), now).unwrap();
        assert_eq!(r.release_conversation("c-a"), 2);
        assert!(r.is_empty());
    }
}
