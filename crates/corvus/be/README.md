# corvus-be

The headless **git backend process** for Model D — the executable the shell
spawns and talks to over IPC instead of running git in-process.

## Status: Stage 2 — git domains moving out-of-process

The process boundary is proven (Stage 1) and the **`bisect` and `stash` domains
now run here** (see
[`docs/corvus-be-bringup.md`](../../../docs/corvus-be-bringup.md)). It owns a
[`corvus_core::CorvusState`] and serves over **framed JSON on stdin/stdout**
(stderr is left for logs):

| Method | What |
|--------|------|
| `be_ping` / `be_echo` / `be_emit` | self-tests: request/response, arg decode, event push |
| `__repo_register` / `__repo_deregister` | shell pushes a tab's repo path on open/close |
| `__set_config` | shell pushes an app-config slice by section (the `recovery` snapshot policy, the global `gitflow` config, the `diff` context-line default, the `status` rename-detection toggle, the `git` executable-override + portable dir) so OOP handlers stop falling back to built-in defaults |
| `bisect_*` / `*_bisect_session` (11) | the bisect domain, via the shared `corvus-git` crate |
| `stash_save` / `stash_apply` / `stash_pop` / `stash_drop` / `stash_rename` / `force_stash_apply` / `abort_stash_apply` / `list_stashes` / `list_graph_stash_refs` / `get_stash_file_content` / `write_workdir_file` (11) | the stash domain, via `corvus-git` (opens the repo by the pushed path); fires `on_stash_push` / `on_stash_pop` to the co-located host |
| `reset_to_commit` / `create_tag` / `delete_tag` | the reset + tags domain, via `corvus-git`; fires `on_tag_create` / `on_tag_delete` to the co-located host |
| `search_commits` | repo-wide commit search (read-only, no hooks), via `corvus-git` |
| `list_local_branches` / `list_remote_branches` / `list_tags` / `get_nearest_tag` / `list_merged_branches` / `list_merged_remote_branches` / `delete_branches` / `delete_remote_branches` / `rename_remote_branch` / `checkout_commit` (10) | the **link-free** slice of the branch domain, via `corvus-git`. `delete_branches` fires `on_branch_delete`, `rename_remote_branch` fires `on_branch_rename`, `checkout_commit` fires `on_checkout` (inline, co-located host). The remote-push delete/rename bind their `push` closure to the `__git_credentials` resolver. `create_branch` (alias registry), `checkout_commit_safe`, and every worktree-link-aware checkout/delete/rename stay in-process (the `WorktreeLinkRegistry` + checkout-sync orchestrator live in the shell's `AppState`) |
| `merge_branch` / `abort_merge` / `complete_merge` / `resolve_conflict` / `resolve_stash_conflict` / `remove_conflict_file` / `get_conflict_content` / `get_conflict_presence` / `get_merge_message` (9) | the merge / conflict-resolution domain, via `corvus-git` (no hooks) |
| `start_rebase` / `rebase_continue` / `rebase_abort` / `rebase_skip` / `get_rebase_todo` / `get_rebase_state` (6) | the rebase domain, via `corvus-git`; fires `on_rebase_start` / `on_rebase_abort` to the co-located host |
| `gitflow_get_status` / `gitflow_init` / `gitflow_init_create_main` / `gitflow_feature_start` / `gitflow_feature_finish` / `gitflow_release_start` / `gitflow_release_finish` / `gitflow_hotfix_start` / `gitflow_hotfix_finish` (9) | the Git Flow operational domain, via `corvus-git`; fires the `on_flow_*` hooks. Effective config = the pushed global `gitflow` section overlaid by the repo's own `.arbor/config.toml` (read straight from the workdir) |
| `get_repo_config` / `set_repo_config` / `list_local_only_tags` / `mark_tag_local` / `mark_tag_pushed` / `get_branch_grouping` / `set_branch_grouping` / `get_repo_ide` / `set_repo_ide` / `get_gitflow_config` / `set_gitflow_repo_config` / `clear_gitflow_repo_config` / `has_gitflow_repo_override` (13) | the per-repo config domain — owns `<repo>/.arbor/config.toml` (the `RepoConfig` struct replicated verbatim, no hooks). Whole-file load-mutate-save like the shell; `get_gitflow_config` merges the pushed global `gitflow` overlaid by the repo override. The global gitflow CRUD (`get`/`set_gitflow_global_config`) stays shell-side (AppConfig); the `[ticket_links]` merge-write lives in the `tickets` domain |
| `list_worktrees` / `add_worktree` / `remove_worktree` / `detect_project_type` | the git-worktree domain (read + create/remove), via `corvus-git`. The IDE-launch / IDE-config / streaming-detection methods stay **in-process** (AppHandle / app config / job registry) |
| `list_recovery_entries` / `preview_recovery_restore` / `restore_recovery_entry` / `delete_recovery_entry` | the recovery-journal domain (read + restore), via `corvus-git` |
| `rb_list_accounts` / `rb_list_repos` / `rb_browse_tree` / `rb_get_file_content` / `rb_download_file` (5) | the remote repo-browser domain (async, network), via the shared `corvus-git-provider-{api,github,gitlab}` crates — host-keyed providers, credentials over the **reverse channel** (no hooks) |
| `resolve_avatar_for_email` (1) | commit-email → avatar (tab-keyed); the REST is the shared `GitProvider::avatar_url_for_email` + the cached `resolve_avatar` wrapper (no hooks) |
| `supports_security` / `fetch_security_summary` / `fetch_security_findings` / `export_security_report` (4) | the security-findings reads (tab-keyed via `provider_for_tab`); `fetch_security_summary` fires `on_security_summary_loaded`. `export_security_report` mints its job in the shell registry over the reverse channel (`JobHandle`), renders via the extracted `corvus-git-provider-api::security_export`, reads the plugin logo via `__branding_logo`, and emits `arbor://job-*` itself |
| `list_mrs` / `get_mr_detail` / `create_mr` / `get_mr_capabilities` / `probe_mr_feature` / `disable_mr_auto_merge` / `close_mr` / `reopen_mr` / `mark_mr_ready` / `add_mr_comment` / `get_mr_files` / `get_mr_commits` / `get_mr_commit_diff` / `get_merged_mr_hints` / `merge_mr` / `mr_start_conflict_resolution` (16) | the MR/PR domain; fires `on_mr_opened` (create), `on_mr_updated` (close/reopen/ready), `on_mr_merged` (merge). `merge_mr`'s GitHub branch-cleanup pushes `:refs/heads/<branch>` via the `__git_credentials` resolver; `mr_start_conflict_resolution` mints a job (`JobHandle`) + runs the extracted `corvus-git` merge-prep flow on a worker, streaming `arbor://mr-conflict-*` |
| `fetch_ci_runs` / `fetch_ci_jobs` / `list_ci_workflows` / `create_ci_pipeline` / `fetch_mr_ci_runs` / `retrigger_ci_run` / `get_ci_provider` (7) | the CI domain (no hooks). `get_ci_provider` detects from remotes via the pure `CiProviderInfo::detect_from_remotes` (`Ok(None)` when none match); its keyring-coupled `has_token` is filled over the reverse channel (`__has_token`) |
| `list_remotes` / `fetch_remote` / `push_branch` / `pull_branch` (4) | the network remote domain via `corvus-git`; git smart-HTTP credentials cross the reverse channel (`__git_credentials`); fires `on_fetch` / `on_push` / `on_pull`. `pull_branch` carries the full safe-pull flow (recovery snapshot → pre-pull stash → fetch/merge → re-apply) and streams `arbor://pull-progress` / `-done` |
| `list_commit_notes` / `check_note_remote_status` / `save_commit_note` / `delete_commit_note` / `push_note_namespace` (5) | the git-notes domain via `corvus-git`; fires `on_note_saved` / `on_note_deleted`. `push_note_namespace` pushes `refs/notes/*` over the shared `__git_credentials` resolver |
| `linear_*` (8) / `jira_*` (8) | the issue-tracker domain (async, network), via the shared `corvus-issues` crate — credentials resolved over the **reverse channel** (`ChildSessionProvider` → shell keyring), never read here |
| `get_commit_diff` / `get_commit_diff_meta` / `get_commit_file_diff` / `get_commits_range_diff_meta` / `get_commits_range_file_diff` / `get_workdir_diff` / `get_file_at_commit` / `get_branch_diff` / `get_file_blame` / `get_file_blame_streaming` / `get_workdir_diff_stream` (11) | the diff + blame domain via `corvus-git` (no hooks); `context_lines` falls back to the pushed `diff.context_lines`. `get_file_blame_streaming` is pure egress (blocks, returns the lines, emits `arbor://blame-stream-chunk` ticks); `get_workdir_diff_stream` returns a `job_id`, parses each file on a worker thread, and streams `arbor://diff-stream-*` — its Jobs entry driven over the reverse channel (`JobHandle`), no `arbor://job-*` lifecycle events (matching in-process) |
| `get_repo_files` / `get_files_last_commit` / `get_repo_fingerprint` / `get_graph` / `get_graph_for_file` / `get_repo_file_tree` / `get_commit_detail` / `start_file_meta_scan` / `export_graph_svg` (9) | the graph + repo-file domain via `corvus-git` (no hooks); `get_graph` is paginated (`offset`/`limit`), so single-shot reads cross as one `Response`. `start_file_meta_scan` streams `arbor://file-meta-batch` / `-done` (per-tab cancellation map is module-local); `export_graph_svg` returns a `job_id`, drives the Jobs entry via `JobHandle`, and emits `arbor://job-started` / `-output` / `-done` + `plugin:notification` |
| `get_status` (1) | the workdir-status scan via `corvus-git` (no hooks); reads the pushed `status.detect_renames` toggle |
| `commit` / `stage_file` / `unstage_file` / `stage_all` / `unstage_all` / `discard_file` / `discard_all` / `stage_patch` / `cherry_pick` / `revert_commit` / `get_git_commit_template` (11) | the stage/index + commit domain (direct libgit2). `commit` fires the **vetoable** `on_pre_commit` (non-empty plugin return aborts with `"Commit blocked by plugin:\n{reason}"`) before mutating + `on_commit` after the repo handle drops — inline at the co-located host. `discard_*` snapshot via the pushed recovery `SnapshotPolicy` (`RecoveryKind::Discard`). `get_git_commit_template` expands `~` via `dirs` |
| `list_submodules` / `submodule_fetch` / `submodule_pull` / `submodule_push` / `submodule_checkout` / `submodule_list_branches` / `update_submodule` / `update_all_submodules` (8) | the submodule domain via `corvus-git` (no hooks); the network ops bind an `AuthArgsResolver` to the shared `__git_credentials` resolver + `http_auth_args_for_credentials` |
| `get_reflog` (1) | the reflog read via `corvus-git` (no hooks) |
| `compute_repo_stats` / `export_repo_stats` (2) | the repo-statistics domain via `corvus-git` (no hooks). Both spawn background work over the event sink; `compute_repo_stats` emits `arbor://repo-stats-ready` / `-error`, memoised by the `CorvusState` `stats_cache` (HEAD+exclude-keyed, JSON) + `stats_computing` dedup guard. `export_repo_stats` mints a job via `JobHandle`, reuses cached stats, reads the branding logo via `__branding_logo`, and emits `arbor://job-*` + `plugin:notification` |
| `get_git_identity` / `get_repo_info` / `check_is_git_repo` / `clone_repo` / `list_remote_branches_for_url` (5) | the pure + network repo probes via `corvus-git` (no hooks). `clone_repo` / `list_remote_branches_for_url` resolve HTTPS auth over the reverse channel (`__git_credentials` + `http_auth_args_for_credentials`); `clone_repo` opens no tab (empty `tab_id`) |
| `init_repo` (1) | the repo-init lifecycle via `corvus-git` (git init + .gitignore/LICENSE/README + initial commit + optional push); fires `on_repo_init`. Provider remote-creation resolves via the reverse-channel provider registry (`crate::provider::for_host`); the GitLab namespace-id lookup + the initial-push HTTP-Basic credentials cross the reverse channel (`__gitlab_namespace_id`, `__git_credentials`). Registers the new tab in **both** registries — corvus-be's own (`register_repo`) and the shell's `RepoManager` (`__shell_open_repo`). `open_repo` / `close_repo` stay shell-side (AppState repo manager + orphan-GC + file dialogs) |
| `fs_git_status` / `fs_git_changes` / `fs_git_branches` / `fs_git_remote_url` / `fs_git_stage` / `fs_git_unstage` / `fs_git_discard` / `fs_git_ignore` / `fs_git_checkout` (9) | the File Explorer git-awareness domain (direct libgit2 on arbitrary paths — no `RepoManager`, no hooks). Status overlays are memoised in a module-static per-repo cache the mutating actions bust; `fs_git_discard` snapshots via the pushed recovery `SnapshotPolicy`. `fs_open_in_arbor` stays shell-side (needs an `AppHandle` to focus the main window) |
| `get_git_status` / `redetect_git` / `verify_git_path` / `set_git_path` / `cancel_git_download` / `download_portable_git` (6) | the git-CLI settings domain (system-git detection + PortableGit download), via the shared `corvus-git-cli` crate — corvus-be **self-detects** its own git (the shell no longer pushes the resolved program). `set_git_path` persists the `[git] executable_path` override shell-side via the `__persist_git_path` host method; `download_portable_git` streams `arbor://git-download-progress` |

The shell spawns this binary at startup, reads its `Hello` (the advertised method
list), and routes exactly those methods to it out-of-process via a `SplitBroker`;
everything else stays in-process. Handlers resolve a `tab_id` to a repo path
through the registry the shell pushes — no `RepoManager` here.

**Hooks fire here, co-located with the handlers** (plugin-relocation Wave 0).
`main` builds an mlua plugin host (via `corvus-plugin`) and hands its
`HookDispatcher` to `CorvusState`, so `stash_save`/`apply`/`pop` fire their
fire-and-forget hooks (`on_stash_push` / `on_stash_pop`) directly to plugins
running in this process — no longer dropped on the OOP path, no shell `post_hooks`
re-derivation. The headless host publishes the **host-pure** `arbor.*` surface
only for now; the git/product `ns_shell` namespaces (`arbor.repo`, …) arrive in
Wave 1, and plugin schedulers are not started here yet. The issue-tracker domain
fires no hooks.

**Config push (W0b).** The shell pushes the app-config slices the OOP handlers
need via `__set_config` (on repo open and on the matching settings change), and
`CorvusState` holds them keyed by section. The snapshotting domains (stash, reset,
recovery) read the **user-tuned** recovery `SnapshotPolicy` through
`crate::repo::snapshot_policy` — the built-in `::default()` is now only the
fallback when no policy has been pushed, so OOP retention/size/extension limits
match in-process. The same push carries `diff` (read by `crate::repo::diff_context_lines`,
the per-call `context_lines` fallback) and `status` (read by
`crate::repo::status_detect_renames`).

**Git-provider REST over the reverse channel.** The provider REST domains
(repo-browser, security reads, MR/PR, CI) resolve credentials the same way
`issues` does, one tier up: `main` seeds a `GitProviderRegistry`
(`crate::provider`) with the keyring-free GitHub/GitLab providers, each injected a
`ChildSessionProvider`, so `session`/`refresh` — including the on-401
refresh-and-retry baked into each provider's HTTP layer — marshal back to the
shell's `VaultSessionProvider`. The shell's proactive `maybe_refresh_for_provider`
pre-call is preserved as a `__maybe_refresh` host method (best-effort, swallowed
on failure), so the OOP path behaves identically.

**Git smart-HTTP credentials over the reverse channel.** The network `remote`
ops (fetch / push / pull) resolve their HTTP-Basic `(user, pass)` — a *different*
credential surface from the REST `AuthSession` — through the `__git_credentials`
host method (the keyring stays shell-side; `credential_store::resolve_credentials`
runs there). The blocking libgit2 work runs on a `spawn_blocking` worker and its
credential callback calls back to the shell from there — the reentrancy the
reverse channel is built for. The proactive `maybe_refresh_for_url` pre-call is
preserved as `__maybe_refresh_url`. `pull_branch` reuses the already-OOP stash +
recovery domains and the shell-pushed `SnapshotPolicy`, and streams its phase
progress through the `CorvusState` event sink.

Providers are resolved two ways, both in `crate::provider`: by **host string**
(`"github"`/`"gitlab"`, e.g. the browser) via `for_host`, or **tab-keyed** via
`provider_for_tab` — the OOP twin of the shell's resolver: it opens the repo by
the pushed path, lists remotes, detects the provider with the **pure**
`CiProviderInfo::detect_from_remotes` (now shared in `corvus-git-provider-api`),
and looks it up, auto-registering a self-hosted GitLab instance on demand (the
registry is a `Mutex` for that). Hooks (`on_security_summary_loaded`,
`on_mr_opened`, `on_mr_updated`) fire inline to the co-located host with
byte-identical payloads.

**Async handlers need a runtime.** The issue-tracker handlers do real network
I/O, so `main` builds a **multi-thread** Tokio runtime and the dispatch loop
`block_on`s them (the serve loop runs each request on its own worker thread, so
concurrent `block_on`s are expected). `jira_get_auth_status` stays in the shell
(it reads the keyring config directly for the domain/auth-method), as do the two
pure/metadata helpers `list_issue_providers` / `branch_name_for_issue` — the
`SplitBroker` routes per-method, so the domain splitting across the two processes
is invisible to the caller.

If this binary isn't built, the shell falls back to a pure in-process loopback
(it keeps in-process copies of these domains too) — the app still works.

## How to exercise it

Build the workspace so the binary exists next to the shell
(`target/debug/corvus-be`), run the app, then from the FE devtools console:

```js
await __TAURI__.core.invoke('rpc', { program: 'corvus', method: 'be_ping', params: {} })  // "pong"
await __TAURI__.core.invoke('rpc', { program: 'corvus', method: 'be_emit', params: { note: 'hi' } })
// listen for 'arbor://corvus-be-pong'
```

## Next

The **non-credential, non-stateful** local-git domains run here (bisect, stash,
reset+tags, search, merge, rebase, worktree, recovery), reading their user-tuned
config through the `__set_config` push (the recovery `SnapshotPolicy` + the
global `gitflow` config are live). The **`gitflow`** operational domain now runs
here too — config gated no more. The **job-registry proxy (ADR-3)** has landed:
the shell's `JobRegistry` is the single source, and OOP handlers drive it over
the reverse channel (`__job_register` → id, `__job_append`, `__job_set_status`,
`__job_is_cancelled`) via `crate::jobs::JobHandle`, emitting the `arbor://job-*`
events themselves through the sink. `export_security_report` and
`mr_start_conflict_resolution` migrated on it. What's left, by gate:

- **M3 credential broker:** the credential-coupled domains resolve over the
  reverse channel. The trait-based REST cohort is **done** — repo-browser,
  security (incl. export), MR/PR (incl. merge + conflict-resolution), CI all run
  OOP via the provider registry seam. What's left here:
    - **`image` proxy:** stays in-process **by design** — it is host-dynamic (the
      target host is an arbitrary URL in an MR body, possibly a self-hosted
      instance or a public CDN), so its per-URL token decision doesn't fit the
      provider registry. (`avatar` *did* move: its REST became the
      `GitProvider::avatar_url_for_email` trait method.) The whole REST cohort is
      now OOP, including `get_ci_provider` (pure detection + a `__has_token`
      reverse-channel probe for the keyring-coupled flag).
- **Per-registry state in `CorvusState`:** still pending — `branch` (the
  worktree-link sync registry + cross-repo checkout-sync orchestrator) and the
  ticket-link cache. The `stage`/`commit` domain is **done** (the `on_pre_commit`
  veto runs via `CorvusState::fire_pre_commit_veto`), and the stats memoisation
  (`stats_cache` + `stats_computing`) now lives in `CorvusState` as JSON (git2-free).
  `brp` stays shell-side (its watch/SSE + Lua-hook firing is bound to the shell's
  AppHandle/plugin host — revisit when AppHandle retirement starts).

The **`diff` / `graph` / `status`** read domains are now OOP. No new transport
primitive was needed: the framed protocol already carries arbitrary-size single
`Response`s (4-byte LE length prefix), and the streaming handlers ride the
transport-agnostic `EventSink` (each emit is an `Event` frame the shell
re-emits) — so the "payload/perf" gate was a non-issue. `diff`'s
`get_workdir_diff_stream` and `graph`'s `export_graph_svg` reuse the `JobHandle`
job-proxy; `get_file_blame_streaming` and `start_file_meta_scan` are pure egress.

Everything here auto-advertises via `Hello` and auto-routes OOP per-method — no
shell router change; the in-process copy stays as the no-spawn fallback.

## Depends on

`arbor-ipc` (the framed transport + `EventSink` + the reverse-channel
`HostCaller` / `ChildSessionProvider`), `arbor-rpc` (the handler registry),
`corvus-core` (`CorvusState`), `corvus-git` (local-git domains), `corvus-issues`
(the injected issue-tracker registry), `arbor-plugin-core` (`PluginHost`) +
`corvus-plugin` (the shared host wiring: dispatcher builder, headless installer,
`AppCtx`), `git2`, `serde_json`, and `tokio` (the runtime for the async issue
handlers + the host's `AppCtx::spawn`). No Tauri.
