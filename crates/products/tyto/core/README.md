# tyto-core

The headless backend core for **Tyto** (the screen-recorder product) — the tyto
twin of [`sitta-core`](../../sitta/core) / [`merula-core`](../../merula/core).
Owns the canonical `TytoState` the `tyto-be` process holds; **Tauri-free by
construction**.

`TytoState` is transport-only: BE→FE event egress + the reverse channel back to
the shell. The recorder's heavy work (screen capture, audio, encoding) lives in
the engine the tyto-be domain handlers drive; this crate keeps no such state.

## Config

`TytoConfig` (capture / encoding / output defaults) persists to the per-profile
`arbor/profiles/<active>/tyto/config.toml`, round-tripped through `toml`. The path
is resolved by tyto-be itself via `arbor_core::prelude::tyto_config_path` — not
pushed by the shell. `load()` is infallible-by-design (defaults on a
missing/corrupt file).

The launcher-owned Tyto settings (the OS-global open shortcut + accelerator) live
in the shell config, not here.

## Public API

Reach this crate through the [`prelude`](src/prelude.rs): `tyto_core::prelude::*`.
