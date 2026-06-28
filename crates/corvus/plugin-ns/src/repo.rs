//! `arbor.repo` (repository introspection + git ops), ported to run through an
//! [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's `ns_shell/repo.rs`:
//! same namespace (`arbor.repo`), same function names (`current` / `branch` /
//! `is_dirty` / `remote` / `fetch_active_tab` / `release_handles` / `branches` /
//! `tags` / `commits` / `untracked` / `staged_files` / `clone`), same argument
//! shapes, same `(value, err)` / `(false, err)` tuple conventions, same
//! permission-gate `RuntimeError` strings, same `repo.<op> …` error prefixes.
//!
//! The only difference is *where the work goes*:
//!
//!   · the shell opened the active repo with `git2` straight on the
//!     `__arbor_current_repo__` path (for the introspection ops) and reached into
//!     `AppState` for the tab-aware ops (`fetch_active_tab`, `release_handles`,
//!     `clone`);
//!   · here every git-touching op goes through the captured `Arc<dyn NsHost>`,
//!     which opens that same path with `git2` and runs the shared logic — so
//!     results and error text match. `current()` reads the global directly (it is
//!     pure Lua-global data, no host needed).
//!
//! Calling convention (unchanged from the shell — see `ns_shell/repo.rs`):
//!   · `current()` is pure data — returns `string|nil`, no error.
//!   · `branch` / `is_dirty` / `remote` / `branches` / `tags` / `commits` /
//!     `untracked` / `staged_files` return `(value, nil)` on success and
//!     `(nil, err)` (or `(false, err)` for `is_dirty`) on git failures. "No repo
//!     open" is recoverable → an empty value (table / `nil` / `false`), never an
//!     error.
//!   · `fetch_active_tab` returns `(true, nil) | (false, err)`.
//!   · `release_handles` returns nothing.
//!   · `clone{...}` returns `(job_id, nil) | (nil, err)`; validation issues raise.
//!   · Permission denied raises a Lua error.
//! Requires git ≥ Read for the introspection ops, git ≥ Write for
//! `fetch_active_tab` / `clone`.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active (mirrors the shell's "no repo open → empty result").
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.repo.*` installer. Holds the host handle the closures call through.
pub struct RepoInstaller {
    host: NsHostHandle,
}

impl RepoInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for RepoInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let repo_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_current(lua, &repo_table)?;
        install_branch(self.host.clone(), ctx, lua, &repo_table)?;
        install_is_dirty(self.host.clone(), ctx, lua, &repo_table)?;
        install_remote(self.host.clone(), ctx, lua, &repo_table)?;
        install_fetch_active_tab(self.host.clone(), ctx, lua, &repo_table)?;
        install_release_handles(self.host.clone(), ctx, lua, &repo_table)?;
        install_branches(self.host.clone(), ctx, lua, &repo_table)?;
        install_tags(self.host.clone(), ctx, lua, &repo_table)?;
        install_commits(self.host.clone(), ctx, lua, &repo_table)?;
        install_untracked(self.host.clone(), ctx, lua, &repo_table)?;
        install_staged_files(self.host.clone(), ctx, lua, &repo_table)?;
        install_clone(self.host.clone(), ctx, lua, &repo_table)?;

        arbor
            .set("repo", repo_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_current(lua: &Lua, repo_table: &Table) -> PluginCoreResult<()> {
    // current() → string | nil   (no error path — global state). Reads the Lua
    // global directly, exactly like the shell (no host round-trip needed).
    let fn_ = lua
        .create_function(|lua_ctx, ()| {
            Ok(lua_ctx
                .globals()
                .get::<Option<String>>("__arbor_current_repo__")
                .unwrap_or(None)
                .map(|s| mlua::Value::String(lua_ctx.create_string(s.as_bytes()).unwrap()))
                .unwrap_or(mlua::Value::Nil))
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("current", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branch(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.branch: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, mlua::Value::Nil);
            };
            match host.repo_branch(&path) {
                Ok(name) => ok2(lua_ctx, name),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("branch", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_dirty(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.is_dirty: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, false);
            };
            match host.repo_is_dirty(&path) {
                Ok(b) => ok2(lua_ctx, b),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("is_dirty", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_remote(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, name: String| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.remote: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, mlua::Value::Nil);
            };
            // Host returns `Ok(Some(url))` when found, `Ok(None)` when the remote
            // is absent (→ Lua nil), `Err` on repo-open failure.
            match host.repo_remote(&path, &name) {
                Ok(Some(url)) => ok2(lua_ctx, url),
                Ok(None) => ok2(lua_ctx, mlua::Value::Nil),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("remote", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_fetch_active_tab(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    // fetch_active_tab() → (true, nil) | (false, err)
    // Fetches origin for the active repo and emits "arbor://graph-refresh" on
    // success (the host owns the event sink). Requires git_write.
    let git_write = ctx.git_write;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_write {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.fetch_active_tab: requires git = \"write\" (or higher)".to_string(),
                ));
            }
            // The shell short-circuited to (false, "no active tab") when no tab was
            // active; the path-resolved equivalent is "no repo open".
            let Some(path) = current_repo(lua_ctx) else {
                return boolerr2(lua_ctx, false, Some("no active tab".into()));
            };
            match host.repo_fetch_active_tab(&path) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("fetch_active_tab", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_release_handles(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ()| {
            let Some(path) = current_repo(lua_ctx) else {
                return Ok(());
            };
            host.repo_release_handles(&path);
            Ok(())
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("release_handles", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branches(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.branches: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            // Host returns a JSON array of { name, is_remote, is_head }.
            let json = match host.repo_branches(&path) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("repo.branches to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("branches", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_tags(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.tags: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            // Host returns a JSON array of { name, target? }.
            let json = match host.repo_tags(&path) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("repo.tags to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("tags", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_commits(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    // commits(opts?) → (commit[], nil) | (nil, err)
    // opts: { from?, to?, limit?, include_merges? } — see ns_shell/repo.rs for the
    // semantics (from = exclusive lower bound; to = inclusive upper, default
    // "HEAD"; limit default 1000; include_merges default true). The installer
    // normalises the option table exactly as the shell did, then hands the four
    // resolved values to the host, which walks the revlog and returns the JSON
    // array of commit records.
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.commits: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };

            let (from, to, limit, include_merges) = match opts {
                Some(t) => (
                    t.get::<Option<String>>("from")
                        .ok()
                        .flatten()
                        .and_then(|s| if s.trim().is_empty() { None } else { Some(s) }),
                    t.get::<Option<String>>("to")
                        .ok()
                        .flatten()
                        .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
                        .unwrap_or_else(|| "HEAD".to_string()),
                    t.get::<Option<i64>>("limit")
                        .ok()
                        .flatten()
                        .map(|n| n.max(0) as u64)
                        .unwrap_or(1000),
                    t.get::<Option<bool>>("include_merges")
                        .ok()
                        .flatten()
                        .unwrap_or(true),
                ),
                None => (None, "HEAD".to_string(), 1000u64, true),
            };

            let json = match host.repo_commits(&path, from.as_deref(), &to, limit, include_merges) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("repo.commits to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("commits", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_untracked(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    // untracked() → (string[], nil) | (nil, err) — relative paths of files that
    // are untracked AND not ignored (see ns_shell/repo.rs).
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.untracked: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            // Host returns a JSON array of relative path strings.
            let json = match host.repo_untracked(&path) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("repo.untracked to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("untracked", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_staged_files(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    // staged_files() → ({path, status}[], nil) | (nil, err) — files whose INDEX
    // differs from HEAD, with status one of "added" | "modified" | "deleted" |
    // "renamed" | "typechange" (see ns_shell/repo.rs).
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.staged_files: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            let Some(path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            // Host returns a JSON array of { path, status } objects.
            let json = match host.repo_staged_files(&path) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("repo.staged_files to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("staged_files", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clone(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    repo_table: &Table,
) -> PluginCoreResult<()> {
    // clone(cfg) → (job_id, nil) | (nil, err)
    // Clone a remote repository in the background. Validation issues (missing
    // url/dest, empty strings) raise — they're programming errors. Backend-state
    // failures (no reverse channel, job registration) come back as (nil, err).
    //
    // The installer validates + normalises the cfg table exactly as the shell did,
    // then hands the resolved values to the host, which mints the job, registers
    // it in the shell's registry, emits arbor://job-started and spawns the clone.
    let git_write = ctx.git_write;
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, cfg: Table| -> LuaTuple {
            if !git_write {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.clone: requires git = \"write\" (or higher)".to_string(),
                ));
            }

            let url: String = cfg.get("url").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.repo.clone: 'url' is required (string)".to_string(),
                )
            })?;
            let dest: String = cfg.get("dest").map_err(|_| {
                mlua::Error::RuntimeError(
                    "arbor.repo.clone: 'dest' is required (string)".to_string(),
                )
            })?;
            if url.trim().is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.clone: 'url' cannot be empty".to_string(),
                ));
            }
            if dest.trim().is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "arbor.repo.clone: 'dest' cannot be empty".to_string(),
                ));
            }

            let branch: Option<String> = cfg.get("branch").ok().filter(|s: &String| !s.is_empty());
            let shallow: bool = cfg.get("shallow").unwrap_or(false);
            let recurse: bool = cfg.get("recurse_submodules").unwrap_or(false);
            let name_override: Option<String> = cfg.get("name").ok();
            let category_override: Option<String> = cfg.get("category").ok();

            // The clone config marshalled to JSON for the host. `on_done` is
            // intentionally NOT forwarded: the host runs the background clone over
            // the reverse-channel job registry, where re-entering this plugin's Lua
            // state from the worker is not available (the shell's
            // __arbor_hooks__/__job_done callback path is shell-process-local). The
            // job still streams arbor://job-* events the same way.
            let cfg_json = serde_json::json!({
                "url": url,
                "dest": dest,
                "branch": branch,
                "shallow": shallow,
                "recurse_submodules": recurse,
                "name": name_override,
                "category": category_override,
                "plugin_name": pname,
            });

            match host.repo_clone(cfg_json) {
                Ok(job_id) => ok2(lua_ctx, job_id),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    repo_table
        .set("clone", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
