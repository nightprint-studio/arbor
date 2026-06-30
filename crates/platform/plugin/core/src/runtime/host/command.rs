//! Command invocation (`arbor.command.fire` / declarative `kind = "command"`).
//!
//! Resolution + capability gating live here, in ONE place, so the two entry
//! points — the `fire_command` Tauri command (frontend node dispatch) and the
//! `arbor.command.fire` Lua function (runtime invocation) — share identical
//! rules. A command may be invoked only when:
//!   1. the caller holds the `command_invoke` permission, AND
//!   2. the caller holds whatever permission tier the target command declares
//!      as `required` (derived from the existing tiers — see `RequiredPerm`).
//!
//! Two kinds of command are invocable:
//!   * **Plugin-contributed** — a command another plugin registered via
//!     `arbor.command.register{ invocable = true }`, addressed as
//!     `<owner>::<id>`. Resolved + dispatched entirely in this crate
//!     (`fire_on` on the owner).
//!   * **Host built-in** — `arbor:area.verb` (e.g. `arbor:git.commit`). The
//!     allowlist of invocable built-ins + the permission tier each requires
//!     lives here, in [`host_command_required`], so a single resolver gates
//!     BOTH kinds. The handler itself lives in the Tauri shell (it needs
//!     `AppState`); this crate hands a gated invocation off through
//!     [`AppCtx::invoke_host_command`](arbor_core::prelude::AppCtx::invoke_host_command).
//!
//! Keep [`host_command_required`] (the perm-tier table here) and the shell-side
//! dispatch match (`src-tauri/src/plugin_host_commands.rs`) in lockstep — a new
//! built-in needs an entry in both.

use super::PluginHost;
use crate::contribution::{points, payloads::CommandPayload};
use arbor_plugin_types::prelude::{GitLevel, RequiredPerm};

/// Error raised when a command invocation is rejected or fails to resolve.
#[derive(Debug)]
pub enum CommandError {
    /// Caller plugin isn't loaded (shouldn't happen for a live UI, but the
    /// frontend could race a reload).
    CallerUnknown(String),
    /// Caller lacks the `command_invoke` permission.
    NotPermitted(String),
    /// No invocable command matches the id (missing, or `invocable = false`).
    NotFound(String),
    /// Caller doesn't hold the permission tier the command requires.
    PermissionDenied(String),
    /// Id targets a known host built-in but the host runtime isn't ready to
    /// dispatch it (no `AppCtx` installed — only happens in the brief boot
    /// window or in headless/test hosts with no built-ins).
    HostUnavailable(String),
    /// Malformed id (neither `<owner>::<id>` nor `arbor:…`).
    BadId(String),
}

impl CommandError {
    pub fn kind(&self) -> &'static str {
        match self {
            CommandError::CallerUnknown(_)    => "caller_unknown",
            CommandError::NotPermitted(_)     => "not_permitted",
            CommandError::NotFound(_)         => "not_found",
            CommandError::PermissionDenied(_) => "permission_denied",
            CommandError::HostUnavailable(_)  => "host_unavailable",
            CommandError::BadId(_)            => "bad_id",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            CommandError::CallerUnknown(m)    => m,
            CommandError::NotPermitted(m)     => m,
            CommandError::NotFound(m)         => m,
            CommandError::PermissionDenied(m) => m,
            CommandError::HostUnavailable(m)  => m,
            CommandError::BadId(m)            => m,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl PluginHost {
    /// Resolve `id`, enforce both capability gates against `caller`, and
    /// dispatch the command's handler. `ctx` is delivered to the owner's
    /// `command:<id>` handler as-is (the frontend merges form values + the
    /// declared `args`; Lua passes its ctx table verbatim).
    pub fn invoke_command(
        &self,
        caller: &str,
        id: &str,
        ctx: &serde_json::Value,
    ) -> std::result::Result<(), CommandError> {
        // ── Gate 1: caller opted into command invocation ─────────────────────
        let caller_plugin = self.plugins.iter()
            .find(|p| p.manifest.name == caller)
            .ok_or_else(|| CommandError::CallerUnknown(format!(
                "calling plugin '{caller}' is not loaded"
            )))?;
        if !caller_plugin.manifest.permissions.command_invoke {
            return Err(CommandError::NotPermitted(format!(
                "plugin '{caller}' lacks the 'command_invoke' permission"
            )));
        }

        // ── Host built-in (`arbor:area.verb`) ─────────────────────────────────
        // Resolved against the static allowlist; gated here, dispatched in the
        // shell through `AppCtx::invoke_host_command`.
        if let Some(required) = host_command_required(id) {
            if !caller_plugin.manifest.permissions.satisfies(&required) {
                return Err(CommandError::PermissionDenied(format!(
                    "plugin '{caller}' lacks the permission required by host command '{id}'"
                )));
            }
            let app_ctx = self.app_ctx().ok_or_else(|| CommandError::HostUnavailable(format!(
                "host runtime not ready to dispatch '{id}'"
            )))?;
            let ctx_json = serde_json::to_string(ctx).unwrap_or_else(|_| "{}".to_string());
            // Non-blocking: the shell impl spawns the handler on the async
            // runtime, so the plugin-host lock the caller holds is released
            // before the handler (which may fire hooks) runs.
            app_ctx.invoke_host_command(id, &ctx_json);
            return Ok(());
        }
        if id.starts_with("arbor:") {
            return Err(CommandError::NotFound(format!(
                "no invocable host command '{id}'"
            )));
        }

        // ── Plugin-contributed (`<owner>::<id>`) ──────────────────────────────
        let (owner, cmd_id) = match id.split_once("::") {
            Some((o, c)) if !o.is_empty() && !c.is_empty() => (o, c),
            _ => return Err(CommandError::BadId(format!(
                "command id '{id}' must be '<plugin>::<id>' or a host 'arbor:area.verb'"
            ))),
        };

        let cmd = self.find_invocable_command(owner, cmd_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "no invocable command '{owner}::{cmd_id}'"
            ))
        })?;

        // ── Gate 2: caller holds the tier the command requires ────────────────
        if !caller_plugin.manifest.permissions.satisfies(&cmd.required) {
            return Err(CommandError::PermissionDenied(format!(
                "plugin '{caller}' lacks the permission required by '{owner}::{cmd_id}'"
            )));
        }

        // Dispatch on the owner. `fire_on` is a no-op when the owner is missing
        // or disabled, so a disabled owner silently drops the command — same
        // semantics as any other targeted hook.
        let ctx_json = serde_json::to_string(ctx).unwrap_or_else(|_| "{}".to_string());
        crate::hook_router::fire_on(self, owner, &format!("command:{cmd_id}"), &ctx_json);
        Ok(())
    }

    /// Find a command marked `invocable = true` by (`owner`, `cmd_id`) in the
    /// command-palette contribution registry. Returns its parsed payload, or
    /// `None` when no match exists or it isn't invocable.
    fn find_invocable_command(&self, owner: &str, cmd_id: &str) -> Option<CommandPayload> {
        self.contributions
            .list_for_point(points::COMMAND_PALETTE)
            .into_iter()
            .find(|c| c.plugin_name == owner && c.item_id == cmd_id)
            .and_then(|c| serde_json::from_value::<CommandPayload>(c.payload).ok())
            .filter(|p| p.invocable)
    }
}

/// Allowlist of host built-in commands a plugin may invoke, mapped to the
/// permission tier the *invoking* plugin must already hold. `None` here means
/// "not a recognised host command" (the caller then tries the `<owner>::<id>`
/// plugin path or fails). A command absent from this table is NOT invocable —
/// closed by default, so destructive / internal host commands simply aren't
/// listed.
///
/// The matching handlers live in `src-tauri/src/plugin_host_commands.rs`; keep
/// the two in lockstep. The `repo.*` / `app.*` entries are frontend intents
/// (no permission tier) the shell relays to the UI.
pub fn host_command_required(id: &str) -> Option<RequiredPerm> {
    Some(match id {
        // Git mutations — require git write. History-rewriting verbs are
        // intentionally NOT exposed here.
        "arbor:git.commit"
        | "arbor:git.push"
        | "arbor:git.fetch"
        | "arbor:git.pull"
        | "arbor:git.branch_create"
        | "arbor:git.checkout"
        | "arbor:git.branch_delete"
        | "arbor:git.stage_all"
        | "arbor:git.unstage_all" => RequiredPerm::Git(GitLevel::Write),

        // Frontend UI intents — no permission tier (only `command_invoke`).
        "arbor:repo.refresh"
        | "arbor:app.open_settings" => RequiredPerm::None,

        _ => return None,
    })
}
