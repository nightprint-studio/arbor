//! Projects the user did not open: the isolated copy an assistant conversation works in.
//!
//! A copy that is **indexed** (so every tool works on it) goes through the same engines as a real project,
//! but it must not show up as one: not in Ctrl+Tab, not in cross-project searches, and its indexing
//! progress must not drive the editor's "Indexing…" card. This is the one registry that says which roots
//! those are; the places that would otherwise list or announce them ask here.
//!
//! Progress is not lost — [`relay_progress`] sends it to the conversation that owns the copy instead.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use arbor_ipc::prelude::EventSink;
use serde_json::json;

/// Normalised root → the conversation that owns it.
static HIDDEN: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

fn norm(root: &str) -> String {
    let k = root.replace('\\', "/");
    let k = k.trim_end_matches('/');
    if cfg!(windows) { k.to_lowercase() } else { k.to_string() }
}

/// Mark `root` as a copy owned by `conversation`.
pub(crate) fn hide(root: &str, conversation: &str) {
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).insert(norm(root), conversation.to_string());
}

pub(crate) fn unhide(root: &str) {
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).remove(&norm(root));
}

pub(crate) fn is_hidden(root: &str) -> bool {
    HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&norm(root))
}

/// If `root` is a hidden copy: tell its conversation how the indexing is going (and swallow the event the
/// editor would have taken for a project of the user's). Returns whether it was hidden.
pub(crate) fn relay_progress(sink: &Arc<dyn EventSink>, root: &str, phase: &str, state: &str) -> bool {
    let Some(conversation) = HIDDEN.lock().unwrap_or_else(|p| p.into_inner()).get(&norm(root)).cloned() else { return false };
    sink.emit(
        "arbor://bennu/assistant",
        json!({ "kind": "worktree", "conversation_id": conversation, "phase": "indexing", "index_phase": phase, "state": state }),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_root_is_found_under_any_spelling_and_forgotten_when_unhidden() {
        hide("C:\\Data\\wt\\c-1\\tree\\", "c-1");
        assert!(is_hidden("C:/Data/wt/c-1/tree"));
        assert!(!is_hidden("C:/Data/wt/c-2/tree"));
        unhide("C:/Data/wt/c-1/tree");
        assert!(!is_hidden("C:\\Data\\wt\\c-1\\tree"));
    }
}
