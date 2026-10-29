//! Where the project will land, and whether it may.
//!
//! The rule is simple and has no override: **an existing, non-empty folder is never written into.**
//! An empty one is fine (the user made it for this), a missing one is created, and a file in the
//! way is an error. The check runs on every keystroke for the live path line, and again, atomically
//! with the move, when the run finishes.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// What is at the destination.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    /// Nothing there, and the parent exists.
    Free,
    /// Nothing there, and some parent folders will be created.
    FreeNewParents,
    /// An empty folder — usable.
    EmptyDir,
    /// A folder with content — refused.
    NonEmptyDir,
    /// A file with that name — refused.
    IsFile,
    /// The location is not usable (relative, has `..`, an ancestor is a file, read-only, …).
    BadLocation,
    /// Nothing typed yet.
    Incomplete,
}

#[derive(Serialize, Clone, Debug)]
pub struct TargetCheck {
    /// The full path, forward slashes; empty while incomplete.
    pub path: String,
    pub state: TargetState,
    /// A sentence for the user; absent when all is well.
    pub message: Option<String>,
}

impl TargetCheck {
    /// Whether a project can be created here.
    pub fn usable(&self) -> bool {
        matches!(self.state, TargetState::Free | TargetState::FreeNewParents | TargetState::EmptyDir)
    }
}

/// `location/name`, or `None` while either is empty.
pub fn join(location: &str, name: &str) -> Option<PathBuf> {
    let (loc, name) = (location.trim(), name.trim());
    if loc.is_empty() || name.is_empty() {
        return None;
    }
    Some(Path::new(loc).join(name))
}

/// A location the wizard accepts: absolute, and free of `..` (which would let a name written
/// next to it mean somewhere else).
pub fn location_problem(location: &str) -> Option<String> {
    let loc = location.trim();
    if loc.is_empty() {
        return Some("Choose a location".into());
    }
    let path = Path::new(loc);
    if !path.is_absolute() {
        return Some("The location must be an absolute path".into());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Some("The location can't contain “..”".into());
    }
    if loc.contains('\0') {
        return Some("The location contains an invalid character".into());
    }
    None
}

fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The nearest ancestor of `p` (or `p`) that exists.
fn nearest_existing(p: &Path) -> Option<&Path> {
    p.ancestors().find(|a| a.exists())
}

/// Inspect the destination for `location` + `name` (already validated as names).
pub fn inspect(location: &str, name: &str) -> TargetCheck {
    let Some(path) = join(location, name) else {
        return TargetCheck { path: String::new(), state: TargetState::Incomplete, message: None };
    };
    let shown = fwd(&path);
    let bad = |message: String| TargetCheck {
        path: shown.clone(),
        state: TargetState::BadLocation,
        message: Some(message),
    };
    if let Some(m) = location_problem(location) {
        return bad(m);
    }
    match std::fs::metadata(&path) {
        Ok(md) if md.is_file() => TargetCheck {
            path: shown,
            state: TargetState::IsFile,
            message: Some("A file with this name already exists here".into()),
        },
        Ok(_) => match std::fs::read_dir(&path) {
            Ok(mut it) => {
                if it.next().is_none() {
                    TargetCheck { path: shown, state: TargetState::EmptyDir, message: None }
                } else {
                    TargetCheck {
                        path: shown,
                        state: TargetState::NonEmptyDir,
                        message: Some("This folder already exists and isn't empty — pick another name".into()),
                    }
                }
            }
            Err(e) => bad(format!("Can't read this folder: {e}")),
        },
        Err(_) => {
            let Some(anchor) = nearest_existing(&path) else {
                return bad("The location doesn't exist".into());
            };
            match std::fs::metadata(anchor) {
                Ok(md) if !md.is_dir() => {
                    bad(format!("“{}” is a file, not a folder", fwd(anchor)))
                }
                Ok(md) if md.permissions().readonly() => {
                    bad(format!("“{}” is read-only", fwd(anchor)))
                }
                Ok(_) => {
                    let parent_exists = path.parent().is_some_and(Path::exists);
                    TargetCheck {
                        path: shown,
                        state: if parent_exists { TargetState::Free } else { TargetState::FreeNewParents },
                        message: (!parent_exists).then(|| "The missing folders will be created".into()),
                    }
                }
                Err(e) => bad(format!("Can't access the location: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-np-target-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn incomplete_until_both_are_typed() {
        assert_eq!(inspect("", "x").state, TargetState::Incomplete);
        assert_eq!(inspect("/tmp", "").state, TargetState::Incomplete);
    }

    #[test]
    fn relative_and_dotdot_locations_are_refused() {
        assert!(location_problem("rel/path").is_some());
        let up = std::env::temp_dir().join("..").join("x");
        assert!(location_problem(&up.to_string_lossy()).is_some());
    }

    #[test]
    fn free_empty_nonempty_and_file() {
        let d = scratch("states");
        let loc = d.to_string_lossy().to_string();
        assert_eq!(inspect(&loc, "fresh").state, TargetState::Free);

        std::fs::create_dir(d.join("empty")).unwrap();
        assert_eq!(inspect(&loc, "empty").state, TargetState::EmptyDir);

        std::fs::create_dir(d.join("full")).unwrap();
        std::fs::write(d.join("full").join("a.txt"), "x").unwrap();
        let full = inspect(&loc, "full");
        assert_eq!(full.state, TargetState::NonEmptyDir);
        assert!(!full.usable());

        std::fs::write(d.join("afile"), "x").unwrap();
        assert_eq!(inspect(&loc, "afile").state, TargetState::IsFile);

        let deep = d.join("a").join("b").to_string_lossy().to_string();
        assert_eq!(inspect(&deep, "p").state, TargetState::FreeNewParents);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_in_the_chain_is_a_bad_location() {
        let d = scratch("chain");
        std::fs::write(d.join("f"), "x").unwrap();
        let loc = d.join("f").join("sub").to_string_lossy().to_string();
        assert_eq!(inspect(&loc, "p").state, TargetState::BadLocation);
        let _ = std::fs::remove_dir_all(&d);
    }
}
