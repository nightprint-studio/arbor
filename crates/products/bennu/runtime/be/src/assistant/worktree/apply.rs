//! Applying an isolated copy back to the project: the combined diff, and the all-or-nothing write.
//!
//! The decision of *what* to apply is the pure `plan_apply` in `bennu-assistant`; this module gathers its
//! inputs from disk and carries the plan out. The guarantee it keeps: **either every change lands or none
//! does**. New contents are first staged next to their targets; then each target is swapped in with its
//! original set aside, and any failure puts every original back. A file the project changed since the copy
//! was made is never overwritten: the whole apply is refused and each such file is named.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use arbor_history::prelude::compare_text;
use bennu_assistant::prelude::{plan_apply, ApplyPlan, Base, ChangeKind, Observation};
use serde_json::{json, Value};

use super::files::{identity, walk};
use super::{Meta, Stored};

/// Everything gathered to decide an apply.
pub(super) struct Gathered {
    pub plan: ApplyPlan,
    /// What each changed file looked like in the project when the plan was made, for the second look.
    pub real_seen: BTreeMap<String, Option<Base>>,
}

fn real_path(meta: &Meta, rel: &str) -> PathBuf {
    PathBuf::from(&meta.real_root).join(rel)
}

fn copy_path(meta: &Meta, rel: &str) -> PathBuf {
    PathBuf::from(&meta.copy_root).join(rel)
}

/// Compare the copy, the project and the manifest the copy started from, and plan the apply.
pub(super) fn gather(meta: &Meta, stored: &Stored) -> Result<Gathered, String> {
    let manifest = &stored.base;
    let copy = walk(Path::new(&meta.copy_root), None).map_err(|(_, w)| w).unwrap_or_else(|w| w);
    let mut now: BTreeMap<String, Base> = BTreeMap::new();
    for (rel, path) in &copy.files {
        // Same size as the snapshot almost always means same file, but only the hash proves it.
        if let Ok(id) = identity(path) {
            now.insert(rel.clone(), id);
        }
    }
    let rels: std::collections::BTreeSet<&String> = manifest.keys().chain(now.keys()).collect();
    let mut observations = Vec::new();
    let mut real_seen = BTreeMap::new();
    for rel in rels {
        let (base, copy_now) = (manifest.get(rel).copied(), now.get(rel).copied());
        if base == copy_now {
            continue;
        }
        let real = identity(&real_path(meta, rel)).ok();
        real_seen.insert(rel.clone(), real);
        // Where making the copy changed the bytes, the project is judged against what it had, not the copy.
        let real_base = stored.real.get(rel).copied().unwrap_or(base);
        observations.push(Observation { rel: rel.clone(), base, real_base, copy: copy_now, real });
    }
    Ok(Gathered { plan: plan_apply(&observations), real_seen })
}

fn read_text(path: &Path) -> Option<String> {
    String::from_utf8(fs::read(path).ok()?).ok()
}

/// The combined diff for the review dialog: one entry per change, with line counts and, for text files that
/// are not huge, the hunks.
pub(super) fn diff_json(meta: &Meta, gathered: &Gathered) -> Value {
    const HUNK_FILES: usize = 80;
    const HUNK_BYTES: u64 = 200 * 1024;
    let files: Vec<Value> = gathered
        .plan
        .changes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let old = if c.kind == ChangeKind::Added { None } else { read_text(&real_path(meta, &c.rel)) };
            let new = if c.kind == ChangeKind::Deleted { None } else { read_text(&copy_path(meta, &c.rel)) };
            let small = [real_path(meta, &c.rel), copy_path(meta, &c.rel)].iter().all(|p| fs::metadata(p).map(|m| m.len() <= HUNK_BYTES).unwrap_or(true));
            let text_pair = match (c.kind, &old, &new) {
                (ChangeKind::Added, _, Some(n)) => Some(("", n.as_str())),
                (ChangeKind::Deleted, Some(o), _) => Some((o.as_str(), "")),
                (ChangeKind::Modified, Some(o), Some(n)) => Some((o.as_str(), n.as_str())),
                _ => None,
            };
            match text_pair {
                Some((o, n)) => {
                    let d = compare_text(o, n, 3);
                    let hunks = if i < HUNK_FILES && small { json!(d.hunks) } else { Value::Null };
                    json!({ "path": c.rel, "kind": c.kind, "renamedFrom": c.renamed_from, "added": d.added, "removed": d.removed, "hunks": hunks })
                }
                None => json!({ "path": c.rel, "kind": c.kind, "renamedFrom": c.renamed_from, "binary": true }),
            }
        })
        .collect();
    json!({ "files": files, "clashes": gathered.plan.clashes, "canApply": gathered.plan.can_apply() })
}

/// One reversible step of the commit phase.
enum Undo {
    /// A file that did not exist: remove it.
    Remove(PathBuf),
    /// A file that was set aside: put it back over whatever is there now.
    Restore { aside: PathBuf, target: PathBuf },
}

fn temp_beside(target: &Path, tag: &str, n: usize) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    target.with_file_name(format!(".{name}.arbor-{tag}-{}-{n}", std::process::id()))
}

fn rollback(undo: Vec<Undo>) {
    for step in undo.into_iter().rev() {
        match step {
            Undo::Remove(path) => {
                let _ = fs::remove_file(path);
            }
            Undo::Restore { aside, target } => {
                let _ = fs::remove_file(&target);
                let _ = fs::rename(aside, target);
            }
        }
    }
}

/// Carry out a plan that has no clashes. Returns the number of files changed.
///
/// `plan` must come from [`gather`] a moment ago: the real files are checked once more against what the
/// plan saw (a second look costs a hash per changed file and closes most of the window in which the
/// project could move on).
pub(super) fn execute(meta: &Meta, gathered: &Gathered) -> Result<usize, String> {
    let plan = &gathered.plan;
    if !plan.can_apply() {
        return Err(bennu_assistant::prelude::clash_summary(&plan.clashes));
    }
    // Stage: every new content next to its target. Nothing real is touched yet.
    let mut staged: Vec<(usize, PathBuf)> = Vec::new();
    for (i, c) in plan.changes.iter().enumerate() {
        if c.kind == ChangeKind::Deleted {
            continue;
        }
        let target = real_path(meta, &c.rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", c.rel))?;
        }
        let temp = temp_beside(&target, "new", i);
        if let Err(e) = fs::copy(copy_path(meta, &c.rel), &temp) {
            staged.iter().for_each(|(_, t)| {
                let _ = fs::remove_file(t);
            });
            let _ = fs::remove_file(&temp);
            return Err(format!("{}: {e}. Nothing was applied.", c.rel));
        }
        staged.push((i, temp));
    }

    // The rest of the world may have moved while staging: look at every target once more.
    let manifest_drift: Vec<String> = plan
        .changes
        .iter()
        .filter(|c| identity(&real_path(meta, &c.rel)).ok() != gathered.real_seen.get(&c.rel).copied().flatten())
        .map(|c| c.rel.clone())
        .collect();
    if !manifest_drift.is_empty() {
        staged.iter().for_each(|(_, t)| {
            let _ = fs::remove_file(t);
        });
        return Err(format!("Nothing was applied: changed in the project while applying: {}", manifest_drift.join(", ")));
    }

    // Commit: swap each target in, keeping the original aside until everything has landed.
    let mut undo: Vec<Undo> = Vec::new();
    let mut asides: Vec<PathBuf> = Vec::new();
    let mut before_bytes: BTreeMap<usize, Option<Vec<u8>>> = BTreeMap::new();
    let temps: BTreeMap<usize, PathBuf> = staged.into_iter().collect();
    for (i, c) in plan.changes.iter().enumerate() {
        let target = real_path(meta, &c.rel);
        let outcome: Result<(), String> = (|| {
            if c.kind != ChangeKind::Added {
                before_bytes.insert(i, fs::read(&target).ok());
                let aside = temp_beside(&target, "old", i);
                fs::rename(&target, &aside).map_err(|e| e.to_string())?;
                undo.push(Undo::Restore { aside: aside.clone(), target: target.clone() });
                asides.push(aside);
            }
            if let Some(temp) = temps.get(&i) {
                fs::rename(temp, &target).map_err(|e| e.to_string())?;
                if c.kind == ChangeKind::Added {
                    undo.push(Undo::Remove(target.clone()));
                }
            }
            Ok(())
        })();
        if let Err(e) = outcome {
            rollback(undo);
            temps.values().for_each(|t| {
                let _ = fs::remove_file(t);
            });
            return Err(format!("{}: {e}. Nothing was applied: every file was put back as it was.", c.rel));
        }
    }
    asides.iter().for_each(|a| {
        let _ = fs::remove_file(a);
    });

    // Local History learns of each write, so an applied change can be undone from there.
    for (i, c) in plan.changes.iter().enumerate() {
        let target = real_path(meta, &c.rel);
        let before = before_bytes.remove(&i).flatten();
        if c.kind == ChangeKind::Deleted {
            // A deletion applied from the copy is as undoable as one made in the project.
            crate::file_ops::record_deleted_bytes(&meta.real_root, &target, before);
        } else {
            crate::history::on_write(&meta.real_root, &target.to_string_lossy(), before, &Default::default());
        }
    }
    Ok(plan.changes.len())
}
