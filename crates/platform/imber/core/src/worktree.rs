//! The pure half of "work on an isolated copy": what a copy is, how its paths map to the project's, and
//! what applying it back means.
//!
//! A conversation can be given its own copy of the project to work in, so nothing it does touches the real
//! files until the user says so. This module holds the rules that must not be wrong and need no disk:
//!
//! * [`Mode`] — the two kinds of copy, fixed when the conversation starts.
//! * [`Base`] / [`Manifest`] — what every file looked like **when the copy was made**. That snapshot is the
//!   stamp the apply step is guarded by.
//! * [`to_copy`] / [`to_real`] — the path mapping the assistant's access policy and the apply step share.
//! * [`plan_apply`] — from what the copy, the project and the snapshot each hold for a file, the list of
//!   changes to make and the files that cannot be applied because the project moved on. Applying is
//!   all-or-nothing: **any** clash means nothing is applied, and every clash is reported.
//!
//! Reading and hashing files, creating the copy and writing the result are the host's; they feed this
//! module observations and carry out its plan.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The two kinds of isolated copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Text only: file, search, build and live tools; no code intelligence. A git worktree when the project is a
    /// repository (cheap), otherwise a size-capped plain copy.
    Copy,
    /// A real git worktree, opened and indexed as a hidden project so every tool works on it.
    Git,
}

impl Mode {
    pub fn parse(word: &str) -> Option<Mode> {
        match word {
            "copy" => Some(Mode::Copy),
            "git" => Some(Mode::Git),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Mode::Copy => "copy",
            Mode::Git => "git",
        }
    }

    /// Whether the copy is indexed, and so whether the code-intelligence tools can be offered.
    pub fn indexed(self) -> bool {
        self == Mode::Git
    }
}

/// A file's identity for comparison: its size and a hash of its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base {
    pub len: u64,
    pub hash: u64,
}

/// Every file of the project as it was when the copy was made, by path relative to the root (forward slashes).
pub type Manifest = BTreeMap<String, Base>;

/// FNV-1a over `bytes`. Not cryptographic: it only has to tell two versions of a file apart, quickly and
/// without a dependency, and a clash would need two different edits of one file with equal length.
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Continue an FNV-1a hash with more bytes, for files read in blocks.
pub fn fnv64_update(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The seed of [`fnv64_update`].
pub const FNV_SEED: u64 = 0xcbf2_9ce4_8422_2325;

// ── Paths ────────────────────────────────────────────────────────────────────────────────────────

fn slashes(p: &str) -> String {
    p.replace('\\', "/")
}

fn fold(p: &str) -> String {
    if cfg!(windows) { p.to_lowercase() } else { p.to_string() }
}

/// `path` relative to `root`, with forward slashes; `None` when it is not inside. The root itself is `""`.
pub fn rel_of(root: &str, path: &str) -> Option<String> {
    let r = slashes(root);
    let r = r.trim_end_matches('/');
    let p = slashes(path);
    let (fr, fp) = (fold(r), fold(&p));
    if fp == fr {
        return Some(String::new());
    }
    let prefix = format!("{fr}/");
    if fp.starts_with(&prefix) { Some(p[r.len() + 1..].to_string()) } else { None }
}

fn join(root: &str, rel: &str) -> String {
    let r = slashes(root);
    let r = r.trim_end_matches('/');
    if rel.is_empty() { r.to_string() } else { format!("{r}/{rel}") }
}

/// The same location inside the copy, for a path inside the project. `None` when it is outside the project.
pub fn to_copy(real_root: &str, copy_root: &str, path: &str) -> Option<String> {
    rel_of(real_root, path).map(|rel| join(copy_root, &rel))
}

/// The same location inside the project, for a path inside the copy. `None` when it is outside the copy.
pub fn to_real(real_root: &str, copy_root: &str, path: &str) -> Option<String> {
    rel_of(copy_root, path).map(|rel| join(real_root, &rel))
}

// ── Applying ─────────────────────────────────────────────────────────────────────────────────────

/// What one file looks like in each of the three places. `None` is "no such file".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub rel: String,
    /// In the copy when it was made: what "the copy did not touch this" means.
    pub base: Option<Base>,
    /// In the real project when the copy was made: what "the project did not move on" means. Equal to
    /// `base` unless making the copy changed the bytes (line-ending conversion on checkout).
    pub real_base: Option<Base>,
    /// In the copy now.
    pub copy: Option<Base>,
    /// In the real project now.
    pub real: Option<Base>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Added,
    Modified,
    Deleted,
}

/// One change to make in the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub rel: String,
    pub kind: Kind,
    /// For an added file whose content is exactly that of a file deleted in the same go: where it came from.
    #[serde(rename = "renamedFrom", skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

/// A file that cannot be applied, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Clash {
    pub rel: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub clashes: Vec<Clash>,
}

impl Plan {
    /// Nothing can be applied while any file clashes.
    pub fn can_apply(&self) -> bool {
        self.clashes.is_empty()
    }
}

/// Decide, file by file, what applying the copy to the project means. The input may list unchanged files;
/// they are skipped. Output is ordered by path.
///
/// A file the copy changed is applied only if the project's file is **still what the copy was made from**;
/// otherwise somebody (the user, another conversation, a build tool) changed it meanwhile and overwriting
/// would lose that. A file whose project version already equals the copy's needs nothing.
pub fn plan_apply(observations: &[Observation]) -> Plan {
    let mut plan = Plan::default();
    for o in observations {
        if o.copy == o.base {
            continue;
        }
        let rb = o.real_base;
        match (o.base, o.copy, o.real) {
            // Added in the copy.
            (None, Some(c), real) => match real {
                None => plan.changes.push(Change { rel: o.rel.clone(), kind: Kind::Added, renamed_from: None }),
                Some(r) if r == c => {}
                Some(_) => plan.clashes.push(Clash { rel: o.rel.clone(), reason: "the project has a file here now that the copy did not start with".into() }),
            },
            // Changed in the copy.
            (Some(_), Some(c), real) => match real {
                Some(r) if Some(r) == rb => plan.changes.push(Change { rel: o.rel.clone(), kind: Kind::Modified, renamed_from: None }),
                Some(r) if r == c => {}
                Some(_) => plan.clashes.push(Clash { rel: o.rel.clone(), reason: "changed in the project since the copy was made".into() }),
                None => plan.clashes.push(Clash { rel: o.rel.clone(), reason: "deleted in the project since the copy was made".into() }),
            },
            // Deleted in the copy.
            (Some(_), None, real) => match real {
                Some(r) if Some(r) == rb => plan.changes.push(Change { rel: o.rel.clone(), kind: Kind::Deleted, renamed_from: None }),
                None => {}
                Some(_) => plan.clashes.push(Clash { rel: o.rel.clone(), reason: "changed in the project since the copy was made, so it was not deleted".into() }),
            },
            (None, None, _) => {}
        }
    }
    mark_renames(&mut plan, observations);
    plan.changes.sort_by(|a, b| a.rel.cmp(&b.rel));
    plan.clashes.sort_by(|a, b| a.rel.cmp(&b.rel));
    plan
}

/// An added file with the content of a deleted one is a rename; say so, so the diff reads as what happened.
fn mark_renames(plan: &mut Plan, observations: &[Observation]) {
    let by_rel: BTreeMap<&str, &Observation> = observations.iter().map(|o| (o.rel.as_str(), o)).collect();
    let deleted: Vec<(String, Base)> = plan
        .changes
        .iter()
        .filter(|c| c.kind == Kind::Deleted)
        .filter_map(|c| by_rel.get(c.rel.as_str()).and_then(|o| o.base).map(|b| (c.rel.clone(), b)))
        .collect();
    let mut taken: Vec<String> = Vec::new();
    for change in plan.changes.iter_mut().filter(|c| c.kind == Kind::Added) {
        let Some(copy) = by_rel.get(change.rel.as_str()).and_then(|o| o.copy) else { continue };
        if let Some((from, _)) = deleted.iter().find(|(rel, b)| *b == copy && !taken.contains(rel)) {
            taken.push(from.clone());
            change.renamed_from = Some(from.clone());
        }
    }
}

/// The wording of a refusal, one line per file, for the model and for the panel.
pub fn clash_summary(clashes: &[Clash]) -> String {
    let mut lines: Vec<String> = clashes.iter().take(20).map(|c| format!("- {}: {}", c.rel, c.reason)).collect();
    if clashes.len() > 20 {
        lines.push(format!("- … and {} more", clashes.len() - 20));
    }
    format!("Nothing was applied: {} file(s) cannot be applied.\n{}", clashes.len(), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(n: u64) -> Option<Base> {
        Some(Base { len: n, hash: n * 7 })
    }

    fn obs(rel: &str, base: Option<Base>, copy: Option<Base>, real: Option<Base>) -> Observation {
        Observation { rel: rel.into(), base, real_base: base, copy, real }
    }

    #[test]
    fn paths_map_both_ways_and_outside_paths_map_to_nothing() {
        assert_eq!(to_copy("C:/w/app", "D:/data/wt/c1", "C:\\w\\app\\src\\A.java").as_deref(), Some("D:/data/wt/c1/src/A.java"));
        assert_eq!(to_real("C:/w/app", "D:/data/wt/c1", "D:/data/wt/c1/src/A.java").as_deref(), Some("C:/w/app/src/A.java"));
        assert_eq!(to_copy("C:/w/app", "D:/wt", "C:/w/application/A.java"), None, "a sibling that shares a prefix is outside");
        assert_eq!(to_real("C:/w/app", "D:/wt", "C:/w/app/A.java"), None);
        assert_eq!(to_copy("C:/w/app/", "D:/wt/", "C:/w/app").as_deref(), Some("D:/wt"));
        assert_eq!(rel_of("/p", "/p/a/b.txt").as_deref(), Some("a/b.txt"));
    }

    #[test]
    fn an_untouched_file_is_not_a_change() {
        let plan = plan_apply(&[obs("a", b(1), b(1), b(1)), obs("gone", None, None, None)]);
        assert_eq!(plan, Plan::default());
        assert!(plan.can_apply());
    }

    #[test]
    fn modified_added_and_deleted_files_are_planned_when_the_project_is_as_it_was() {
        let plan = plan_apply(&[
            obs("m.java", b(1), b(2), b(1)),
            obs("new.java", None, b(3), None),
            obs("old.java", b(4), None, b(4)),
        ]);
        let kinds: Vec<(&str, Kind)> = plan.changes.iter().map(|c| (c.rel.as_str(), c.kind)).collect();
        assert_eq!(kinds, [("m.java", Kind::Modified), ("new.java", Kind::Added), ("old.java", Kind::Deleted)]);
        assert!(plan.can_apply());
    }

    #[test]
    fn a_file_the_project_changed_meanwhile_is_a_clash_and_blocks_everything() {
        let plan = plan_apply(&[
            obs("fine.java", b(1), b(2), b(1)),
            obs("moved-on.java", b(1), b(2), b(9)),
            obs("vanished.java", b(1), b(2), None),
            obs("kept.java", b(5), None, b(6)),
            obs("taken.java", None, b(3), b(4)),
        ]);
        assert!(!plan.can_apply());
        let rels: Vec<&str> = plan.clashes.iter().map(|c| c.rel.as_str()).collect();
        assert_eq!(rels, ["kept.java", "moved-on.java", "taken.java", "vanished.java"]);
        assert!(plan.clashes.iter().find(|c| c.rel == "vanished.java").unwrap().reason.contains("deleted in the project"));
        let text = clash_summary(&plan.clashes);
        assert!(text.starts_with("Nothing was applied: 4 file(s)"));
    }

    #[test]
    fn a_checkout_that_changed_the_bytes_is_judged_against_what_the_project_had() {
        // The copy started with hash 2 (CRLF converted); the project had hash 1 and still has.
        let o = Observation { rel: "a".into(), base: b(2), real_base: b(1), copy: b(3), real: b(1) };
        let plan = plan_apply(&[o]);
        assert!(plan.can_apply() && plan.changes.len() == 1);
        let moved = Observation { rel: "a".into(), base: b(2), real_base: b(1), copy: b(3), real: b(9) };
        assert!(!plan_apply(&[moved]).can_apply());
    }

    #[test]
    fn a_file_already_equal_in_the_project_needs_nothing() {
        let plan = plan_apply(&[obs("a", b(1), b(2), b(2)), obs("b", None, b(3), b(3)), obs("c", b(4), None, None)]);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn an_added_file_with_a_deleted_files_content_is_a_rename() {
        let plan = plan_apply(&[obs("old/A.java", b(8), None, b(8)), obs("new/A.java", None, b(8), None), obs("other.txt", None, b(3), None)]);
        let added = plan.changes.iter().find(|c| c.rel == "new/A.java").unwrap();
        assert_eq!(added.renamed_from.as_deref(), Some("old/A.java"));
        assert_eq!(plan.changes.iter().find(|c| c.rel == "other.txt").unwrap().renamed_from, None);
        assert_eq!(plan.changes.len(), 3, "the deletion is still applied");
    }

    #[test]
    fn the_hash_tells_two_versions_apart_and_blocks_agree_with_one_shot() {
        assert_ne!(fnv64(b"one"), fnv64(b"two"));
        let whole = fnv64(b"hello world");
        let parts = fnv64_update(fnv64_update(FNV_SEED, b"hello "), b"world");
        assert_eq!(whole, parts);
    }

    #[test]
    fn modes_round_trip_and_only_git_is_indexed() {
        for m in [Mode::Copy, Mode::Git] {
            assert_eq!(Mode::parse(m.id()), Some(m));
        }
        assert_eq!(Mode::parse("nonsense"), None);
        assert!(Mode::Git.indexed() && !Mode::Copy.indexed());
    }
}
