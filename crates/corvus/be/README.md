# corvus-be

The headless **git backend process** for Model D — the executable the shell
spawns and talks to over IPC instead of running git in-process.

## Status: Stage 2 — first git domain out-of-process

The process boundary is proven (Stage 1) and the **first real git domain,
`bisect`, now runs here** (see
[`docs/corvus-be-bringup.md`](../../../docs/corvus-be-bringup.md)). It owns a
[`corvus_core::CorvusState`] and serves over **framed JSON on stdin/stdout**
(stderr is left for logs):

| Method | What |
|--------|------|
| `be_ping` / `be_echo` / `be_emit` | self-tests: request/response, arg decode, event push |
| `__repo_register` / `__repo_deregister` | shell pushes a tab's repo path on open/close |
| `__set_git_program` | shell pushes the resolved git binary |
| `bisect_*` / `*_bisect_session` (11) | the bisect domain, via the shared `corvus-git` crate |

The shell spawns this binary at startup, reads its `Hello` (the advertised method
list), and routes exactly those methods to it out-of-process via a `SplitBroker`;
everything else stays in-process. The bisect handlers resolve a `tab_id` to a
repo path through the registry the shell pushes — no `RepoManager` here.

If this binary isn't built, the shell falls back to a pure in-process loopback
(it keeps an in-process bisect copy too) — the app still works.

## How to exercise it

Build the workspace so the binary exists next to the shell
(`target/debug/corvus-be`), run the app, then from the FE devtools console:

```js
await __TAURI__.core.invoke('rpc', { program: 'corvus', method: 'be_ping', params: {} })  // "pong"
await __TAURI__.core.invoke('rpc', { program: 'corvus', method: 'be_emit', params: { note: 'hi' } })
// listen for 'arbor://corvus-be-pong'
```

## Next

`stash` and `reset` move here next (Stage 2b/2c): same shape as bisect, but they
use `git2::Repository` and fire hooks, so `corvus-git`'s `GitError` gains a
`Git(git2::Error)` variant and the shell keeps firing the (fire-and-forget) hooks
after the call returns. They auto-advertise via `Hello` and auto-route
out-of-process — no shell router change.

## Depends on

`arbor-ipc` (the framed transport + `EventSink`), `arbor-rpc` (the handler
registry), `corvus-core` (`CorvusState`), `serde_json`. No Tauri.
