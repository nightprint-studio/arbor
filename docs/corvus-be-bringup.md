# Bringing up `corvus-be` as a real process

Goal: validate the **process boundary** early — spawn a real headless backend,
talk to it over IPC, prove request/response + event push + error wire-format —
*before* moving domains onto it, so the contract is locked and we don't discover
a wrong decision after mass-migrating handlers. Strategy agreed with the user:
**prove the seam with a trivial op (`ping`) first, then move the simple domains
(stash, reset, bisect) onto the already-proven boundary.**

This complements [`ipc-design.md`](ipc-design.md) (the target design) and
[`migration-roadmap.md`](migration-roadmap.md) M3 — it's the concrete bring-up.

## Transport (Stage 1 choice): framed JSON over the child's stdio

The production target is a named pipe (Windows) / unix socket (`0600` +
`SO_PEERCRED`) with a nonce/ACL handshake. For the **first real boundary** we use
the simplest thing that proves everything and needs **no new crate**: the shell
spawns `corvus-be` as a child process and frames messages over its
**stdin/stdout** (stderr stays free for logs).

- **Why stdio first**: zero dependencies (std `Command` + pipes), inherently
  private (only the parent holds the child's pipe handles → no port, no ACL, no
  nonce needed), trivial lifecycle (child dies → pipes close). It exercises the
  *entire* seam — spawn, handshake, req/resp, event push, error mapping.
- **Swap later, not rewrite**: the protocol (framing + `Frame` + the
  `ChildClient` that implements [`BrokerClient`]) is transport-agnostic. Moving to
  a named pipe / unix socket is swapping the byte stream under `ChildClient`, not
  touching the router, the handlers, or the frame protocol. `tarpc` / bincode can
  replace JSON framing the same way if/when a typed Rust↔Rust hop is wanted.

### Frame protocol

Length-prefixed (`u32` LE) JSON frames over one duplex stream:

| Frame | Direction | Meaning |
|-------|-----------|---------|
| `Hello { methods }` | BE → shell | first frame: the method names this backend serves |
| `Request { id, method, params }` | shell → BE | a call |
| `Response { id, result: Result<Value,String> }` | BE → shell | the matching reply; `Err` carries the wire string |
| `Event { topic, payload }` | BE → shell | a push event, re-emitted by the shell to the FE |

The shell's `ChildClient` (implements `BrokerClient`): a background reader thread
demuxes the child's stdout — `Response` frames wake the matching blocked caller
(id → channel), `Event` frames go to a callback that re-emits to the FE via
`AppHandle::emit`. `call` is synchronous (the generic `rpc` command already runs
it on the blocking pool), blocking on the response channel.

## Split routing (how migration stays incremental)

The shell registers **one** `"corvus"` backend, a `SplitBroker`:

```
SplitBroker.call(method):
    method ∈ corvus-be's advertised set  →  ChildClient (out of process)
    otherwise                            →  LoopbackBroker (in process)
```

`corvus-be` advertises its served methods in `Hello`. **Moving a handler into
`corvus-be` therefore auto-routes it out-of-process** — no per-method router
edits. If `corvus-be` isn't built/spawnable, the shell falls back to a pure
`LoopbackBroker` (everything in-process, today's behavior) — the app never breaks
on a missing backend.

## Stages

- **Stage 1 — the seam (this is the de-risking step)**: real `corvus-be` binary
  (`crates/corvus/be`) owning a `CorvusState`, serving a `ping` (+ `echo`, + an
  event-push proof) over framed stdio. Shell spawns it, builds the `SplitBroker`,
  forwards its events. **No git** — proves spawn/transport/handshake/req-resp/
  event/error end to end.
- **Stage 2 — move stash / reset / bisect**: extract their git deps into a crate
  `corvus-be` consumes, move the handlers there (they auto-route OOP via `Hello`),
  remove them from the shell.
  - **2a — bisect git extracted ✅**: `bisect` + `bisect_sessions` live in the
    Tauri-free crate **`corvus-git`** (path-based, no hooks — the cleanest cut).
    Git invocation is explicit via `GitCli` (no global state to sync across
    processes); errors are a local `GitError` the shell maps to `AppError`
    (wire-string-identical).
  - **2a — bisect served out-of-process ✅**: `corvus-be` now serves the 11 bisect
    methods against its own `CorvusState`. The shell keeps a **`tab_id → repo
    path`** registry inside `CorvusState`, pushed on repo open/close
    (`__repo_register` / `__repo_deregister`) along with the resolved git program
    (`__set_git_program`); a handler resolves the tab to a path with no
    `RepoManager`. The shell's in-process bisect copy stays as a **fallback** (used
    only when `corvus-be` isn't running — bisect never breaks on a missing
    backend); when `corvus-be` is up, `Hello` advertises the methods and the
    `SplitBroker` routes them to the process.
  - **2b — stash git extracted ✅**: `stash` (+ `encoding`) now live in `corvus-git`.
    `GitError` gained `Git(git2::Error)` / `StashNotFound`. Two couplings were
    decoupled cleanly: git invocation via the explicit `GitCli`, and the
    **recovery snapshot** via a `snapshot: &dyn Fn(&Repository, &str)` callback
    (so stash doesn't drag `recovery`/`config` into the crate). `encoding` is a
    re-export shim (`crate::git::encoding` → `corvus_git::encoding`) for its ~17
    in-shell consumers. **`crate::git::stash` is now a thin wrapper** keeping the
    original signatures (so its ~6 in-process consumers — checkout-safe, graph
    markers, pull-with-stash, linked-worktree sync, the IPC handlers — are
    untouched): it builds the `GitCli` and binds the shell's `recovery::try_snapshot`
    to the callback. Still **in-process**.
  - **2b — recovery git extracted ✅**: the snapshot/journal logic (`snapshot_with_policy`,
    `list_entries`, `preview_restore`, `restore`, `delete` + the `SnapshotPolicy` /
    `RecoveryKind` / `RecoveryEntry` types) now lives in `corvus_git::recovery`. The
    same two couplings were decoupled the same way: git invocation via the explicit
    `GitCli`, and the **snapshot policy / retention** passed in as a parameter (so the
    crate drags in neither the `git_cli` global nor the app config). **`crate::git::recovery`
    is now a thin wrapper** keeping the original signatures + the config-loading
    `snapshot` / `try_snapshot` convenience, so its ~9 in-process consumers
    (checkout, discard, pull, reset, linked-worktree sync, the recovery IPC handlers)
    are untouched. Still **in-process**.
  - **2b — stash served out-of-process ✅**: `corvus-be` now serves the 11 stash
    methods against its own `CorvusState` (a `stash` module: opens the repo by the
    shell-pushed path, `GitCli` from `git_program`, binds the force-apply / abort
    recovery snapshot to `corvus_git::recovery::snapshot_with_policy` with **its own
    `GitCli` + `SnapshotPolicy::default()`**). The in-process stash handlers stay as a
    **fallback** (used only when `corvus-be` isn't running). **Hook seam**: the
    `on_stash_push` / `on_stash_pop` firings were lifted *out* of the in-process
    handlers into a new shell-side `crate::ipc::corvus::post_hooks`, called from the
    generic `rpc` command **after a successful `dispatch_rpc`** — so they fire exactly
    once whether the method ran in-process (loopback) or out-of-process. The payload
    split is preserved (stash entry `index`/`message` from the result, `tab_id` /
    `include_untracked` / pop-index from the params; the `on_stash_pop` pair fire only
    when the apply/pop was conflict-free). **Known gap (settings):** the OOP recovery
    snapshot uses the *default* policy, not the user's configured `[recovery]` limits —
    closed by the settings migration (push the configured policy to `CorvusState`).
  - **2c — reset git extracted ✅ (in-process)**: `reset_to_commit`'s CLI shell-out
    (`corvus_git::reset::run_reset`) + `create_tag` / `delete_tag` (git2) now live in
    `corvus_git::reset`; `GitError` gained `CommitNotFound`. The shell handler keeps
    the OID validation, the `find_object` check, the Hard-mode `recovery::try_snapshot`
    (config-loading) and the `on_tag_create` / `on_tag_delete` firings — these stay
    shell-side. **Still in-process**; serving it OOP (a `reset` module in `corvus-be` +
    moving the tag hooks into `post_hooks`) is the next reset step.
  - **2-issues — Linear / Jira served out-of-process ✅**: the first
    **credential-coupled** domain to move, on the **reverse channel** rather than
    a pushed repo path. Shared glue lives in the new keyring-free crate
    **`corvus-issues`** (`build_registry(session_for)` → the `IssueTracker`
    registry + the concrete `JiraTracker`; the `NewIssue` builders). The shell
    injects a keyring-backed `VaultSessionProvider`; `corvus-be` injects a
    **`ChildSessionProvider`** that marshals `session`/`refresh` back to the shell
    (the sole keyring holder) over the reverse channel. `corvus-be` serves the
    **16 async network handlers** (Linear ×8, Jira ×8) on a **multi-thread Tokio
    runtime** (`block_on` in the dispatch loop — each request already runs on its
    own serve-loop worker thread). Wire error strings are byte-identical to the
    in-process path (`to_app_error(e).to_string()` replicated locally). **Stays
    in-process**: `jira_get_auth_status` (reads the keyring config directly for
    the domain/auth-method) and the two pure/metadata sync helpers
    (`list_issue_providers`, `branch_name_for_issue`) — the `SplitBroker` routes
    per-method, so the domain splits transparently. No hooks in this domain.
    **P0 fix (dispatch semantics):** the generic `rpc` command short-circuited
    *every* async method to the in-process `dispatch_async` *before* the router —
    which would have pinned these OOP handlers in-process. The command now skips
    that short-circuit when the method is advertised OOP (`is_async_method &&
    !is_oop_method`), letting it flow through the `SplitBroker` to `corvus-be`;
    when no backend is up the OOP set is empty, so async methods stay in-process
    (the correct fallback).
  - **Repos**: Stage-2 decision — the shell resolves `tab_id → repo path` and
    forwards the path so `corvus-be` opens by path (stateless per call, like the
    diff commands already do), deferring repo-lifecycle replication.
  - **Hooks**: stash/reset/bisect hooks (`on_stash_push`, `on_tag_create`, …) are
    all **fire-and-forget** → the shell fires them after the call returns (it owns
    the Lua plugin host). Vetoable hooks (`on_pre_commit`) are out of these
    domains and will need the hook-bridge round-trip when their domain migrates.
- **Stage 3+ — grow**: more domains; eventually flip the stdio transport to a
  named pipe / unix socket with the nonce/ACL handshake; then the bigger git
  surface.

## Security note

stdio is parent-private, so Stage 1 has no auth surface. The nonce + ACL
handshake from `ipc-design.md` lands when the transport moves to a named pipe /
unix socket (Stage 3+), where a third local process could otherwise connect.
