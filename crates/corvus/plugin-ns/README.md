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

Every installer is byte-for-byte the shell's surface (names, arg shapes,
`(value, err)` tuples, error strings).

Most installers are **DIRECT** (the `NsHost` impl does the work in-process, e.g.
opening a repo by path). A few are **PROXY**: the state they touch lives only in
the shell, so the impl calls `CorvusState::host_call("__<name>", …)` and a
matching handler in `src-tauri/src/ipc/mod.rs` (`host_dispatch`) reads/mutates the
real shell state. `ToolchainInstaller` is the reference PROXY namespace.

The `corvus-be` binary implements `NsHost` (`CorvusNsHost`) over its `CorvusState`
+ the shared `corvus-git` logic, builds the installer `Vec`, and hands it to
`corvus_be_api_installer(...)`.

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

This is **additive** — the shell keeps its own `ns_shell/*` for the in-process
host. The double-load is removed in a later relocation step.

Public API is exposed through `corvus_plugin_ns::prelude`.
