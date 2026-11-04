//! Who wrote what, and who a broken build should be blamed on — as a hint, never a verdict.
//!
//! Conversations running at once share one tree. When a build fails, the errors an agent reads may be
//! someone else's: in Rust an error often *appears* in A's file and is *caused* by B changing an API in
//! another. This module keeps the record that makes a fair guess possible and words the guess:
//!
//! * [`Journal`] — a monotonic **epoch** bumped by every write a conversation makes, the last writer of
//!   each file, and the epoch each build target was last green at.
//! * [`attribute`] — splits a failed build's errors into *yours*, *another conversation's* and *nobody's*
//!   (a file no conversation of this session wrote: the user by hand, or untouched), and names the
//!   **suspects**: every other conversation that wrote any file since the target was last green.
//! * [`Inbox`] — the system notes Bennu (not the model) leaves for suspects, deduplicated per
//!   (conversation, error set) and capped, so two agents cannot ping-pong forever.
//!
//! Everything is heuristic and says so. Pure on purpose: no clock, no filesystem; the host passes paths
//! already resolved to one spelling.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use crate::reservations::key_of;

/// The most errors one note carries.
pub const NOTE_ERRORS: usize = 8;
/// The most notes waiting for one conversation: past this the oldest-first rule keeps the first ones.
pub const MAX_PENDING: usize = 3;
/// The most notes one conversation receives between two runs of its own.
pub const MAX_PER_RUN: u32 = 4;
/// The longest an error message is allowed to be in a note.
const MESSAGE_CHARS: usize = 220;

// ── The journal ──────────────────────────────────────────────────────────────────────────────────

/// Who last wrote a file, and when (in epochs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writer {
    pub conversation: String,
    pub title: String,
    pub epoch: u64,
    /// The file as the writer named it, for display.
    pub file: String,
}

#[derive(Default)]
struct JournalState {
    epoch: u64,
    files: HashMap<String, Writer>,
    green: HashMap<String, u64>,
}

/// The record of writes, shared by every conversation of the process.
#[derive(Default)]
pub struct Journal {
    state: Mutex<JournalState>,
}

impl Journal {
    fn lock(&self) -> std::sync::MutexGuard<'_, JournalState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A conversation wrote `file`. Returns the new epoch.
    pub fn record(&self, file: &str, conversation: &str, title: &str) -> u64 {
        let mut st = self.lock();
        st.epoch += 1;
        let epoch = st.epoch;
        st.files.insert(key_of(file), Writer { conversation: conversation.into(), title: title.into(), epoch, file: file.into() });
        epoch
    }

    /// The epoch now: what a build that starts this instant is a build *of*.
    pub fn epoch(&self) -> u64 {
        self.lock().epoch
    }

    pub fn last_writer(&self, file: &str) -> Option<Writer> {
        self.lock().files.get(&key_of(file)).cloned()
    }

    /// Every file written after `epoch`, newest write per file, oldest first.
    pub fn changed_since(&self, epoch: u64) -> Vec<Writer> {
        let mut out: Vec<Writer> = self.lock().files.values().filter(|w| w.epoch > epoch).cloned().collect();
        out.sort_by_key(|w| w.epoch);
        out
    }

    /// A build of `target` that began at `epoch` came out clean: the tree as of then is green.
    pub fn mark_green(&self, target: &str, epoch: u64) {
        let mut st = self.lock();
        let slot = st.green.entry(key_of(target)).or_insert(0);
        *slot = (*slot).max(epoch);
    }

    /// The epoch `target` was last green at (0 = never, in this session).
    pub fn green_epoch(&self, target: &str) -> u64 {
        self.lock().green.get(&key_of(target)).copied().unwrap_or(0)
    }

    /// A conversation was deleted: its writes stop being blamed on a name that no longer exists.
    pub fn forget_conversation(&self, conversation: &str) {
        self.lock().files.retain(|_, w| w.conversation != conversation);
    }
}

// ── Attribution ──────────────────────────────────────────────────────────────────────────────────

/// One error to attribute. `file` is already resolved to an absolute path when the build named one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Error {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

impl Error {
    fn short(&self) -> String {
        let place = match (&self.file, self.line) {
            (Some(f), Some(l)) => format!("{f}:{l}"),
            (Some(f), None) => f.clone(),
            _ => "(no file)".into(),
        };
        let msg: String = self.message.lines().next().unwrap_or("").chars().take(MESSAGE_CHARS).collect();
        format!("{place} {msg}")
    }
}

/// A conversation that may have caused the failure: it wrote files since the target was last green.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspect {
    pub conversation: String,
    pub title: String,
    pub files: Vec<String>,
}

/// Errors in files last written by one other conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elsewhere {
    pub conversation: String,
    pub title: String,
    pub errors: Vec<Error>,
}

/// The failed build, read against the journal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attribution {
    /// In files the asking conversation wrote itself.
    pub mine: Vec<Error>,
    /// In files another conversation last wrote.
    pub elsewhere: Vec<Elsewhere>,
    /// In files no conversation wrote this session (the user, or nobody), or with no file at all.
    pub nobody: Vec<Error>,
    /// Other conversations that wrote anything since the last green build.
    pub suspects: Vec<Suspect>,
}

impl Attribution {
    pub fn is_interesting(&self) -> bool {
        !self.elsewhere.is_empty() || !self.suspects.is_empty()
    }
}

/// Read `errors` (only the errors, not warnings) against the journal, from `me`'s point of view.
/// `target` is the build target whose last green epoch bounds the suspects.
pub fn attribute(me: &str, errors: &[Error], journal: &Journal, target: &str) -> Attribution {
    let mut out = Attribution::default();
    let mut by_other: BTreeMap<String, Elsewhere> = BTreeMap::new();
    for e in errors {
        let writer = e.file.as_deref().and_then(|f| journal.last_writer(f));
        match writer {
            Some(w) if w.conversation == me => out.mine.push(e.clone()),
            Some(w) => by_other
                .entry(w.conversation.clone())
                .or_insert_with(|| Elsewhere { conversation: w.conversation.clone(), title: w.title.clone(), errors: Vec::new() })
                .errors
                .push(e.clone()),
            None => out.nobody.push(e.clone()),
        }
    }
    out.elsewhere = by_other.into_values().collect();

    let mut suspects: BTreeMap<String, Suspect> = BTreeMap::new();
    for w in journal.changed_since(journal.green_epoch(target)) {
        if w.conversation == me {
            continue;
        }
        let s = suspects.entry(w.conversation.clone()).or_insert_with(|| Suspect { conversation: w.conversation.clone(), title: w.title.clone(), files: Vec::new() });
        if !s.files.contains(&w.file) {
            s.files.push(w.file);
        }
    }
    out.suspects = suspects.into_values().collect();
    out
}

fn quoted(title: &str) -> String {
    if title.trim().is_empty() { String::new() } else { format!(" (\"{}\")", title.trim()) }
}

fn names(files: &[String], max: usize) -> String {
    let mut shown: Vec<String> = files.iter().take(max).map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).to_string()).collect();
    if files.len() > max {
        shown.push(format!("and {} more", files.len() - max));
    }
    shown.join(", ")
}

fn file_names_of(errors: &[Error]) -> Vec<String> {
    let set: BTreeSet<&str> = errors.iter().filter_map(|e| e.file.as_deref()).collect();
    set.into_iter().map(str::to_string).collect()
}

/// What the conversation that ran the build is told. `active(conversation)` says whether that conversation
/// has a run going, so the text can say whether waiting for it makes sense. `None` when there is nothing to
/// say beyond the errors themselves (every error is the asker's own, or nobody has written anything else).
pub fn for_requester(a: &Attribution, active: &dyn Fn(&str) -> bool) -> Option<String> {
    if !a.is_interesting() {
        return None;
    }
    let mut lines = vec!["Who these errors belong to (a guess from the edit record, not a fact):".to_string()];
    if !a.mine.is_empty() {
        lines.push(format!("- {} error(s) are in files you edited: {}.", a.mine.len(), names(&file_names_of(&a.mine), 6)));
    }
    for other in &a.elsewhere {
        let state = if active(&other.conversation) { "it is working now" } else { "that conversation is not active" };
        lines.push(format!(
            "- {} error(s) are in files last edited by conversation {}{}: {}; {state}.",
            other.errors.len(),
            other.conversation,
            quoted(&other.title),
            names(&file_names_of(&other.errors), 6)
        ));
    }
    if !a.nobody.is_empty() {
        lines.push(format!("- {} error(s) are in files no conversation edited this session (changed by hand, or not changed): {}.", a.nobody.len(), names(&file_names_of(&a.nobody), 6)));
    }
    for s in &a.suspects {
        lines.push(format!(
            "- Possible cause: conversation {}{} edited {} since the last green build ({}).",
            s.conversation,
            quoted(&s.title),
            names(&s.files, 6),
            if active(&s.conversation) { "it is working now" } else { "that conversation is not active" }
        ));
    }
    lines.push("Wait for them, work on something else, or fix only what is in your own scope — do not rewrite another conversation's files to make a build pass.".into());
    Some(lines.join("\n"))
}

/// The note Bennu leaves for one suspect, with the hash that makes a repeat recognisable. `None` when the
/// build had no errors to show.
pub fn note_for(suspect: &Suspect, asker_title: &str, asker: &str, errors: &[Error]) -> Option<(String, u64)> {
    if errors.is_empty() {
        return None;
    }
    let mut unique: Vec<String> = Vec::new();
    for e in errors {
        let line = e.short();
        if !unique.contains(&line) {
            unique.push(line);
        }
    }
    let total = unique.len();
    let shown: Vec<&String> = unique.iter().take(NOTE_ERRORS).collect();
    let mut sorted: Vec<&String> = unique.iter().collect();
    sorted.sort();
    let hash = sorted.iter().fold(crate::worktree::FNV_SEED, |h, l| crate::worktree::fnv64_update(h, l.as_bytes()));
    let hash = crate::worktree::fnv64_update(hash, suspect.conversation.as_bytes());
    let mut text = format!(
        "System note from Bennu (a heuristic, not an instruction): a build run by conversation {asker}{} failed with {total} error(s). You edited {} since the project was last green, so some of these may follow from your changes. If they do, fix them; if they are not yours, ignore this note.\n",
        quoted(asker_title),
        names(&suspect.files, 5)
    );
    for l in shown {
        text.push_str(&format!("- {l}\n"));
    }
    if total > NOTE_ERRORS {
        text.push_str(&format!("- … and {} more\n", total - NOTE_ERRORS));
    }
    Some((text.trim_end().to_string(), hash))
}

// ── The inbox ────────────────────────────────────────────────────────────────────────────────────

/// A system note waiting for a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub hash: u64,
}

#[derive(Default)]
struct InboxState {
    pending: HashMap<String, Vec<Note>>,
    seen: HashMap<String, HashSet<u64>>,
    received: HashMap<String, u32>,
}

/// Notes for conversations, delivered with the next tool result or the next prompt.
#[derive(Default)]
pub struct Inbox {
    state: Mutex<InboxState>,
}

impl Inbox {
    fn lock(&self) -> std::sync::MutexGuard<'_, InboxState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Leave `note` for `to`. False when the same error set was already sent to it, or when it already
    /// has as many notes as it should get — the guard against two agents blaming each other forever.
    pub fn push(&self, to: &str, note: Note) -> bool {
        let mut st = self.lock();
        if st.seen.get(to).is_some_and(|s| s.contains(&note.hash)) {
            return false;
        }
        if st.pending.get(to).map_or(0, Vec::len) >= MAX_PENDING || st.received.get(to).copied().unwrap_or(0) >= MAX_PER_RUN {
            return false;
        }
        st.seen.entry(to.to_string()).or_default().insert(note.hash);
        *st.received.entry(to.to_string()).or_insert(0) += 1;
        st.pending.entry(to.to_string()).or_default().push(note);
        true
    }

    /// Hand over everything waiting for `to`.
    pub fn take(&self, to: &str) -> Vec<Note> {
        self.lock().pending.remove(to).unwrap_or_default()
    }

    pub fn has_pending(&self, to: &str) -> bool {
        self.lock().pending.get(to).is_some_and(|p| !p.is_empty())
    }

    /// The conversation starts a run of its own: the per-run cap starts over.
    pub fn begin_run(&self, conversation: &str) {
        self.lock().received.remove(conversation);
    }

    /// The target went green: what was said about its errors is stale, so a later failure may say it again.
    pub fn forget_seen(&self) {
        self.lock().seen.clear();
    }

    pub fn forget_conversation(&self, conversation: &str) {
        let mut st = self.lock();
        st.pending.remove(conversation);
        st.seen.remove(conversation);
        st.received.remove(conversation);
    }
}

/// The notes as they are appended to a tool result or put before a prompt.
pub fn render_notes(notes: &[Note]) -> String {
    notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(file: &str, msg: &str) -> Error {
        Error { file: Some(file.into()), line: Some(3), message: msg.into() }
    }

    fn journal() -> Journal {
        let j = Journal::default();
        j.mark_green("/p", 0);
        j
    }

    #[test]
    fn every_write_moves_the_epoch_and_the_last_writer_wins() {
        let j = Journal::default();
        assert_eq!(j.epoch(), 0);
        j.record("/p/a.rs", "c-a", "A");
        let e = j.record("/p/a.rs", "c-b", "B");
        assert_eq!(e, 2);
        assert_eq!(j.last_writer("/p/a.rs").unwrap().conversation, "c-b");
        assert!(j.last_writer("/p/other.rs").is_none());
    }

    #[test]
    fn spellings_of_a_path_are_one_file() {
        let j = Journal::default();
        j.record("C:\\p\\src\\a.rs", "c-a", "A");
        assert!(j.last_writer("C:/p/src/a.rs").is_some());
    }

    #[test]
    fn a_green_build_bounds_what_counts_as_since() {
        let j = Journal::default();
        j.record("/p/a.rs", "c-a", "A");
        let at = j.epoch();
        j.record("/p/b.rs", "c-b", "B");
        j.mark_green("/p", at);
        let since: Vec<String> = j.changed_since(j.green_epoch("/p")).into_iter().map(|w| w.file).collect();
        assert_eq!(since, vec!["/p/b.rs"]);
        j.mark_green("/p", 0);
        assert_eq!(j.green_epoch("/p"), at, "an older green never moves it back");
    }

    #[test]
    fn errors_split_by_who_last_wrote_the_file() {
        let j = journal();
        j.record("/p/mine.rs", "me", "Mine");
        j.record("/p/theirs.rs", "other", "Theirs");
        let errors = [err("/p/mine.rs", "e1"), err("/p/theirs.rs", "e2"), err("/p/hand.rs", "e3"), Error { file: None, line: None, message: "linker".into() }];
        let a = attribute("me", &errors, &j, "/p");
        assert_eq!(a.mine.len(), 1);
        assert_eq!((a.elsewhere.len(), a.elsewhere[0].conversation.as_str()), (1, "other"));
        assert_eq!(a.nobody.len(), 2, "a file nobody wrote, and an error with no file");
    }

    #[test]
    fn a_conversation_that_wrote_since_green_is_a_suspect_even_if_the_error_is_in_my_file() {
        let j = journal();
        j.record("/p/api.rs", "other", "Other");
        j.record("/p/use_api.rs", "me", "Me");
        let a = attribute("me", &[err("/p/use_api.rs", "no method `go`")], &j, "/p");
        assert_eq!(a.mine.len(), 1);
        assert!(a.elsewhere.is_empty());
        assert_eq!(a.suspects.len(), 1);
        assert_eq!(a.suspects[0].files, vec!["/p/api.rs"]);
        assert!(a.is_interesting());
    }

    #[test]
    fn nothing_to_blame_means_nothing_is_said() {
        let j = journal();
        j.record("/p/mine.rs", "me", "Me");
        let a = attribute("me", &[err("/p/mine.rs", "e")], &j, "/p");
        assert!(!a.is_interesting());
        assert!(for_requester(&a, &|_| true).is_none());
    }

    #[test]
    fn after_a_green_build_earlier_writers_are_no_longer_suspects() {
        let j = journal();
        j.record("/p/api.rs", "other", "Other");
        j.mark_green("/p", j.epoch());
        j.record("/p/mine.rs", "me", "Me");
        let a = attribute("me", &[err("/p/mine.rs", "e")], &j, "/p");
        assert!(a.suspects.is_empty());
    }

    #[test]
    fn the_requester_is_told_whose_files_and_whether_the_owner_is_working() {
        let j = journal();
        j.record("/p/theirs.rs", "other", "Fix the cart");
        let a = attribute("me", &[err("/p/theirs.rs", "e")], &j, "/p");
        let working = for_requester(&a, &|c| c == "other").unwrap();
        assert!(working.contains("last edited by conversation other (\"Fix the cart\")") && working.contains("it is working now"), "{working}");
        assert!(working.contains("Possible cause") && working.contains("theirs.rs"));
        let idle = for_requester(&a, &|_| false).unwrap();
        assert!(idle.contains("not active"));
        assert!(idle.contains("work on something else"));
    }

    #[test]
    fn a_file_nobody_wrote_is_reported_as_the_users() {
        let j = journal();
        j.record("/p/other.rs", "other", "O");
        let a = attribute("me", &[err("/p/by_hand.rs", "e")], &j, "/p");
        let text = for_requester(&a, &|_| true).unwrap();
        assert!(text.contains("no conversation edited this session") && text.contains("by hand"), "{text}");
    }

    #[test]
    fn a_note_lists_the_errors_once_and_caps_them() {
        let s = Suspect { conversation: "other".into(), title: "Other".into(), files: vec!["/p/api.rs".into()] };
        let mut errors: Vec<Error> = (0..20).map(|i| err("/p/a.rs", &format!("error {i}"))).collect();
        errors.push(err("/p/a.rs", "error 0"));
        let (text, _) = note_for(&s, "Me", "me", &errors).unwrap();
        assert!(text.contains("20 error(s)"), "the duplicate is counted once: {text}");
        assert!(text.contains("and 12 more") && text.lines().filter(|l| l.starts_with("- /p/a.rs")).count() == NOTE_ERRORS);
        assert!(text.contains("heuristic"));
        assert!(note_for(&s, "Me", "me", &[]).is_none());
    }

    #[test]
    fn the_same_error_set_hashes_the_same_whatever_its_order() {
        let s = Suspect { conversation: "other".into(), title: "".into(), files: vec!["/p/x.rs".into()] };
        let a = [err("/p/a.rs", "one"), err("/p/b.rs", "two")];
        let b = [err("/p/b.rs", "two"), err("/p/a.rs", "one")];
        assert_eq!(note_for(&s, "", "me", &a).unwrap().1, note_for(&s, "", "me", &b).unwrap().1);
        let other = [err("/p/a.rs", "three")];
        assert_ne!(note_for(&s, "", "me", &a).unwrap().1, note_for(&s, "", "me", &other).unwrap().1);
    }

    #[test]
    fn a_note_is_not_repeated_and_the_inbox_is_capped() {
        let inbox = Inbox::default();
        let n = |h: u64| Note { text: format!("note {h}"), hash: h };
        assert!(inbox.push("c", n(1)));
        assert!(!inbox.push("c", n(1)), "same error set, same conversation");
        assert!(inbox.push("other", n(1)), "another conversation is another recipient");
        assert!(inbox.push("c", n(2)) && inbox.push("c", n(3)));
        assert!(!inbox.push("c", n(4)), "no more than MAX_PENDING wait at once");
        assert_eq!(inbox.take("c").len(), 3);
        assert!(inbox.take("c").is_empty());
    }

    #[test]
    fn the_per_run_cap_starts_over_when_the_conversation_runs_again() {
        let inbox = Inbox::default();
        for h in 0..MAX_PER_RUN as u64 {
            assert!(inbox.push("c", Note { text: "t".into(), hash: h }));
            inbox.take("c");
        }
        assert!(!inbox.push("c", Note { text: "t".into(), hash: 99 }), "enough notes for one run");
        inbox.begin_run("c");
        assert!(inbox.push("c", Note { text: "t".into(), hash: 99 }));
    }

    #[test]
    fn a_green_build_lets_a_later_failure_say_it_again() {
        let inbox = Inbox::default();
        assert!(inbox.push("c", Note { text: "t".into(), hash: 7 }));
        inbox.take("c");
        assert!(!inbox.push("c", Note { text: "t".into(), hash: 7 }));
        inbox.forget_seen();
        assert!(inbox.push("c", Note { text: "t".into(), hash: 7 }));
    }

    #[test]
    fn deleting_a_conversation_forgets_its_notes_and_its_writes() {
        let inbox = Inbox::default();
        inbox.push("c", Note { text: "t".into(), hash: 1 });
        inbox.forget_conversation("c");
        assert!(!inbox.has_pending("c"));
        let j = Journal::default();
        j.record("/p/a.rs", "c", "C");
        j.forget_conversation("c");
        assert!(j.last_writer("/p/a.rs").is_none());
    }
}
