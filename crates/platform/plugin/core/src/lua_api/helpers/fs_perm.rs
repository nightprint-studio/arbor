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

use arbor_plugin_types::prelude::{fs_resolve_in_scope, AccessLevel};

/// `fp.0` = AccessLevel, `fp.1` = scope list. Bundled as a tuple so the
/// per-closure `let fp = fp.clone()` capture pattern stays terse across
/// the ~20 fs ops.
pub type FsPerm = (AccessLevel, Vec<String>);

/// Check a read, and hand back the absolute path it resolved to.
///
/// The path is a return value because one caller genuinely needs it: `arbor.ext.call_*_file`
/// performs its I/O in the SHELL process, whose working directory is not the backend's, so a
/// relative path that means one thing here would mean another there. Every other caller does
/// its own I/O in this process and writes `check_fs_read(..)?;`, discarding it.
pub fn check_fs_read(
    lua: &Lua,
    path: &Path,
    fp: &FsPerm,
) -> mlua::Result<PathBuf> {
    if fp.0 < AccessLevel::Read {
        return Err(mlua::Error::RuntimeError(
            "arbor.fs: filesystem read denied (set fs = \"read\" or \"write\" in plugin.toml)".to_string()
        ));
    }
    check_in_scope(lua, path, &fp.1)
}

/// Check a write, and hand back the absolute path it resolved to. See [`check_fs_read`].
pub fn check_fs_write(
    lua: &Lua,
    path: &Path,
    fp: &FsPerm,
) -> mlua::Result<PathBuf> {
    if fp.0 < AccessLevel::Write {
        return Err(mlua::Error::RuntimeError(
            "arbor.fs: filesystem write denied (set fs = \"write\" in plugin.toml)".to_string()
        ));
    }
    check_in_scope(lua, path, &fp.1)
}

fn check_in_scope(lua: &Lua, path: &Path, fs_scope: &[String]) -> mlua::Result<PathBuf> {
    // The rule itself — roots, the `"*"` sentinel, relative resolution — is shared with the wasm
    // host (`arbor_plugin_types::fs_scope`): "may this package reach this path" must not mean two
    // different things depending on which language the package was written in.
    let repo = lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None);
    fs_resolve_in_scope(repo.as_deref().map(Path::new), fs_scope, path)
        .map_err(|e| mlua::Error::RuntimeError(format!("arbor.fs: {e}")))
}

/// The current active repo path, looked up from the Lua global maintained by
/// the host. Raises a Lua error when no repo is active — used by callers that
/// can't proceed without one (e.g. project settings).
pub fn current_repo(lua: &Lua) -> mlua::Result<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
        .ok_or_else(|| mlua::Error::RuntimeError(
            "arbor.settings.project: no active repository".to_string()
        ))
}
