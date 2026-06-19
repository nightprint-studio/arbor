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
| `__set_git_program` | shell pushes the resolved git binary |
| `__set_config` | shell pushes an app-config slice by section (e.g. the `recovery` snapshot policy) so OOP handlers stop falling back to built-in defaults |
| `bisect_*` / `*_bisect_session` (11) | the bisect domain, via the shared `corvus-git` crate |
| `stash_save` / `stash_apply` / `stash_pop` / `stash_drop` / `stash_rename` / `force_stash_apply` / `abort_stash_apply` / `list_stashes` / `list_graph_stash_refs` / `get_stash_file_content` / `write_workdir_file` (11) | the stash domain, via `corvus-git` (opens the repo by the pushed path); fires `on_stash_push` / `on_stash_pop` to the co-located host |
| `reset_to_commit` / `create_tag` / `delete_tag` | the reset + tags domain, via `corvus-git`; fires `on_tag_create` / `on_tag_delete` to the co-located host |
| `search_commits` | repo-wide commit search (read-only, no hooks), via `corvus-git` |
| `merge_branch` / `abort_merge` / `complete_merge` / `resolve_conflict` / `resolve_stash_conflict` / `remove_conflict_file` / `get_conflict_content` / `get_conflict_presence` / `get_merge_message` (9) | the merge / conflict-resolution domain, via `corvus-git` (no hooks) |
| `start_rebase` / `rebase_continue` / `rebase_abort` / `rebase_skip` / `get_rebase_todo` / `get_rebase_state` (6) | the rebase domain, via `corvus-git`; fires `on_rebase_start` / `on_rebase_abort` to the co-located host |
| `list_worktrees` / `add_worktree` / `remove_worktree` / `detect_project_type` | the git-worktree domain (read + create/remove), via `corvus-git`. The IDE-launch / IDE-config / streaming-detection methods stay **in-process** (AppHandle / app config / job registry) |
| `list_recovery_entries` / `preview_recovery_restore` / `restore_recovery_entry` / `delete_recovery_entry` | the recovery-journal domain (read + restore), via `corvus-git` |
| `rb_list_accounts` / `rb_list_repos` / `rb_browse_tree` / `rb_get_file_content` / `rb_download_file` (5) | the remote repo-browser domain (async, network), via the shared `corvus-git-provider-{api,github,gitlab}` crates — host-keyed providers, credentials over the **reverse channel** (no hooks) |
| `linear_*` (8) / `jira_*` (8) | the issue-tracker domain (async, network), via the shared `corvus-issues` crate — credentials resolved over the **reverse channel** (`ChildSessionProvider` → shell keyring), never read here |

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
match in-process.

**Git-provider REST over the reverse channel.** The repo-browser (and the rest
of the REST cohort as it lands) resolves credentials the same way `issues` does,
one tier up: `main` seeds a `GitProviderRegistry` (`crate::provider`) with the
keyring-free GitHub/GitLab providers, each injected a `ChildSessionProvider`, so
`session`/`refresh` — including the on-401 refresh-and-retry baked into each
provider's HTTP layer — marshal back to the shell's `VaultSessionProvider`. The
shell's proactive `maybe_refresh_for_provider` pre-call is preserved as a
`__maybe_refresh` host method (best-effort, swallowed on failure), so the OOP
path behaves identically. Providers are resolved by **host string**
(`"github"`/`"gitlab"`) — no tab / `RepoManager` — which is why the browser leads
the cohort.

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

The **non-credential, non-stateful** local-git domains now run here (bisect,
stash, reset+tags, search, merge, rebase, worktree, recovery), reading their
user-tuned config through the `__set_config` push (W0b done — the recovery
`SnapshotPolicy` is live; `diff`-context / `gitflow` config ride the same
mechanism when those domains move). What's left, by gate:

- **M3 credential broker (in progress):** the credential-coupled domains resolve
  over the reverse channel. The REST cohort moves OOP by reusing the provider
  registry seam — **repo-browser done**; `avatar`, `security` (summary/findings),
  MR/PR, and CI are the same shape (the tab-keyed ones additionally resolve the
  provider from the pushed repo path). Still gated: the git-protocol surface —
  `remote` (fetch/push/pull) needs a `__git_credentials` host method (HTTP-Basic
  `(user,pass)`, not the REST `AuthSession`); plus `notes` push and `gitflow`
  finish. `security`'s `export_security_report` needs the job registry (proxied
  to the shell) before it moves.
- **Per-registry state in `CorvusState`:** `branch` (the worktree-link sync
  registry), `stage`/`commit` (the `on_pre_commit` veto already works via
  `CorvusState::fire_pre_commit_veto`, but the handlers need the repo lock shape),
  the ticket-link cache, the BRP registry.
- **Payload/perf judgement:** the large-read domains (`diff`, `graph`) may want a
  streaming transport before they cross the process boundary.

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
