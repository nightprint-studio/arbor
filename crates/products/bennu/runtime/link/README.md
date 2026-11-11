# bennu-link

What a running thing Arbor attaches to declares it can do — and the link to a **game over the fulcrum `dev` channel**.

Bennu attaches to two kinds of live target: a **debugger** session (JDWP, DAP — `bennu-be::debug_backend`) and a **game**, which
can speak either the Bevy Remote Protocol (`bennu-be::brp`) or the fulcrum `dev` channel (this crate). This crate holds the part
they share and the part that is new, and nothing of Tauri or of Bennu's backend, so it is tested on its own against a fake game.

| Module | What |
|---|---|
| `target` | `LiveTarget` (who it is) and `Capability` (what it can do): `info screenshot input command actions stats hud chat mods world mod_state ecs bevy_states` for a game, `pause stack eval output` for a debugger. `Capability::belongs_to` keeps the line: **a game is never paused**. `unsupported()` builds the one sentence a tool answers when its link lacks a capability. |
| `game` | `GameLink`: the operations of a game (every method defaults to "this link has no …"). Implemented here by `DevChannelLink` and in `bennu-be::brp::link` by the Remote Protocol link. |
| `pick` | `pick(preference, dev_open, brp_open)`: `auto` (`dev` first, then BRP), `dev`, `brp` — pure, with the sentences that say which link *is* open. |
| `dev` | `DevChannelLink`: discovery (`list_games`), connection, and every call, over `fulcrum-dev-channel` (`DevClient`, run on `spawn_blocking`). |
| `session` | the one `dev` session of the process and the protocol the user chose. |
| `args` | the argument structs of the game tools, with the schema descriptions an MCP client reads. |

## Depends on the fulcrum checkout

`fulcrum-debug-channel` and `fulcrum-dev-channel` are **path dependencies outside this workspace**
(`../../../../../../../engine/fulcrum/crates/core/…`): the protocol types and the client are fulcrum's and are not copied, so a
protocol change is a compile error here instead of silent drift. Both are pure Rust (no Bevy). A clone of Arbor without the
fulcrum checkout beside it does not build `bennu-link`, hence not `bennu-be`.

## Tests

```sh
cargo nextest run -p bennu-link        # or: cargo test -p bennu-link
```

`tests/dev_link.rs` starts a real `ChannelServer` (the transport the fulcrum games use) on a loopback port with a canned
protocol and drives every call through the link: discovery, connect by id and by port, command (player and console), named
action, key with modifiers, text, mouse (click, double click, drag), screenshot, HUD, chat, mods, world, mod state, the
"not supported" sentences, and a game that goes away. For the real thing: `voriz --mute --dev-channel`, then
`bevy_game_connect` from an MCP client or `live_attach` from Imber.
