//! `history` domain — local history: what every project file used to be.
//!
//! The store itself is `arbor-history`, which knows only paths and bytes. What lives
//! here is everything that store deliberately refuses to know:
//!
//! * **which files are worth recording.** Bennu skips what git ignores — build output
//!   and generated artifacts are regenerated, not recovered, and recording them would
//!   spend the whole size budget on the part of the tree nobody misses. A note vault
//!   would answer this differently, which is exactly why the store does not answer it.
//! * **what the settings are.** They come from bennu's own config, so the same dialog
//!   that sets the editor font sets how long history is kept.
//! * **what text is.** A revision holds bytes; showing one means decoding it in the
//!   encoding that file is actually in, which is `bennu-project`'s job and not a
//!   content store's.
//!
//! ## Where the revisions come from
//!
//! Every door that writes a project file goes through [`on_write`] / [`on_rename`], so
//! there is one place that decides what gets recorded rather than one per feature. The
//! first time history hears about a file it also records what was on disk **before** the
//! write, as a baseline — without it, a refactor that lands on a file nobody ever saved
//! would have nothing behind it to go back to.

use std::path::{Path, PathBuf};

use arbor_history::prelude::{
    compare_text, ChangeGroup, DeletedEntry, FileHistory, FolderEntry, HistoryConfig,
    HistoryStore, RecordCtx, RevisionKind, TextDelta, Usage,
};
use bennu_core::prelude::BennuState;
use bennu_project::prelude::{decode_bytes, IgnoreStack};
use serde::{Deserialize, Serialize};

/// How many timeline rows a folder or project view asks for. Enough that scrolling is
/// worth it, small enough that the answer stays one message.
const TIMELINE_LIMIT: usize = 400;

/// Context lines around each change in a revision diff — the three everybody expects.
const DIFF_CONTEXT: usize = 3;

/// The store for `root`, configured from bennu's own settings.
///
/// Opened per call rather than cached. It is a directory handle and a config struct: the
/// expensive part is reading logs, which every call does anyway, and a cache keyed by
/// root is a cache that has to be invalidated when the settings change.
pub(crate) fn store(root: &str) -> Result<HistoryStore, String> {
    let cfg = bennu_core::config::load();
    let mb = |n: u64| n.saturating_mul(1024 * 1024);
    HistoryStore::open(
        &arbor_core::prelude::bennu_data_dir(),
        Path::new(root),
        HistoryConfig {
            enabled: cfg.local_history,
            keep_days: cfg.local_history_days,
            max_bytes: mb(cfg.local_history_max_mb),
            max_file_bytes: mb(cfg.local_history_max_file_mb),
        },
    )
    .map_err(|e| e.to_string())
}

/// Whether this file is worth a history.
///
/// Only the gitignore question — the size ceiling is the store's, and "is it inside the
/// project" is answered by the store refusing anything else. Deliberately fail-**open**:
/// an unreadable `.gitignore` means the file gets a history it maybe did not need, which
/// costs a few kilobytes. Failing the other way would silently stop recording, and
/// nobody discovers that until the moment they need the file back.
fn worth_recording(root: &Path, file: &Path) -> bool {
    let Some(parent) = file.parent() else { return true };
    let mut stack = IgnoreStack::at(parent);
    stack.enter(parent);
    let _ = root;
    !stack.is_ignored(file, file.is_dir())
}

// ── recording (called by the write paths, not by the frontend) ──────────────────

/// Record the state a write produced, plus a baseline if this is the first time history
/// has heard of the file.
///
/// `disk_before` is what the file held before the caller wrote — passed in rather than
/// read here, because by the time this runs the write has already happened.
pub(crate) fn on_write(root: &str, file: &str, disk_before: Option<Vec<u8>>, ctx: &RecordCtx) {
    let (root_path, file_path) = (Path::new(root), Path::new(file));
    if !worth_recording(root_path, file_path) {
        return;
    }
    let Ok(st) = store(root) else { return };

    // The baseline. Only when there is nothing behind this file at all: after the first
    // one, the previous save IS the "before".
    let empty = st.history(file_path).map(|h| h.revisions.is_empty()).unwrap_or(false);
    if empty {
        if let Some(before) = disk_before {
            let _ = st.record(file_path, RevisionKind::Created, Some(&before), &RecordCtx::default());
        }
    }
    let _ = st.record_from_disk(file_path, RevisionKind::Saved, ctx);
}

/// Record a rename: the old path is gone, the new one carries the content and a note of
/// where it came from.
///
/// Recorded as a **deletion** at the old path on purpose. From the point of view of
/// somebody looking for a file that is no longer where they left it, a move and a delete
/// are the same event, and the Deleted list is where they will look — the title says
/// which of the two it was.
pub(crate) fn on_rename(root: &str, old: &str, new: &str) {
    let Ok(st) = store(root) else { return };
    let (old_path, new_path) = (Path::new(old), Path::new(new));
    if worth_recording(Path::new(root), new_path) {
        let rel_old = st.rel(old_path).unwrap_or_else(|_| old.to_string());
        let _ = st.record_from_disk(
            new_path,
            RevisionKind::Renamed,
            &RecordCtx { change: None, title: None, from: Some(rel_old) },
        );
    }
    let rel_new = st.rel(new_path).unwrap_or_else(|_| new.to_string());
    let _ = st.record(
        old_path,
        RevisionKind::Deleted,
        None,
        &RecordCtx { change: None, title: Some(format!("Moved to {rel_new}")), from: None },
    );
}

/// Apply the retention policy in the background. Called when a project opens: it is the
/// one moment that is already slow for other reasons and that happens once per session.
pub(crate) fn purge_in_background(root: &str) {
    let root = root.to_string();
    std::thread::spawn(move || {
        if let Ok(st) = store(&root) {
            let _ = st.purge();
        }
    });
}

// ── handlers ────────────────────────────────────────────────────────────────────

/// Args naming one file in one project.
#[derive(Deserialize)]
pub struct FileArgs {
    pub root: String,
    pub file: String,
}

/// One file's revisions, newest first.
#[arbor_rpc::handler]
fn bennu_history_file(_ctx: &BennuState, args: FileArgs) -> Result<FileHistory, String> {
    store(&args.root)?.history(Path::new(&args.file)).map_err(|e| e.to_string())
}

/// Args for a directory view.
#[derive(Deserialize)]
pub struct FolderArgs {
    pub root: String,
    /// The directory to look at. Absent (or the root itself) means the whole project.
    #[serde(default)]
    pub dir: String,
    /// The moment to reconstruct, as unix milliseconds. Absent means now.
    #[serde(default)]
    pub at: Option<i64>,
}

/// What a folder held and what happened in it.
#[derive(Serialize)]
pub struct FolderHistory {
    /// The entries history knows about — merged by the frontend with the live tree,
    /// which is the only side that knows about files nobody ever edited.
    pub entries: Vec<FolderEntry>,
    /// One row per operation, newest first.
    pub timeline: Vec<ChangeGroup>,
}

/// A directory's history: what it held then, and what has happened in it since.
#[arbor_rpc::handler]
fn bennu_history_folder(_ctx: &BennuState, args: FolderArgs) -> Result<FolderHistory, String> {
    let st = store(&args.root)?;
    let dir = if args.dir.is_empty() { args.root.clone() } else { args.dir.clone() };
    Ok(FolderHistory {
        entries: st.folder(Path::new(&dir), args.at).map_err(|e| e.to_string())?,
        timeline: st.timeline(Path::new(&dir), TIMELINE_LIMIT).map_err(|e| e.to_string())?,
    })
}

/// Args naming a project.
#[derive(Deserialize)]
pub struct RootArgs {
    pub root: String,
}

/// Every file the history knows and the project no longer has, newest loss first.
///
/// The reason this is its own call and not a filter on something else: a deleted file has
/// no row in any tree to right-click, so the only way to reach its history is a list that
/// does not depend on the filesystem having it.
#[arbor_rpc::handler]
fn bennu_history_deleted(_ctx: &BennuState, args: RootArgs) -> Result<Vec<DeletedEntry>, String> {
    Ok(store(&args.root)?.deleted())
}

/// Args for reading one revision.
#[derive(Deserialize)]
pub struct RevisionArgs {
    pub root: String,
    pub file: String,
    /// The revision id. Absent means "the newest content this file ever had", which is
    /// what a deleted file is restored from.
    #[serde(default)]
    pub revision: Option<String>,
}

/// A revision's content, decoded.
#[derive(Serialize)]
pub struct RevisionContent {
    pub text: String,
    pub encoding: String,
    /// `false` when the bytes did not decode as text — a `.png` has a history like
    /// anything else, and the viewer must be told rather than shown mojibake.
    pub is_text: bool,
}

/// Read the bytes of one revision, in the project's own encoding.
fn revision_bytes(st: &HistoryStore, args: &RevisionArgs) -> Result<Vec<u8>, String> {
    match &args.revision {
        Some(id) => st.content(Path::new(&args.file), id).map_err(|e| e.to_string()),
        None => {
            let rel = st.rel(Path::new(&args.file)).map_err(|e| e.to_string())?;
            st.last_content(&rel).map_err(|e| e.to_string())
        }
    }
}

/// Decode `bytes` the way this project's files are decoded.
fn decode(root: &str, file: &str, bytes: &[u8]) -> (String, String, bool) {
    let cfg = bennu_core::config::load();
    let label = cfg
        .encoding_overrides
        .get(file)
        .or_else(|| cfg.encoding_overrides.get(root))
        .cloned()
        .unwrap_or_else(|| cfg.default_encoding.clone());
    // A NUL in the first kilobyte is the same "this is not text" test every diff tool
    // uses, and it is right far more often than any charset heuristic.
    let is_text = !bytes.iter().take(1024).any(|b| *b == 0);
    let (text, used) = decode_bytes(bytes, &label);
    (text, used, is_text)
}

/// One revision's content, as text.
#[arbor_rpc::handler]
fn bennu_history_content(
    _ctx: &BennuState,
    args: RevisionArgs,
) -> Result<RevisionContent, String> {
    let st = store(&args.root)?;
    let bytes = revision_bytes(&st, &args)?;
    let (text, encoding, is_text) = decode(&args.root, &args.file, &bytes);
    Ok(RevisionContent { text, encoding, is_text })
}

/// Args for comparing two revisions.
#[derive(Deserialize)]
pub struct DiffArgs {
    pub root: String,
    pub file: String,
    /// The older side.
    pub revision: String,
    /// The newer side. Absent means **what is on disk now**, which is the comparison the
    /// dialog opens on: "what would restoring this change?"
    #[serde(default)]
    pub against: Option<String>,
}

/// What changed between two revisions, or between one and the file as it stands.
#[arbor_rpc::handler]
fn bennu_history_diff(_ctx: &BennuState, args: DiffArgs) -> Result<TextDelta, String> {
    let st = store(&args.root)?;
    let old = st
        .content(Path::new(&args.file), &args.revision)
        .map_err(|e| e.to_string())?;
    let new = match &args.against {
        Some(id) => st.content(Path::new(&args.file), id).map_err(|e| e.to_string())?,
        // A file that is gone compares against nothing — which reads as "everything in
        // it was removed", and is the truth.
        None => std::fs::read(&args.file).unwrap_or_default(),
    };
    let (old_text, _, _) = decode(&args.root, &args.file, &old);
    let (new_text, _, _) = decode(&args.root, &args.file, &new);
    Ok(compare_text(&old_text, &new_text, DIFF_CONTEXT))
}

/// Args for putting a revision back.
#[derive(Deserialize)]
pub struct RestoreArgs {
    pub root: String,
    pub file: String,
    /// Absent restores the newest content the file ever had — how a deleted file comes
    /// back.
    #[serde(default)]
    pub revision: Option<String>,
    /// Where to put it. Absent means back where it was.
    #[serde(default)]
    pub to: Option<String>,
}

/// Where a restore landed.
#[derive(Serialize)]
pub struct RestoreResult {
    pub file: String,
}

/// Put a revision back on disk.
///
/// Writes the **bytes**, not a re-encoding of the text: history stored what the file
/// actually was, and a restore that hands back something merely equivalent is not a
/// restore. Refuses to clobber: a destination that already exists is an error, because
/// the one thing worse than losing a file is losing a different one while getting it back.
#[arbor_rpc::handler]
fn bennu_history_restore(_ctx: &BennuState, args: RestoreArgs) -> Result<RestoreResult, String> {
    let st = store(&args.root)?;
    let bytes = revision_bytes(
        &st,
        &RevisionArgs {
            root: args.root.clone(),
            file: args.file.clone(),
            revision: args.revision.clone(),
        },
    )?;
    let target = PathBuf::from(args.to.clone().unwrap_or_else(|| args.file.clone()));
    if args.to.is_some() && target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create directory: {e}"))?;
    }
    // What is being overwritten is itself worth a revision: restoring is a write like
    // any other, and the state you are leaving is the one you will want if the restore
    // turns out to be the wrong one.
    let before = std::fs::read(&target).ok();
    std::fs::write(&target, &bytes).map_err(|e| format!("restore: {e}"))?;
    let _ = st.record(
        &target,
        RevisionKind::Created,
        before.as_deref(),
        &RecordCtx::default(),
    );
    let _ = st.record_from_disk(
        &target,
        RevisionKind::Saved,
        &RecordCtx { change: None, title: Some("Restored from local history".into()), from: None },
    );
    Ok(RestoreResult { file: target.to_string_lossy().replace('\\', "/") })
}

/// Args for labelling a revision.
#[derive(Deserialize)]
pub struct LabelArgs {
    pub root: String,
    pub file: String,
    pub revision: String,
    /// The name. Empty clears it.
    pub label: String,
}

/// Pin a name on a moment. A labelled revision never expires.
#[arbor_rpc::handler]
fn bennu_history_label(_ctx: &BennuState, args: LabelArgs) -> Result<bool, String> {
    store(&args.root)?
        .label(Path::new(&args.file), &args.revision, &args.label)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

/// Args for recording what changed outside the editor.
#[derive(Deserialize)]
pub struct ExternalArgs {
    pub root: String,
    /// The files whose on-disk state no longer matches what the editor read.
    pub files: Vec<String>,
}

/// Record files that changed (or vanished) outside Bennu.
///
/// The one trigger the editor cannot infer from its own actions. It stores the state that
/// **arrived** — what the file used to hold is the revision before it, which is there
/// because every file gets a baseline the first time history hears about it. Also how an
/// external `rm` reaches the Deleted list: a file the editor watched and can no longer
/// stat records a deletion, and a deletion is what the Deleted scope lists.
#[arbor_rpc::handler]
fn bennu_history_external(_ctx: &BennuState, args: ExternalArgs) -> Result<usize, String> {
    let st = store(&args.root)?;
    let root = Path::new(&args.root);
    let mut n = 0;
    for f in &args.files {
        let path = Path::new(f);
        if !worth_recording(root, path) {
            continue;
        }
        if st
            .record_from_disk(path, RevisionKind::External, &RecordCtx::default())
            .ok()
            .flatten()
            .is_some()
        {
            n += 1;
        }
    }
    Ok(n)
}

/// What the history is costing, for the settings page.
#[arbor_rpc::handler]
fn bennu_history_usage(_ctx: &BennuState, args: RootArgs) -> Result<Usage, String> {
    Ok(store(&args.root)?.usage())
}

/// Throw away this project's whole history. The user asked.
#[arbor_rpc::handler]
fn bennu_history_clear(_ctx: &BennuState, args: RootArgs) -> Result<bool, String> {
    store(&args.root)?.clear().map(|_| true).map_err(|e| e.to_string())
}
