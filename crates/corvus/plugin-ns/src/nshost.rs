//! [`NsHost`] — the host-abstraction trait the ported `ns_shell` namespaces
//! call through.
//!
//! ## Why a trait
//!
//! The shell's `ns_shell/*` installers reach straight into `tauri::AppState`
//! (downcasting `ApiCtx::app_ctx`). That binds them to the Tauri shell. To run
//! the same `arbor.*` surface **inside** `corvus-be` (a binary that can't depend
//! on the shell), each namespace instead captures an `Arc<dyn NsHost>` and calls
//! coarse, JSON-shaped methods on it. The host (the `corvus-be` binary, or any
//! future product backend) implements `NsHost` over its own state + the shared
//! `corvus-git` logic, marshalling everything to/from `serde_json::Value` so this
//! crate stays light (mlua + plugin-core + serde only — no git2, no provider).
//!
//! ## Method shape conventions
//!
//! - **Coarse**: one method per Lua-visible namespace operation (not per git2
//!   call). The installer does the Lua <-> `Value` marshalling; the host does the
//!   repo work and returns plain data.
//! - **Repo identity**: methods that operate on "the active repo" take the repo
//!   **path** as `&str`. The installer reads it from the `__arbor_current_repo__`
//!   Lua global (the active repo path the hook router / sandbox seeds on every
//!   plugin) — the same value the shell's `arbor.repo.*` namespace reads. The host
//!   opens that path with git2 exactly as the shell does, so behaviour and error
//!   strings match byte-for-byte.
//! - **Errors**: `Result<_, String>`. The `String` is surfaced verbatim to Lua as
//!   the `(nil, err)` second return, so it MUST carry the same prefix/text the
//!   shell produced (e.g. `"notes.list: …"`).
//! - **JSON**: collection results come back as `serde_json::Value` (already the
//!   serde-serialized domain type, e.g. `Vec<CommitNote>`), which the installer
//!   hands to `lua.to_value(...)`. Scalars use plain Rust types.
//!
//! ## Extending this trait (phase 2+)
//!
//! `NsHost` is a normal trait designed to **grow**: each later namespace
//! (`repo`, `workspace`, `mr`, `ci`, `security`, `linked_worktrees`, …) appends
//! its own coarse methods here, and the single `CorvusNsHost` impl in `corvus-be`
//! grows the matching bodies. Keep additions grouped by namespace with a banner
//! comment, mirror the conventions above, and add nothing the namespace doesn't
//! actually need.

use std::sync::Arc;

/// Everything a ported `ns_shell` namespace needs from the host, abstracted so
/// the namespace installers don't depend on any concrete backend. `Send + Sync`
/// because the installers capture an `Arc<dyn NsHost>` into Lua closures that the
/// host may invoke from background threads.
///
/// One method group per ported namespace (`notes`, `repo`, `workspace`,
/// `linked_worktrees`, `mr`, `ci`, `security`). The single `CorvusNsHost` impl in
/// `corvus-be` carries the matching bodies.
pub trait NsHost: Send + Sync {
    // ── notes (`arbor.notes.*`) ──────────────────────────────────────────────

    /// `arbor.notes.list(commit_oid)` — every note attached to `commit_oid`
    /// across all `refs/notes/*` namespaces in the repo at `repo_path`.
    ///
    /// Returns the serde-serialized `Vec<CommitNote>` as a JSON array (the
    /// installer feeds it to `lua.to_value`). The error `String` is surfaced
    /// verbatim to Lua.
    fn notes_list(
        &self,
        repo_path: &str,
        commit_oid: &str,
    ) -> Result<serde_json::Value, String>;

    /// `arbor.notes.get(commit_oid, namespace)` — the message of the note in
    /// `refs/notes/<namespace>` for `commit_oid`, or `None` when there is no such
    /// note (the installer maps `None` to Lua `nil`).
    fn notes_get(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
    ) -> Result<Option<String>, String>;

    /// `arbor.notes.set{commit_oid, namespace, content}` — create/overwrite the
    /// note. On success the host fires `on_note_saved` with `plugin` set to
    /// `plugin_name` (the host owns hook firing, identically to the shell).
    fn notes_set(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
        content: &str,
        plugin_name: &str,
    ) -> Result<(), String>;

    /// `arbor.notes.delete(commit_oid, namespace)` — remove the note. On success
    /// the host fires `on_note_deleted` with `plugin` set to `plugin_name`.
    fn notes_delete(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
        plugin_name: &str,
    ) -> Result<(), String>;

    // ── repo (`arbor.repo.*`) ────────────────────────────────────────────────
    //
    // Every git-touching op opens `repo_path` (the `__arbor_current_repo__`
    // global) with git2 and runs the same logic the shell's `ns_shell/repo.rs`
    // ran. Errors carry the shell's exact `repo.<op> …: …` prefixes; the installer
    // applies only the `repo.<op> to_value:` prefix for the JSON-returning ops.

    /// `arbor.repo.branch()` — the active branch shorthand (`"HEAD"` when detached).
    fn repo_branch(&self, repo_path: &str) -> Result<String, String>;

    /// `arbor.repo.is_dirty()` — whether the workdir has any change (untracked
    /// included).
    fn repo_is_dirty(&self, repo_path: &str) -> Result<bool, String>;

    /// `arbor.repo.remote(name)` — the URL of remote `name`, or `None` when the
    /// remote (or its URL) is absent. `Err` only on repo-open failure.
    fn repo_remote(&self, repo_path: &str, name: &str) -> Result<Option<String>, String>;

    /// `arbor.repo.fetch_active_tab()` — fetch `origin` for the active repo and,
    /// on success, emit `arbor://graph-refresh`. Credential-resolving (over the
    /// reverse channel). The error is the bare `fetch failed: …` string the shell
    /// produced (the installer surfaces it verbatim).
    fn repo_fetch_active_tab(&self, repo_path: &str) -> Result<(), String>;

    /// `arbor.repo.release_handles()` — evict cached repo handles. corvus-be holds
    /// none (opens by path per call), so this is a no-op kept for Lua-surface
    /// fidelity.
    fn repo_release_handles(&self, repo_path: &str);

    /// `arbor.repo.branches()` — JSON array of `{ name, is_remote, is_head }`.
    fn repo_branches(&self, repo_path: &str) -> Result<serde_json::Value, String>;

    /// `arbor.repo.tags()` — JSON array of `{ name, target? }`.
    fn repo_tags(&self, repo_path: &str) -> Result<serde_json::Value, String>;

    /// `arbor.repo.commits{from?, to, limit, include_merges}` — JSON array of
    /// commit records (`{ oid, short_oid, summary, message, author_name,
    /// author_email, author_time, parents }`). `from` is an exclusive lower bound;
    /// `to` the inclusive upper (default resolved installer-side to `"HEAD"`).
    fn repo_commits(
        &self,
        repo_path: &str,
        from: Option<&str>,
        to: &str,
        limit: u64,
        include_merges: bool,
    ) -> Result<serde_json::Value, String>;

    /// `arbor.repo.untracked()` — JSON array of relative paths that are untracked
    /// and not ignored.
    fn repo_untracked(&self, repo_path: &str) -> Result<serde_json::Value, String>;

    /// `arbor.repo.staged_files()` — JSON array of `{ path, status }` for files
    /// whose INDEX differs from HEAD (`status` ∈ added/modified/deleted/renamed/
    /// typechange).
    fn repo_staged_files(&self, repo_path: &str) -> Result<serde_json::Value, String>;

    /// `arbor.repo.clone(cfg)` — register a background clone job (the shell mints
    /// the id over the reverse channel), emit `arbor://job-started`, spawn the
    /// credentialed clone, and return the `job_id`. `cfg` carries
    /// `url`/`dest`/`branch?`/`shallow`/`recurse_submodules`/`name?`/`category?`/
    /// `plugin_name`. The Lua `on_done` callback is **not** forwarded (the shell's
    /// callback registry is shell-process-local) — the one Lua-surface delta.
    fn repo_clone(&self, cfg: serde_json::Value) -> Result<String, String>;

    // ── workspace (`arbor.workspace.*`) ──────────────────────────────────────
    //
    // Reads the corvus-be `workspace::{store,registry}` (reload-on-access). The
    // read ops swallow lock/missing into an empty value installer-side; only
    // `switch` returns a real error and fires `on_workspace_switched` /
    // `arbor://workspace-switched`. Workspace tables carry `{ id, name, color_idx,
    // group_id, repo_ids, repo_count }`; repo-entry tables `{ id, path,
    // display_name, remote_url }` — hand-built so the Lua shape stays identical to
    // the shell's `ws_to_lua` / `entry_to_lua`.

    /// `arbor.workspace.list()` — all workspaces in display order, as a JSON array
    /// of workspace tables.
    fn workspace_list(&self) -> Result<serde_json::Value, String>;

    /// `arbor.workspace.active()` — the active workspace table, or `None`.
    fn workspace_active(&self) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.workspace.get(ws_id)` — the workspace table, or `None`.
    fn workspace_get(&self, ws_id: &str) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.workspace.list_repos(ws_id?)` — repo-entry tables. `Some` →
    /// that workspace's members (in `repo_ids` order, skipping unknown ids);
    /// `None` → the whole registry (in `RepoRegistry::list()` order).
    fn workspace_list_repos(&self, ws_id: Option<&str>) -> Result<serde_json::Value, String>;

    /// `arbor.workspace.repo(repo_id)` — the registry entry table, or `None`.
    fn workspace_repo(&self, repo_id: &str) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.workspace.switch(ws_id)` — mark `ws_id` active (persist), emit
    /// `arbor://workspace-switched` and fire `on_workspace_switched`. Error strings
    /// match the shell: `workspace '{ws_id}' not found` /
    /// `workspace '{ws_id}' vanished mid-switch`.
    fn workspace_switch(&self, ws_id: &str, plugin_name: &str) -> Result<(), String>;

    // ── linked_worktrees (`arbor.linked_worktrees.*`) ────────────────────────
    //
    // Reads/writes the shared `linked_worktrees.toml` via `be::worktree_links`
    // (reload-on-access). Not repo-scoped (a single global file).

    /// `arbor.linked_worktrees.list()` — JSON array of `{ id, name, sync_enabled,
    /// member_count }` projection rows.
    fn linked_worktrees_list(&self) -> Result<serde_json::Value, String>;

    /// `arbor.linked_worktrees.get(id)` — the full serde-serialized link record,
    /// or `None` when absent.
    fn linked_worktrees_get(&self, id: &str) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.linked_worktrees.set_sync_enabled(id, enabled)` — toggle the link's
    /// sync flag (persist) and emit `arbor://worktree-links-changed`. Returns the
    /// bare error; the installer adds the `set_sync_enabled: …` prefix.
    fn linked_worktrees_set_sync_enabled(&self, id: &str, enabled: bool) -> Result<(), String>;

    // ── mr (`arbor.mr.*`) ────────────────────────────────────────────────────
    //
    // Resolves the repo (explicit `repo_id` via the workspace registry, else the
    // active path), resolves the provider over the reverse channel, and blocks on
    // the corvus-be tokio runtime for the async REST calls — same as the shell's
    // `block_on_provider!`. Errors carry the shell's `arbor.mr.<op>[ resolve|
    // encode]: …` prefixes (note `current_user`'s bare `encode:`).

    /// `arbor.mr.list({repo_id?, state?, author?, labels?, query?})` — JSON array
    /// of MRs/PRs. `resolve_current_user` (the `author = "current_user"` sentinel)
    /// → resolve the login on the provider, or an empty array on auth failure.
    #[allow(clippy::too_many_arguments)]
    fn mr_list(
        &self,
        active_repo_path: Option<&str>,
        repo_id: Option<&str>,
        state_filter: &str,
        author: Option<&str>,
        resolve_current_user: bool,
        labels: Option<&[String]>,
        query: Option<&str>,
    ) -> Result<serde_json::Value, String>;

    /// `arbor.mr.current_user({repo_id?})` — the authenticated provider user as a
    /// JSON object.
    fn mr_current_user(
        &self,
        active_repo_path: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<serde_json::Value, String>;

    // ── ci (`arbor.ci.*`) ────────────────────────────────────────────────────

    /// `arbor.ci.runs({repo_id?, branch?, status?, mr_number?, per_page?})` — JSON
    /// array of the most recent CI runs (read-only). `per_page` defaults to 20.
    /// Errors carry the shell's `arbor.ci.runs[ resolve| encode]: …` prefixes.
    #[allow(clippy::too_many_arguments)]
    fn ci_runs(
        &self,
        repo_path: Option<&str>,
        repo_id: Option<&str>,
        branch: Option<&str>,
        status: Option<&str>,
        mr_number: Option<u64>,
        per_page: Option<u32>,
    ) -> Result<serde_json::Value, String>;

    // ── security (`arbor.security.*`) ────────────────────────────────────────
    //
    // Resolves the repo (`repo_id` via the registry, else `current_repo`) +
    // provider, then blocks on the runtime for the async provider calls. Errors
    // carry the shell's `arbor.security.<op>[ resolve| encode]: …` prefixes; the
    // installer applies only the `… to_value:` prefix.

    /// `arbor.security.supports({repo_id?})` — whether the provider exposes a
    /// security dashboard. `Ok(false)` (not an error) when no provider is
    /// registered for the remote.
    fn security_supports(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
    ) -> Result<bool, String>;

    /// `arbor.security.summary({repo_id?, range_days})` — the serde-serialized
    /// `SecuritySummary` as JSON (`range_days` already clamped installer-side).
    fn security_summary(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
        range_days: u32,
    ) -> Result<serde_json::Value, String>;

    /// `arbor.security.findings({…})` — serde-serialized `Vec<SecurityFinding>`.
    /// The raw lowercase token arrays are parsed host-side (unknown tokens
    /// dropped; empty `states` → `[Detected, Confirmed]`).
    #[allow(clippy::too_many_arguments)]
    fn security_findings(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
        severities: &[String],
        states: &[String],
        report_types: &[String],
        search: Option<&str>,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, String>;

    /// `arbor.security.refresh_active_tab({range_days})` — like `summary` for the
    /// active repo, plus emit `arbor://security-refresh { tab_id, summary }`. The
    /// host resolves `current_repo` to an open tab id for the payload, surfacing
    /// `arbor.security.refresh_active_tab: no active tab` when absent.
    fn security_refresh_active_tab(
        &self,
        current_repo: Option<&str>,
        range_days: u32,
    ) -> Result<serde_json::Value, String>;

    // ── toolchain (`arbor.toolchain.*`) ──────────────────────────────────────
    //
    // PROXY namespace: the toolchain registry lives in the SHELL's `AppState`
    // (`toolchain_registry`), not in `corvus-be`. So unlike the repo/notes
    // methods (which open a repo by path in-process), every method here is a
    // reverse-channel round-trip — the `CorvusNsHost` impl calls
    // `host_call("__toolchain_<op>", …)` and the matching shell handler in
    // `src-tauri/src/ipc/mod.rs` reads/mutates the real registry exactly as the
    // shell's `ns_shell/toolchain.rs` did. The registry is **not** repo-scoped (a
    // single global), so none of these take a `repo_path`. Errors carry the
    // shell's `toolchain.<op>[ lock| encode]: …` text; the installer applies no
    // extra prefix (it surfaces the host `String` verbatim).

    /// `arbor.toolchain.list(kind)` — every registered entry for `kind`, as the
    /// serde-serialized `Vec<ToolchainEntry>` JSON array.
    fn toolchain_list(&self, kind: &str) -> Result<serde_json::Value, String>;

    /// `arbor.toolchain.active(kind)` — the active entry for `kind` as a JSON
    /// object, or `None` when none is active (the installer maps `None` to Lua
    /// `nil`).
    fn toolchain_active(&self, kind: &str) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.toolchain.env{kind, id?}` — the env vars to inject for `kind`,
    /// resolved from entry `id` when given else the active entry, as the
    /// serde-serialized `HashMap<String, String>` JSON object.
    fn toolchain_env(&self, kind: &str, id: Option<&str>) -> Result<serde_json::Value, String>;

    /// `arbor.toolchain.detect(kind)` — newly discovered (not-yet-added) entries
    /// for `kind`, as the serde-serialized `Vec<ToolchainEntry>` JSON array.
    fn toolchain_detect(&self, kind: &str) -> Result<serde_json::Value, String>;

    /// `arbor.toolchain.add(kind, entry)` — add/replace the entry (matched by id)
    /// and persist. `entry` is the JSON the installer marshalled from the Lua
    /// table; the shell handler deserializes it into the typed `ToolchainEntry`.
    fn toolchain_add(&self, kind: &str, entry: serde_json::Value) -> Result<(), String>;

    /// `arbor.toolchain.remove(kind, id)` — drop the entry with `id` and persist.
    fn toolchain_remove(&self, kind: &str, id: &str) -> Result<(), String>;

    /// `arbor.toolchain.set_active(kind, id)` — mark exactly `id` active (all
    /// others inactive) and persist.
    fn toolchain_set_active(&self, kind: &str, id: &str) -> Result<(), String>;

    // ── tabs (`arbor.tabs.*`) ────────────────────────────────────────────────
    //
    // DIRECT namespace: corvus-be owns the work. `tabs_open_repo` resolves
    // `repo_id` against the workspace repo registry (reload-on-access) and emits
    // `arbor://open-repo-tab { repo_id, path, display_name, remote_url? }` — the
    // same payload the shell's `ns_shell/tabs.rs` emitted; the FE's AppShell
    // listens and runs the ensure-registered → activate/open flow. No reverse
    // channel. The error string is the shell's `repo '{repo_id}' not in registry`
    // verbatim (the shell's `registry lock: …` branch has no analogue here — the
    // corvus-be registry is reload-on-access, infallible — so it never fires).

    /// `arbor.tabs.open_repo(repo_id)` — bring the registered repo into focus as a
    /// tab. Resolves `repo_id` against the repo registry and emits
    /// `arbor://open-repo-tab` with `{ repo_id, path, display_name, remote_url? }`.
    /// `Err("repo '{repo_id}' not in registry")` for an unknown id.
    fn tabs_open_repo(&self, repo_id: &str) -> Result<(), String>;

    // ── issues (`arbor.issues.*`) ────────────────────────────────────────────
    //
    // DIRECT: corvus-be owns the reverse-channel-backed issue-tracker registry
    // (`crate::issues`), so these run in-process, blocking on the backend tokio
    // runtime. `search/get/transition/comment` are Linear-only; `lookup` routes
    // per-repo across trackers. Error text is mapped through `crate::issues::err`
    // so it is byte-identical to the shell's `to_app_error` string, then carries
    // the `issues.<op>:` prefix the shell's `ns_shell/issues.rs` applied.

    /// `arbor.issues.search(filters)` — Linear issue search. `filters` is the JSON
    /// the installer marshalled from the optional Lua table (`null`/malformed →
    /// `IssueFilters::default`). Returns the serde-serialized `Vec<Issue>`.
    fn issues_search(&self, filters: serde_json::Value) -> Result<serde_json::Value, String>;

    /// `arbor.issues.get(id)` — fetch a single Linear issue by id.
    fn issues_get(&self, id: &str) -> Result<serde_json::Value, String>;

    /// `arbor.issues.lookup(identifier)` — resolve an issue by its human
    /// identifier against the tracker configured for the active repo
    /// (`ticket_links.tracker` override wins over `issue_tracker`). Empty
    /// identifier / no tracker / no match → `Ok(None)` (Lua nil).
    fn issues_lookup(
        &self,
        repo_path: &str,
        identifier: &str,
    ) -> Result<Option<serde_json::Value>, String>;

    /// `arbor.issues.transition(id, status_id)` — move a Linear issue to a new
    /// workflow state.
    fn issues_transition(&self, id: &str, status_id: &str) -> Result<serde_json::Value, String>;

    /// `arbor.issues.comment(issue_id, body)` — post a comment on a Linear issue.
    fn issues_comment(&self, issue_id: &str, body: &str) -> Result<serde_json::Value, String>;

    /// `arbor.issues.branch_name(issue)` — pure compute: slugify an issue into a
    /// branch name. A malformed `issue` table is a programming error → the
    /// installer raises the returned `String` as a Lua `RuntimeError`.
    fn issues_branch_name(&self, issue: serde_json::Value) -> Result<String, String>;

    // ── terminal (`arbor.terminal.*`) ────────────────────────────────────────
    //
    // DIRECT: corvus-be runs the command itself with `std::process::Command` +
    // `NoWindowExt::no_window()` (no console popup). Permission gating runs
    // installer-side; this just spawns. A spawn failure maps to `exec failed: …`
    // (the same text the shell's `AppError::Other` carried before the installer's
    // `terminal.exec: …` prefix). A non-zero exit is data, not an error.

    /// `arbor.terminal.exec(command, cwd?)` — split `command` on whitespace (first
    /// token = program, rest = args), run with `cwd` as the working directory when
    /// given, and return `(exit_code, stdout, stderr)`.
    fn terminal_exec(
        &self,
        command: &str,
        cwd: Option<&str>,
    ) -> Result<(i32, String, String), String>;

    // ── job (`arbor.job.*`) ──────────────────────────────────────────────────
    //
    // PROXY namespace: the `JobRegistry` (and the OS process the job drives) lives
    // in the SHELL's `AppState` (`jobs`), so every op is a reverse-channel
    // round-trip (`host_call("__job_<op>", …)`). `job_new_id` reuses the
    // pre-existing `__job_register` handler to reserve an id; the other ops route
    // to the new `__job_spawn`/`__job_list`/`__job_cancel`/`__job_dismiss`/
    // `__job_clear_finished` handlers, which mirror `ns_shell/job.rs` byte-for-byte.

    /// Reserve a job id from the shell registry (`__job_register`), registering a
    /// Running `JobInfo`, so the synthetic on_done hook name and the
    /// `arbor://job-started` payload can carry the real id before the spawn.
    fn job_new_id(
        &self,
        name: &str,
        plugin_name: &str,
        command: &str,
        category: Option<&str>,
        hidden: bool,
        target: Option<&str>,
    ) -> Result<String, String>;

    /// `arbor.job.spawn(config)` — drive the real process spawn for an
    /// already-reserved job: the shell emits `arbor://job-started` and runs
    /// `crate::jobs::spawn_job`. `spec` carries the resolved job fields.
    fn job_spawn(&self, spec: serde_json::Value) -> Result<(), String>;

    /// `arbor.job.list()` — the serde-serialized job list as a JSON array.
    fn job_list(&self) -> Result<serde_json::Value, String>;

    /// `arbor.job.cancel(job_id)` — best-effort cancel (never fails on the Lua
    /// surface; the installer swallows any host-call error).
    fn job_cancel(&self, job_id: &str) -> Result<(), String>;

    /// `arbor.job.dismiss(job_id)` — drop a terminal-state job; `true` when
    /// removed, `false` for running/unknown (or a host-call error).
    fn job_dismiss(&self, job_id: &str) -> Result<bool, String>;

    /// `arbor.job.clear_finished()` — drop every terminal-state job; returns the
    /// ids dismissed.
    fn job_clear_finished(&self) -> Result<Vec<String>, String>;

    // ── ui branding (`arbor.ui.{set,clear}_branding` + theme tokens) ─────────
    //
    // PROXY namespace: the Tauri window-icon API + `AppState.branding` store +
    // `arbor://*` rebroadcast live in the SHELL. The pure validation runs
    // installer-side; the side-effecting half round-trips via
    // `host_call("__set_branding" | "__clear_branding" | "__set_theme_overlay" |
    // "__clear_theme_overlay", …)`. The only host-originated error is
    // `set_branding`'s `window_icon_path failed: …`, surfaced verbatim to Lua.

    /// `arbor.ui.set_branding{svg? | svg_path?, window_icon_path?}` — apply the OS
    /// window-icon (when `window_icon_path` is given) BEFORE writing
    /// `AppState.branding`, then emit `arbor://branding-changed`. `svg` is the
    /// already-resolved inline body; `plugin_name` is the override owner.
    fn ui_set_branding(
        &self,
        svg: Option<&str>,
        window_icon_path: Option<&str>,
        plugin_name: &str,
    ) -> Result<(), String>;

    /// `arbor.ui.clear_branding()` — clear the override only when `plugin_name`
    /// owns it; restore the bundled window icon when the cleared state carried one;
    /// emit `arbor://branding-changed`. Always `Ok(())`.
    fn ui_clear_branding(&self, plugin_name: &str) -> Result<(), String>;

    /// `arbor.ui.set_theme_tokens{vars}` — rebroadcast the theme-token overlay via
    /// `arbor://theme-overlay { plugin, vars }`. `vars` is the assembled
    /// `{ "--x": value, … }` JSON object. Frontend-only; always `Ok(())`.
    fn ui_set_theme_overlay(
        &self,
        plugin_name: &str,
        vars: serde_json::Value,
    ) -> Result<(), String>;

    /// `arbor.ui.clear_theme_tokens()` — emit `arbor://theme-overlay` with an empty
    /// `vars` object (the agreed "release my overlay" signal). Always `Ok(())`.
    fn ui_clear_theme_overlay(&self, plugin_name: &str) -> Result<(), String>;
}

/// Shorthand for the captured handle every namespace installer holds.
pub type NsHostHandle = Arc<dyn NsHost>;
