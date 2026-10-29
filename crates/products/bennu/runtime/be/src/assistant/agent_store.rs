//! Sub-agents on disk: what an agent was, said and concluded, kept for as long as its conversation keeps it.
//!
//! One file per agent under `assistant/agents/<conversation>/`. It holds the agent's record (title, task, status,
//! cost, summary), the thread the modal shows, and the thread the model was sent (`bennu_assistant::slim_for_storage`:
//! the files it read are named, not copied), so that after a restart the Agents list and the modal still work and
//! `resume_agent` can pick the agent up where it stopped.
//!
//! What ends a file: the agent being deleted, the conversation being deleted, or the conversation being compacted
//! (a compaction folds what the agents were for into the summary; their threads are not kept past it).
//!
//! Written beside and renamed over, like the conversations (`store`): a crash mid-write leaves the previous version.

use std::fs;
use std::path::{Path, PathBuf};

use bennu_assistant::prelude::{ChildInfo, Message};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store;

/// The most agents kept per conversation; past it the oldest finished ones go.
const MAX_KEPT: usize = 30;

/// An agent as kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Record {
    pub info: ChildInfo,
    /// The thread as the modal draws it.
    #[serde(default)]
    pub display: Option<Value>,
    /// The thread as the model was sent it, with long tool results replaced by a note.
    #[serde(default)]
    pub messages: Vec<Message>,
}

fn root() -> PathBuf {
    store::dir().join("agents")
}

/// An agent id (`r18f3a.a2`) as a file name.
fn file_stem(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn folder(root: &Path, conversation: &str) -> Result<PathBuf, String> {
    if !store::valid_id(conversation) {
        return Err(format!("`{conversation}` is not a conversation id"));
    }
    Ok(root.join(conversation))
}

fn path(root: &Path, conversation: &str, id: &str) -> Result<PathBuf, String> {
    Ok(folder(root, conversation)?.join(format!("{}.json", file_stem(id))))
}

// ── on a given root (tested) ─────────────────────────────────────────────────────────────────────

fn save_in(root: &Path, conversation: &str, record: &Record) -> Result<(), String> {
    let file = path(root, conversation, &record.info.id)?;
    let dir = file.parent().ok_or("no folder")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    prune_in(root, conversation);
    Ok(())
}

fn read(file: &Path) -> Option<Record> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

fn load_in(root: &Path, conversation: &str, id: &str) -> Option<Record> {
    read(&path(root, conversation, id).ok()?)
}

/// Every agent of a conversation, oldest first. A file that does not parse is skipped.
fn list_in(root: &Path, conversation: &str) -> Vec<Record> {
    let Ok(dir) = folder(root, conversation) else { return Vec::new() };
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Record> = entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).filter_map(|e| read(&e.path())).collect();
    out.sort_by_key(|r| r.info.started_at);
    out
}

fn find_in(root: &Path, id: &str) -> Option<(String, Record)> {
    let name = format!("{}.json", file_stem(id));
    for entry in fs::read_dir(root).ok()?.flatten() {
        let candidate = entry.path().join(&name);
        if let Some(record) = read(&candidate) {
            if record.info.id == id {
                return Some((entry.file_name().to_string_lossy().to_string(), record));
            }
        }
    }
    None
}

fn delete_in(root: &Path, conversation: &str, id: &str) {
    if let Ok(file) = path(root, conversation, id) {
        let _ = fs::remove_file(file);
    }
}

fn delete_all_in(root: &Path, conversation: &str, except: &[String]) {
    if except.is_empty() {
        if let Ok(dir) = folder(root, conversation) {
            let _ = fs::remove_dir_all(dir);
        }
        return;
    }
    for record in list_in(root, conversation) {
        if !except.contains(&record.info.id) {
            delete_in(root, conversation, &record.info.id);
        }
    }
}

/// Keep the newest [`MAX_KEPT`]: a conversation that starts agents all day does not grow without end.
fn prune_in(root: &Path, conversation: &str) {
    let records = list_in(root, conversation);
    let finished: Vec<&Record> = records.iter().filter(|r| !r.info.status.is_running()).collect();
    if finished.len() > MAX_KEPT {
        for old in &finished[..finished.len() - MAX_KEPT] {
            delete_in(root, conversation, &old.info.id);
        }
    }
}

// ── on the assistant's data folder ───────────────────────────────────────────────────────────────

pub(super) fn save(conversation: &str, record: &Record) {
    if let Err(e) = save_in(&root(), conversation, record) {
        eprintln!("assistant: could not keep agent {}: {e}", record.info.id);
    }
}

pub(super) fn load(conversation: &str, id: &str) -> Option<Record> {
    load_in(&root(), conversation, id)
}

pub(super) fn list(conversation: &str) -> Vec<Record> {
    list_in(&root(), conversation)
}

/// An agent by id alone, with the conversation it belongs to — for a card in a thread opened after a restart.
pub(super) fn find(id: &str) -> Option<(String, Record)> {
    find_in(&root(), id)
}

pub(super) fn delete(conversation: &str, id: &str) {
    delete_in(&root(), conversation, id);
}

/// All of a conversation's agents, or all but the ones named (those still running).
pub(super) fn delete_all(conversation: &str, except: &[String]) {
    delete_all_in(&root(), conversation, except);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::ChildStatus;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bennu-agent-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn record(id: &str, status: ChildStatus, started: u64) -> Record {
        Record {
            info: ChildInfo {
                id: id.into(),
                conversation_id: "c1".into(),
                run_id: "r1".into(),
                title: format!("t-{id}"),
                task: "the task".into(),
                tools: "read".into(),
                model: "m".into(),
                status,
                cost: 0.5,
                started_at: started,
                ended_at: None,
                summary: Some("s".into()),
                error: None,
                waiting: None,
            },
            display: Some(json!({"blocks": [{"type": "text", "text": "hello"}]})),
            messages: vec![Message::user("the task")],
        }
    }

    #[test]
    fn an_agent_round_trips_with_both_threads() {
        let d = scratch("rt");
        let r = record("r1.a1", ChildStatus::Done, 5);
        save_in(&d, "c1", &r).unwrap();
        let back = load_in(&d, "c1", "r1.a1").unwrap();
        assert_eq!(back.info.title, "t-r1.a1");
        assert_eq!(back.info.status, ChildStatus::Done);
        assert_eq!(back.messages, r.messages);
        assert_eq!(back.display, r.display);
    }

    #[test]
    fn a_card_opened_after_a_restart_finds_its_agent_by_id_alone() {
        let d = scratch("find");
        save_in(&d, "c1", &record("r1.a1", ChildStatus::Done, 1)).unwrap();
        save_in(&d, "c2", &record("r2.a1", ChildStatus::Done, 2)).unwrap();
        let (conversation, record) = find_in(&d, "r2.a1").unwrap();
        assert_eq!((conversation.as_str(), record.info.id.as_str()), ("c2", "r2.a1"));
        assert!(find_in(&d, "r9.a9").is_none());
    }

    #[test]
    fn the_list_is_oldest_first_and_skips_damaged_files() {
        let d = scratch("list");
        save_in(&d, "c1", &record("r1.a2", ChildStatus::Done, 9)).unwrap();
        save_in(&d, "c1", &record("r1.a1", ChildStatus::Done, 3)).unwrap();
        fs::write(d.join("c1").join("broken.json"), "{ nope").unwrap();
        let ids: Vec<String> = list_in(&d, "c1").into_iter().map(|r| r.info.id).collect();
        assert_eq!(ids, vec!["r1.a1", "r1.a2"]);
    }

    #[test]
    fn deleting_one_all_or_all_but_the_running_ones() {
        let d = scratch("del");
        save_in(&d, "c1", &record("r1.a1", ChildStatus::Done, 1)).unwrap();
        save_in(&d, "c1", &record("r1.a2", ChildStatus::Running, 2)).unwrap();
        save_in(&d, "c1", &record("r1.a3", ChildStatus::Failed, 3)).unwrap();
        delete_in(&d, "c1", "r1.a1");
        assert!(load_in(&d, "c1", "r1.a1").is_none());
        delete_all_in(&d, "c1", &["r1.a2".to_string()]);
        assert!(load_in(&d, "c1", "r1.a2").is_some() && load_in(&d, "c1", "r1.a3").is_none(), "a compaction leaves the one still running");
        delete_all_in(&d, "c1", &[]);
        assert!(!d.join("c1").exists(), "a deleted conversation takes the whole folder");
        delete_in(&d, "c1", "ghost");
    }

    #[test]
    fn only_the_newest_finished_agents_are_kept() {
        let d = scratch("prune");
        for n in 0..(MAX_KEPT as u64 + 5) {
            save_in(&d, "c1", &record(&format!("r1.a{n}"), ChildStatus::Done, n)).unwrap();
        }
        let kept = list_in(&d, "c1");
        assert_eq!(kept.len(), MAX_KEPT);
        assert_eq!(kept[0].info.started_at, 5, "the oldest five went");
    }

    #[test]
    fn an_id_cannot_name_a_path() {
        let d = scratch("ids");
        assert!(save_in(&d, "../x", &record("r1.a1", ChildStatus::Done, 1)).is_err());
        save_in(&d, "c1", &record("../../etc/x", ChildStatus::Done, 1)).unwrap();
        assert!(d.join("c1").join("______etc_x.json").exists(), "an agent id is flattened, never joined");
    }
}
