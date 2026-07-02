# tyto-be

The headless **screen-recorder** backend process for Model D — the tyto twin of
[`sitta-be`](../../sitta/be) / [`merula-be`](../../merula/be). Serves the tyto
domains over framed-stdio IPC; loads **host-pure** Lua plugins; has **no
credentials and no pushed config** — it resolves its own `tyto/config.toml` once
`init_active_profile()` has run.

## State

`TytoState` ([`tyto-core`](../core)) — transport-only: BE→FE event egress + the
reverse channel back to the shell.

## Domains

| Module        | Methods |
|---------------|---------|
| `selftest`    | `be_ping` / `be_echo` |
| `config_cmds` | `get_tyto_config` / `set_tyto_config` |
| `sources`     | `list_capture_sources` / `list_audio_inputs` |
| `session`     | `start_recording` / `stop_recording` / `pause_recording` / `take_screenshot` / `session_state` |
| `region`      | `select_region` / `clear_region` |
| `library`     | `list_captures` / `rename_capture` / `remove_capture` / `clear_captures` / `reveal_capture` / `open_capture` |

Every capture handler is a **stub** today (the recording engine is a later wave):
it returns empty lists or a "capture backend not available" error, so the frontend
degrades gracefully instead of showing fake devices.

## Plugins

Host-pure Lua only (`plugin.rs`): the base `arbor.*` namespaces, no product
namespaces, no vetoable hooks. Recorder lifecycle hooks (`on_recording_started`,
…) join the shared catalog when the engine fires them.

## Lifecycle

Lazily spawned by the shell (`ensure_tyto_be`) when the Tyto window opens, on the
blocking pool. Serves until the shell disconnects (clean EOF).

## Self-test

```
rpc("tyto", "be_ping", {})            → "pong"
rpc("tyto", "be_echo", { message })   → message
```
