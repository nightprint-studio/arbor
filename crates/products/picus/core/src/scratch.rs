//! The query tabs nobody saved.
//!
//! A SQL scratchpad is where the real work happens: a `SELECT` refined eight times,
//! the `UPDATE` it turned into, and the `COMMIT` at the bottom nobody has run yet.
//! None of it is a file, so before this existed all of it went away with the window
//! — which made closing Picus something users learned to be careful about, and that
//! is the wrong thing to teach anybody about a tool.
//!
//! ## Not a document store
//!
//! What is kept is small and deliberately unambitious: the text of each open query
//! tab, its title and which connection it was bound to. No history, no versions, no
//! undo stack. Restoring a tab means "the text is where you left it", nothing more.
//!
//! ## Why JSON and not TOML
//!
//! Every other Picus file is TOML because a human edits it. This one holds
//! multi-line SQL with quotes, backslashes and dollar-quoting in it, and TOML's
//! multi-line string rules turn that into an escaping problem for no benefit —
//! nobody hand-edits their scratchpad through a config file.
//!
//! ## A corrupt file loses the scratchpad, never the studio
//!
//! Both reads fall back to "nothing was saved". A window that refused to open
//! because a scratch file was half-written would be a far worse failure than a lost
//! buffer, and the file is rewritten on the next keystroke anyway.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One unsaved query tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScratchTab {
    /// The tab's own id, so a restored tab keeps the identity its result and its
    /// pending edits were filed under within one session.
    pub id: String,
    pub title: String,
    /// The connection it was bound to. Kept even when that connection has since
    /// been deleted — the text is worth restoring either way, and the tab then
    /// simply opens unbound.
    #[serde(default)]
    pub connection_id: String,
    pub sql: String,
}

/// Everything the studio remembers about its unsaved tabs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scratch {
    #[serde(default)]
    pub tabs: Vec<ScratchTab>,
    /// Which of them was in front, by id. Empty when none.
    #[serde(default)]
    pub active: String,
}

/// `arbor/profiles/<active>/picus/scratch.json`.
pub fn scratch_path() -> PathBuf {
    arbor_core::prelude::picus_config_path("scratch.json")
}

/// Read the saved scratchpad. Missing or unreadable yields an empty one.
pub fn load_scratch() -> Scratch {
    let Ok(text) = std::fs::read_to_string(scratch_path()) else {
        return Scratch::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Write the scratchpad, creating the directory if needed.
///
/// Written whole every time rather than patched per tab: the file is a few kilobytes
/// and the alternative is a merge, which is a chance to lose a tab in exchange for
/// saving a write nobody was waiting on.
pub fn save_scratch(scratch: &Scratch) -> Result<(), String> {
    let path = scratch_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(scratch).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratchpad_round_trips_through_its_own_format() {
        // The point of the test: the SQL people actually keep in a scratchpad —
        // quotes, backslashes, dollar-quoting, accents, several lines — has to come
        // back byte for byte.
        let awkward = "-- prova d'accento\nSELECT 'C:\\temp\\a''b'\nFROM catalogo_widget\n$$ body $$;";
        let scratch = Scratch {
            tabs: vec![ScratchTab {
                id: "t1".into(),
                title: "scratch 1".into(),
                connection_id: "c1".into(),
                sql: awkward.to_string(),
            }],
            active: "t1".into(),
        };
        let text = serde_json::to_string(&scratch).expect("serialises");
        let back: Scratch = serde_json::from_str(&text).expect("parses");
        assert_eq!(back.tabs[0].sql, awkward);
        assert_eq!(back, scratch);
    }

    #[test]
    fn a_corrupt_file_reads_as_an_empty_scratchpad() {
        // Not a round-trip test — a direct one on the tolerance this file promises.
        assert_eq!(serde_json::from_str::<Scratch>("{ not json").ok(), None);
        let partial: Scratch = serde_json::from_str("{}").expect("an empty object is a scratchpad");
        assert!(partial.tabs.is_empty());
        assert!(partial.active.is_empty());
    }

    #[test]
    fn a_tab_with_no_connection_is_representable() {
        // A scratchpad opened before any connection existed, and a tab whose
        // connection was deleted since: both are the empty string, and both restore.
        let one: Scratch =
            serde_json::from_str(r#"{"tabs":[{"id":"a","title":"a","sql":"select 1"}]}"#)
                .expect("parses");
        assert_eq!(one.tabs[0].connection_id, "");
    }
}
