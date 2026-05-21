//! `arbor.repo` — repository introspection + git ops.
//!
//! Calling convention (Phase 1+2):
//!   · `current()` is pure data — returns `string|nil`, no error.
//!   · Git-touching ops (`branch`, `is_dirty`, `remote`, `branches`,
//!     `tags`, `fetch_active_tab`) return `(value, nil)` on success and
//!     `(nil, err)` (or `(false, err)` for the boolean ops) on git failures.
//!     "No repo open" is a recoverable case → `(nil, "no active repo")`.
//!   · Permission denied raises a Lua error.
//!   · `clone` returns `(job_id, nil)` on launch and `(nil, err)` on
//!     synchronous failure (missing handle, lock poisoning, …).

use mlua::{Lua, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let repo_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_current(lua, &repo_table)?;
    install_branch(ctx, lua, &repo_table)?;
    install_is_dirty(ctx, lua, &repo_table)?;
    install_remote(ctx, lua, &repo_table)?;
    install_fetch_active_tab(ctx, lua, &repo_table)?;
    install_release_handles(ctx, lua, &repo_table)?;
    install_branches(ctx, lua, &repo_table)?;
    install_tags(ctx, lua, &repo_table)?;
    install_commits(ctx, lua, &repo_table)?;
    install_untracked(ctx, lua, &repo_table)?;
    install_clone(ctx, lua, &repo_table)?;

    arbor.set("repo", repo_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_current(lua: &Lua, repo_table: &Table) -> Result<()> {
    // current() → string | nil   (no error path — global state)
    let fn_ = lua.create_function(|lua_ctx, ()| {
        Ok(lua_ctx.globals().get::<Option<String>>("__arbor_current_repo__")
            .unwrap_or(None)
            .map(|s| mlua::Value::String(lua_ctx.create_string(s.as_bytes()).unwrap()))
            .unwrap_or(mlua::Value::Nil))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("current", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branch(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.branch: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, mlua::Value::Nil); };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.branch open: {e}")),
        };
        let head = match repo.head() {
            Ok(h)  => h,
            Err(e) => return err2(lua_ctx, format!("repo.branch head: {e}")),
        };
        ok2(lua_ctx, head.shorthand().unwrap_or("HEAD").to_string())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("branch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_is_dirty(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.is_dirty: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, false); };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.is_dirty open: {e}")),
        };
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let dirty = match repo.statuses(Some(&mut opts)) {
            Ok(s)  => Ok(!s.is_empty()),
            Err(e) => Err(format!("repo.is_dirty statuses: {e}")),
        };
        match dirty {
            Ok(b)  => ok2(lua_ctx, b),
            Err(e) => err2(lua_ctx, e),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("is_dirty", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_remote(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, name: String| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.remote: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, mlua::Value::Nil); };

        // Extract URL in an inner block so `repo` and the borrowed
        // `Remote<'_>` are both dropped before we re-enter Lua.
        let result: std::result::Result<Option<String>, String> = {
            match git2::Repository::open(&path) {
                Ok(repo) => Ok(repo.find_remote(&name).ok()
                    .and_then(|r| r.url().map(|s| s.to_string()))),
                Err(e)   => Err(e.to_string()),
            }
        };
        match result {
            Ok(Some(url)) => ok2(lua_ctx, url),
            Ok(None)      => ok2(lua_ctx, mlua::Value::Nil),
            Err(e)        => err2(lua_ctx, format!("repo.remote open: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("remote", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_fetch_active_tab(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    // fetch_active_tab() → (true, nil) | (false, err)
    // Fetches origin for the currently active tab. Emits
    // "arbor://graph-refresh" on success.  Requires git_write = true.
    let git_write = ctx.git_write;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_write {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.fetch_active_tab: requires git = \"write\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("app handle unavailable".into()));
        };
        let state = h.state::<crate::AppState>();
        let tab_id = {
            let lock = state.active_tab_id.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            lock.clone()
        };
        let Some(tab_id) = tab_id else {
            return boolerr2(lua_ctx, false, Some("no active tab".into()));
        };
        let result = {
            let mut mgr = state.repos.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            match mgr.get(&tab_id) {
                Ok(repo) => crate::git::remote::fetch(repo.inner(), "origin"),
                Err(e)   => {
                    return boolerr2(lua_ctx, false,
                        Some(format!("tab not in manager: {e}")));
                }
            }
        };
        match result {
            Ok(_) => {
                let _ = h.emit("arbor://graph-refresh",
                    serde_json::json!({ "tab_id": tab_id }));
                boolerr2(lua_ctx, true, None)
            }
            Err(e) => {
                tracing::warn!("auto-fetch failed: {e}");
                boolerr2(lua_ctx, false, Some(format!("fetch failed: {e}")))
            }
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("fetch_active_tab", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_release_handles(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, ()| {
        let Some(ref h) = handle else { return Ok(()); };
        let state = h.state::<crate::AppState>();
        if let Ok(mut mgr) = state.repos.lock() {
            mgr.evict_all();
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("release_handles", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branches(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.branches: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, lua_ctx.create_table()?); };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.branches open: {e}")),
        };
        let head_name = repo.head().ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()));
        let branches = match repo.branches(None) {
            Ok(b)  => b,
            Err(e) => return err2(lua_ctx, format!("repo.branches list: {e}")),
        };
        let out = lua_ctx.create_table()?;
        let mut idx = 1;
        for b in branches.flatten() {
            let (branch, btype) = b;
            if let Ok(Some(name)) = branch.name() {
                let entry = lua_ctx.create_table()?;
                let is_remote = matches!(btype, git2::BranchType::Remote);
                let is_head   = head_name.as_deref() == Some(name);
                entry.set("name",      name)?;
                entry.set("is_remote", is_remote)?;
                entry.set("is_head",   is_head)?;
                out.set(idx, entry)?;
                idx += 1;
            }
        }
        ok2(lua_ctx, out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("branches", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_tags(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.tags: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, lua_ctx.create_table()?); };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.tags open: {e}")),
        };
        let out = lua_ctx.create_table()?;
        let mut idx = 1;
        let names = match repo.tag_names(None) {
            Ok(n)  => n,
            Err(e) => return err2(lua_ctx, format!("repo.tags list: {e}")),
        };
        for maybe in names.iter() {
            let Some(name) = maybe else { continue };
            let entry = lua_ctx.create_table()?;
            entry.set("name", name)?;
            if let Ok(obj) = repo.revparse_single(&format!("refs/tags/{}", name)) {
                entry.set("target", obj.id().to_string())?;
            }
            out.set(idx, entry)?;
            idx += 1;
        }
        ok2(lua_ctx, out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("tags", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_commits(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    // commits(opts?) → (commit[], nil) | (nil, err)
    // opts: { from?, to?, limit?, include_merges? }
    //   from           — exclusive lower bound (commit/tag/branch); default = none
    //                    (walk from `to` back to root)
    //   to             — inclusive upper bound; default = "HEAD"
    //   limit          — max number of commits; default = 1000
    //   include_merges — default true; when false, skip commits with >1 parent
    // Each commit: { oid, short_oid, summary, message, author_name,
    //                author_email, author_time, parents }
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.commits: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, lua_ctx.create_table()?); };

        let (from, to, limit, include_merges) = match opts {
            Some(t) => (
                t.get::<Option<String>>("from").ok().flatten()
                    .and_then(|s| if s.trim().is_empty() { None } else { Some(s) }),
                t.get::<Option<String>>("to").ok().flatten()
                    .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
                    .unwrap_or_else(|| "HEAD".to_string()),
                t.get::<Option<i64>>("limit").ok().flatten()
                    .map(|n| n.max(0) as usize)
                    .unwrap_or(1000),
                t.get::<Option<bool>>("include_merges").ok().flatten().unwrap_or(true),
            ),
            None => (None, "HEAD".to_string(), 1000usize, true),
        };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.commits open: {e}")),
        };
        let to_oid = match repo.revparse_single(&to) {
            Ok(o)  => o.id(),
            Err(e) => return err2(lua_ctx, format!("repo.commits revparse '{to}': {e}")),
        };
        let from_oid: Option<git2::Oid> = if let Some(ref f) = from {
            match repo.revparse_single(f) {
                Ok(o)  => Some(o.id()),
                Err(e) => return err2(lua_ctx, format!("repo.commits revparse '{f}': {e}")),
            }
        } else { None };

        let mut walk = match repo.revwalk() {
            Ok(w)  => w,
            Err(e) => return err2(lua_ctx, format!("repo.commits revwalk: {e}")),
        };
        if let Err(e) = walk.set_sorting(git2::Sort::TIME) {
            return err2(lua_ctx, format!("repo.commits sort: {e}"));
        }
        if let Err(e) = walk.push(to_oid) {
            return err2(lua_ctx, format!("repo.commits push to: {e}"));
        }
        if let Some(fo) = from_oid {
            if let Err(e) = walk.hide(fo) {
                return err2(lua_ctx, format!("repo.commits hide from: {e}"));
            }
        }

        let out = lua_ctx.create_table()?;
        let mut idx = 1usize;
        for oid_res in walk {
            if idx > limit { break; }
            let oid = match oid_res {
                Ok(o)  => o,
                Err(_) => continue,
            };
            let commit = match repo.find_commit(oid) {
                Ok(c)  => c,
                Err(_) => continue,
            };
            if !include_merges && commit.parent_count() > 1 { continue; }

            let entry = lua_ctx.create_table()?;
            let oid_s = oid.to_string();
            let short = oid_s.chars().take(7).collect::<String>();
            let message = commit.message().unwrap_or("").to_string();
            let summary = commit.summary().unwrap_or("").to_string();
            let author  = commit.author();
            let author_name  = author.name().unwrap_or("").to_string();
            let author_email = author.email().unwrap_or("").to_string();
            let when = author.when();
            let author_time = when.seconds();
            let parents = lua_ctx.create_table()?;
            for (i, p) in commit.parent_ids().enumerate() {
                parents.set(i + 1, p.to_string())?;
            }
            entry.set("oid",          oid_s)?;
            entry.set("short_oid",    short)?;
            entry.set("summary",      summary)?;
            entry.set("message",      message)?;
            entry.set("author_name",  author_name)?;
            entry.set("author_email", author_email)?;
            entry.set("author_time",  author_time)?;
            entry.set("parents",      parents)?;
            out.set(idx, entry)?;
            idx += 1;
        }
        ok2(lua_ctx, out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("commits", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_untracked(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    // untracked() → (string[], nil) | (nil, err)
    // Returns relative paths of files that are untracked AND not ignored.
    // Used by housekeeping plugins (e.g. gitignore-suggester) that want to
    // propose new ignore entries based on what's actually in the working
    // tree but not yet under git's control.
    let git_read = ctx.git_read;
    let fn_ = lua.create_function(move |lua_ctx, ()| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.untracked: requires git = \"read\" (or higher)".to_string()
            ));
        }
        let Some(path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, lua_ctx.create_table()?); };

        let repo = match git2::Repository::open(&path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("repo.untracked open: {e}")),
        };
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true);
        let statuses = match repo.statuses(Some(&mut opts)) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("repo.untracked statuses: {e}")),
        };
        let out = lua_ctx.create_table()?;
        let mut idx = 1usize;
        for entry in statuses.iter() {
            if !entry.status().is_wt_new() { continue; }
            if let Some(p) = entry.path() {
                out.set(idx, p)?;
                idx += 1;
            }
        }
        ok2(lua_ctx, out)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("untracked", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_clone(ctx: &ApiCtx, lua: &Lua, repo_table: &Table) -> Result<()> {
    // clone(cfg) → (job_id, nil) | (nil, err)
    // Clone a remote repository in the background. Validation issues
    // (missing url/dest, empty strings) raise — they're programming
    // errors. App-state failures (no handle, mutex poisoned) come back
    // as the (nil, err) tuple so the caller can fall through.
    let git_write = ctx.git_write;
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: Table| -> LuaTuple {
        if !git_write {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.clone: requires git = \"write\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "repo.clone: app handle unavailable");
        };

        let url: String = cfg.get("url").map_err(|_| mlua::Error::RuntimeError(
            "arbor.repo.clone: 'url' is required (string)".to_string()
        ))?;
        let dest: String = cfg.get("dest").map_err(|_| mlua::Error::RuntimeError(
            "arbor.repo.clone: 'dest' is required (string)".to_string()
        ))?;
        if url.trim().is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.clone: 'url' cannot be empty".to_string()
            ));
        }
        if dest.trim().is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.repo.clone: 'dest' cannot be empty".to_string()
            ));
        }

        let branch:  Option<String> = cfg.get("branch").ok()
            .filter(|s: &String| !s.is_empty());
        let shallow: bool = cfg.get("shallow").unwrap_or(false);
        let recurse: bool = cfg.get("recurse_submodules").unwrap_or(false);
        let name_override:     Option<String> = cfg.get("name").ok();
        let category_override: Option<String> = cfg.get("category").ok();

        let state  = h.state::<crate::AppState>();
        let job_id = match state.jobs.lock() {
            Ok(mut jobs) => jobs.new_id(),
            Err(e)       => return err2(lua_ctx, format!("repo.clone jobs lock: {e}")),
        };

        let on_done_action: Option<String> =
            if let Ok(func) = cfg.get::<mlua::Function>("on_done") {
                let synthetic = format!("__job_done_{}__", job_id);
                let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
                let list = lua_ctx.create_table()?;
                list.push(func)?;
                registry.set(synthetic.clone(), list)?;
                Some(synthetic)
            } else { None };

        let display_name = name_override.unwrap_or_else(|| format!("Clone: {}", url));
        let category     = category_override.or_else(|| Some("Clone".to_string()));
        let display_cmd  = {
            let mut parts: Vec<String> = vec!["git".into(), "clone".into(), "--progress".into()];
            if let Some(ref b) = branch { parts.push("--branch".into()); parts.push(b.clone()); }
            if shallow { parts.push("--depth".into()); parts.push("1".into()); }
            if recurse { parts.push("--recurse-submodules".into()); }
            parts.push("--".into()); parts.push(url.clone()); parts.push(dest.clone());
            parts.join(" ")
        };

        {
            let mut jobs = match state.jobs.lock() {
                Ok(j)  => j,
                Err(e) => return err2(lua_ctx, format!("repo.clone jobs lock: {e}")),
            };
            let info = crate::jobs::JobInfo {
                id:              job_id.clone(),
                name:            display_name.clone(),
                plugin_name:     pname.clone(),
                command:         display_cmd.clone(),
                started_at:      crate::jobs::JobRegistry::now_secs(),
                status:          crate::jobs::JobStatus::Running,
                category:        category.clone(),
                non_cancellable: false,
                is_system:       false,
                finished_at:     None,
                hidden:          false,
            };
            jobs.register(info);
        }

        let _ = h.emit("arbor://job-started", serde_json::json!({
            "job_id":      &job_id,
            "name":        &display_name,
            "plugin_name": &pname,
            "command":     &display_cmd,
            "category":    &category,
        }));

        crate::git::repo::spawn_clone_job(
            crate::git::repo::CloneJobRequest {
                job_id:             job_id.clone(),
                plugin_name:        pname.clone(),
                url,
                dest,
                branch,
                shallow,
                recurse_submodules: recurse,
                on_done_action,
            },
            h.clone(),
        );

        ok2(lua_ctx, job_id)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    repo_table.set("clone", fn_)
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
