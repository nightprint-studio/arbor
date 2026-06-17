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

`arbor-ipc` (the `EventSink` contract), `serde_json`. No Tauri, no git, no
product types yet — intentionally minimal.

## Consumed by

`arbor` (the shell): builds a `CorvusState` in `setup()` with a Tauri-backed
`EventSink` and routes `AppState::emit` / `AppState::event_sink` through it.
