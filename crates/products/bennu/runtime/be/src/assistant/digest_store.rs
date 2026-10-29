//! Where the full text behind a digest waits for the model to ask for it.
//!
//! A big tool result is answered with a digest (see `bennu_assistant::digest`); the complete result is kept
//! here, per conversation, under a short id, and `expand_result` pages through it. Bounded — a conversation
//! keeps its few most recent — and in memory only: after a restart the id is gone, and the message says to run
//! the tool again, which is cheap next to keeping megabytes of old build output on disk.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

/// How many digests a conversation keeps expandable.
const KEEP: usize = 12;
/// The most text kept for one digest.
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct Shelf {
    next: u32,
    items: VecDeque<(String, String)>,
}

fn registry() -> &'static Mutex<HashMap<String, Shelf>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Shelf>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Keep `full` for `conversation`; returns the id to quote.
pub(super) fn store(conversation: &str, full: String) -> String {
    let mut all = registry().lock().unwrap_or_else(|p| p.into_inner());
    let shelf = all.entry(conversation.to_string()).or_default();
    shelf.next += 1;
    let id = format!("d{}", shelf.next);
    let text = if full.len() > MAX_BYTES { full.chars().take(MAX_BYTES / 2).collect() } else { full };
    shelf.items.push_back((id.clone(), text));
    while shelf.items.len() > KEEP {
        shelf.items.pop_front();
    }
    id
}

/// The text behind `id`, if it is still kept.
pub(super) fn get(conversation: &str, id: &str) -> Option<String> {
    let all = registry().lock().unwrap_or_else(|p| p.into_inner());
    all.get(conversation)?.items.iter().find(|(i, _)| i == id.trim()).map(|(_, t)| t.clone())
}

/// Forget a conversation's digests (it was deleted).
pub(super) fn forget(conversation: &str) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(conversation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_kept_under_an_id_and_the_oldest_make_room() {
        let conv = "digest-test-a";
        let first = store(conv, "one".into());
        assert_eq!(get(conv, &first).as_deref(), Some("one"));
        for i in 0..KEEP {
            store(conv, format!("n{i}"));
        }
        assert!(get(conv, &first).is_none(), "pushed out by newer ones");
        assert!(get(conv, " d13 ").is_some() || get(conv, "d13").is_some());
        assert!(get("another-conversation", "d1").is_none());
        forget(conv);
        assert!(get(conv, "d13").is_none());
    }
}
