//! What Bennu has learned, sized for the process monitor.
//!
//! Every table of habits, names, sequences … keeps two things: what each file contributed (so a saved file can replace its
//! own counts) and the totals those contributions add up to. The first belongs to a project, by the path of the file; the
//! second is one table for the process; and what other projects taught (`prior`) is carried in beside them. This module
//! is the shared vocabulary for saying so, and the code that turns a list of files into one line per open project.
//!
//! Estimates, like the rest of the monitor: strings by their length plus a header, map slots by a fixed size. The gap to the
//! process total is visible there, so nothing here pretends to be exact.

use std::collections::HashMap;

use arbor_be::prelude::MemoryItem;

/// A `String`'s header and the allocator's slack, on top of its bytes.
const STR: usize = 32;
/// One hash-map slot: key, value and control bytes.
const SLOT: usize = 48;

pub(crate) fn text(s: &str) -> usize {
    s.len() + STR
}

/// A table of `place → (text → count)`, sized with `place` for the outer key.
pub(crate) fn two_level<K>(table: &HashMap<K, HashMap<String, u32>>, place: impl Fn(&K) -> usize) -> usize {
    table.iter().map(|(k, row)| place(k) + SLOT + row.keys().map(|t| text(t) + 4 + SLOT).sum::<usize>()).sum()
}

/// What one table holds, split the way the monitor shows it.
#[derive(Default)]
pub(crate) struct Sizes {
    /// File key (an absolute path) → bytes its contribution takes.
    pub per_file: HashMap<String, usize>,
    /// The totals, for every project together: how many entries and what they take.
    pub shared_entries: usize,
    pub shared: usize,
    /// What other projects taught, carried in.
    pub carried: usize,
}

impl Sizes {
    pub(crate) fn add_file(&mut self, key: &str, bytes: usize) {
        *self.per_file.entry(key.to_string()).or_insert_with(|| text(key)) += bytes;
    }
}

/// `path` normalised for comparing: forward slashes, no trailing one.
fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

/// The open project a file belongs to: the longest root it sits under.
fn project_of<'a>(file: &str, roots: &'a [String]) -> Option<&'a str> {
    let file = norm(file);
    roots.iter().filter(|r| file.starts_with(&format!("{r}/"))).max_by_key(|r| r.len()).map(String::as_str)
}

/// The monitor's lines for what was learned: one per open project (what its files contributed, across every table), and
/// the process's own (each table's totals, what other projects taught, the counters of corrections and rules).
pub(crate) fn items(tables: &[(&'static str, Sizes)], extra: Vec<MemoryItem>, open_roots: &[String]) -> Vec<MemoryItem> {
    let roots: Vec<String> = open_roots.iter().map(|r| norm(r)).collect();
    let mut by_project: HashMap<&str, (usize, usize)> = HashMap::new();
    let mut elsewhere = (0usize, 0usize);
    let mut out = Vec::new();
    let mut carried = 0usize;
    for (label, sizes) in tables {
        for (file, bytes) in &sizes.per_file {
            let slot = match project_of(file, &roots) {
                Some(root) => by_project.entry(root).or_default(),
                None => &mut elsewhere,
            };
            slot.0 += 1;
            slot.1 += bytes;
        }
        if sizes.shared > 0 {
            out.push(MemoryItem::estimate("process", format!("Learned · {label} (totals, all projects)"), sizes.shared_entries, sizes.shared));
        }
        carried += sizes.carried;
    }
    if carried > 0 {
        out.push(MemoryItem::estimate("process", "Learned · carried over from other projects", 0, carried));
    }
    for (root, (records, bytes)) in by_project {
        out.push(MemoryItem::estimate(root, "Learned · what this project's files contributed (arguments, names, sequences…)", records, bytes));
    }
    if elsewhere.0 > 0 {
        out.push(MemoryItem::estimate("process", "Learned · files of projects no longer open", elsewhere.0, elsewhere.1));
    }
    out.extend(extra);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_charged_to_the_longest_root_it_sits_under() {
        let roots = vec!["/w/app".to_string(), "/w/app/sub".to_string()];
        assert_eq!(project_of("/w/app/sub/A.java", &roots), Some("/w/app/sub"));
        assert_eq!(project_of("/w/app/B.java", &roots), Some("/w/app"));
        assert_eq!(project_of("/w/application/C.java", &roots), None);
    }

    #[test]
    fn lines_are_split_by_project_and_process() {
        let mut s = Sizes::default();
        s.add_file("/w/app/A.java", 100);
        s.shared = 500;
        s.shared_entries = 3;
        let lines = items(&[("arguments", s)], Vec::new(), &["/w/app/".to_string()]);
        assert!(lines.iter().any(|i| i.scope == "/w/app" && i.bytes.is_some()));
        assert!(lines.iter().any(|i| i.scope == "process" && i.count == Some(3)));
    }
}
