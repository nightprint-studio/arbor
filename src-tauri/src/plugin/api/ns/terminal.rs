//! `arbor.terminal.exec`.
//!
//! POLICY NOTE — git from plugins:
//!   Plugins must NOT shell out to `git` via this function (or any other
//!   route).  Arbor's own git invocations are centralised through
//!   `crate::git_cli::command()` which honours the user-configured
//!   executable path (Settings → Git → Git Executable, or PortableGit
//!   bundled by Arbor).  A plugin calling `arbor.terminal.exec("git ...")`
//!   would silently use the system PATH lookup instead and bypass the
//!   user's choice — leading to two different git binaries running in
//!   the same session and confusing failure modes.
//!
//!   Plugins should use the built-in Arbor APIs that wrap git:
//!     - arbor.repo.fetch_active_tab() / arbor.repo.clone(...)
//!     - the Tauri commands exposed via the rest of the API
//!   We deliberately do NOT auto-rewrite "git" → configured-path here,
//!   since that would silently change the meaning of plugin code.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};
use crate::plugin::runtime::TerminalLevel;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let terminal_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    let tp   = ctx.terminal_perm;
    let tc   = ctx.terminal_scope.clone();
    let pname = ctx.plugin_name.clone();

    // exec{command, cwd?} → (ExecResult, nil) | (nil, err)
    //
    // Permission denial / disallowed command in "commands" mode raises
    // (programming error). Process spawn failure comes back as the
    // (nil, err) tuple so the plugin can fall through. A non-zero exit
    // is data — `result.exit_code` carries it on success.
    let exec_fn = lua
        .create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            let command: String = cfg.get("command").map_err(|_|
                mlua::Error::RuntimeError("arbor.terminal.exec: 'command' is required".into()))?;
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
                    let basename = first.rsplit(|c| c == '/' || c == '\\')
                        .next().unwrap_or(first);
                    let basename = basename.strip_suffix(".exe").unwrap_or(basename);
                    if !tc.iter().any(|a| basename.eq_ignore_ascii_case(a.as_str())) {
                        return Err(mlua::Error::RuntimeError(format!(
                            "plugin '{pname}' is not allowed to run '{basename}' \
                             (allowed: {tc:?})"
                        )));
                    }
                }
            }

            match crate::terminal::TerminalManager::exec_command(&command, cwd.as_deref()) {
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
        .map_err(|e| AppError::Plugin(e.to_string()))?;
    terminal_table.set("exec", exec_fn).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("terminal", terminal_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
