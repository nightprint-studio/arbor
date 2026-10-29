//! Applying a proposal the user approved — all of it or none of it.
//!
//! A proposal can span several files, each read at its own moment. Between that moment and the click
//! the user may have typed, another proposal may have been applied, a tool may have rewritten the file.
//! So applying is two phases: first every file is looked at again and, if any changed, **nothing is
//! written** and the proposal comes back as outdated; then the files are written, and should one write
//! fail after others went in, those are put back as they were. A half-applied proposal never remains.
//!
//! The disk is behind [`Disk`] so the whole policy is testable without a project on it.

use serde_json::{json, Value};

use arbor_history::prelude::compare_text;

use super::runs::{self, PendingEdit};
use super::text_tools;

/// What applying needs from the file system: the same four operations the editor itself uses.
pub(super) trait Disk {
    /// The file's text and stamp, or `None` when it does not exist.
    fn read(&self, edit: &PendingEdit) -> Result<Option<(String, String)>, String>;
    /// Write `text`, refused when the file's stamp is not `expect_stamp` (empty: no check).
    fn write(&self, edit: &PendingEdit, text: &str, expect_stamp: &str) -> Result<(), String>;
    fn rename(&self, edit: &PendingEdit, from: &str, to: &str) -> Result<(), String>;
    fn delete(&self, edit: &PendingEdit) -> Result<(), String>;
}

/// [`Disk`] over the backend's own handlers (`bennu_read_file`, `bennu_write_file`, …).
pub(super) struct CallDisk<F>(pub F);

impl<F: Fn(&str, Value) -> Result<Value, String>> Disk for CallDisk<F> {
    fn read(&self, edit: &PendingEdit) -> Result<Option<(String, String)>, String> {
        match (self.0)("bennu_read_file", json!({"args": {"root": edit.root, "file": edit.file}})) {
            Ok(v) => {
                let text = v.get("text").and_then(Value::as_str).ok_or("the file could not be read as text")?.to_string();
                let stamp = v.get("stamp").and_then(Value::as_str).unwrap_or("").to_string();
                Ok(Some((text, stamp)))
            }
            // A file that is not there is an answer, not a failure; anything else is.
            Err(_) if !std::path::Path::new(&edit.file).exists() => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, edit: &PendingEdit, text: &str, expect_stamp: &str) -> Result<(), String> {
        (self.0)("bennu_write_file", json!({"args": {"root": edit.root, "file": edit.file, "text": text, "expect_stamp": expect_stamp}})).map(|_| ())
    }

    fn rename(&self, edit: &PendingEdit, from: &str, to: &str) -> Result<(), String> {
        (self.0)("bennu_rename_path", json!({"args": {"root": edit.root, "file": from, "new_path": to}})).map(|_| ())
    }

    fn delete(&self, edit: &PendingEdit) -> Result<(), String> {
        (self.0)("bennu_delete_paths", json!({"args": {"root": edit.root, "paths": [edit.file]}})).map(|_| ())
    }
}

/// How applying went.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Applied {
    /// Every file written; the paths.
    Done(Vec<String>),
    /// These files (by their card label) are not as they were when the edit was proposed. Nothing written.
    Outdated(Vec<String>),
}

/// Whether the file is no longer what the edit was made against.
fn is_stale(edit: &PendingEdit, now: &Option<(String, String)>) -> bool {
    match (&edit.stamp, now) {
        // A new file: the place must still be free.
        (None, now) => now.is_some(),
        // Read at some point; the file must still be there, unchanged. An empty stamp means the
        // file's was unknown, which disables the check as it does for a save.
        (Some(s), _) if s.is_empty() => false,
        (Some(s), Some((_, current))) => current != s,
        (Some(_), None) => true,
    }
}

pub(super) fn snapshot(edits: &[PendingEdit], disk: &impl Disk) -> Result<Vec<Option<(String, String)>>, String> {
    edits.iter().map(|e| disk.read(e).map_err(|err| format!("{}: {err}", e.label))).collect()
}

/// One file as it was before it was written, to put it back.
struct Undo<'a> {
    edit: &'a PendingEdit,
    /// Where the file was moved to, when it was.
    moved_to: Option<String>,
    /// Its text before; `None` for a file the apply created.
    previous: Option<String>,
}

/// Put every written file back, newest first. Returns the ones that could not be.
fn roll_back(journal: &[Undo], disk: &impl Disk) -> Vec<String> {
    let mut stuck = Vec::new();
    for u in journal.iter().rev() {
        let mut ok = true;
        if let Some(to) = &u.moved_to {
            ok &= disk.rename(u.edit, to, &u.edit.file).is_ok();
        }
        ok &= match &u.previous {
            Some(text) => disk.write(u.edit, text, "").is_ok(),
            None => disk.delete(u.edit).is_ok(),
        };
        if !ok {
            stuck.push(u.edit.label.clone());
        }
    }
    stuck
}

/// Write every edit, or none. See the module doc.
pub(super) fn apply_all(edits: &[PendingEdit], disk: &impl Disk) -> Result<Applied, String> {
    let before = snapshot(edits, disk)?;
    let stale: Vec<String> = edits.iter().zip(&before).filter(|(e, now)| is_stale(e, now)).map(|(e, _)| e.label.clone()).collect();
    if !stale.is_empty() {
        return Ok(Applied::Outdated(stale));
    }

    let mut journal: Vec<Undo> = Vec::new();
    for (edit, now) in edits.iter().zip(&before) {
        let guard = edit.stamp.clone().unwrap_or_default();
        let previous = now.as_ref().map(|(text, _)| text.clone());
        let step = disk.write(edit, &edit.new_text, &guard).map_err(|e| e.to_string()).and_then(|()| {
            journal.push(Undo { edit, moved_to: None, previous });
            match &edit.move_to {
                Some(to) => disk
                    .rename(edit, &edit.file, to)
                    .map(|()| journal.last_mut().expect("just pushed").moved_to = Some(to.clone()))
                    .map_err(|e| format!("could not be moved to {to}: {e}")),
                None => Ok(()),
            }
        });
        if let Err(err) = step {
            let restored = journal.len();
            let stuck = roll_back(&journal, disk);
            return Err(if stuck.is_empty() {
                format!("{}: {err}. Nothing was applied{}.", edit.label, if restored > 0 { " — the files already written were put back" } else { "" })
            } else {
                format!("{}: {err}. Putting back what was already written failed for {} — check it by hand (Local History keeps the earlier text).", edit.label, stuck.join(", "))
            });
        }
    }
    Ok(Applied::Done(edits.iter().map(|e| e.file.clone()).collect()))
}

/// `text` with the change `base → new` made again on top of it: every hunk of that change, with its
/// surrounding lines, is looked up in `text` and must be found exactly once.
pub(super) fn rebase_text(base: &str, new: &str, text: &str) -> Result<String, String> {
    if text == base {
        return Ok(new.to_string());
    }
    if text == new {
        return Ok(text.to_string());
    }
    let delta = compare_text(base, new, 3);
    let total = delta.hunks.len();
    let mut out = text.to_string();
    for (i, hunk) in delta.hunks.iter().enumerate() {
        use arbor_history::prelude::DiffLineKind::{Add, Context, Del};
        let join = |keep: [bool; 3]| {
            hunk.lines
                .iter()
                .filter(|l| keep[match l.kind { Context => 0, Del => 1, Add => 2 }])
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (old_chunk, new_chunk) = (join([true, true, false]), join([true, false, true]));
        let nth = format!("change {} of {} (near line {})", i + 1, total, hunk.old_start);
        if old_chunk == new_chunk {
            return Err(format!("{nth} only differs in line endings or blank space"));
        }
        // Already made by whoever changed the file: this part has nothing left to do.
        if !out.contains(&old_chunk) && out.matches(&new_chunk).count() == 1 {
            continue;
        }
        out = text_tools::replace(&out, &old_chunk, &new_chunk, None).map_err(|e| format!("{nth} no longer applies: {e}"))?;
    }
    Ok(out)
}

/// A proposal brought up to date with the files as they are now.
#[derive(Debug)]
pub(super) struct Rebased {
    /// The edits to approve now: unchanged files as they were, changed ones re-made on the current text.
    pub edits: Vec<PendingEdit>,
    /// Files whose proposed change is already in them — nothing left to apply.
    pub already_there: Vec<String>,
}

/// Re-make every outdated edit on the file as it is now. All or nothing: if one cannot be, none is.
pub(super) fn rebase_all(edits: &[PendingEdit], disk: &impl Disk) -> Result<Rebased, String> {
    let before = snapshot(edits, disk)?;
    let (mut kept, mut already_there, mut problems) = (Vec::new(), Vec::new(), Vec::new());
    for (edit, now) in edits.iter().zip(&before) {
        if !is_stale(edit, now) {
            kept.push(edit.clone());
            continue;
        }
        let Some((current, stamp)) = now else {
            problems.push(format!("{}: the file no longer exists", edit.label));
            continue;
        };
        if edit.stamp.is_none() {
            problems.push(format!("{}: the file was created in the meantime", edit.label));
            continue;
        }
        match rebase_text(&edit.base, &edit.new_text, current) {
            Ok(next) if next == *current => already_there.push(edit.label.clone()),
            Ok(next) => kept.push(PendingEdit { new_text: next, stamp: Some(stamp.clone()), base: current.clone(), ..edit.clone() }),
            Err(e) => problems.push(format!("{}: {e}", edit.label)),
        }
    }
    if problems.is_empty() {
        Ok(Rebased { edits: kept, already_there })
    } else {
        Err(problems.join("; "))
    }
}

/// What the user's decision came to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Rejected,
    Applied(Vec<String>),
    /// Not applied, and not lost: the proposal stays, marked outdated, for these files.
    Outdated(Vec<String>),
}

const GONE: &str = "this proposal was already decided, or its session ended";

/// Apply or drop a proposal. The proposal is taken out of the registry while this runs, so two clicks
/// cannot apply it twice, and put back whenever it was not applied — a refusal must not cost the user
/// the proposal.
pub(super) fn decide(proposal_id: &str, apply: bool, disk: &impl Disk) -> Result<Verdict, String> {
    let edits = runs::take_proposal(proposal_id).ok_or(GONE)?;
    if !apply {
        return Ok(Verdict::Rejected);
    }
    // What the files are right now, to be able to put them back (see `undo`). Best effort: without it the
    // apply still goes ahead, it just cannot be undone from here.
    let before = snapshot(&edits, disk).ok();
    match apply_all(&edits, disk) {
        Ok(Applied::Done(files)) => {
            if let Some(before) = before {
                super::undo::remember(proposal_id, super::undo::record(&edits, &before, disk));
            }
            Ok(Verdict::Applied(files))
        }
        Ok(Applied::Outdated(labels)) => {
            runs::store_proposal(proposal_id, edits);
            Ok(Verdict::Outdated(labels))
        }
        Err(e) => {
            runs::store_proposal(proposal_id, edits);
            Err(e)
        }
    }
}

/// An outdated proposal, made again on the files as they are now.
#[derive(Debug)]
pub(super) struct Remade {
    /// The proposal's files as the card shows them now (nothing left if all were already in place).
    pub files: Vec<Value>,
    pub already_there: Vec<String>,
}

/// Replace a proposal's edits by their re-made versions, ready to be approved again. When it cannot be
/// done the proposal is left exactly as it was and the reason comes back.
pub(super) fn remake(proposal_id: &str, disk: &impl Disk) -> Result<Remade, String> {
    let edits = runs::take_proposal(proposal_id).ok_or(GONE)?;
    match rebase_all(&edits, disk) {
        Ok(done) => {
            let files = done
                .edits
                .iter()
                .map(|e| {
                    let mut entry = card_entry(&e.label, &e.base, &e.new_text);
                    if runs::pending_elsewhere(&e.file, proposal_id) {
                        entry["shared"] = json!(true);
                    }
                    entry
                })
                .collect();
            if !done.edits.is_empty() {
                runs::store_proposal(proposal_id, done.edits);
            }
            Ok(Remade { files, already_there: done.already_there })
        }
        Err(e) => {
            runs::store_proposal(proposal_id, edits);
            Err(e)
        }
    }
}

/// A file's line on the proposal card.
pub(super) fn card_entry(label: &str, base: &str, new: &str) -> Value {
    let delta = compare_text(base, new, 3);
    json!({ "path": label, "added": delta.added, "removed": delta.removed, "hunks": delta.hunks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// An in-memory disk: path → (text, stamp). `fail_writes` names files whose write is refused.
    #[derive(Default)]
    struct Fake {
        files: RefCell<HashMap<String, (String, String)>>,
        fail_writes: Vec<String>,
        fail_rename: bool,
        log: RefCell<Vec<String>>,
    }

    impl Fake {
        fn with(files: &[(&str, &str, &str)]) -> Self {
            let f = Fake::default();
            for (p, t, s) in files {
                f.files.borrow_mut().insert(p.to_string(), (t.to_string(), s.to_string()));
            }
            f
        }
        fn text(&self, p: &str) -> Option<String> {
            self.files.borrow().get(p).map(|(t, _)| t.clone())
        }
    }

    impl Disk for Fake {
        fn read(&self, e: &PendingEdit) -> Result<Option<(String, String)>, String> {
            Ok(self.files.borrow().get(&e.file).cloned())
        }
        fn write(&self, e: &PendingEdit, text: &str, expect: &str) -> Result<(), String> {
            if self.fail_writes.contains(&e.file) {
                return Err("disk full".into());
            }
            let mut files = self.files.borrow_mut();
            if let Some((_, stamp)) = files.get(&e.file) {
                if !expect.is_empty() && stamp != expect {
                    return Err("changed on disk".into());
                }
            }
            self.log.borrow_mut().push(format!("write {}", e.file));
            files.insert(e.file.clone(), (text.to_string(), format!("s-{}", text.len())));
            Ok(())
        }
        fn rename(&self, _e: &PendingEdit, from: &str, to: &str) -> Result<(), String> {
            if self.fail_rename && to.ends_with("New.java") {
                return Err("target exists".into());
            }
            let mut files = self.files.borrow_mut();
            let v = files.remove(from).ok_or("missing")?;
            files.insert(to.to_string(), v);
            self.log.borrow_mut().push(format!("rename {from} {to}"));
            Ok(())
        }
        fn delete(&self, e: &PendingEdit) -> Result<(), String> {
            self.files.borrow_mut().remove(&e.file);
            Ok(())
        }
    }

    fn edit(file: &str, base: &str, new: &str, stamp: Option<&str>) -> PendingEdit {
        PendingEdit {
            root: "/p".into(),
            file: file.into(),
            label: file.into(),
            new_text: new.into(),
            base: base.into(),
            stamp: stamp.map(String::from),
            move_to: None,
        }
    }

    #[test]
    fn every_file_is_written_when_none_changed() {
        let disk = Fake::with(&[("a", "1\n", "s1"), ("b", "2\n", "s2")]);
        let edits = [edit("a", "1\n", "one\n", Some("s1")), edit("b", "2\n", "two\n", Some("s2"))];
        assert_eq!(apply_all(&edits, &disk), Ok(Applied::Done(vec!["a".into(), "b".into()])));
        assert_eq!((disk.text("a").unwrap(), disk.text("b").unwrap()), ("one\n".into(), "two\n".into()));
    }

    #[test]
    fn one_stale_file_means_nothing_is_written() {
        let disk = Fake::with(&[("a", "1\n", "s1"), ("b", "changed\n", "other")]);
        let edits = [edit("a", "1\n", "one\n", Some("s1")), edit("b", "2\n", "two\n", Some("s2"))];
        assert_eq!(apply_all(&edits, &disk), Ok(Applied::Outdated(vec!["b".into()])));
        assert_eq!(disk.text("a").unwrap(), "1\n");
        assert!(disk.log.borrow().is_empty());
    }

    #[test]
    fn a_failure_midway_puts_the_written_files_back() {
        let mut disk = Fake::with(&[("a", "1\n", "s1"), ("b", "2\n", "s2")]);
        disk.fail_writes = vec!["b".into()];
        let edits = [edit("a", "1\n", "one\n", Some("s1")), edit("b", "2\n", "two\n", Some("s2"))];
        let err = apply_all(&edits, &disk).unwrap_err();
        assert!(err.starts_with("b: disk full") && err.contains("put back"), "{err}");
        assert_eq!(disk.text("a").unwrap(), "1\n", "the first file is as it was");
    }

    #[test]
    fn a_created_file_is_removed_again_on_rollback() {
        let mut disk = Fake::with(&[("b", "2\n", "s2")]);
        disk.fail_writes = vec!["b".into()];
        let edits = [edit("new", "", "hello\n", None), edit("b", "2\n", "two\n", Some("s2"))];
        assert!(apply_all(&edits, &disk).is_err());
        assert!(disk.text("new").is_none());
    }

    #[test]
    fn a_new_file_whose_place_is_taken_is_outdated() {
        let disk = Fake::with(&[("new", "somebody's\n", "s")]);
        assert_eq!(apply_all(&[edit("new", "", "mine\n", None)], &disk), Ok(Applied::Outdated(vec!["new".into()])));
    }

    #[test]
    fn a_rename_is_done_after_the_text_and_undone_with_it() {
        let disk = Fake::with(&[("Old.java", "class Old {}\n", "s1")]);
        let mut e = edit("Old.java", "class Old {}\n", "class New {}\n", Some("s1"));
        e.move_to = Some("New.java".into());
        assert_eq!(apply_all(&[e.clone()], &disk), Ok(Applied::Done(vec!["Old.java".into()])));
        assert_eq!(disk.text("New.java").unwrap(), "class New {}\n");
        assert!(disk.text("Old.java").is_none());

        let mut failing = Fake::with(&[("Old.java", "class Old {}\n", "s1")]);
        failing.fail_rename = true;
        assert!(apply_all(&[e], &failing).unwrap_err().contains("could not be moved"));
        assert_eq!(failing.text("Old.java").unwrap(), "class Old {}\n", "the text is back too");
    }

    #[test]
    fn a_refused_proposal_stays_for_the_next_try() {
        let disk = Fake::with(&[("a", "changed\n", "other")]);
        runs::store_proposal("apply-t1", vec![edit("a", "1\n", "one\n", Some("s1"))]);
        assert_eq!(decide("apply-t1", true, &disk), Ok(Verdict::Outdated(vec!["a".into()])));
        assert!(runs::pending_elsewhere("a", "someone-else"), "still held, not consumed");
        assert_eq!(decide("apply-t1", false, &disk), Ok(Verdict::Rejected));
        assert!(decide("apply-t1", true, &disk).unwrap_err().contains("already decided"));
    }

    #[test]
    fn an_outdated_proposal_is_remade_then_applied() {
        let new = BASE.replace("\nb\n", "\nB\n");
        let disk = Fake::with(&[("r", &BASE.replace("\nm\n", "\nM\n"), "s9")]);
        runs::store_proposal("apply-t2", vec![edit("r", BASE, &new, Some("s1"))]);
        assert!(matches!(decide("apply-t2", true, &disk), Ok(Verdict::Outdated(_))));
        let remade = remake("apply-t2", &disk).unwrap();
        assert_eq!(remade.files.len(), 1);
        assert!(matches!(decide("apply-t2", true, &disk), Ok(Verdict::Applied(_))));
        assert_eq!(disk.text("r").unwrap(), BASE.replace("\nb\n", "\nB\n").replace("\nm\n", "\nM\n"));
    }

    #[test]
    fn a_proposal_that_cannot_be_remade_is_left_as_it_was() {
        let disk = Fake::with(&[("q", &BASE.replace("\nb\n", "\nbee\n"), "s9")]);
        runs::store_proposal("apply-t3", vec![edit("q", BASE, &BASE.replace("\nb\n", "\nB\n"), Some("s1"))]);
        assert!(remake("apply-t3", &disk).is_err());
        assert_eq!(decide("apply-t3", false, &disk), Ok(Verdict::Rejected), "it was put back");
    }

    #[test]
    fn an_unknown_stamp_disables_the_check() {
        let disk = Fake::with(&[("a", "1\n", "s1")]);
        assert!(matches!(apply_all(&[edit("a", "1\n", "one\n", Some(""))], &disk), Ok(Applied::Done(_))));
    }

    const BASE: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\n";

    #[test]
    fn a_change_is_remade_on_a_file_edited_elsewhere() {
        let new = BASE.replace("\nb\n", "\nB\n");
        let current = BASE.replace("\nm\n", "\nM\n");
        let got = rebase_text(BASE, &new, &current).unwrap();
        assert_eq!(got, BASE.replace("\nb\n", "\nB\n").replace("\nm\n", "\nM\n"));
        assert_eq!(rebase_text(BASE, &new, BASE).unwrap(), new);
    }

    #[test]
    fn a_change_whose_lines_were_rewritten_is_refused() {
        let new = BASE.replace("\nb\n", "\nB\n");
        let current = BASE.replace("\nb\n", "\nbee\n");
        assert!(rebase_text(BASE, &new, &current).unwrap_err().contains("no longer applies"));
    }

    #[test]
    fn rebasing_all_is_all_or_nothing_and_notices_what_is_already_there() {
        let new = BASE.replace("\nb\n", "\nB\n");
        let disk = Fake::with(&[("a", &BASE.replace("\nm\n", "\nM\n"), "s9"), ("b", "2\n", "s2"), ("c", &new, "s7")]);
        let edits = [
            edit("a", BASE, &new, Some("s1")),
            edit("b", "2\n", "two\n", Some("s2")),
            edit("c", BASE, &new, Some("s3")),
        ];
        let r = rebase_all(&edits, &disk).unwrap();
        assert_eq!(r.already_there, vec!["c".to_string()]);
        assert_eq!(r.edits.len(), 2);
        assert_eq!(r.edits[0].stamp.as_deref(), Some("s9"), "the fresh stamp is what the new apply will check");
        assert_eq!(apply_all(&r.edits, &disk), Ok(Applied::Done(vec!["a".into(), "b".into()])));

        let broken = Fake::with(&[("a", &BASE.replace("\nb\n", "\nbee\n"), "s9"), ("b", "2\n", "s2")]);
        let err = rebase_all(&edits[..2], &broken).unwrap_err();
        assert!(err.starts_with("a:"), "{err}");
    }
}
