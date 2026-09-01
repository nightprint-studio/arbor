# Backend (BE) architecture

How the product backends work, and how to create a new one from scratch.

This is the durable reference that used to live only in scattered notes. If you are
onboarding on a fresh machine and there is no local memory, **read this first**, then
`docs/ipc-design.md`, `docs/corvus-be-bringup.md` and `docs/reverse-channel.md` for the
deep dives.

---

## 1. The big picture — "Model D: 1 FE + N BE"

Arbor is **one frontend + many headless backends**.

- The **shell** (`src-tauri`, binary `arbor`) owns the *only* WebView2 window(s). It is
  glue: window management, OS integration, a **router**, and the **credential broker**.
  It contains almost no product logic.
- Each **product** has a **BE**: a standalone headless binary — `corvus-be`, `sitta-be`,
  `merula-be`, `tyto-be`, `bennu-be` — with **no webview**. A BE serves a set of RPC
  methods over **length-prefixed JSON frames on its own stdin/stdout**.
- The transport is **abstracted**: the exact same `#[handler]` functions run either
  **in-process** inside the shell (a `LoopbackBroker`) or **out-of-process** as a spawned
  child (`ChildClient`). The shell's `SplitBroker` decides per-method which side answers.
  Today `corvus` is *hybrid* (has a loopback fallback); `merula`/`sitta`/`tyto`/`bennu`
  are *pure OOP*.

```
          ┌──────────────────────────── src-tauri (arbor) ────────────────────────────┐
  WebView │  invoke("rpc", {program, method, params})                                  │
  (Svelte)│        │                                                                   │
          │        ▼                                                                   │
          │   Router ──> SplitBroker(program)                                          │
          │                 │  method advertised by child?                             │
          │        ┌────────┴─────────┐                                                │
          │        ▼                  ▼                                                 │
          │  LoopbackBroker      ChildClient  ──frames on stdio──▶  corvus-be (headless)│
          │  (in-process)             ▲                                   │            │
          │        │   reverse channel│ (HostRequest/HostResponse)        ▼            │
          │        └── host_dispatch ◀┴───────────────────────  #[handler] fn(&State) │
          └───────────────────────────────────────────────────────────────────────────┘
```

Canonical docs: `docs/ipc-design.md`, `docs/corvus-be-bringup.md`, `docs/reverse-channel.md`.

> Terminology note: the docs sometimes use the old path `crates/corvus/be`. The real path
> today is `crates/products/corvus/be`. "grove" appears only as a *future* product name in
> `docs/profiles-and-product-config.md` — no crate exists yet.

---

## 2. Crate layout

Root `Cargo.toml` lists **explicit** workspace members (not a glob — several dirs are
un-activated stubs). Grouped:

**Foundation / platform** (`crates/foundation/*`, `crates/platform/*`) — product-agnostic:

| Crate | Path | Role |
|---|---|---|
| `arbor-core` | `foundation/core` | profiles, paths, `AppCtx`, error, http |
| `arbor-be` | `foundation/be` | **the BE runtime scaffold** — `App`, `BackendIo`, `Dispatcher` |
| `arbor-ipc` | `foundation/ipc` | the transport seam — `BrokerClient`, `LoopbackBroker`, `ChildClient`, `serve_stdio`, framing, `EventSink`, `HostCaller` |
| `arbor-rpc` (+ `-macros`) | `foundation/rpc` | the `#[handler]` macro + `inventory` registry |
| `arbor-shell-common` | `foundation/shell-common` | shell-side `Router` + credential `broker` |
| `arbor-fs`, `arbor-auth`, `arbor-process-ext`, `arbor-feedback`, `arbor-scheduler`, `arbor-plugin-*` | `foundation/`, `platform/` | shared services |

**Products** (`crates/products/<product>/…`). Each product has a `core` crate (holds the
`*State`) and a `be` crate (the binary):

- **corvus** (git GUI) — the reference. `core`, `be`, plus `git`, `git-cli`, `plugin`,
  `plugin-ns`, `issues`, `git-provider/*`, `issue-tracker/*`, `pipeline/*`, `provider-descriptor`.
- **sitta** (file explorer) — `core`, `be`.
- **merula** (music/DAW) — `core`, `be`, plus `merula-{pattern,lang,audio,engine,import,transcribe}`.
- **tyto** (screen recorder) — `core`, `be`.
- **bennu** (Java editor/analysis) — `core`, `be`, plus analysis crates.

**Shell**: `src-tauri` → binary `arbor`. Not a library, has no prelude.

---

## 3. Anatomy of a BE

A BE is a **binary crate** (`[[bin]] name = "<product>-be"`) whose `main` is declarative,
built on three scaffold pieces from `arbor-be`:

- **`BackendIo`** (`crates/foundation/be/src/io.rs`) — builds the four framed-stdio pieces
  in one call:
  - `stdout` — the single `SharedWriter` (the *protocol channel*; **all logs go to stderr**),
  - `sink` — `FrameEventSink`, the event egress (`state.emit(...)`),
  - `host` — `FrameHostCaller`, the **reverse channel** back to the shell,
  - `rt` — a **multi-thread** tokio runtime (mandatory: each request is dispatched on its
    own worker thread).

- **`App`** (`crates/foundation/be/src/app.rs`) — a fluent builder:
  - `.plugin_host(product_id, build_hooks)` wires the whole Lua plugin runtime,
  - `.api_installer(...)` publishes the `arbor.*` namespaces,
  - `.init(...)` registers pre-serve steps,
  - `.on_ready(...)` post-`Hello` hook (default: reload plugins on a background thread),
  - `.run(dispatcher)` starts the serve loop.

- **`Dispatcher<S>`** (`crates/foundation/be/src/dispatch.rs`) — assembles method→handler
  routing from the `#[handler]` **inventory** (`.inventory(program)`) plus optional "extra
  groups" that carry their own per-call context (`.group(...)`). `.methods()` produces the
  `Hello` name union; `.into_fn()` produces the closure the serve loop calls.

**Reference `main` (corvus)**: `crates/products/corvus/be/src/main.rs`. Sequence:

```
init_active_profile()                       // seed the active profile from the pointer file
→ App::new(BackendIo::new())
→ .plugin_host("corvus", build_hook_dispatcher)
→ build Arc<CorvusState>
→ .api_installer(corvus_be_api_installer(...))   // publish arbor.* namespaces
→ host_handle::install(...)                       // module-static plugin host handle
→ Dispatcher::new(state, rt).inventory("").group(plugin_rpc::methods(), ...)
→ .init(...)                                      // pre-serve steps
→ app.run(dispatcher)
```

The **minimal** BE is much smaller — see `crates/products/sitta/be/src/main.rs` and
`crates/products/tyto/be/README.md`.

**Where the public surface lives**: a BE binary has *no* prelude. Its **state** lives in
the `*-core` crate and is exported through that crate's prelude — e.g.
`corvus_core::prelude::CorvusState`. `main.rs` reaches everything via preludes.

---

## 4. The RPC seam — full request path

Transport = length-prefixed (u32-LE) JSON frames over the child's stdio
(`crates/foundation/ipc/src/transport.rs`). The `Frame` enum:
`Hello{methods}`, `Request{id,method,params}`, `Response{id,result}`, `Event{topic,payload}`,
plus the reverse channel `HostRequest`/`HostResponse`.

**FE → BE → FE** (out-of-process path):

1. FE calls **one generic** Tauri command: `invoke("rpc", { program, method, params })`
   (`src-tauri/src/commands/rpc_commands.rs`). There is **no per-command shell edit** —
   adding a handler needs zero shell changes.
2. `rpc` decides sync vs async and runs sync handlers on `spawn_blocking`, calling
   `crate::ipc::dispatch_rpc`.
3. `dispatch_rpc` (`src-tauri/src/ipc/mod.rs`) serializes params → `Router::call(program, method, bytes)`.
4. `Router` (`crates/foundation/shell-common/src/router.rs`) looks up the per-product
   `BrokerClient` — a `SplitBroker`.
5. `SplitBroker::call` (`src-tauri/src/ipc/split_broker.rs`): if the attached child
   advertised `method` → route to `ChildClient`; else fall to the in-process
   `LoopbackBroker` (hybrid) or return `BackendNotRunning`/`UnknownMethod` (pure OOP).
6. `ChildClient::call` writes a `Request` frame and blocks on a channel; the BE's
   `serve_stdio` reader spawns a **worker thread** that runs the dispatch closure and
   writes a `Response`; the shell's reader thread demuxes it and wakes the caller.
7. Backend dispatch: the `Dispatcher::into_fn` closure downcasts `&dyn Any` → `&CorvusState`
   and calls the `#[handler]` thunk (decodes args, serializes the result).
8. Result flows back; `dispatch_rpc` re-wraps `IpcError::Backend(s)` → `AppError::Other(s)`,
   preserving the wire string byte-for-byte.
9. **Events**: a handler calls `state.emit(topic, payload)` → `FrameEventSink` writes an
   `Event` frame → the shell's `on_event` callback re-emits via `AppHandle::emit` → FE `listen`s.

The **in-process** path is identical minus the process: the `LoopbackBroker` calls the
dispatch closure against the live state directly.

---

## 5. Handler pattern inside a BE

A handler is a plain function annotated `#[arbor_rpc::handler]` that **self-registers via
`inventory`** — no central list, no `match`. Concrete example
(`crates/products/corvus/be/src/stash.rs`):

```rust
use corvus_core::prelude::hooks;

#[arbor_rpc::handler]
fn stash_save(
    state: &CorvusState,
    tab_id: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<StashEntry, String> {
    let workdir = { let repo = open(state, &tab_id)?; repo.workdir()... };
    let entry = corvus_git::stash::stash_save(&git(state), &workdir, ...)
        .map_err(|e| e.to_string())?;
    state.fire_hook(hooks::STASH_PUSH, serde_json::json!({
        "tab_id": tab_id, "index": entry.index, ...
    }));
    Ok(entry)
}
```

Conventions (do all of these):

- **First param is the context** (`&CorvusState`), recovered by downcasting `&dyn Any`.
- **Method name defaults to the fn name** (`stash_save`). Override with
  `#[handler("x.y")]` or `#[handler(program=…, name=…)]`.
- **Naming**: `snake_case`, grouped by **domain module** (`stash.rs`, `branch.rs`,
  `stage.rs`, `bisect.rs`, …), one file per domain, listed as `mod` in `main.rs`.
- **Args** are decoded by name from the JSON params object; a missing key → `null` → an
  `Option` param defaults to `None`.
- **Errors** cross the seam as the error's `Display` string — that is the exact wire text
  the FE matches on. Keep the OOP string **byte-identical** to the in-process path
  (`.map_err(|e| e.to_string())` matching the shell's mapping).
- **Hooks** fire inline *after* the repo handle drops, via `state.fire_hook(name, ctx)`.
- **Hook names are always constants, never literals.** Every name lives once in
  `arbor_plugin_types::hook_names`, re-exported by the product's `hooks.rs`
  (`corvus_core::prelude::hooks`, `garrulus_core::prelude::hooks`) — the same constant the
  entry in `HOOK_CATALOG` is built from. A literal compiles, fires, and reaches nobody: it
  is absent from the catalog, so it bypasses the manifest opt-in *and* no subscription can
  ever resolve to it. If a name you need is missing, add it to `hook_names` and give it a
  catalog entry in the same pass — the completeness tests require both halves.
- **Names are `<namespace>:<event>`.** The namespace is the product that owns the *concept*
  (`corvus:commit`), `arbor:` for anything the host runtime owns (plugin lifecycle, views,
  theme, which project is open), or a subsystem's own (`pipeline:started`). The event half
  never repeats the namespace: `garrulus:note_saved`, not `garrulus:vault_note_saved`.
- **Async handlers** (`async fn`, e.g. network/provider calls) register as `Kind::Async`
  and are served by `block_on` on the runtime **handle** on a serve-loop worker thread —
  never on the runtime itself.

A trivial handler with self-resolved config: `sitta/be/src/config_cmds.rs`
(`get_sitta_config`/`set_sitta_config`, `_state` unused, path resolved from the profile).

---

## 6. Hosting plugins in a BE

Hooks fired by OOP handlers must reach the Lua plugins, so the **plugin host lives inside
the product BE** (not the shell).

- `.plugin_host("corvus", build_hook_dispatcher)` builds the `PluginHost` (filtered to the
  product), its headless `BackendAppCtx`, the hook dispatcher, and the scheduler. Nothing
  about plugin **roots** is passed: a profile keeps its packages in two directories —
  `installed/` and `marketplace_plugins/` — and `arbor_plugin_core::prelude::plugin_roots`
  answers with both, recomputed per discovery so a live profile switch needs no
  re-registration. It used to be a per-call-site decision, and every site that took only the
  first directory was silently wrong: in a debug build both resolve to something populated,
  so the mistake only surfaced in a release build, as an empty Plugin Manager or an
  extension reported missing while installed.
- `arbor.*` namespaces are published via `.api_installer(...)`. The host-side `NsHost` impl
  (e.g. `CorvusNsHost` in corvus's `main.rs`) bridges git/provider/workspace calls and
  fires hooks onto the **same** state broker the RPC handlers use.
- The **Plugin-Manager RPC** is a reusable bundle: `plugin_rpc::CorvusRpcCtx` is a local
  newtype implementing the foreign `PluginRpcContext` (orphan-rule workaround);
  `plugin_rpc::methods()` is added to the dispatcher as an **extra group** with a per-call
  context factory.
- The host handle is published module-statically via `host_handle::install`/`host()`, kept
  **out** of `*State` so the core crate stays mlua-free.

A **host-pure** BE (no product namespaces, no vetoable hooks) writes no plugin module at
all — `arbor-plugin-core`'s prelude ships the pair ready-made, so the whole wiring is two
lines in `main`:

```rust
app.plugin_host("sitta", arbor_plugin_core::prelude::host_pure_hook_dispatcher);
app.api_installer(arbor_plugin_core::prelude::host_pure_api_installer());
```

`host_pure_hook_dispatcher` registers the shared `HOOK_CATALOG` with nothing vetoable and
binds one `LuaHookListener`; `host_pure_api_installer` publishes only the base `arbor.*`
namespaces (empty `extra` list). `sitta-be`, `tyto-be` and `garrulus-be` all use it.
`build_hook_dispatcher` is the same builder with `corvus:pre_commit` marked vetoable (the shell
and `corvus-be`); `build_hook_dispatcher_with(host, &[…])` is the parametrised form behind
both. A product that grows its own namespaces graduates to a real `LuaApiInstaller` passing
them as `extra`, the way corvus does — it does not fork the host-pure one.

**If the BE also overrides `on_ready`, it must spawn the plugin reload itself.** The
override *replaces* `App`'s default, and that default's entire body is the reload — omit it
and the host loads zero plugins while every hook fire silently reaches nobody. See
`garrulus/be/src/main.rs`, the only BE that combines the two.

---

## 7. Patterns to follow / landmines

- **Never block a runtime worker on framed IPC.** `ensure_<product>_be` / `sync_config` do
  a synchronous `rx.recv()` and the BE may fire a reverse-channel credential request the
  shell answers with `block_on` needing **free** workers. Blocking a worker → deadlock →
  white windows. Always run these on `tokio::task::spawn_blocking` (see
  `src-tauri/src/window/corvus.rs::open_corvus_window`).
- **Reverse-channel discipline**: the serve loop must dispatch each `Request` on its **own
  worker thread** so a handler that calls back to the shell mid-dispatch doesn't stall the
  reader that must deliver its `HostResponse`. There is a load-bearing test for exactly this
  in `transport.rs`. (`docs/reverse-channel.md`.)
- **Error strings are the contract.** Errors cross the seam as `Display` strings, never
  structured. Keep OOP wire strings byte-identical to the in-process path.
- **Config sync**: OOP handlers can't see app config. The shell pushes JSON sections into
  the state via the `__set_config` RPC; the handler deserializes its own section and falls
  back to `Default` when absent. Only corvus-be currently receives pushed config (its
  product dir + git executable override); other BEs self-resolve everything from
  `init_active_profile()`.
- **`Hello` ordering**: on-load hooks emit events, and events must **not** precede the
  `Hello` frame or the shell rejects the connection. Plugin reload is therefore deferred to
  the post-`Hello` `on_ready` hook, run on a background thread.
- **stdout is the protocol channel** — every log line goes to **stderr**.
- **Lifecycle races**: attach/detach use spawn **generations** + `detach_if_current` so a
  stale disconnect of a replaced child can't rip out the live one. Blocking child teardown
  (`kill()+wait()`) is offloaded to a throwaway thread, never under the routing lock.
- **Async spawn tools** — pick deliberately:
  - Shell Tauri commands: the single `rpc` command is `async` + one central `spawn_blocking`.
  - `ns_shell/*` (Lua, sync) uses `tauri::async_runtime::spawn`.
  - In a Tauri-agnostic crate use `AppCtx::spawn`, not `tokio::spawn` directly.
- **Prelude**: every library crate exposes its public API via `pub mod prelude`; call sites
  import through it. When you add a `pub` item, add it to `prelude.rs` in the same change.

---

## 8. Scaffolding a brand-new product BE (`foo-be`)

Concrete, ordered steps. The **minimum viable seam** is steps 1–2 (state + a `be_ping`
handler) and 3–6 (workspace + router + ensure + window); everything else grows one
`#[handler]` at a time.

1. **Core crate** `crates/products/foo/core/`:
   - `src/state.rs`: `FooState` holding at least `events: Arc<dyn EventSink>`, optional
     `host: Option<Arc<dyn HostCaller>>`, `hooks: Arc<HookDispatcher>`. Model on the minimal
     `TytoState` or the fuller `CorvusState`. Provide `new(sink)`, `with_host_caller`,
     `with_hooks`, `emit`, `event_sink`, `host_call`, `fire_hook`.
   - `src/prelude.rs`: `pub use crate::state::FooState;`.
   - `Cargo.toml`: depend on `arbor-ipc` + `arbor-plugin-api` + `serde_json` (mirror
     `corvus/core`).

2. **BE crate** `crates/products/foo/be/` with `[[bin]] name = "foo-be"`. Deps: `arbor-be`,
   `arbor-rpc`, `arbor-ipc`, `arbor-core`, `foo-core`, plus `arbor-plugin-core` if it hosts
   plugins.
   - `src/main.rs`: copy the `sitta-be` skeleton — `init_active_profile()` →
     `App::new(BackendIo::new())` → `.plugin_host("foo", foo_hook_dispatcher)` →
     `.api_installer(...)` → build `Arc<FooState>` → `Dispatcher::new(state, rt).inventory("")`
     → `app.run(dispatcher)`.
   - `src/selftest.rs`: `be_ping` / `be_echo` handlers to prove the seam first.
   - Domain modules: one file per domain, each a set of
     `#[arbor_rpc::handler] fn(&FooState, …) -> Result<T, String>`.
   - `src/plugin.rs` if it needs plugins — host-pure copy of `sitta/be/src/plugin.rs`.

3. **Register in the workspace**: add `"crates/products/foo/core"` and
   `"crates/products/foo/be"` to `Cargo.toml` members.

4. **Register the router program** in `build_router` (`src-tauri/src/ipc/mod.rs`):
   `router.register("foo", Arc::new(SplitBroker::pure_oop("foo")));` (use `pure_oop` unless
   the shell keeps in-process `foo` handlers).

5. **Add `ensure_foo_be`** in `src-tauri/src/ipc/mod.rs` — copy `ensure_merula_be` +
   `spawn_merula_be`: spawn lock, `is_attached` guard, `next_gen`,
   `backend_binary(app, "foo-be")`, `ChildClient::spawn` with `(on_event → emit to the foo
   window, host_dispatch, detach_if_current on disconnect)`, `split_broker::attach`. Add a
   `sync_config`-style push **only** if foo-be needs shell-resolved config (most don't).

6. **Call it from the window**: in `src-tauri/src/window/foo.rs::open_foo_window`, run
   `ensure_foo_be` on `tokio::task::spawn_blocking` **before** showing the window (the
   blocking-pool rule is mandatory).

7. **Config (if needed)**: put the typed `FooConfig` + `load`/`save` in `foo-core::config`
   and expose `get_foo_config` / `set_foo_config` handlers that self-resolve
   `foo/config.toml` from the active profile. No shell push required.

8. **FE**: add a `foo(method, params)` helper that calls
   `invoke("rpc", { program: "foo", method, params })`. No per-command shell edits.

---

## 9. Storage & paths (appendix)

Persistence is **profile × product**, all under one OS root via the `dirs` crate, namespaced
with a literal `arbor` segment. Helpers in `crates/foundation/core/src/paths.rs` and
`crates/foundation/core/src/profile.rs`.

| Helper | Windows | macOS |
|---|---|---|
| `arbor_config_dir()` | `%APPDATA%\arbor` | `~/Library/Application Support/arbor` |
| `arbor_cache_dir()` | `%LOCALAPPDATA%\arbor` | `~/Library/Caches/arbor` |
| `arbor_global_data_dir()` | `%APPDATA%\arbor\data` | `~/Library/Application Support/arbor/data` |

The active profile is selected by a plain-text pointer file at the arbor root
(`active-profile`, or `active-profile-dev` in debug builds). The real per-profile root is:

- Windows: `%APPDATA%\arbor\profiles\<profile>\`
- macOS: `~/Library/Application Support/arbor/profiles/<profile>/`

Key files (all under `profiles/<profile>/`, TOML for config / JSON for state):

| File | Owner | Contents |
|---|---|---|
| `profile.toml` | shell (`AppConfig`) | theme, keybindings, appearance, animations, IDE/terminal/git prefs, activity bar, launcher, `recent_repos` |
| `corvus/config.toml` | corvus-be (`CorvusConfig`) | git-product prefs (diff, graph, cache, issues, mr, commit, branches, gitflow, …) |
| `corvus/workspaces.json` | corvus-be | workspaces + groups + active id (repos referenced by **UUID**) |
| `corvus/repos.json` | corvus-be | UUID → `{ path, remote_url, display_name }` — **the only place absolute repo paths live** |
| `corvus/workspace-state/<id>.json` | corvus-be | per-workspace open-tab snapshots |
| `plugins/installed/<name>/` | plugin host | installed plugin folders (`plugin.toml` + `main.lua` + …). In a **debug** build `plugin_dir()` points at the workspace's `plugins/` instead, so this pool is release-only |
| `plugins/marketplace_plugins/<name>/` | marketplace | marketplace-installed plugin folders — the bulk of a real installation |
| `plugins/plugin_states.json` | plugin host | `name → enabled` |
| `plugins/marketplace_installed.json` | marketplace | install ledger (name → version/sha/`install_path`) |
| `plugins/plugin_data/<name>/global.json` | plugin (`arbor.settings.global`) | **small** per-plugin state (e.g. compile/run commands) |
| `plugins/toolchains/<kind>.json` | plugin host | JDK/Node/Rust registries (**absolute, platform-specific** paths) |

At the arbor **root** (cross-profile, not in a profile): `oauth.toml` (OAuth overrides),
`config.toml` (legacy, migration-only), `git/` (Windows portable git binary), the
`active-profile*` pointer files.

**Heavy / machine-specific** — rebuildable, keep out of any settings migration:
`arbor/data/*` (bennu symbol indices, merula sample/VSCO banks, sitta/tyto thumbnail caches),
`arbor/cache`, `toolchains/*.json` (Windows JDK paths), the portable git binary.

**Secrets** are **not on disk**: the OS keyring holds OAuth tokens / SSH creds
(`keyring` service `"arbor"`; a plugin's own secrets go in its credential slots), shared
across profiles. They **do not migrate** between machines — re-authenticate on the new one.

Per-repo config lives inside the working tree at `<repo>/.arbor/config.toml` (`RepoConfig`)
and sidecars (`studio.toml`, `links.toml`, `bisect/`, `plugins/<name>/project.json`).
`.arbor/` is gitignored, so it travels only with a physical folder copy, not via git.
