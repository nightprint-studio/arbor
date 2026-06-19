# corvus-core

The headless backend core for **Corvus** (Arbor's git product) — the Tauri-free
state the future `corvus-be` process will own.

## Why it exists

Model D splits each product into a headless backend (`corvus-be`, `merula-be`,
`sitta-be`) behind the shell's router + credential broker. The git backend's
state must therefore live in a crate that knows nothing about Tauri, so the
eventual process split is `cargo new` + move, not a refactor.

`CorvusState` is the seed of that owned state. **Today it runs in-process**: the
shell builds one and its `AppState` delegates event egress here. It grows
field-by-field as git domains are extracted from the shell.

## What it holds (today)

- **`events: Arc<dyn EventSink>`** — backend → frontend event egress
  ([`arbor_ipc::prelude::EventSink`]). In-process the shell backs it with
  `AppHandle::emit`; once `corvus-be` splits out it wraps the `arbor-ipc` event
  channel and each `emit` becomes an `Event::Notify` the shell re-emits. **The
  call site (`state.emit(...)`) never changes — only the backing.**
- **`repos` / `git_program`** — the tab→path map + the resolved git binary the
  shell pushes, so handlers resolve a tab to its repo without a `RepoManager`.
- **`host: Option<Arc<dyn HostCaller>>`** — reverse channel to the shell (vault /
  plugin-UI round-trips); set only once split into `corvus-be`.
- **`hooks: Arc<HookDispatcher>`** — the runtime hook broker, so a handler fires
  its plugin hooks *where it runs* (`state.fire_hook(...)` /
  `state.fire_pre_commit_veto(...)`). In-process the shell shares its own
  dispatcher here (so a `&CorvusState` fire and a `&AppState` fire hit the same
  host); `corvus-be` owns one bound to its co-located plugin host. The default is
  an empty dispatcher → fires are clean no-ops.

## What moves in next (the gradual scaffold)

As each shell git registry / domain becomes transport-ready it joins
`CorvusState` as a field, and the matching IPC handlers shift from `&AppState`
to `&CorvusState`:

1. `JobRegistry` (already `Arc`-shared — low-friction).
2. Plugin log buffer, stats caches (Arc-shared).
3. `RepoManager` + the `crate::git` leaf modules (the big extraction).

When `CorvusState` holds enough that handlers take `&CorvusState`, the struct +
handlers move into `bins/corvus-be` and the shell talks to it over `arbor-ipc`.

## Public API: use the prelude

`corvus_core::prelude::CorvusState`.

## Depends on

`arbor-ipc` (the `EventSink` contract), `arbor-plugin-api` (the `HookDispatcher`
the fire seam bridges to — the Tauri-free api crate, no mlua), `serde_json`. No
Tauri, no git, no product types yet — intentionally minimal.

## Consumed by

`arbor` (the shell): builds a `CorvusState` in `setup()` with a Tauri-backed
`EventSink` and routes `AppState::emit` / `AppState::event_sink` through it.
