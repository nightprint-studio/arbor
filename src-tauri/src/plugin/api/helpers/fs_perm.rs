//! Filesystem permission check for `arbor.fs.*`.
//!
//! Closures capture the permission as a `(AccessLevel, Vec<String>)` tuple
//! (`fs_perm` and `fs_scope`). The level controls read vs. write; the scope
//! controls path bounds:
//!   - empty list (default) → sandboxed to the active repo's directory
//!   - `["*"]`              → unrestricted (any path on disk)
//!   - other absolute paths → allowed in addition to the active repo

use std::path::{Path, PathBuf};

use mlua::Lua;

use crate::plugin::runtime::AccessLevel;

/// `fp.0` = AccessLevel, `fp.1` = scope list. Bundled as a tuple so the
/// per-closure `let fp = fp.clone()` capture pattern stays terse across
/// the ~20 fs ops.
pub(crate) type FsPerm = (AccessLevel, Vec<String>);

pub(crate) fn check_fs_read(
    lua: &Lua,
    path: &Path,
    fp: &FsPerm,
) -> mlua::Result<()> {
    if fp.0 < AccessLevel::Read {
        return Err(mlua::Error::RuntimeError(
            "arbor.fs: filesystem read denied (set fs = \"read\" or \"write\" in plugin.toml)".to_string()
        ));
    }
    check_in_scope(lua, path, &fp.1)
}

pub(crate) fn check_fs_write(
    lua: &Lua,
    path: &Path,
    fp: &FsPerm,
) -> mlua::Result<()> {
    if fp.0 < AccessLevel::Write {
        return Err(mlua::Error::RuntimeError(
            "arbor.fs: filesystem write denied (set fs = \"write\" in plugin.toml)".to_string()
        ));
    }
    check_in_scope(lua, path, &fp.1)
}

fn check_in_scope(lua: &Lua, path: &Path, fs_scope: &[String]) -> mlua::Result<()> {
    // Unrestricted sentinel — any path is allowed.
    if fs_scope.iter().any(|s| s == "*") {
        return Ok(());
    }

    // Active repo directory is always part of the sandbox.
    let repo = lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None);

    // Materialize the absolute path of the input. Relative paths resolve
    // against the active repo when present.
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(ref r) = repo {
        PathBuf::from(r).join(path)
    } else {
        // No active repo and no extra scope means we cannot resolve a relative
        // path — bail out with a clear message.
        if fs_scope.is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.fs: no active repository (default sandbox requires an open repo)".to_string()
            ));
        }
        path.to_path_buf()
    };

    // Allowed roots = active repo (if any) + every entry in fs_scope.
    let mut allowed: Vec<PathBuf> = Vec::new();
    if let Some(ref r) = repo { allowed.push(PathBuf::from(r)); }
    for s in fs_scope { allowed.push(PathBuf::from(s)); }

    if allowed.iter().any(|root| abs.starts_with(root)) {
        return Ok(());
    }

    let scope_desc = if let Some(ref r) = repo {
        format!("repo: {}, extra: {:?}", r, fs_scope)
    } else {
        format!("scope: {:?}", fs_scope)
    };
    Err(mlua::Error::RuntimeError(format!(
        "arbor.fs: path '{}' is outside the allowed scope ({})",
        path.display(), scope_desc
    )))
}

/// The current active repo path, looked up from the Lua global maintained by
/// the host. Raises a Lua error when no repo is active — used by callers that
/// can't proceed without one (e.g. project settings).
pub(crate) fn current_repo(lua: &Lua) -> mlua::Result<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
        .ok_or_else(|| mlua::Error::RuntimeError(
            "arbor.settings.project: no active repository".to_string()
        ))
}
