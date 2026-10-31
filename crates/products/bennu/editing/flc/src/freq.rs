//! The project's own repeated lines — what the author has already written, in more than one file.
//!
//! Most of a codebase is the same few lines in different places: the guard at the top of a handler, the way a
//! reactive chain is closed, how a client call is wrapped. When the start of a line is something several files
//! continue the same way, the continuation is not a guess, so it is offered ahead of the model.
//!
//! Only lines that occur in **at least two files** are kept as text; a line seen once is kept as a hash, so the index
//! never holds the project's source. Each file's contribution can be replaced, so it follows the code as it is saved.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Shorter lines (`}`, `return;`) repeat everywhere and say nothing about the project.
const MIN_LINE: usize = 12;
/// A shorter start is too little to say which line is meant.
const MIN_PREFIX: usize = 8;
/// A continuation has to be written in this many other files.
const MIN_FILES: usize = 2;
/// ...and hold this share of everything that continues the same start.
const MIN_SHARE: f32 = 0.8;

fn line_hash(line: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    line.hash(&mut h);
    h.finish()
}

/// The line worth counting, trimmed; `None` for comments, imports, annotations and the short ones.
fn useful(line: &str) -> Option<&str> {
    let t = line.trim();
    let skipped = ["*", "//", "/*", "import ", "package ", "@"].iter().any(|p| t.starts_with(p));
    (t.len() >= MIN_LINE && !skipped).then_some(t)
}

#[derive(Default)]
pub struct LineIndex {
    ids: HashMap<String, u32>,
    /// A line seen in one file so far: its hash and that file. No text.
    single: HashMap<u64, u32>,
    /// Lines seen in two or more files, with those files.
    shared: BTreeMap<String, Vec<u32>>,
    text_of: HashMap<u64, String>,
    /// What each file contributed, so it can be taken back.
    by_file: HashMap<u32, Vec<u64>>,
}

/// A continuation the project agrees on.
#[derive(Debug, PartialEq)]
pub struct LineSuggestion {
    pub text: String,
    /// The share of the continuations of this start that this one holds.
    pub share: f32,
    /// In how many other files it is written.
    pub files: usize,
}

impl LineIndex {
    fn id_of(&mut self, key: &str) -> u32 {
        let next = self.ids.len() as u32;
        *self.ids.entry(key.to_string()).or_insert(next)
    }

    /// Takes back what `key` contributed before, then counts `source`.
    pub fn add_file(&mut self, key: &str, source: &str) {
        let id = self.id_of(key);
        self.remove(id);
        let mut mine = Vec::new();
        let mut seen = HashSet::new();
        for t in source.lines().filter_map(useful) {
            let h = line_hash(t);
            if !seen.insert(h) {
                continue;
            }
            mine.push(h);
            if let Some(text) = self.text_of.get(&h) {
                if let Some(files) = self.shared.get_mut(text) {
                    files.push(id);
                }
            } else if let Some(&other) = self.single.get(&h) {
                self.single.remove(&h);
                self.shared.insert(t.to_string(), vec![other, id]);
                self.text_of.insert(h, t.to_string());
            } else {
                self.single.insert(h, id);
            }
        }
        self.by_file.insert(id, mine);
    }

    pub fn remove_file(&mut self, key: &str) {
        if let Some(&id) = self.ids.get(key) {
            self.remove(id);
        }
    }

    fn remove(&mut self, id: u32) {
        for h in self.by_file.remove(&id).unwrap_or_default() {
            match self.text_of.get(&h).cloned() {
                Some(text) => {
                    let Some(files) = self.shared.get_mut(&text) else { continue };
                    files.retain(|f| *f != id);
                    if files.len() < 2 {
                        // back to a single sighting: the text goes, the hash stays
                        if let Some(&left) = files.first() {
                            self.single.insert(h, left);
                        }
                        self.shared.remove(&text);
                        self.text_of.remove(&h);
                    }
                }
                None => {
                    if self.single.get(&h) == Some(&id) {
                        self.single.remove(&h);
                    }
                }
            }
        }
    }

    /// How the project usually continues `prefix` (the start of the line at the caret), ignoring what `current`
    /// itself says; `None` when no continuation is written in enough other files, or the files disagree.
    pub fn suggest(&self, current: &str, prefix: &str) -> Option<LineSuggestion> {
        let start = prefix.trim_start();
        if start.len() < MIN_PREFIX {
            return None;
        }
        let me = self.ids.get(current).copied();
        let (mut total, mut best): (usize, Option<(usize, &str)>) = (0, None);
        for (line, files) in self.shared.range(start.to_string()..) {
            if !line.starts_with(start) {
                break;
            }
            let others = files.iter().filter(|f| Some(**f) != me).count();
            if others == 0 || line.len() == start.len() {
                continue;
            }
            total += others;
            if best.is_none_or(|(n, _)| others > n) {
                best = Some((others, &line[start.len()..]));
            }
        }
        let (files, rest) = best?;
        let share = files as f32 / total as f32;
        (files >= MIN_FILES && share >= MIN_SHARE).then(|| LineSuggestion { text: rest.to_string(), share, files })
    }

    /// How many lines are kept as text, for the status line.
    pub fn shared_lines(&self) -> usize {
        self.shared.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARD: &str = "class A {\n    void f() {\n        return repository.findById(id).orElseThrow(NotFoundException::new);\n    }\n}\n";

    fn three_files() -> LineIndex {
        let mut ix = LineIndex::default();
        ix.add_file("A.java", GUARD);
        ix.add_file("B.java", GUARD);
        ix.add_file("C.java", GUARD);
        ix
    }

    #[test]
    fn a_start_several_files_continue_alike_is_completed() {
        let ix = three_files();
        let s = ix.suggest("D.java", "        return repository.find").expect("agreed");
        assert_eq!(s.text, "ById(id).orElseThrow(NotFoundException::new);");
        assert_eq!(s.files, 3);
    }

    #[test]
    fn the_current_file_does_not_vouch_for_itself() {
        let mut ix = LineIndex::default();
        ix.add_file("A.java", GUARD);
        ix.add_file("B.java", GUARD);
        // from A's point of view only B writes it: one other file is not enough
        assert!(ix.suggest("A.java", "return repository.find").is_none());
        assert!(ix.suggest("C.java", "return repository.find").is_some());
    }

    #[test]
    fn files_that_disagree_give_nothing() {
        let mut ix = three_files();
        for (name, tail) in [("D.java", "orElse(null);"), ("D2.java", "orElse(null);"), ("E.java", "orElseGet(Foo::new);"), ("E2.java", "orElseGet(Foo::new);"), ("F.java", "orElseThrow();"), ("F2.java", "orElseThrow();")] {
            ix.add_file(name, &format!("        return repository.findById(id).{tail}\n"));
        }
        // 3 files write one ending, 6 others write three different ones: no ending holds 80%
        assert!(ix.suggest("G.java", "return repository.find").is_none());
    }

    #[test]
    fn a_short_start_and_a_line_seen_once_give_nothing() {
        let mut ix = three_files();
        assert!(ix.suggest("D.java", "ret").is_none());
        ix.add_file("D.java", "        unique.statement(thatNobodyElseWrote);\n");
        assert!(ix.suggest("E.java", "unique.statement(").is_none());
    }

    #[test]
    fn replacing_a_file_takes_its_old_lines_back() {
        let mut ix = three_files();
        ix.add_file("B.java", "class B { }\n");
        ix.add_file("C.java", "class C { }\n");
        assert!(ix.suggest("D.java", "return repository.find").is_none(), "only A still writes it");
        assert_eq!(ix.shared_lines(), 0);
    }

    #[test]
    fn comments_imports_annotations_and_short_lines_are_not_counted() {
        let mut ix = LineIndex::default();
        let src = "import java.util.List;\n// a long comment line here\n@RequestMapping(\"/orders\")\n    }\n";
        ix.add_file("A.java", src);
        ix.add_file("B.java", src);
        assert_eq!(ix.shared_lines(), 0);
    }
}
