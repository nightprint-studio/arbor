# arbor-bevy-remote

A Bevy plugin that extends the stock **Bevy Remote Protocol** server so an external tool — Arbor's Bevy Remote
panel, or an AI assistant — can *drive* a running game, not only inspect it:

- **Screenshots** (PNG, size-capped)
- **Input**: keyboard (keys, modifiers, typed text), mouse (move, click, double click, drag, wheel), gamepad
- **Named actions** the game registers (`restart`, `spawn_wave`, …)
- **Runtime statistics**: frame time with percentiles, FPS, entity count and every other registered diagnostic, window size
- **Presence indicator**: a soft coloured border and a small label while a client is attached

Stand-alone: this crate has its own `[workspace]` and does not depend on Arbor or Tauri. Use it by path
(`arbor-bevy-remote = { path = "…/crates/products/bennu/bevy-remote" }`).

## Adding it to a game

```rust
use arbor_bevy_remote::prelude::*;
use bevy::prelude::*;
use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            // The stock server, with this crate's methods added. `RemotePlugin` only accepts methods when it
            // is built, so this replaces `RemotePlugin::default()` rather than being a second plugin.
            with_arbor_methods(RemotePlugin::default()),
            RemoteHttpPlugin::default(),
            // The systems the methods run against (input delivery, frame history, the indicator).
            ArborRemotePlugin::default(),
        ))
        // Anything the game wants a remote client to be able to trigger:
        .register_remote_action("restart", "Start a new game", |world, _arg| {
            world.resource_mut::<NextState<GameState>>().set(GameState::Playing);
            Ok(serde_json::json!("restarted"))
        })
        .run();
}
```

`ArborRemotePlugin::without_indicator()` turns the on-screen indicator off; every method still works. Building
with `default-features = false` drops the indicator's dependency on Bevy's UI altogether.

## Telling the inspector how to draw a field

`#[derive(RemoteInspect)]` (from [`arbor-bevy-remote-derive`](../bevy-remote-derive), re-exported in the prelude) attaches
display hints to a struct's fields, and `app.register_inspect::<T>()` makes them available to the host:

```rust
#[derive(Component, Reflect, RemoteInspect)]
struct Lamp {
    #[inspect(color)] tint: [f32; 3],
    #[inspect(range = 0.0..=1.0, step = 0.01)] intensity: f32,
    #[inspect(angle)] beam: f32,
    #[inspect(label = "speed", unit = "m/s", readonly)] measured: f32,
}

app.register_inspect::<Lamp>();
```

See the derive crate's README for every option. The hints are keyed by the type path BRP knows the type under
(`TypePath::type_path()`), so the type must derive `Reflect`.

## Methods

All names are prefixed `arbor.`. Params are JSON objects; `arbor.info` lists everything below at runtime.

| Method | Params | Result |
|---|---|---|
| `arbor.info` | — | version, methods, key / gamepad names, actions, presence |
| `arbor.screenshot` | `window?` (entity), `max_width?`, `max_bytes?` | `{ id }` — the capture finishes a few frames later |
| `arbor.screenshot.fetch` | `id` | `{ ready: false }`, or `{ ready, width, height, source_width, source_height, bytes, png_base64 }` |
| `arbor.input.key` | `key`, `action?` (`press`/`down`/`up`), `modifiers?`, `hold_ms?` | echo |
| `arbor.input.text` | `text`, `interval_ms?` | `{ typed }` |
| `arbor.input.mouse` | `action` (`move`/`down`/`up`/`click`/`double_click`/`drag`/`wheel`), `x`, `y`, `button?`, `to_x`/`to_y`/`steps`/`duration_ms` (drag), `dx`/`dy`/`unit` (wheel) | echo |
| `arbor.input.gamepad` | `button` (`South`, `a`, `DPadUp`, …) with `action?`, or `axis` with `value` | echo |
| `arbor.input.cancel` | — | `{ cancelled }` — drops input still waiting (a release not yet sent) |
| `arbor.action` | `name`, `arg?` | whatever the action returns |
| `arbor.actions.list` | — | `{ actions: [{ name, description }] }` |
| `arbor.stats` | `history?` (frames, ≤ 600) | `{ uptime_s, frame: { avg/min/max/p50/p95/p99 ms, fps }, frame_ms_recent, diagnostics, window }` |
| `arbor.presence` | `state?` (`idle`/`connected`/`paused`), `label?`, `enabled?` | the presence now |
| `arbor.inspect_hints` | — | `{ type path: { field: hint } }` registered with `#[derive(RemoteInspect)]` |

Notes:

- **Screenshots are two calls** because a capture finishes after the frame that asked for it. The PNG is encoded
  without compression (no image dependency), so its size is predictable: a picture too large for `max_bytes` (default
  and ceiling 4 MB) is shrunk by whole-factor box averaging until it fits. Pass `max_width` for a smaller one.
  Camera-specific capture is not supported; a window is.
- **Held inputs release later.** `press` writes the down message now and the up message `hold_ms` later (default 60),
  because both in one frame would leave `pressed()` false. Input is written as Bevy's own `KeyboardInput`,
  `MouseButtonInput`, `MouseWheel`, `CursorMoved` and raw gamepad messages, so `ButtonInput`, UI and `Gamepad`
  react as to a real device. A gamepad must already be connected.
- **Presence is declared**: BRP over HTTP has no connection, so a client calls `arbor.presence` when it attaches and
  every acting method marks the indicator *acting* for 1.5 s. A client that never declares itself is still shown the
  first time it acts. Colours: blue connected, orange acting, grey paused.
- **Stats** include whatever `DiagnosticsStore` holds, so adding Bevy's system-information or per-system diagnostics
  plugins shows their numbers too. Memory and asset load state are not collected by this crate.

## Version matrix

| arbor-bevy-remote | Bevy |
|---|---|
| 0.1 | 0.18 |

Older Bevy versions are meant to be added behind feature flags later; the Bevy-facing code is confined to
`input.rs`, `screenshot.rs`, `telemetry.rs`, `overlay.rs`, `actions.rs`, `inspect.rs` and `methods.rs`. Everything else (`png`,
`base64`, `keys`, `stats`, `presence`, `schedule`) is plain Rust and tested without a window.

## Status

Written against the Bevy 0.18 documentation and **not yet compiled**: the first `cargo check` is expected to need
small fixes in the Bevy-facing modules (message and observer names, `BorderColor::all`, `Window::set_cursor_position`,
raw gamepad events). The pure modules carry the unit tests (`cargo test`).
