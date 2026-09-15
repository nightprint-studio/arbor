//! What a raw filesystem event means to the Project tree — the pure half of [`crate::tree_watch`].
//!
//! The watcher thread owns handles, clocks and the channel; everything here is a function of its
//! arguments, so the decisions that went wrong in practice — which events are worth a reload, how a
//! rename's two halves find each other, what an editor's "safe write" nets out to — are tested
//! without a filesystem.
//!
//! ## Structural vs content
//!
//! The tree draws names, not bytes. A create, a remove or a rename changes what it lists; a write
//! into a file does not. Reporting every write kept the debounce from ever going quiet while a
//! server appended to a log inside the project, so content events are dropped here — except for
//! the few files whose content the tree or the project model does follow: [`CONTENT_FOLLOWED`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use notify::event::{EventKind, ModifyKind, RenameMode};
use serde::Serialize;
use serde_json::{json, Value};

/// Files whose **content** is worth an event: the manifests the project model is parsed from, and
/// `.gitignore`, which decides which rows the tree dims.
pub const CONTENT_FOLLOWED: &[&str] = &["pom.xml", "Cargo.toml", ".gitignore"];

/// Upper bound on paths and changes carried in one event. Past this the frontend reloads wholesale.
pub const MAX_PATHS: usize = 200;

/// Raw structural changes kept per burst. Past this the burst is a bulk operation and is reported
/// as a rescan instead: on Windows `notify` reads into a fixed 16 KiB buffer and ignores the
/// overflow status, so a burst this large has very likely lost part of itself already.
const MAX_RAW: usize = 1000;

/// What one raw event says about one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raw {
    Created,
    Removed,
    RenamedFrom,
    RenamedTo,
    Content,
}

/// Split one event into what it says about each of its paths.
///
/// `exists` answers for the kinds that do not say which side of a rename a path is on — macOS
/// reports `Name(Any)` once per path, and only the disk knows old name from new. Access events
/// cannot change a listing and produce nothing.
pub fn expand(
    kind: &EventKind,
    paths: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Vec<(Raw, PathBuf)> {
    let each = |raw: Raw| -> Vec<(Raw, PathBuf)> { paths.iter().map(|p| (raw, p.clone())).collect() };
    match kind {
        EventKind::Create(_) => each(Raw::Created),
        EventKind::Remove(_) => each(Raw::Removed),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => each(Raw::RenamedFrom),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => each(Raw::RenamedTo),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if paths.len() == 2 => {
            vec![(Raw::RenamedFrom, paths[0].clone()), (Raw::RenamedTo, paths[1].clone())]
        }
        EventKind::Modify(ModifyKind::Name(_)) => paths
            .iter()
            .map(|p| (if exists(p) { Raw::Created } else { Raw::Removed }, p.clone()))
            .collect(),
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => each(Raw::Content),
        EventKind::Access(_) => Vec::new(),
    }
}

/// Whether a content change to this root-relative path is worth reporting.
pub fn content_followed(rel: &str) -> bool {
    CONTENT_FOLLOWED.contains(&rel.rsplit('/').next().unwrap_or(rel))
}

/// One structural change as the watcher saw it, before its halves are paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawChange {
    pub kind: Raw,
    /// Root-relative, forward slashes.
    pub rel: String,
    /// Known to be a directory. For a path that no longer exists this is a best guess — see
    /// [`infer_dirs`].
    pub is_dir: bool,
}

/// A change to what the tree lists, net of everything the burst did to the same path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Added { path: String, is_dir: bool, module: bool },
    Removed { path: String, is_dir: bool },
    Renamed { from: String, path: String, is_dir: bool, module: bool },
}

impl Change {
    fn path(&self) -> &str {
        match self {
            Change::Added { path, .. } | Change::Removed { path, .. } | Change::Renamed { path, .. } => path,
        }
    }

    fn origin(&self) -> Option<&str> {
        match self {
            Change::Renamed { from, .. } => Some(from),
            _ => None,
        }
    }

    fn is_dir(&self) -> bool {
        match self {
            Change::Added { is_dir, .. } | Change::Removed { is_dir, .. } | Change::Renamed { is_dir, .. } => *is_dir,
        }
    }
}

/// The net changes of a burst, in the order they first happened.
///
/// `is_module` is asked about every surviving added or renamed directory — whether it holds a
/// `pom.xml` — at the end, when whatever was being moved into it has arrived.
pub fn coalesce(raw: &[RawChange], is_module: impl Fn(&str) -> bool) -> Vec<Change> {
    let raw = infer_dirs(raw);
    let mut net: Vec<Change> = Vec::new();
    for change in pair_renames(&raw) {
        fold(&mut net, change);
    }
    let mut net = prune_descendants(pair_moves(net));
    for change in &mut net {
        match change {
            Change::Added { path, is_dir: true, module } | Change::Renamed { path, is_dir: true, module, .. } => {
                *module = is_module(path);
            }
            _ => {}
        }
    }
    net
}

/// A removed path cannot be asked whether it was a directory. Another change in the same burst
/// *under* it can answer: deleting or moving a folder reports its children too.
fn infer_dirs(raw: &[RawChange]) -> Vec<RawChange> {
    raw.iter()
        .map(|c| {
            let mut c = c.clone();
            if !c.is_dir {
                c.is_dir = raw.iter().any(|o| under(&o.rel, &c.rel));
            }
            c
        })
        .collect()
}

/// Join each `RenamedFrom` with the next unpaired `RenamedTo`. Windows reports the two halves as
/// separate, adjacent actions; an unmatched half is a move across the watch boundary, which to the
/// tree is a plain removal or addition.
fn pair_renames(raw: &[RawChange]) -> Vec<Change> {
    let mut used = vec![false; raw.len()];
    let mut out = Vec::new();
    for i in 0..raw.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let c = &raw[i];
        match c.kind {
            Raw::RenamedFrom => {
                match (i + 1..raw.len()).find(|&j| !used[j] && raw[j].kind == Raw::RenamedTo) {
                    Some(j) => {
                        used[j] = true;
                        out.push(Change::Renamed {
                            from: c.rel.clone(),
                            path: raw[j].rel.clone(),
                            is_dir: c.is_dir || raw[j].is_dir,
                            module: false,
                        });
                    }
                    None => out.push(Change::Removed { path: c.rel.clone(), is_dir: c.is_dir }),
                }
            }
            Raw::Created | Raw::RenamedTo => {
                out.push(Change::Added { path: c.rel.clone(), is_dir: c.is_dir, module: false })
            }
            Raw::Removed => out.push(Change::Removed { path: c.rel.clone(), is_dir: c.is_dir }),
            Raw::Content => {}
        }
    }
    out
}

/// Add one change to the net set, cancelling or chaining it against what the burst already did to
/// the same name.
///
/// The case that decides the shape is an IDE's safe write — create a temp file, rename the original
/// aside, rename the temp over it, delete the aside copy — which is four structural events for an
/// edit and must net out to nothing.
fn fold(net: &mut Vec<Change>, change: Change) {
    match change {
        Change::Added { path, is_dir, module } => {
            if let Some(i) = net.iter().position(|c| matches!(c, Change::Removed { path: p, .. } if *p == path)) {
                // Removed and back again: replaced in place.
                net.remove(i);
            } else if let Some(i) = net.iter().position(|c| c.origin() == Some(path.as_str())) {
                // Moved aside and a new one put in its place: the name never left, the other one appeared.
                if let Change::Renamed { path: aside, is_dir, module, .. } = net.remove(i) {
                    fold(net, Change::Added { path: aside, is_dir, module });
                }
            } else {
                net.push(Change::Added { path, is_dir, module });
            }
        }
        Change::Removed { path, is_dir } => {
            if let Some(i) = net.iter().position(|c| matches!(c, Change::Added { path: p, .. } if *p == path)) {
                // Came and went.
                net.remove(i);
            } else if let Some(i) =
                net.iter().position(|c| matches!(c, Change::Renamed { path: p, .. } if *p == path))
            {
                if let Change::Renamed { from, is_dir, .. } = net.remove(i) {
                    net.push(Change::Removed { path: from, is_dir });
                }
            } else {
                net.push(Change::Removed { path, is_dir });
            }
        }
        Change::Renamed { from, path, is_dir, module } => {
            if let Some(i) = net.iter().position(|c| matches!(c, Change::Added { path: p, .. } if *p == from)) {
                net.remove(i);
                fold(net, Change::Added { path, is_dir, module });
            } else if let Some(i) =
                net.iter().position(|c| matches!(c, Change::Renamed { path: p, .. } if *p == from))
            {
                if let Change::Renamed { from: origin, .. } = net.remove(i) {
                    if origin != path {
                        fold(net, Change::Renamed { from: origin, path, is_dir, module });
                    }
                }
            } else if let Some(i) =
                net.iter().position(|c| matches!(c, Change::Removed { path: p, .. } if *p == path))
            {
                // Renamed over a name removed earlier: that name was replaced, and the source is gone.
                net.remove(i);
                fold(net, Change::Removed { path: from, is_dir });
            } else {
                net.push(Change::Renamed { from, path, is_dir, module });
            }
        }
    }
}

/// One directory removed and one added beside it, and nothing else of the kind, is a rename done
/// the long way — create the new folder, move the children, delete the old one — which is how an
/// IDE moves a module it has open.
fn pair_moves(mut net: Vec<Change>) -> Vec<Change> {
    let removed: Vec<usize> = (0..net.len()).filter(|&i| matches!(net[i], Change::Removed { is_dir: true, .. })).collect();
    let added: Vec<usize> = (0..net.len()).filter(|&i| matches!(net[i], Change::Added { is_dir: true, .. })).collect();
    let ([r], [a]) = (removed.as_slice(), added.as_slice()) else { return net };
    if parent(net[*r].path()) != parent(net[*a].path()) {
        return net;
    }
    let from = net[*r].path().to_string();
    let path = net[*a].path().to_string();
    net[*a] = Change::Renamed { from, path, is_dir: true, module: false };
    net.remove(*r);
    net
}

/// Drop what happened inside a directory that was itself added, removed or renamed — the tree is
/// reloaded either way, and a notice naming the folder is the one that says anything.
fn prune_descendants(net: Vec<Change>) -> Vec<Change> {
    let dirs: Vec<(String, Option<String>)> = net
        .iter()
        .filter(|c| c.is_dir())
        .map(|c| (c.path().to_string(), c.origin().map(str::to_string)))
        .collect();
    net.into_iter()
        .filter(|c| {
            let names: Vec<&str> = std::iter::once(c.path()).chain(c.origin()).collect();
            !dirs.iter().any(|(dir, origin)| {
                names.iter().any(|n| under(n, dir) || origin.as_deref().is_some_and(|o| under(n, o)))
            })
        })
        .collect()
}

/// Whether `path` is strictly inside `dir` (both root-relative, forward slashes).
fn under(path: &str, dir: &str) -> bool {
    path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
}

/// The root-relative parent, `""` for a top-level entry.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// Everything one root accumulated while the tree was busy.
#[derive(Debug, Default)]
pub struct Burst {
    paths: BTreeSet<String>,
    raw: Vec<RawChange>,
    structural: bool,
    rescan: bool,
}

impl Burst {
    /// Nothing to report.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.rescan
    }

    /// A create, remove or rename.
    pub fn record(&mut self, change: RawChange) {
        self.structural = true;
        self.paths.insert(change.rel.clone());
        if self.raw.len() < MAX_RAW {
            self.raw.push(change);
        } else {
            self.rescan = true;
        }
    }

    /// A content change to a file in [`CONTENT_FOLLOWED`].
    pub fn record_content(&mut self, rel: String) {
        self.paths.insert(rel);
    }

    /// Events may have been lost: the listener must trust a fresh read over anything it holds.
    pub fn mark_rescan(&mut self) {
        self.structural = true;
        self.rescan = true;
    }

    /// The `tree-changed` payload for `root`.
    pub fn into_payload(self, root: &str, is_module: impl Fn(&str) -> bool) -> Value {
        let truncated = self.paths.len() > MAX_PATHS;
        let mut changes = coalesce(&self.raw, is_module);
        changes.truncate(MAX_PATHS);
        json!({
            "root": root,
            "paths": self.paths.into_iter().take(MAX_PATHS).collect::<Vec<_>>(),
            "truncated": truncated,
            "structural": self.structural,
            "rescan": self.rescan,
            "changes": changes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, RemoveKind};

    fn raw(kind: Raw, rel: &str, is_dir: bool) -> RawChange {
        RawChange { kind, rel: rel.to_string(), is_dir }
    }

    fn no_modules(_: &str) -> bool {
        false
    }

    #[test]
    fn events_split_into_what_they_say_about_each_path() {
        let p = |s: &str| PathBuf::from(s);
        let both = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
        assert_eq!(
            expand(&both, &[p("/a"), p("/b")], |_| true),
            vec![(Raw::RenamedFrom, p("/a")), (Raw::RenamedTo, p("/b"))],
        );
        // A rename that does not say which side it is: the disk decides.
        let any = EventKind::Modify(ModifyKind::Name(RenameMode::Any));
        assert_eq!(expand(&any, &[p("/gone")], |_| false), vec![(Raw::Removed, p("/gone"))]);
        assert_eq!(expand(&any, &[p("/here")], |_| true), vec![(Raw::Created, p("/here"))]);
        assert_eq!(
            expand(&EventKind::Create(CreateKind::Folder), &[p("/d")], |_| true),
            vec![(Raw::Created, p("/d"))],
        );
        assert_eq!(
            expand(&EventKind::Remove(RemoveKind::Any), &[p("/d")], |_| false),
            vec![(Raw::Removed, p("/d"))],
        );
        assert_eq!(
            expand(&EventKind::Modify(ModifyKind::Data(DataChange::Any)), &[p("/f")], |_| true),
            vec![(Raw::Content, p("/f"))],
        );
        assert!(expand(&EventKind::Access(AccessKind::Any), &[p("/f")], |_| true).is_empty());
    }

    #[test]
    fn only_the_files_whose_content_matters_are_followed() {
        assert!(content_followed("pom.xml"));
        assert!(content_followed("core/pom.xml"));
        assert!(content_followed("crates/x/Cargo.toml"));
        assert!(content_followed(".gitignore"));
        assert!(!content_followed("logs/server.log"));
        assert!(!content_followed("src/Main.java"));
    }

    #[test]
    fn a_windows_rename_is_one_change() {
        let changes = coalesce(
            &[raw(Raw::RenamedFrom, "core", false), raw(Raw::RenamedTo, "kernel", true)],
            |p| p == "kernel",
        );
        assert_eq!(
            changes,
            vec![Change::Renamed { from: "core".into(), path: "kernel".into(), is_dir: true, module: true }],
        );
    }

    #[test]
    fn an_ide_safe_write_nets_out_to_nothing() {
        let changes = coalesce(
            &[
                raw(Raw::Created, "src/A.java___jb_tmp___", false),
                raw(Raw::RenamedFrom, "src/A.java", false),
                raw(Raw::RenamedTo, "src/A.java___jb_old___", false),
                raw(Raw::RenamedFrom, "src/A.java___jb_tmp___", false),
                raw(Raw::RenamedTo, "src/A.java", false),
                raw(Raw::Removed, "src/A.java___jb_old___", false),
            ],
            no_modules,
        );
        assert!(changes.is_empty(), "{changes:?}");
    }

    #[test]
    fn transient_and_replaced_entries_cancel() {
        assert!(coalesce(&[raw(Raw::Created, "tmp", false), raw(Raw::Removed, "tmp", false)], no_modules).is_empty());
        assert!(coalesce(&[raw(Raw::Removed, "a.txt", false), raw(Raw::Created, "a.txt", false)], no_modules).is_empty());
    }

    #[test]
    fn chained_renames_collapse_and_a_rename_back_is_nothing() {
        let chain = coalesce(
            &[
                raw(Raw::RenamedFrom, "a", true),
                raw(Raw::RenamedTo, "b", true),
                raw(Raw::RenamedFrom, "b", true),
                raw(Raw::RenamedTo, "c", true),
            ],
            no_modules,
        );
        assert_eq!(chain, vec![Change::Renamed { from: "a".into(), path: "c".into(), is_dir: true, module: false }]);
        let back = coalesce(
            &[
                raw(Raw::RenamedFrom, "a", true),
                raw(Raw::RenamedTo, "b", true),
                raw(Raw::RenamedFrom, "b", true),
                raw(Raw::RenamedTo, "a", true),
            ],
            no_modules,
        );
        assert!(back.is_empty(), "{back:?}");
    }

    #[test]
    fn what_happens_inside_a_moved_directory_folds_into_it() {
        let changes = coalesce(
            &[
                raw(Raw::RenamedFrom, "core", true),
                raw(Raw::RenamedTo, "kernel", true),
                raw(Raw::Created, "kernel/src/New.java", false),
                raw(Raw::Removed, "core/src/Old.java", false),
            ],
            no_modules,
        );
        assert_eq!(changes.len(), 1, "{changes:?}");
    }

    #[test]
    fn a_removed_folder_is_known_to_be_one_from_its_children() {
        let changes = coalesce(
            &[raw(Raw::Removed, "docs/a.md", false), raw(Raw::Removed, "docs", false)],
            no_modules,
        );
        assert_eq!(changes, vec![Change::Removed { path: "docs".into(), is_dir: true }]);
    }

    #[test]
    fn a_move_done_the_long_way_reads_as_a_rename() {
        let changes = coalesce(
            &[
                raw(Raw::Created, "modules/billing", true),
                raw(Raw::Created, "modules/billing/pom.xml", false),
                raw(Raw::Removed, "modules/invoices/pom.xml", false),
                raw(Raw::Removed, "modules/invoices", false),
            ],
            |p| p == "modules/billing",
        );
        assert_eq!(
            changes,
            vec![Change::Renamed {
                from: "modules/invoices".into(),
                path: "modules/billing".into(),
                is_dir: true,
                module: true,
            }],
        );
    }

    #[test]
    fn unrelated_folders_are_not_paired() {
        // Different parents: an addition and a removal, not a move.
        let changes = coalesce(&[raw(Raw::Created, "docs", true), raw(Raw::Removed, "old/site", true)], no_modules);
        assert_eq!(changes.len(), 2);
        // Two of each: nothing says which goes with which.
        let many = coalesce(
            &[
                raw(Raw::Created, "a", true),
                raw(Raw::Created, "b", true),
                raw(Raw::Removed, "c", true),
                raw(Raw::Removed, "d", true),
            ],
            no_modules,
        );
        assert_eq!(many.len(), 4);
    }

    #[test]
    fn a_burst_says_whether_the_listing_changed() {
        let mut content = Burst::default();
        content.record_content("pom.xml".into());
        let payload = content.into_payload("/w", no_modules);
        assert_eq!(payload["structural"], false);
        assert_eq!(payload["paths"][0], "pom.xml");

        let mut structural = Burst::default();
        structural.record(raw(Raw::Created, "docs", true));
        let payload = structural.into_payload("/w", no_modules);
        assert_eq!(payload["structural"], true);
        assert_eq!(payload["rescan"], false);
        assert_eq!(payload["changes"][0]["kind"], "added");
        assert_eq!(payload["changes"][0]["path"], "docs");
    }

    #[test]
    fn a_bulk_burst_is_reported_as_a_rescan() {
        let mut burst = Burst::default();
        for i in 0..=MAX_RAW {
            burst.record(raw(Raw::Created, &format!("f{i}"), false));
        }
        let payload = burst.into_payload("/w", no_modules);
        assert_eq!(payload["rescan"], true);
        assert_eq!(payload["truncated"], true);
    }
}
