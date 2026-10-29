//! What the model has already read in this conversation, so reading it again costs a sentence.
//!
//! A long session reads the same file many times: to check an edit, to look at a second function, to
//! re-orient after a detour. Each read puts the whole text in the conversation again — and the conversation
//! is re-sent with every request. The files carry a `stamp` (a cheap fingerprint of their state on disk), so
//! the host can tell, for free, whether a file is what the model last saw:
//!
//!  * **unchanged** → one line saying so (the text is already in the conversation, above);
//!  * **changed**, and the same range was read before → the lines that differ, with a little context;
//!  * anything else → the text, as always.
//!
//! The one thing that makes "you already have it above" true is that the earlier text is still *in* the
//! conversation. Compaction, clearing and stubbing remove it, so every entry remembers the calls whose
//! results hold its text (`holders`), and [`ReadMemory::retain_holders`] forgets an entry the moment one of
//! those results is no longer intact. Memory per conversation, bounded, and gone with the process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Mutex, OnceLock};

use arbor_history::prelude::{compare_text, DiffLineKind};

/// The most text remembered for one conversation.
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// A single read bigger than this is not remembered (and so never answered with "unchanged").
const MAX_ENTRY_BYTES: usize = 512 * 1024;
/// A diff longer than this share of the new text is not worth sending: the text is.
const MAX_DIFF_SHARE: f64 = 0.5;

/// One read, as the host got it back from the file tool.
#[derive(Debug, Clone)]
pub(super) struct Read<'a> {
    /// The on-disk fingerprint; empty when unknown, which disables everything here.
    pub stamp: &'a str,
    pub text: &'a str,
    /// The 1-based line `text` starts at.
    pub from_line: u32,
    pub total_lines: u32,
}

impl Read<'_> {
    fn lines(&self) -> u32 {
        self.text.lines().count() as u32
    }
    fn to_line(&self) -> u32 {
        self.from_line.max(1) + self.lines().saturating_sub(1)
    }
}

#[derive(Debug, Clone)]
struct Seen {
    from: u32,
    to: u32,
    stamp: String,
    text: String,
    /// The calls whose results hold this text — the original read and every diff since.
    holders: Vec<String>,
}

/// What a read leaves behind once its answer has been sent.
#[derive(Debug, Clone)]
pub(super) struct Pending(Seen);

/// What to tell the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Reply {
    /// Send the text.
    Full,
    /// Nothing changed: send this line.
    Unchanged(String),
    /// Send this description of what changed.
    Diff(String),
}

#[derive(Default)]
pub(super) struct ReadMemory {
    files: HashMap<String, Vec<Seen>>,
    /// Oldest first, for eviction.
    order: VecDeque<String>,
    bytes: usize,
}

impl ReadMemory {
    /// Decide the answer to a read of `file` and remember it. `call_id` is the call whose result will carry
    /// whatever is sent now.
    pub fn answer(&mut self, call_id: &str, file: &str, read: &Read) -> Reply {
        let (reply, pending) = self.plan(call_id, file, read);
        if let Some(p) = pending {
            self.commit(file, p);
        }
        reply
    }

    /// The answer, and what to remember if it is sent — without remembering it yet. For a caller that may
    /// decide not to send it (a multi-file read that ran out of room).
    pub fn plan(&self, call_id: &str, file: &str, read: &Read) -> (Reply, Option<Pending>) {
        if read.stamp.is_empty() || read.text.is_empty() {
            return (Reply::Full, None);
        }
        let (from, to) = (read.from_line.max(1), read.to_line());
        let entries = self.files.get(file).map(Vec::as_slice).unwrap_or(&[]);

        if entries.iter().any(|e| e.stamp == read.stamp && e.from <= from && e.to >= to) {
            let line = format!(
                "unchanged since your last read (stamp {}): lines {from}-{to} of {}. You already have them above — do not read them again.",
                read.stamp, read.total_lines
            );
            return (Reply::Unchanged(line), None);
        }

        // The file changed. A diff only makes sense against a read that began at the same line (the range may have
        // grown or shrunk with the edit: lines were added above its end).
        if let Some(prev) = entries.iter().find(|e| e.from == from && e.to == to).or_else(|| entries.iter().find(|e| e.from == from)) {
            if let Some(diff) = diff_against(&prev.text, read) {
                let holders = prev.holders.iter().cloned().chain([call_id.to_string()]).collect();
                return (Reply::Diff(diff), Some(Pending(Seen { from, to, stamp: read.stamp.into(), text: read.text.into(), holders })));
            }
        }
        (Reply::Full, Some(Pending(Seen { from, to, stamp: read.stamp.into(), text: read.text.into(), holders: vec![call_id.to_string()] })))
    }

    /// Remember what a [`Self::plan`] said to remember.
    pub fn commit(&mut self, file: &str, pending: Pending) {
        self.replace(file, pending.0);
    }

    /// Forget every file whose remembered text is no longer in the conversation: one of its holders is not in
    /// `intact`, the ids of the tool results the conversation still holds whole.
    pub fn retain_holders(&mut self, intact: &HashSet<String>) {
        for list in self.files.values_mut() {
            list.retain(|e| e.holders.iter().all(|h| intact.contains(h)));
        }
        self.files.retain(|_, v| !v.is_empty());
        let files = &self.files;
        self.order.retain(|f| files.contains_key(f));
        self.recount();
    }

    /// Put `seen` in place of whatever was remembered of `file` (a changed file makes the old versions useless).
    fn replace(&mut self, file: &str, seen: Seen) {
        if seen.text.len() > MAX_ENTRY_BYTES {
            self.drop_file(file);
            return;
        }
        // Other ranges of the file are of the old version; keep only ranges still at this stamp.
        let previous = self.files.remove(file).unwrap_or_default();
        let mut list: Vec<Seen> = previous.into_iter().filter(|e| e.stamp == seen.stamp && e.from != seen.from).collect();
        list.push(seen);
        self.files.insert(file.to_string(), list);
        self.order.retain(|f| f != file);
        self.order.push_back(file.to_string());
        self.recount();
        while self.bytes > MAX_BYTES {
            let Some(oldest) = self.order.pop_front() else { break };
            self.files.remove(&oldest);
            self.recount();
        }
    }

    fn drop_file(&mut self, file: &str) {
        self.files.remove(file);
        self.order.retain(|f| f != file);
        self.recount();
    }

    fn recount(&mut self) {
        self.bytes = self.files.values().flatten().map(|e| e.text.len()).sum();
    }
}

/// The lines that differ between what the model has and what the file says now, or `None` when that is not
/// shorter than the text itself.
fn diff_against(old: &str, read: &Read) -> Option<String> {
    let delta = compare_text(old, read.text, 3);
    if delta.identical {
        // Same text under a new stamp (touched, not changed).
        return Some(format!("the file was touched but its text is the same (lines {}-{} of {}); you already have it above.", read.from_line.max(1), read.to_line(), read.total_lines));
    }
    let changed = delta.added + delta.removed;
    if (changed as f64) > MAX_DIFF_SHARE * read.lines().max(1) as f64 {
        return None;
    }
    let base = read.from_line.max(1) - 1;
    let mut out = format!(
        "changed since your last read (+{} −{}; stamp {}), lines {}-{} of {}. Apply this diff to the version you have above (line numbers are the file's):\n",
        delta.added,
        delta.removed,
        read.stamp,
        read.from_line.max(1),
        read.to_line(),
        read.total_lines
    );
    for h in &delta.hunks {
        out.push_str(&format!("@@ line {} @@\n", base + h.new_start.max(1)));
        for l in &h.lines {
            let sign = match l.kind {
                DiffLineKind::Add => '+',
                DiffLineKind::Del => '-',
                DiffLineKind::Context => ' ',
            };
            out.push(sign);
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    Some(out.trim_end().to_string())
}

// ── One memory per conversation, for the life of the process ─────────────────────────────────────

fn registry() -> &'static Mutex<HashMap<String, ReadMemory>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, ReadMemory>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Run `f` on the conversation's memory.
pub(super) fn with<R>(conversation: &str, f: impl FnOnce(&mut ReadMemory) -> R) -> R {
    let mut all = registry().lock().unwrap_or_else(|p| p.into_inner());
    f(all.entry(conversation.to_string()).or_default())
}

/// Forget what a conversation has read (it was deleted).
pub(super) fn forget(conversation: &str) {
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(conversation);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(n: usize) -> String {
        (1..=n).map(|i| format!("line {i}\n")).collect()
    }
    fn read<'a>(stamp: &'a str, text: &'a str) -> Read<'a> {
        Read { stamp, text, from_line: 1, total_lines: text.lines().count() as u32 }
    }
    fn intact(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_first_read_is_full_and_the_second_of_an_unchanged_file_is_a_sentence() {
        let mut m = ReadMemory::default();
        let t = text(50);
        assert_eq!(m.answer("c1", "A.java", &read("1:9", &t)), Reply::Full);
        let Reply::Unchanged(line) = m.answer("c2", "A.java", &read("1:9", &t)) else { panic!("unchanged") };
        assert!(line.contains("unchanged since your last read (stamp 1:9)") && line.contains("lines 1-50 of 50") && line.contains("already have them above"));
        assert!(line.len() < 200);
    }

    #[test]
    fn a_range_inside_a_full_read_of_an_unchanged_file_is_covered() {
        let mut m = ReadMemory::default();
        let t = text(50);
        m.answer("c1", "A.java", &read("s", &t));
        let part: String = (10..=20).map(|i| format!("line {i}\n")).collect();
        let r = Read { stamp: "s", text: &part, from_line: 10, total_lines: 50 };
        assert!(matches!(m.answer("c2", "A.java", &r), Reply::Unchanged(l) if l.contains("lines 10-20 of 50")));
    }

    #[test]
    fn a_changed_file_gets_a_diff_with_absolute_line_numbers() {
        let mut m = ReadMemory::default();
        let old = text(100);
        m.answer("c1", "A.java", &read("s1", &old));
        let new = old.replace("line 60\n", "line 60 changed\nline 60b\n");
        let r2 = m.answer("c2", "A.java", &read("s2", &new)); let Reply::Diff(d) = r2 else { panic!("diff: {r2:?}") };
        assert!(d.contains("+2 −1"), "{d}");
        assert!(d.contains("@@ line 57 @@") && d.contains("-line 60\n") && d.contains("+line 60 changed") && d.contains("+line 60b"), "{d}");
        assert!(!d.contains("line 1\n"), "only the neighbourhood of the change");
        // The diff is now what the model holds, so the next read of the same state is unchanged.
        assert!(matches!(m.answer("c3", "A.java", &read("s2", &new)), Reply::Unchanged(_)));
    }

    #[test]
    fn a_rewrite_is_sent_whole_not_as_a_diff_longer_than_the_text() {
        let mut m = ReadMemory::default();
        m.answer("c1", "A.java", &read("s1", &text(40)));
        let other: String = (1..=40).map(|i| format!("other {i}\n")).collect();
        assert_eq!(m.answer("c2", "A.java", &read("s2", &other)), Reply::Full);
    }

    #[test]
    fn a_touched_file_with_the_same_text_is_still_a_sentence() {
        let mut m = ReadMemory::default();
        let t = text(30);
        m.answer("c1", "A.java", &read("s1", &t));
        assert!(matches!(m.answer("c2", "A.java", &read("s2", &t)), Reply::Diff(d) if d.contains("same")));
    }

    #[test]
    fn a_different_range_of_a_changed_file_is_read_in_full() {
        let mut m = ReadMemory::default();
        let t = text(50);
        m.answer("c1", "A.java", &read("s1", &t));
        let part: String = (10..=20).map(|i| format!("line {i}\n")).collect();
        let r = Read { stamp: "s2", text: &part, from_line: 10, total_lines: 50 };
        assert_eq!(m.answer("c2", "A.java", &r), Reply::Full);
    }

    #[test]
    fn what_the_conversation_no_longer_holds_is_forgotten() {
        let mut m = ReadMemory::default();
        let t = text(50);
        m.answer("c1", "A.java", &read("s", &t));
        m.answer("c2", "B.java", &read("s", &t));
        m.retain_holders(&intact(&["c2"]));
        assert_eq!(m.answer("c3", "A.java", &read("s", &t)), Reply::Full, "its holder was compacted away");
        assert!(matches!(m.answer("c4", "B.java", &read("s", &t)), Reply::Unchanged(_)));
        // A diff chain needs every link.
        let new = t.replace("line 25\n", "line 25!\n");
        m.answer("c5", "B.java", &read("s2", &new));
        m.retain_holders(&intact(&["c2"]));
        assert_eq!(m.answer("c6", "B.java", &read("s2", &new)), Reply::Full, "the diff result c5 is gone, so the new state is not in the conversation");
    }

    #[test]
    fn an_unknown_stamp_or_an_empty_file_is_never_answered_from_memory() {
        let mut m = ReadMemory::default();
        let t = text(5);
        m.answer("c1", "A", &read("", &t));
        assert_eq!(m.answer("c2", "A", &read("", &t)), Reply::Full);
        assert_eq!(m.answer("c3", "E", &read("s", "")), Reply::Full);
    }

    #[test]
    fn memory_is_bounded() {
        let mut m = ReadMemory::default();
        let big = "x".repeat(MAX_ENTRY_BYTES - 1) + "\n";
        for i in 0..20 {
            m.answer(&format!("c{i}"), &format!("F{i}"), &read("s", &big));
        }
        assert!(m.bytes <= MAX_BYTES, "{}", m.bytes);
        assert_eq!(m.answer("z", "F0", &read("s", &big)), Reply::Full, "the oldest was evicted");
        let huge = "y".repeat(MAX_ENTRY_BYTES + 10);
        m.answer("h1", "H", &read("s", &huge));
        assert_eq!(m.answer("h2", "H", &read("s", &huge)), Reply::Full, "too big to be remembered");
    }

    #[test]
    fn each_conversation_has_its_own_memory() {
        let t = text(10);
        with("conv-a", |m| m.answer("c1", "A", &read("s", &t)));
        assert_eq!(with("conv-b", |m| m.answer("c1", "A", &read("s", &t))), Reply::Full);
        assert!(matches!(with("conv-a", |m| m.answer("c2", "A", &read("s", &t))), Reply::Unchanged(_)));
        forget("conv-a");
        assert_eq!(with("conv-a", |m| m.answer("c3", "A", &read("s", &t))), Reply::Full);
    }
}
