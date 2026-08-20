//! `file_ops` domain — deleting project files, and taking it back.
//!
//! Both halves live here because they are one operation seen from two ends, and splitting
//! them is how they drift: the delete decides what makes the undo possible, and the undo
//! can only put back what the delete thought to keep.
//!
//! ## The delete records first
//!
//! Every file is written into the local history **before** it is unlinked, so what the
//! undo restores is the content that was actually there — not the last thing that happened
//! to be saved through the editor. It records even the files a save would skip: history
//! ignores what git ignores, because build output is regenerated rather than recovered,
//! but an explicit delete is not a save. If you pointed at it and pressed Delete, you may
//! want it back, and being told "that one was ignored" afterwards is no answer.
//!
//! ## What it will not do
//!
//! It will not leave the project (the store refuses a path outside the root), it will not
//! delete the root itself, and it does not touch the operating system's trash. The history
//! IS the trash here — which is the whole reason this waited for it: macOS has no API to
//! put a file back where it came from, so an undo built on the OS trash would have to find
//! the file by name and hope, and hope is not a thing to build an undo out of.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arbor_history::prelude::{HistoryStore, RecordCtx, RevisionKind};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// How many files one delete will record before it stops recording (and says so).
///
/// Not a limit on what is deleted — a limit on what is kept. Somebody clearing a
/// generated directory of forty thousand files is not going to undo it one blob at a
/// time, and spending minutes writing a history nobody will read is a worse outcome than
/// saying plainly that this one is not undoable.
const MAX_RECORDED: usize = 2_000;

/// Args for [`bennu_delete_paths`].
#[derive(Deserialize)]
pub struct DeleteArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Absolute paths to delete — files or directories.
    pub paths: Vec<String>,
}

/// One path that could not be deleted, and why.
#[derive(Serialize)]
pub struct DeleteFailure {
    pub path: String,
    pub error: String,
}

/// What a delete did.
#[derive(Serialize)]
pub struct DeleteResult {
    /// The files that are gone, absolute with forward slashes. Directories are reported
    /// through the files that were inside them: the caller's open tabs and its tree are
    /// keyed by file, and a directory name closes nothing.
    pub deleted: Vec<String>,
    /// How many of them the history kept — i.e. how many the undo can bring back.
    pub recorded: usize,
    /// The change set the whole delete shares. Hand it to [`bennu_undelete`].
    pub change: String,
    pub failed: Vec<DeleteFailure>,
}

/// Every file under `dir`, depth-first. Bounded by the caller's remaining budget so a
/// huge tree is walked only as far as it will be recorded.
fn files_under(dir: &Path, budget: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= budget {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, budget, out);
        } else {
            out.push(p);
        }
        if out.len() >= budget {
            return;
        }
    }
}

/// Write `file` into the history as a deletion, keeping its content first.
///
/// Two revisions and not one: the `Deleted` marker says the file's life ended, and the
/// content revision before it is what an undo reads. A file the history has never heard
/// of has no content revision at all, so recording one here is what makes the very first
/// delete of a never-edited file undoable.
fn record_deletion(st: &HistoryStore, file: &Path, ctx: &RecordCtx) -> bool {
    if let Ok(bytes) = std::fs::read(file) {
        // Refused when it is over the size ceiling, or identical to what is already the
        // newest revision — both fine, both leave something to restore.
        let _ = st.record(file, RevisionKind::Created, Some(&bytes), ctx);
    }
    st.record(file, RevisionKind::Deleted, None, ctx).ok().flatten().is_some()
}

/// Delete files and directories, keeping what they held so the delete can be undone.
///
/// The whole call shares one change set, so a delete of six files is one row in the
/// history timeline and one thing to undo — not six.
#[arbor_rpc::handler]
fn bennu_delete_paths(_ctx: &BennuState, args: DeleteArgs) -> Result<DeleteResult, String> {
    let st = crate::history::store(&args.root)?;
    let root = Path::new(&args.root);
    let change = format!("del-{}", now_ms());

    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    let mut recorded = 0usize;

    for raw in &args.paths {
        let path = PathBuf::from(raw);
        // Both guards before anything is touched: a path the store would refuse is a path
        // whose deletion could not be undone, and deleting the root is never what was meant.
        if st.rel(&path).is_err() {
            failed.push(DeleteFailure { path: raw.clone(), error: "outside the project".into() });
            continue;
        }
        if path == root {
            failed.push(DeleteFailure { path: raw.clone(), error: "that is the project root".into() });
            continue;
        }

        let is_dir = path.is_dir();
        let mut files = Vec::new();
        if is_dir {
            files_under(&path, MAX_RECORDED.saturating_sub(recorded), &mut files);
        } else {
            files.push(path.clone());
        }

        let title = if is_dir {
            format!("Deleted {}", st.rel(&path).unwrap_or_else(|_| raw.clone()))
        } else {
            "Deleted".to_string()
        };
        let ctx = RecordCtx { change: Some(change.clone()), title: Some(title), from: None };
        for f in &files {
            if recorded < MAX_RECORDED && record_deletion(&st, f, &ctx) {
                recorded += 1;
            }
        }

        let outcome =
            if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        match outcome {
            Ok(()) => deleted.extend(files.iter().map(|f| f.to_string_lossy().replace('\\', "/"))),
            Err(e) => failed.push(DeleteFailure { path: raw.clone(), error: e.to_string() }),
        }
    }

    Ok(DeleteResult { deleted, recorded, change, failed })
}

/// Args for [`bennu_undelete`].
#[derive(Deserialize)]
pub struct UndeleteArgs {
    pub root: String,
    /// The change set to put back — [`DeleteResult::change`].
    pub change: String,
}

/// What an undo did.
#[derive(Serialize)]
pub struct UndeleteResult {
    /// Absolute paths that are back.
    pub restored: Vec<String>,
    /// Paths that were left alone because something is there now. Skipped rather than
    /// overwritten: the one thing worse than losing a file is losing a different one while
    /// getting the first back.
    pub skipped: Vec<String>,
}

/// Put back everything a delete removed.
#[arbor_rpc::handler]
fn bennu_undelete(_ctx: &BennuState, args: UndeleteArgs) -> Result<UndeleteResult, String> {
    let st = crate::history::store(&args.root)?;
    let mut restored = Vec::new();
    let mut skipped = Vec::new();

    for rel in st.deleted_in_change(&args.change) {
        let target = st.abs(&rel);
        if target.exists() {
            skipped.push(target.to_string_lossy().replace('\\', "/"));
            continue;
        }
        let Ok(bytes) = st.last_content(&rel) else {
            skipped.push(target.to_string_lossy().replace('\\', "/"));
            continue;
        };
        if let Some(parent) = target.parent() {
            // The directories went with the files; a restore has to make them again.
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&target, &bytes) {
            // Recorded as a state like any other, so the timeline shows the file coming
            // back rather than jumping from "deleted" to whatever happens to it next.
            Ok(()) => {
                let _ = st.record(
                    &target,
                    RevisionKind::Created,
                    Some(&bytes),
                    &RecordCtx {
                        change: None,
                        title: Some("Restored — delete undone".into()),
                        from: None,
                    },
                );
                restored.push(target.to_string_lossy().replace('\\', "/"));
            }
            Err(_) => skipped.push(target.to_string_lossy().replace('\\', "/")),
        }
    }
    Ok(UndeleteResult { restored, skipped })
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
