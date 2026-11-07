//! Where on disk a package may reach — one definition, for every host.
//!
//! The rule lived inside `arbor.fs`'s Lua binding. A wasm guest reads files too (the `fs`
//! interface in `wit/host.wit`), and two copies of "is this path inside the sandbox" is two
//! places to get a root wrong — in a check whose whole job is to be right. Same story as
//! [`crate::network`].
//!
//! ## The rule
//!
//! `[permissions] fs_scope` plus the active repository decide the allowed roots:
//!
//! * empty (the default) — the active repository's directory, and nothing else;
//! * `["*"]` — anything, and the manifest is saying so out loud;
//! * other absolute paths — allowed **in addition to** the active repository.
//!
//! A relative path resolves against the active repository; without one it cannot be resolved
//! and is refused (unless an extra root exists, in which case it is taken as written, which is
//! what the Lua host always did).
//!
//! ## What this does and does not do
//!
//! It is **lexical and pure**: no I/O, so it can run anywhere and be tested without a disk. The
//! path is cleaned first — `.` dropped, `..` folded — so `<repo>/../etc` is outside, not
//! "inside because it starts with the repo". What it cannot see is a *symlink*: that needs the
//! filesystem, and a host that reads files (the wasm one) canonicalises the result and checks
//! it against [`allowed_roots`] again.

use std::path::{Component, Path, PathBuf};

/// Why a path was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsScopeDenial {
    /// A relative path, no active repository and no extra root: nothing to resolve it against.
    NoRepo,
    /// The path is not under any allowed root.
    Outside { path: PathBuf, scope: String },
}

impl std::fmt::Display for FsScopeDenial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsScopeDenial::NoRepo => {
                write!(f, "no active repository (default sandbox requires an open repo)")
            }
            FsScopeDenial::Outside { path, scope } => {
                write!(f, "path '{}' is outside the allowed scope ({scope})", path.display())
            }
        }
    }
}

impl std::error::Error for FsScopeDenial {}

/// Fold `.` and `..` without touching the disk. A `..` at the root stays at the root.
pub fn lexical_clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never pop past a root or prefix: `/..` is `/`.
                let can_pop = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether the scope list says "any path".
pub fn is_unrestricted(fs_scope: &[String]) -> bool {
    fs_scope.iter().any(|s| s == "*")
}

/// The roots a package may reach: the active repository first, then every `fs_scope` entry.
pub fn allowed_roots(repo: Option<&Path>, fs_scope: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(r) = repo {
        roots.push(lexical_clean(r));
    }
    for s in fs_scope.iter().filter(|s| s.as_str() != "*") {
        roots.push(lexical_clean(Path::new(s)));
    }
    roots
}

/// The whole lexical gate: may a package with this scope reach `path`? Returns the absolute,
/// cleaned path it resolved to.
pub fn resolve_in_scope(
    repo: Option<&Path>,
    fs_scope: &[String],
    path: &Path,
) -> Result<PathBuf, FsScopeDenial> {
    // Materialise the absolute path. Relative paths resolve against the active repo.
    let abs = if path.is_absolute() {
        lexical_clean(path)
    } else if let Some(r) = repo {
        lexical_clean(&r.join(path))
    } else if fs_scope.is_empty() {
        return Err(FsScopeDenial::NoRepo);
    } else {
        lexical_clean(path)
    };

    if is_unrestricted(fs_scope) {
        return Ok(abs);
    }
    let roots = allowed_roots(repo, fs_scope);
    if roots.iter().any(|root| abs.starts_with(root)) {
        return Ok(abs);
    }
    let scope = match repo {
        Some(r) => format!("repo: {}, extra: {:?}", r.display(), fs_scope),
        None => format!("scope: {:?}", fs_scope),
    };
    Err(FsScopeDenial::Outside { path: path.to_path_buf(), scope })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_path_inside_the_repo_resolves_to_itself() {
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("/repo/a/b.txt")).unwrap();
        assert_eq!(r, p("/repo/a/b.txt"));
    }

    #[test]
    fn a_relative_path_resolves_against_the_repo() {
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("a/b.txt")).unwrap();
        assert_eq!(r, p("/repo/a/b.txt"));
    }

    #[test]
    fn dotdot_cannot_climb_out_of_the_repo() {
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("/repo/../etc/passwd"));
        assert!(matches!(r, Err(FsScopeDenial::Outside { .. })));
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("../etc"));
        assert!(matches!(r, Err(FsScopeDenial::Outside { .. })));
    }

    #[test]
    fn dotdot_that_stays_inside_is_fine() {
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("/repo/a/../b")).unwrap();
        assert_eq!(r, p("/repo/b"));
    }

    #[test]
    fn a_sibling_that_shares_a_prefix_is_outside() {
        // `starts_with` is by component, so `/repo-other` is not under `/repo`.
        let r = resolve_in_scope(Some(Path::new("/repo")), &[], Path::new("/repo-other/x"));
        assert!(r.is_err());
    }

    #[test]
    fn an_extra_root_is_allowed_besides_the_repo() {
        let scope = vec!["/data".to_string()];
        assert!(resolve_in_scope(Some(Path::new("/repo")), &scope, Path::new("/data/x")).is_ok());
        assert!(resolve_in_scope(Some(Path::new("/repo")), &scope, Path::new("/repo/x")).is_ok());
        assert!(resolve_in_scope(Some(Path::new("/repo")), &scope, Path::new("/other")).is_err());
    }

    #[test]
    fn the_star_scope_is_unrestricted() {
        let scope = vec!["*".to_string()];
        assert!(resolve_in_scope(None, &scope, Path::new("/anywhere/x")).is_ok());
    }

    #[test]
    fn no_repo_and_no_scope_refuses_a_relative_path() {
        assert_eq!(resolve_in_scope(None, &[], Path::new("x")), Err(FsScopeDenial::NoRepo));
    }

    #[test]
    fn no_repo_and_no_scope_refuses_an_absolute_path() {
        assert!(matches!(
            resolve_in_scope(None, &[], Path::new("/x")),
            Err(FsScopeDenial::Outside { .. })
        ));
    }

    #[test]
    fn lexical_clean_never_pops_past_the_root() {
        assert_eq!(lexical_clean(Path::new("/../..")), p("/"));
        assert_eq!(lexical_clean(Path::new("a/./b/../c")), p("a/c"));
    }
}
