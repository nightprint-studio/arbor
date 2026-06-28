# corvus-plugin-ns

The Corvus git **`ns_shell` namespaces** (`arbor.repo`, `arbor.notes`, …) ported
to run inside **any** host — the Tauri shell *or* the headless `corvus-be`
backend — instead of being pinned to `tauri::AppState`.

## The problem it solves

The shell's `src-tauri/src/plugin/ns_shell/*` installers reach into
`tauri::AppState` (downcasting `ApiCtx::app_ctx`). That binds the whole
`arbor.repo.* / arbor.notes.* / …` Lua surface to the shell process. Plugins and
hooks that run **inside** `corvus-be` (Wave 0 relocation) couldn't call them — a
plugin hitting `arbor.notes.list` in an OOP hook got a clear nil-field error.

This crate reimplements each namespace as a `LuaNamespaceInstaller` that holds an
`Arc<dyn NsHost>` and calls coarse, JSON-shaped methods on it — never touching a
concrete backend.

| Piece | What |
|---|---|
| `NsHost` (trait) | everything a namespace needs from the host, abstracted to `serde_json::Value` / scalars + `Result<_, String>`. One method-group per namespace. |
| `NotesInstaller` | `arbor.notes.*`: `list` / `get` / `set` / `delete`. |
| `RepoInstaller` | `arbor.repo.*`: `current` / `branch` / `is_dirty` / `remote` / `fetch_active_tab` / `release_handles` / `branches` / `tags` / `commits` / `untracked` / `staged_files` / `clone`. |
| `WorkspaceInstaller` | `arbor.workspace.*`: `list` / `active` / `get` / `list_repos` / `repo` / `switch`. |
| `LinkedWorktreesInstaller` | `arbor.linked_worktrees.*`: `list` / `get` / `set_sync_enabled`. |
| `MrInstaller` | `arbor.mr.*`: `list` / `current_user`. |
| `CiInstaller` | `arbor.ci.*`: `runs`. |
| `SecurityInstaller` | `arbor.security.*`: `supports` / `summary` / `findings` / `refresh_active_tab`. |
| `ToolchainInstaller` | `arbor.toolchain.*`: `list` / `active` / `env` / `detect` / `add` / `remove` / `set_active`. **PROXY** — the registry lives in the shell's `AppState`, so the `NsHost` impl round-trips each op over the reverse channel (`__toolchain_*`). |
| `TabsInstaller` | `arbor.tabs.*`: `open_repo`. **DIRECT** — emits `arbor://open-repo-tab`. |
| `IssuesInstaller` | `arbor.issues.*`: `search` / `get` / `lookup` / `transition` / `comment` / `branch_name`. **DIRECT** — corvus-be owns the issue-tracker registry. |
| `TerminalInstaller` | `arbor.terminal.*`: `exec`. **DIRECT** — runs the command in-process. |
| `JobInstaller` | `arbor.job.*`: `new_id` / `spawn` / `list` / `cancel` / `dismiss` / `clear_finished`. **PROXY** (`__job_*`) — the `JobRegistry` + OS process live in the shell. |
| `PipelineInstaller` | `arbor.pipeline.*`: `define` / `run` / `resume` / `discard` / `is_locked` / `list` / `get` / `cancel` / `list_runs` / `get_run` / `list_ops` (`register_op`/`unregister_op` are Lua-local). **PROXY** (`__pipeline_*`) — the `PipelineEngine`/`PipelineRuntime` live in the shell. ⚠️ `lua_op` callback-into-BE delivery degrades (same gap as `arbor.job.on_done`). |
| `CloudInstaller` | `arbor.cloud.*`: secrets / `test_connection[_async]` / `list[_stream]` / `search_stream` / `stat` / `delete` / `copy` / `download[_many]` / `upload` / `sync` / `concat_files` / `cancel` / `is_cancelled` / `report_progress` / `report_done` / `pick_chunk_order` / `oauth_start`. **PROXY** (`__cloud_*`) — the whole cloud stack lives in the shell. ⚠️ streamed/async-reply tails fire on the shell's plugin host, not corvus-be's. |
| `BrpInstaller` | `arbor.brp.*`: `connect` / `disconnect` / `status` / `call` / `watch` / `unwatch`. **PROXY** (`__brp_*`) — the `BrpRegistry` (HTTP client + SSE) lives in the shell. ⚠️ `watch` SSE events fire shell-side and never reach corvus-be VMs. |

Every installer is byte-for-byte the shell's surface (names, arg shapes,
`(value, err)` tuples, error strings).

Most installers are **DIRECT** (the `NsHost` impl does the work in-process, e.g.
opening a repo by path). A few are **PROXY**: the state they touch lives only in
the shell, so the impl calls `CorvusState::host_call("__<name>", …)` and a
matching handler in `src-tauri/src/ipc/mod.rs` (`host_dispatch`) reads/mutates the
real shell state. `ToolchainInstaller` is the reference PROXY namespace.

The `corvus-be` binary implements `NsHost` (`CorvusNsHost`) over its `CorvusState`
+ the shared `corvus-git` logic, then hands `installers(host)` (the ordered set
below, owned by this crate) to `corvus_be_api_installer(...)`. The order — and the
invariant that `UiBrandingInstaller` runs **after** the host-pure core namespaces
(it attaches onto the `arbor.ui` table `arbor-plugin-core`'s `ns::ui` publishes) —
is domain knowledge of these namespaces, so it lives in `installers()` rather than
in each host's `main`.

## Light by design

Depends only on `mlua` (same `lua54 + vendored + serialize + send` pin as
`arbor-plugin-core`, so Cargo unifies the single mlua artifact), `arbor-plugin-core`
(for `LuaNamespaceInstaller` + `ApiCtx` + the tuple helpers), and `serde` /
`serde_json`. It must **not** depend on `corvus-be` (a binary) or on
`git2`/provider crates — those live behind the `NsHost` impl in the binary.

## Fidelity

Each ported namespace preserves the Lua-visible contract exactly: same namespace
name, function names, argument shapes, `(value, err)` return tuples, and error
strings as the shell's `ns_shell`. The active repo is read from the
`__arbor_current_repo__` Lua global (the same value `arbor.repo.*` reads); the
host opens that path with git2, so behaviour and error text match.

`corvus-be` is now the **sole** loader of the Corvus product's plugins: with the
product-relocation flip the shell became the launcher and its `ns_shell/*` copies
were deleted, so these namespaces run only here (no more double-load).

Public API is exposed through `corvus_plugin_ns::prelude` — the per-namespace
installers plus `installers(host)`, the ordered builder.
