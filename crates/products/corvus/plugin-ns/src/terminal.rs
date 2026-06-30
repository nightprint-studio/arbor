//! `arbor.terminal` (`exec` — run a shell command, capture output), ported to
//! run through an [`NsHost`] instead of a `tauri::AppState` / `TerminalManager`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/terminal.rs`: same namespace (`arbor.terminal`), same function name
//! (`exec`), same table-config argument shape (`{command, cwd?}`), the same
//! `(ExecResult, nil) | (nil, err)` tuple convention with `result.exit_code`,
//! `result.stdout`, `result.stderr`, the same permission-gate `RuntimeError`
//! strings, and the same `terminal.exec: …` error prefix on a spawn failure.
//!
//! This is a **DIRECT** namespace: `corvus-be` can run the command itself, so —
//! exactly like the repo/notes git namespaces — the work goes through the
//! captured `Arc<dyn NsHost>` whose `corvus-be` impl spawns the process
//! in-process with `std::process::Command` + `NoWindowExt::no_window()` (no
//! console popup), returning `(exit_code, stdout, stderr)`. No reverse-channel
//! shell handler is involved.
//!
//! Permission gating is performed **installer-side** (matching the shell):
//!   · `terminal = none` → hard `RuntimeError` ("plugin '…' has no terminal
//!     permission").
//!   · `terminal = any`  → any command allowed.
//!   · `terminal = commands` → only commands whose basename (sans `.exe`,
//!     case-insensitive) appears in `terminal_scope` are allowed; otherwise a
//!     hard `RuntimeError` ("plugin '…' is not allowed to run '…' (allowed: …)").
//! A spawn failure is *not* a programming error — it comes back as the
//! `(nil, err)` tuple so the plugin can fall through. A non-zero exit is data
//! (`result.exit_code`), not an error.
//!
//! POLICY NOTE — git from plugins (unchanged from the shell): plugins must NOT
//! shell out to `git` via this function. Arbor centralises its git invocations
//! through the configured executable; a plugin calling
//! `arbor.terminal.exec("git ...")` would bypass that. Plugins should use the
//! built-in `arbor.repo.*` APIs instead. We deliberately do NOT auto-rewrite
//! `git` here.
//!
//! Calling convention (unchanged from the shell — see `ns_shell/terminal.rs`):
//!   · `exec{command, cwd?}` is a table-config. `command` is required (a missing
//!     `command` raises). `cwd` is optional; when omitted the host falls back to
//!     the active repo path (`__arbor_current_repo__`), so a plugin's command
//!     runs against the repo the user is looking at.

use mlua::{Lua, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};
use arbor_plugin_types::prelude::TerminalLevel;

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. Used
/// as the `cwd` fallback when the plugin does not pass an explicit `cwd` (the
/// shell ran with the process cwd; here we anchor to the active repo so the
/// command runs against the repo the user is looking at). `None` when no repo is
/// active.
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.terminal.*` installer. Holds the host handle the closures call through.
pub struct TerminalInstaller {
    host: NsHostHandle,
}

impl TerminalInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for TerminalInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let terminal_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_exec(self.host.clone(), ctx, lua, &terminal_table)?;

        arbor
            .set("terminal", terminal_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_exec(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    terminal_table: &Table,
) -> PluginCoreResult<()> {
    let tp = ctx.terminal_perm;
    let tc = ctx.terminal_scope.clone();
    let pname = ctx.plugin_name.clone();

    // exec{command, cwd?} → (ExecResult, nil) | (nil, err)
    //
    // Permission denial / disallowed command in "commands" mode raises
    // (programming error). Process spawn failure comes back as the
    // (nil, err) tuple so the plugin can fall through. A non-zero exit
    // is data — `result.exit_code` carries it on success.
    let fn_ = lua
        .create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            let command: String = cfg.get("command").map_err(|_| {
                mlua::Error::RuntimeError("arbor.terminal.exec: 'command' is required".into())
            })?;
            let cwd: Option<String> = cfg.get::<Option<String>>("cwd").unwrap_or(None);

            match tp {
                TerminalLevel::None => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "plugin '{pname}' has no terminal permission"
                    )));
                }
                TerminalLevel::Any => { /* allowed */ }
                TerminalLevel::Commands => {
                    let first = command.split_whitespace().next().unwrap_or("");
                    let basename = first
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(first);
                    let basename = basename.strip_suffix(".exe").unwrap_or(basename);
                    if !tc.iter().any(|a| basename.eq_ignore_ascii_case(a.as_str())) {
                        return Err(mlua::Error::RuntimeError(format!(
                            "plugin '{pname}' is not allowed to run '{basename}' \
                             (allowed: {tc:?})"
                        )));
                    }
                }
            }

            // Explicit `cwd` wins; otherwise fall back to the active repo path so
            // the command runs against the repo the user is looking at.
            let repo = current_repo(lua_ctx);
            let effective_cwd = cwd.as_deref().or(repo.as_deref());

            match host.terminal_exec(&command, effective_cwd) {
                Ok((exit_code, stdout, stderr)) => {
                    let result = lua_ctx.create_table()?;
                    result.set("exit_code", exit_code)?;
                    result.set("stdout", stdout)?;
                    result.set("stderr", stderr)?;
                    ok2(lua_ctx, result)
                }
                Err(e) => err2(lua_ctx, format!("terminal.exec: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    terminal_table
        .set("exec", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
