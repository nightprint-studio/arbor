//! Taking back a proposal that was applied: exactly the files it wrote, as they were, or nothing.
//!
//! When a proposal is applied (`apply::decide`) what each file was before, and the stamp it had right after,
//! is remembered here (process memory, the last [`KEEP`] proposals: a restart forgets them, and Local History
//! still has every write). Undoing is two-phase like applying: every file is looked at again and, if any is not
//! as the apply left it, **nothing is touched** and the proposal comes back as outdated; then the files are
//! put back — a created file is deleted, a moved one is moved back, the rest get their earlier text — and
//! should one step fail the ones already restored are written as they were after the apply. The writes go
//! through the same door as a save, so Local History records the undo too.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use super::apply::{snapshot, Disk};
use super::runs::PendingEdit;

/// How many applied proposals stay undoable.
const KEEP: usize = 20;

/// One applied edit, with what is needed to reverse it.
#[derive(Debug, Clone)]
pub(super) struct AppliedEdit {
    pub edit: PendingEdit,
    /// The text before the apply; `None` for a file the apply created.
    pub previous: Option<String>,
    /// The stamp of the file as the apply left it (empty: unknown, which disables the check).
    pub stamp_after: String,
}

impl AppliedEdit {
    /// The edit addressed to where the file is after the apply (a rename moved it).
    fn after(&self) -> PendingEdit {
        match &self.edit.move_to {
            Some(to) => PendingEdit { file: to.clone(), ..self.edit.clone() },
            None => self.edit.clone(),
        }
    }
}

/// What the apply changed, read from the disk once it is done: `before` is what [`snapshot`] saw first.
pub(super) fn record(edits: &[PendingEdit], before: &[Option<(String, String)>], disk: &impl Disk) -> Vec<AppliedEdit> {
    edits
        .iter()
        .zip(before)
        .map(|(edit, was)| {
            let mut a = AppliedEdit { edit: edit.clone(), previous: was.as_ref().map(|(t, _)| t.clone()), stamp_after: String::new() };
            a.stamp_after = disk.read(&a.after()).ok().flatten().map(|(_, s)| s).unwrap_or_default();
            a
        })
        .collect()
}

#[derive(Default)]
struct Book {
    order: VecDeque<String>,
    by_proposal: HashMap<String, Vec<AppliedEdit>>,
}

static BOOK: LazyLock<Mutex<Book>> = LazyLock::new(Default::default);

fn book() -> std::sync::MutexGuard<'static, Book> {
    BOOK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Keep what is needed to undo `proposal_id`, dropping the oldest beyond [`KEEP`].
pub(super) fn remember(proposal_id: &str, records: Vec<AppliedEdit>) {
    if records.is_empty() {
        return;
    }
    let mut b = book();
    b.order.retain(|id| id != proposal_id);
    b.order.push_back(proposal_id.to_string());
    b.by_proposal.insert(proposal_id.to_string(), records);
    while b.order.len() > KEEP {
        if let Some(old) = b.order.pop_front() {
            b.by_proposal.remove(&old);
        }
    }
}

fn take(proposal_id: &str) -> Option<Vec<AppliedEdit>> {
    let mut b = book();
    b.order.retain(|id| id != proposal_id);
    b.by_proposal.remove(proposal_id)
}

/// Whether a proposal can still be undone from memory.
pub(super) fn can_undo(proposal_id: &str) -> bool {
    book().by_proposal.contains_key(proposal_id)
}

/// How undoing went.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Undone {
    /// Every file is back as it was; the labels.
    Done(Vec<String>),
    /// These files (by their card label) are not as the apply left them. Nothing touched.
    Outdated(Vec<String>),
}

const GONE: &str = "this proposal can no longer be undone from here (Arbor restarted, or it is too old): restore the files from Local History";

/// Undo an applied proposal. Taken out of the memory while it runs and put back unless it worked, so a
/// refusal does not cost the chance to try again.
pub(super) fn undo(proposal_id: &str, disk: &impl Disk) -> Result<Undone, String> {
    let records = take(proposal_id).ok_or(GONE)?;
    match undo_all(&records, disk) {
        Ok(Undone::Done(labels)) => Ok(Undone::Done(labels)),
        other => {
            remember(proposal_id, records);
            other
        }
    }
}

/// Put every file back, or none. See the module doc.
pub(super) fn undo_all(records: &[AppliedEdit], disk: &impl Disk) -> Result<Undone, String> {
    let places: Vec<PendingEdit> = records.iter().map(AppliedEdit::after).collect();
    let now = snapshot(&places, disk)?;
    let changed: Vec<String> = records
        .iter()
        .zip(&now)
        .filter(|(r, now)| match now {
            None => true,
            Some((_, stamp)) => !r.stamp_after.is_empty() && *stamp != r.stamp_after,
        })
        .map(|(r, _)| r.edit.label.clone())
        .collect();
    if !changed.is_empty() {
        return Ok(Undone::Outdated(changed));
    }

    let mut restored: Vec<&AppliedEdit> = Vec::new();
    for r in records.iter().rev() {
        if let Err(e) = restore(r, disk) {
            let stuck = reapply(&restored, disk);
            return Err(if stuck.is_empty() {
                format!("{}: {e}. Nothing was undone — the files already restored were put back as the apply left them.", r.edit.label)
            } else {
                format!("{}: {e}. Putting the other files back failed for {} — check them by hand (Local History keeps the earlier text).", r.edit.label, stuck.join(", "))
            });
        }
        restored.push(r);
    }
    Ok(Undone::Done(records.iter().map(|r| r.edit.label.clone()).collect()))
}

/// One file back to what it was before the apply.
fn restore(r: &AppliedEdit, disk: &impl Disk) -> Result<(), String> {
    if let Some(to) = &r.edit.move_to {
        disk.rename(&r.edit, to, &r.edit.file)?;
    }
    match &r.previous {
        Some(text) => disk.write(&r.edit, text, ""),
        None => disk.delete(&r.edit),
    }
}

/// Make the files in `done` the way the apply left them again. Returns the labels that could not be.
fn reapply(done: &[&AppliedEdit], disk: &impl Disk) -> Vec<String> {
    let mut stuck = Vec::new();
    for r in done.iter().rev() {
        let mut ok = disk.write(&r.edit, &r.edit.new_text, "").is_ok();
        if let Some(to) = &r.edit.move_to {
            ok &= disk.rename(&r.edit, &r.edit.file, to).is_ok();
        }
        if !ok {
            stuck.push(r.edit.label.clone());
        }
    }
    stuck
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// In-memory disk: path → (text, stamp); a write gives the file a stamp derived from its text.
    #[derive(Default)]
    struct Fake {
        files: RefCell<HashMap<String, (String, String)>>,
        fail_write: Option<String>,
    }

    impl Fake {
        fn with(files: &[(&str, &str)]) -> Self {
            let f = Fake::default();
            for (p, t) in files {
                f.files.borrow_mut().insert(p.to_string(), (t.to_string(), format!("s-{t}")));
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
        fn write(&self, e: &PendingEdit, text: &str, _expect: &str) -> Result<(), String> {
            if self.fail_write.as_deref() == Some(e.file.as_str()) {
                return Err("disk full".into());
            }
            self.files.borrow_mut().insert(e.file.clone(), (text.to_string(), format!("s-{text}")));
            Ok(())
        }
        fn rename(&self, _e: &PendingEdit, from: &str, to: &str) -> Result<(), String> {
            let v = self.files.borrow_mut().remove(from).ok_or("missing")?;
            self.files.borrow_mut().insert(to.to_string(), v);
            Ok(())
        }
        fn delete(&self, e: &PendingEdit) -> Result<(), String> {
            self.files.borrow_mut().remove(&e.file);
            Ok(())
        }
    }

    fn edit(file: &str, new: &str, stamp: Option<&str>) -> PendingEdit {
        PendingEdit { root: "/p".into(), file: file.into(), label: file.into(), new_text: new.into(), base: String::new(), stamp: stamp.map(String::from), move_to: None }
    }

    /// Apply by hand what `apply_all` would, and record it.
    fn applied(disk: &Fake, edits: &[PendingEdit]) -> Vec<AppliedEdit> {
        let before = snapshot(edits, disk).unwrap();
        for e in edits {
            disk.write(e, &e.new_text, "").unwrap();
            if let Some(to) = &e.move_to {
                disk.rename(e, &e.file, to).unwrap();
            }
        }
        record(edits, &before, disk)
    }

    #[test]
    fn undoing_puts_every_file_back_as_it_was() {
        let disk = Fake::with(&[("a", "1"), ("b", "2")]);
        let records = applied(&disk, &[edit("a", "one", Some("s-1")), edit("b", "two", Some("s-2"))]);
        assert_eq!(disk.text("a").unwrap(), "one");
        assert_eq!(undo_all(&records, &disk), Ok(Undone::Done(vec!["a".into(), "b".into()])));
        assert_eq!((disk.text("a").unwrap(), disk.text("b").unwrap()), ("1".into(), "2".into()));
    }

    #[test]
    fn a_file_changed_since_the_apply_stops_the_whole_undo() {
        let disk = Fake::with(&[("a", "1"), ("b", "2")]);
        let records = applied(&disk, &[edit("a", "one", Some("s-1")), edit("b", "two", Some("s-2"))]);
        disk.write(&edit("b", "", None), "typed by hand", "").unwrap();
        assert_eq!(undo_all(&records, &disk), Ok(Undone::Outdated(vec!["b".into()])));
        assert_eq!(disk.text("a").unwrap(), "one", "nothing was touched");
        disk.files.borrow_mut().remove("a");
        assert_eq!(undo_all(&records, &disk), Ok(Undone::Outdated(vec!["a".into(), "b".into()])), "a file that is gone counts as changed");
    }

    #[test]
    fn a_created_file_is_deleted_and_a_moved_one_moved_back() {
        let disk = Fake::with(&[("Old.java", "class Old {}")]);
        let mut moved = edit("Old.java", "class New {}", Some("s-class Old {}"));
        moved.move_to = Some("New.java".into());
        let records = applied(&disk, &[edit("fresh", "hello", None), moved]);
        assert!(disk.text("New.java").is_some() && disk.text("fresh").is_some() && disk.text("Old.java").is_none());
        assert_eq!(undo_all(&records, &disk), Ok(Undone::Done(vec!["fresh".into(), "Old.java".into()])));
        assert!(disk.text("fresh").is_none() && disk.text("New.java").is_none());
        assert_eq!(disk.text("Old.java").unwrap(), "class Old {}");
    }

    #[test]
    fn a_failure_midway_leaves_the_files_as_the_apply_left_them() {
        let mut disk = Fake::with(&[("a", "1"), ("b", "2")]);
        let records = applied(&disk, &[edit("a", "one", Some("s-1")), edit("b", "two", Some("s-2"))]);
        disk.fail_write = Some("a".into());
        let err = undo_all(&records, &disk).unwrap_err();
        assert!(err.starts_with("a: disk full") && err.contains("Nothing was undone"), "{err}");
        assert_eq!((disk.text("a").unwrap(), disk.text("b").unwrap()), ("one".into(), "two".into()));
    }

    #[test]
    fn a_refused_undo_can_be_tried_again_and_a_done_one_cannot() {
        let disk = Fake::with(&[("u1", "1")]);
        let records = applied(&disk, &[edit("u1", "one", Some("s-1"))]);
        remember("undo-t1", records);
        assert!(can_undo("undo-t1"));
        disk.write(&edit("u1", "", None), "edited", "").unwrap();
        assert_eq!(undo("undo-t1", &disk), Ok(Undone::Outdated(vec!["u1".into()])));
        assert!(can_undo("undo-t1"), "kept for another try");
        disk.write(&edit("u1", "", None), "one", "").unwrap();
        assert_eq!(undo("undo-t1", &disk), Ok(Undone::Done(vec!["u1".into()])));
        assert!(undo("undo-t1", &disk).unwrap_err().contains("Local History"));
    }

    #[test]
    fn only_the_latest_proposals_are_remembered() {
        for i in 0..KEEP + 3 {
            remember(&format!("keep-{i}"), vec![AppliedEdit { edit: edit("f", "x", None), previous: None, stamp_after: String::new() }]);
        }
        assert!(!can_undo("keep-0") && can_undo(&format!("keep-{}", KEEP + 2)));
    }
}
