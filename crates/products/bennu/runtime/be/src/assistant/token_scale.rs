//! Each conversation's gap between the character estimate and the provider's count.
//!
//! Learned from every request the provider counts (`bennu_assistant::tokens::scale_of`), kept here for the run
//! in progress and saved with the conversation, so its next run starts measured right. Everything Bennu estimates
//! about a conversation's size goes through it: the meter, the compaction threshold, the nudge to hand work to a
//! sub-agent. Read from the run's thread and written from it; the panel's handlers only read.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bennu_assistant::prelude::scaled_tokens;

use super::store::Context;

fn scales() -> &'static Mutex<HashMap<String, f64>> {
    static SCALES: OnceLock<Mutex<HashMap<String, f64>>> = OnceLock::new();
    SCALES.get_or_init(Default::default)
}

/// The conversation's ratio; 1 when nothing was counted yet.
pub(super) fn get(conversation: &str) -> f64 {
    scales().lock().ok().and_then(|m| m.get(conversation).copied()).unwrap_or(1.0)
}

/// A request was counted: this is the ratio now.
pub(super) fn set(conversation: &str, scale: f64) {
    if let Ok(mut m) = scales().lock() {
        m.insert(conversation.to_string(), scale);
    }
}

/// A run begins: the ratio saved with the conversation, unless this session already learned a newer one.
pub(super) fn seed(conversation: &str, saved: Option<f64>) {
    if let (Some(saved), Ok(mut m)) = (saved, scales().lock()) {
        m.entry(conversation.to_string()).or_insert(saved);
    }
}

/// The conversation is gone.
pub(super) fn forget(conversation: &str) {
    if let Ok(mut m) = scales().lock() {
        m.remove(conversation);
    }
}

/// The meter's figures corrected by `scale`.
pub(super) fn context(c: Context, scale: f64) -> Context {
    let s = |n: u64| scaled_tokens(n, scale);
    Context {
        system: s(c.system),
        project: s(c.project),
        conversation: s(c.conversation),
        tools: s(c.tools),
        instructions: s(c.instructions),
        skills: s(c.skills),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_ratio_seeds_a_run_and_a_newer_one_wins() {
        let conv = "c-token-scale-test";
        assert_eq!(get(conv), 1.0);
        seed(conv, Some(1.8));
        assert_eq!(get(conv), 1.8);
        set(conv, 1.9);
        seed(conv, Some(1.8));
        assert_eq!(get(conv), 1.9, "what this session learned is newer than what was saved");
        let c = context(Context { system: 10_000, conversation: 100_000, ..Context::default() }, 2.0);
        assert_eq!((c.system, c.conversation), (20_000, 200_000));
        forget(conv);
        assert_eq!(get(conv), 1.0);
    }
}
