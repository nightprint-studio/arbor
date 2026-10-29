//! Conversations on disk: one JSON file each, under the Bennu data directory.
//!
//! The file holds two views of one conversation, and both are needed. `messages` is what the panel
//! draws (text, tool receipts, proposals, questions — already in the shape the screen reads, so loading
//! is a read and not a translation); `transcript` is what the model is sent next time (the neutral
//! messages, tool calls and their results included). They are written together, so they cannot drift.
//!
//! A conversation is a file and not a row in a database because the access pattern is "open one, append
//! to it, list the titles" — and a file per conversation makes the last of those a directory read and
//! the first a single parse, with nothing to migrate and nothing one corrupt entry can take down.

use std::fs;
use std::path::{Path, PathBuf};

use bennu_assistant::prelude::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The conversation as stored. Field names are the panel's (camelCase), so `messages` and the header
/// fields go to the frontend untouched.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct Conversation {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
    pub model: String,
    pub cost: f64,
    /// Tokens by part, as the context bar shows them.
    pub context: Context,
    /// A chat: no tools, no access to files. Fixed when the conversation starts — the tools a conversation
    /// had are part of what its earlier messages mean, and the panel keeps the two kinds apart.
    #[serde(default)]
    pub chat: bool,
    /// What the panel draws.
    #[serde(default)]
    pub messages: Vec<Value>,
    /// What the model is sent. Not part of what the panel needs, so not sent to it.
    #[serde(default)]
    pub transcript: Vec<Message>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub(super) struct Context {
    pub system: u64,
    pub project: u64,
    pub conversation: u64,
}

/// A row of the conversation list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Summary {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
    pub model: String,
    pub cost: f64,
    pub context: Context,
    pub chat: bool,
}

pub(super) fn dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("assistant")
}

/// Ids become file names, so they are held to a plain alphabet: nothing here may name a path.
pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn file(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err(format!("`{id}` is not a conversation id"));
    }
    Ok(dir.join(format!("{id}.json")))
}

pub(super) fn save_in(dir: &Path, c: &Conversation) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = file(dir, &c.id)?;
    let json = serde_json::to_vec(c).map_err(|e| e.to_string())?;
    // Written beside and renamed over, so a crash mid-write leaves the previous version, not half a file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub(super) fn load_in(dir: &Path, id: &str) -> Result<Conversation, String> {
    let bytes = fs::read(file(dir, id)?).map_err(|e| format!("conversation `{id}`: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("conversation `{id}` could not be read: {e}"))
}

pub(super) fn delete_in(dir: &Path, id: &str) -> Result<(), String> {
    match fs::remove_file(file(dir, id)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Newest first. A file that does not parse is skipped, not fatal — one damaged conversation must not
/// hide the rest.
pub(super) fn list_in(dir: &Path) -> Vec<Summary> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Summary> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let id = e.path().file_stem()?.to_string_lossy().to_string();
            let c = load_in(dir, &id).ok()?;
            Some(Summary { id: c.id, title: c.title, updated_at: c.updated_at, model: c.model, cost: c.cost, context: c.context, chat: c.chat })
        })
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-assistant-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn conv(id: &str, at: u64) -> Conversation {
        Conversation { id: id.into(), title: format!("t-{id}"), updated_at: at, model: "m".into(), ..Default::default() }
    }

    #[test]
    fn a_conversation_round_trips_with_both_views() {
        let d = scratch("rt");
        let mut c = conv("c1", 5);
        c.messages = vec![serde_json::json!({"id": "m1", "role": "user"})];
        c.transcript = vec![Message::user("hello")];
        save_in(&d, &c).unwrap();
        let back = load_in(&d, "c1").unwrap();
        assert_eq!(back.messages, c.messages);
        assert_eq!(back.transcript, c.transcript);
    }

    #[test]
    fn a_chat_stays_a_chat_and_an_old_file_is_an_agent_conversation() {
        let d = scratch("chat");
        let mut c = conv("c1", 1);
        c.chat = true;
        save_in(&d, &c).unwrap();
        assert!(load_in(&d, "c1").unwrap().chat);
        assert!(list_in(&d)[0].chat);
        // A file written before the field existed has no `chat` and is read as an agent conversation.
        fs::write(d.join("old.json"), r#"{"id":"old","title":"t","updatedAt":1,"model":"m","cost":0,"context":{"system":0,"project":0,"conversation":0}}"#).unwrap();
        assert!(!load_in(&d, "old").unwrap().chat);
    }

    #[test]
    fn the_list_is_newest_first_and_skips_damaged_files() {
        let d = scratch("list");
        save_in(&d, &conv("old", 1)).unwrap();
        save_in(&d, &conv("new", 9)).unwrap();
        fs::write(d.join("broken.json"), "{ not json").unwrap();
        let ids: Vec<_> = list_in(&d).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["new", "old"]);
    }

    #[test]
    fn an_id_cannot_name_a_path() {
        let d = scratch("ids");
        for bad in ["../x", "a/b", "a.b", "", "a b"] {
            assert!(load_in(&d, bad).is_err(), "{bad:?}");
            assert!(delete_in(&d, bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn deleting_what_is_not_there_is_fine() {
        let d = scratch("del");
        assert!(delete_in(&d, "ghost").is_ok());
        save_in(&d, &conv("c", 1)).unwrap();
        delete_in(&d, "c").unwrap();
        assert!(list_in(&d).is_empty());
    }
}
