//! The undo/redo history of an open tab, kept on disk so closing Arbor does not cost it.
//!
//! The editor serialises a tab's history (CodeMirror's `historyField`) when the tab is left, and now and then
//! while it is edited; the panel hands it here, and asks for it back when the tab is opened without one in
//! memory — a restart. The history is only meaningful for the exact text it was recorded against, so the panel
//! sends a hash of that text along and the editor refuses a history whose hash is not the document's. This
//! module never looks inside: it stores a JSON value per (project, file) and gives it back.
//!
//! Layout: `bennu_data_dir()/editor-history/<hash of root>/<hash of file>.json`. Bounded per project, oldest
//! first out, so a year of opened files is not a year of files on disk.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bennu_core::prelude::BennuState;
use serde_json::{json, Value};

/// Files kept per project. A history is a few kilobytes to a few hundred; this is "every tab you could have open".
const KEEP_PER_PROJECT: usize = 200;
/// A history larger than this is not worth the write: it is an hour of typing in one file, and loading it back
/// would cost more than the undo it buys.
const MAX_BYTES: usize = 4 * 1024 * 1024;

/// FNV-1a, the same as the index directories use: short, stable, filesystem-safe names.
fn fnv(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn project_dir(root: &str) -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("editor-history").join(fnv(root))
}

fn file_in(dir: &Path, file: &str) -> PathBuf {
    dir.join(format!("{}.json", fnv(file)))
}

/// Write `bytes` to `path` through a sibling file, so a crash mid-write leaves the old history, not half a new one.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Drop the oldest histories of a project beyond [`KEEP_PER_PROJECT`].
fn trim(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut files: Vec<(SystemTime, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= KEEP_PER_PROJECT {
        return;
    }
    files.sort_by_key(|(at, _)| *at);
    let excess = files.len() - KEEP_PER_PROJECT;
    for (_, path) in files.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// Keep `history` for `file`, with the hash of the text it applies to. An empty history is forgotten instead.
#[arbor_rpc::handler]
fn bennu_editor_history_save(_ctx: &BennuState, root: String, file: String, hash: u32, history: Value) -> Result<(), String> {
    let dir = project_dir(&root);
    let path = file_in(&dir, &file);
    if is_empty(&history) {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let bytes = serde_json::to_vec(&json!({ "file": file, "hash": hash, "history": history })).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("editor history: {e}"))?;
    write_atomically(&path, &bytes).map_err(|e| format!("editor history: {e}"))?;
    trim(&dir);
    Ok(())
}

/// The kept history of `file` as `{ hash, history }`, or `null` when there is none (or it is unreadable — a
/// history is a convenience, never worth an error).
#[arbor_rpc::handler]
fn bennu_editor_history_load(_ctx: &BennuState, root: String, file: String) -> Result<Value, String> {
    let path = file_in(&project_dir(&root), &file);
    let Ok(bytes) = fs::read(&path) else { return Ok(Value::Null) };
    let Ok(mut stored) = serde_json::from_slice::<Value>(&bytes) else {
        let _ = fs::remove_file(&path);
        return Ok(Value::Null);
    };
    // A hash collision on the file name would hand one file the history of another; the name inside tells.
    if stored.get("file").and_then(Value::as_str) != Some(file.as_str()) {
        return Ok(Value::Null);
    }
    Ok(json!({ "hash": stored["hash"].take(), "history": stored["history"].take() }))
}

/// Forget the history of `file` — its tab was closed.
#[arbor_rpc::handler]
fn bennu_editor_history_forget(_ctx: &BennuState, root: String, file: String) -> Result<(), String> {
    let _ = fs::remove_file(file_in(&project_dir(&root), &file));
    Ok(())
}

/// CodeMirror's serialised history is `{ done: [...], undone: [...] }`; with both empty there is nothing to undo.
fn is_empty(history: &Value) -> bool {
    let len = |key: &str| history.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    history.is_null() || (len("done") == 0 && len("undone") == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_history_is_nothing_to_keep() {
        assert!(is_empty(&Value::Null));
        assert!(is_empty(&json!({ "done": [], "undone": [] })));
        assert!(!is_empty(&json!({ "done": [{}], "undone": [] })));
        assert!(!is_empty(&json!({ "done": [], "undone": [{}] })));
    }

    #[test]
    fn a_file_has_one_stable_name_per_project() {
        let a = file_in(Path::new("/d"), "/p/A.java");
        assert_eq!(a, file_in(Path::new("/d"), "/p/A.java"));
        assert_ne!(a, file_in(Path::new("/d"), "/p/B.java"));
        assert_ne!(project_dir("/p"), project_dir("/q"));
    }

    #[test]
    fn the_oldest_histories_go_first() {
        let dir = std::env::temp_dir().join(format!("bennu-eh-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..KEEP_PER_PROJECT + 3 {
            fs::write(dir.join(format!("{i:04}.json")), b"{}").unwrap();
        }
        trim(&dir);
        let left = fs::read_dir(&dir).unwrap().count();
        assert_eq!(left, KEEP_PER_PROJECT);
        let _ = fs::remove_dir_all(&dir);
    }
}
