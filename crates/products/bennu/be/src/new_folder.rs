//! `new_folder` domain — `bennu_new_folder`: create a directory, or a whole package, from one
//! line of typing.
//!
//! ## One field, several levels
//!
//! `ciao/ciao` is two directories, and typing it should make two — the alternative is opening
//! the same dialog once per level, which is the thing you notice every time you scaffold a
//! package chain. So the name is a **path**, split on the separators, and each level is created
//! in turn.
//!
//! Under a Java source root a **dot** separates too, because that is how the thing being named
//! is written: `it.acme.web` is a package, and a package is directories. Whether the target is
//! package territory is the caller's to say ([`NewFolderArgs::as_package`]) — the project tree
//! already decides it to draw the row as a package instead of a folder chain, and having the two
//! disagree is how a `.github` under a source root would become a `github`.
//!
//! ## Only what is missing
//!
//! The levels are walked one at a time, and an existing one is stepped through rather than
//! objected to: typing `src/main/resources` where `src/main` is already there creates
//! `resources` and nothing else. What was actually created comes back in
//! [`NewFolderResult::created`], so the caller can say so rather than guess.
//!
//! A failure part-way leaves the levels already made — they are directories, they are empty, and
//! removing them would mean removing something that might have been there a moment before this
//! call for reasons of its own.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_new_folder`].
#[derive(Deserialize)]
pub struct NewFolderArgs {
    /// Absolute path to the project root — nothing is created outside it.
    pub root: String,
    /// The directory to create in (absolute, forward slashes).
    pub dir: String,
    /// What the user typed: one name, or a path of them (`assets/icons`), or — in package
    /// territory — a package (`it.acme.web`).
    pub name: String,
    /// Whether a `.` separates levels, i.e. whether `dir` is a Java source root or inside one.
    #[serde(default)]
    pub as_package: bool,
}

/// What a create did.
#[derive(Serialize)]
pub struct NewFolderResult {
    /// Absolute path (forward slashes) of the **deepest** directory — the one to reveal.
    pub path: String,
    /// The directories that were actually created, outermost first. Empty when every level
    /// was already there.
    pub created: Vec<String>,
    /// True when nothing was created because the whole path already existed.
    pub existed: bool,
}

/// Characters no filesystem takes in a name. The separators are absent on purpose: they have
/// already done their job by the time a segment is checked.
const INVALID: &[char] = &[':', '*', '?', '"', '<', '>', '|'];

/// The directory levels `name` stands for, in order.
///
/// Empty levels are dropped rather than refused — a trailing slash, a doubled one and a stray
/// space around a segment are all typing, not intent. `.` and `..` are refused outright: they
/// are the two names that would make the result mean somewhere else.
fn segments(name: &str, as_package: bool) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for level in name.split(['/', '\\']) {
        // In package territory the dot is a separator like the slash; everywhere else it is a
        // perfectly ordinary character in a directory name (`.github`, `my.config`).
        let parts: Vec<&str> = if as_package { level.split('.').collect() } else { vec![level] };
        for part in parts {
            let seg = part.trim();
            if seg.is_empty() {
                continue;
            }
            if seg == "." || seg == ".." {
                return Err("A folder cannot be named “.” or “..”".into());
            }
            if seg.contains(INVALID) || seg.chars().any(char::is_control) {
                return Err(format!("“{seg}” can't be a folder name: : * ? \" < > | aren't allowed"));
            }
            out.push(seg.to_string());
        }
    }
    if out.is_empty() {
        return Err("Type a name".into());
    }
    Ok(out)
}

/// A path with forward slashes and no trailing one — the shape every path crosses the seam in.
fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Whether `dir` is the project root or somewhere under it.
fn inside(root: &str, dir: &str) -> bool {
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let dir = dir.replace('\\', "/");
    let dir = dir.trim_end_matches('/');
    dir == root || dir.starts_with(&format!("{root}/"))
}

/// Create a directory (or a chain of them) under `dir`.
#[arbor_rpc::handler]
fn bennu_new_folder(_ctx: &BennuState, args: NewFolderArgs) -> Result<NewFolderResult, String> {
    if !inside(&args.root, &args.dir) {
        return Err("outside the project".into());
    }
    let segs = segments(&args.name, args.as_package)?;

    let mut path = PathBuf::from(args.dir.trim_end_matches(['/', '\\']));
    let mut created = Vec::new();
    for seg in &segs {
        path.push(seg);
        if path.is_dir() {
            continue;
        }
        // A file sitting where a directory should go: said plainly, because `create_dir`'s own
        // error for it names an errno and not the thing that is in the way.
        if path.exists() {
            return Err(format!("“{seg}” is already a file"));
        }
        std::fs::create_dir(&path).map_err(|e| format!("{}: {e}", fwd(&path)))?;
        created.push(fwd(&path));
    }

    Ok(NewFolderResult { path: fwd(&path), existed: created.is_empty(), created })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(name: &str, pkg: bool) -> Vec<String> {
        segments(name, pkg).expect("should split")
    }

    #[test]
    fn one_name_is_one_level() {
        assert_eq!(segs("assets", false), ["assets"]);
    }

    #[test]
    fn slashes_nest() {
        assert_eq!(segs("ciao/ciao", false), ["ciao", "ciao"]);
        assert_eq!(segs("a\\b/c", false), ["a", "b", "c"]);
    }

    #[test]
    fn dots_nest_only_in_package_territory() {
        assert_eq!(segs("it.acme.web", true), ["it", "acme", "web"]);
        // Outside a source root a dot is a character, not a separator — `.github` is one folder.
        assert_eq!(segs(".github", false), [".github"]);
        assert_eq!(segs("my.config", false), ["my.config"]);
    }

    #[test]
    fn the_two_separators_mix() {
        assert_eq!(segs("it.acme/web", true), ["it", "acme", "web"]);
    }

    #[test]
    fn stray_separators_and_spaces_are_typing() {
        assert_eq!(segs("  a / / b/ ", false), ["a", "b"]);
        assert_eq!(segs("it..acme.", true), ["it", "acme"]);
    }

    #[test]
    fn traversal_is_refused() {
        assert!(segments("../evil", false).is_err());
        assert!(segments(".", false).is_err());
        // In package mode `..` splits into nothing at all, which leaves no name to create.
        assert!(segments("..", true).is_err());
    }

    #[test]
    fn empty_is_refused() {
        assert!(segments("   ", false).is_err());
        assert!(segments("///", false).is_err());
    }

    #[test]
    fn illegal_characters_are_refused() {
        assert!(segments("a:b", false).is_err());
        assert!(segments("what?", false).is_err());
    }

    #[test]
    fn the_root_itself_is_inside() {
        assert!(inside("/proj", "/proj"));
        assert!(inside("/proj", "/proj/src/main"));
        assert!(inside("C:\\proj", "C:/proj/src"));
        assert!(!inside("/proj", "/proj-other/src"));
        assert!(!inside("/proj", "/etc"));
    }
}
