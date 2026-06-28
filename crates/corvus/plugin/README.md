# corvus-plugin

Shared plugin-host **wiring** for the Corvus product. The mlua host itself lives
in `arbor-plugin-core` (Tauri-free, product-agnostic); this crate holds the
Corvus-specific glue so the host is built one way in both processes that can run
it:

| Piece | What | Used by |
|---|---|---|
| `build_hook_dispatcher` | register the hook catalog + bind a `LuaHookListener` to a `PluginHost` | shell (in-process host) **and** `corvus-be` (OOP host) |
| `CorvusBeApiInstaller` | publish the `arbor.*` surface in a headless backend (host-pure base + the git/product namespaces it's handed) | `corvus-be` |

The point of the split: the Tauri shell is a binary, `corvus-be` is a binary, and
a binary can't depend on another binary — so the wiring both need lives in this
library. (The generic headless `AppCtx` had no Corvus coupling, so it moved to
`arbor-be` as `BackendAppCtx`; `corvus-be` builds it via `arbor_be::App`.) See
`docs/w0a-host-relocation-spec.md` and `docs/plugin-relocation-inventory.md`.

Public API is exposed through `corvus_plugin::prelude`.
