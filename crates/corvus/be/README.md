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
| `bisect_*` / `*_bisect_session` (11) | the bisect domain, via the shared `corvus-git` crate |
| `stash_save` / `stash_apply` / `stash_pop` / `stash_drop` / `stash_rename` / `force_stash_apply` / `abort_stash_apply` / `list_stashes` / `list_graph_stash_refs` / `get_stash_file_content` / `write_workdir_file` (11) | the stash domain, via `corvus-git` (opens the repo by the pushed path) |
| `linear_*` (8) / `jira_*` (8) | the issue-tracker domain (async, network), via the shared `corvus-issues` crate — credentials resolved over the **reverse channel** (`ChildSessionProvider` → shell keyring), never read here |

The shell spawns this binary at startup, reads its `Hello` (the advertised method
list), and routes exactly those methods to it out-of-process via a `SplitBroker`;
everything else stays in-process. Handlers resolve a `tab_id` to a repo path
through the registry the shell pushes — no `RepoManager` here.

**Hooks stay shell-side.** `stash_save`/`apply`/`pop` owe fire-and-forget plugin
hooks (`on_stash_push` / `on_stash_pop`); this process fires none — the shell
fires them after the call returns, routing-independently
(`crate::ipc::corvus::post_hooks`). The issue-tracker domain fires no hooks at
all, so it moves OOP cleanly. **Recovery policy gap (known):** the
force-apply / abort recovery snapshots use `SnapshotPolicy::default()` because
this process has no app config yet — closing it is the first item of the settings
migration.

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

`reset` moves here next (Stage 2c): its git logic is already extracted into
`corvus-git` (`run_reset` + `create_tag` / `delete_tag`); serving it OOP needs a
`reset` module here (open the repo by path, take the hard-reset recovery snapshot
with this process's policy, validate the OID) plus moving `on_tag_create` /
`on_tag_delete` into `post_hooks` (as was done for the stash hooks). It will
auto-advertise via `Hello` and auto-route out-of-process — no shell router change.

## Depends on

`arbor-ipc` (the framed transport + `EventSink` + the reverse-channel
`HostCaller` / `ChildSessionProvider`), `arbor-rpc` (the handler registry),
`corvus-core` (`CorvusState`), `corvus-git` (local-git domains), `corvus-issues`
(the injected issue-tracker registry), `git2`, `serde_json`, and `tokio` (the
runtime for the async issue handlers). No Tauri.
