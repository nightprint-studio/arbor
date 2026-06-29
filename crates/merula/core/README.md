# merula-core

The Tauri-free canonical state **+ audio substrate** the `merula-be` process
owns — the merula twin of [`corvus-core`](../../corvus/core).

## Why it exists

Model D splits each product into a headless backend (`corvus-be`, `merula-be`,
`sitta-be`) behind the shell's router + credential broker. The audio backend's
state must therefore live in a crate that knows nothing about Tauri, so the
eventual process split is `cargo new` + move, not a refactor.

## Why it is heavier than `corvus-core`

`corvus-core` is a featherweight state seed. `merula-core` is not — and that is
structurally honest. `MerulaState`'s `session: Mutex<Option<Session>>` field
ties the struct's *type definition* to `Session`, which pulls in the whole audio
substrate: the `!Send`, cpal-backed **audio thread**, the **control** channel,
the BE→FE **events** contract, and the **`MerulaConfig`** type. So the full state
substrate lives here; only the `#[arbor_rpc::handler]` command bodies stay in
`merula-be`.

## What it holds

- **`state`** — [`MerulaState`]: the event egress, the lazily-started audio
  `session` slot, the last-good evaluation, and the reverse channel to the shell.
- **`session`** — the running [`Session`] (audio-thread `JoinHandle` + control
  `Sender` + shared `loaded` set), the lazy `ensure` / `send_if_live` / `shutdown`
  free functions, and the typed process-global `Latest` last-evaluation slot.
- **`control`** — `MerulaControl` (the audio-thread message set) + `Prepared` (an
  off-thread-decoded registry handed across).
- **`events`** — the frozen BE→FE payload contract (`EVT_*` topics + typed
  structs) the audio thread + domain handlers emit through `emit`.
- **`audio_thread`** — the dedicated `!Send` cpal audio thread + its `speech`
  submodule. `build_registry` is called off-thread by `merula-be`'s `audio_cmds`.
- **`config`** — the typed global `MerulaConfig` (`config.toml`) + `load` / `save`.
- **`packs`** — the sample-pack read surface (descriptor table, install status,
  active-pack allow-list) + the lazy `load_subset_into` the audio thread decodes
  through. The `merula_packs` / `merula_pack_set_active` handlers stay in
  `merula-be` and re-import these helpers.
- **`aliases`** — the global `alias → target` read helper the registry builder
  needs (the `get/set_merula_aliases` handlers stay in `merula-be`'s `fstate`).

## Public API: use the prelude

`merula_core::prelude::...` — `MerulaState`, the session/control/config/events
surface the `merula-be` handlers reach through.

## Depends on

`arbor-ipc` (the `EventSink` / `HostCaller` contracts), the Tauri-free `merula`
facade (the four merula crates' prelude — `ControlMap` / `Transport` /
`open_output_stream` / `AudioSink` / `Epoch` / …), `arbor-core` (the profile-aware
`merula_config_dir` / `merula_data_dir` resolver), `serde` / `serde_json` /
`toml`. No Tauri, no `arbor-rpc`, no `arbor-be`.

## Consumed by

`merula-be`: builds a `MerulaState` in `main` and routes its domain handlers
against this substrate.
