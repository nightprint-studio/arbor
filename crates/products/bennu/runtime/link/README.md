# bennu-link

What a running thing Arbor attaches to declares it can do — and the link to a **game over the fulcrum `dev` channel**.

Bennu attaches to two kinds of live target: a **debugger** session (JDWP, DAP — `bennu-be::debug_backend`) and a **game**, which
can speak either the Bevy Remote Protocol (`bennu-be::brp`) or the fulcrum `dev` channel (this crate). This crate holds the part
they share and the part that is new, and nothing of Tauri or of Bennu's backend, so it is tested on its own against a fake game.

| Module | What |
|---|---|
| `target` | `LiveTarget` (who it is) and `Capability` (what it can do): `info screenshot input command actions stats hud chat mods world mod_state ui_tree image_analysis pose region props ecs bevy_states` for a game, `pause stack eval output` for a debugger. `Capability::belongs_to` keeps the line: **a game is never paused**. `unsupported()` builds the one sentence a tool answers when its link lacks a capability. |
| `game` | `GameLink`: the operations of a game (every method defaults to "this link has no …"). Implemented here by `DevChannelLink` and in `bennu-be::brp::link` by the Remote Protocol link. |
| `pick` | `pick(preference, dev_open, brp_open)`: `auto` (`dev` first, then BRP), `dev`, `brp` — pure, with the sentences that say which link *is* open. |
| `dev` | `DevChannelLink`: discovery (`list_games`), connection, and every call, over `fulcrum-dev-channel` (`DevClient`, run on `spawn_blocking`). |
| `vision` | the two **picture-free ways to look at a game** (protocol 2): `ui_json` shapes the UI tree, `parse_ops` / `results_json` parse and shape image analysis (`fulcrum-vision`), `analyze_file` runs it on a PNG on disk, `scratch_path` says which PNGs may be read without a project root. Pure, held by unit tests. |
| `numbers` | the **world as numbers** (protocol 4): `props_json` / `summary_json_value` shape the game's answers to `Query::Region` and `Query::Props`; `map_scheda` and `map_percorso` read a map **file** (`.fmap.ron`) with `fulcrum-map-scan` and need no game. Pure, held by unit tests. |
| `session` | the one `dev` session of the process and the protocol the user chose. |
| `args` | the argument structs of the game tools, with the schema descriptions an MCP client reads. |

## Looking without a screenshot

A 1280×720 screenshot is ~1.2 MB and ~1 200 tokens to read. The `dev` channel (protocol 2) answers two questions better: `GameLink::ui_tree`
(the interface as data: node identity, resolved text, rectangle, visible, opacity, colours; `bevy_game_ui`) and `GameLink::analyze` (statistics,
palette, edges, differences computed **in the game**, only the answer crosses; `bevy_game_analyze`). The same operations on a PNG file need no
game (`image_analyze`, `vision::analyze_file`). The tool descriptions say when to prefer them to `bevy_game_screenshot`. Design, cost table
and the whole API: Voriz's `docs/analisi-immagini.md`.

## Looking at the world without a screenshot (protocol 4)

`GameLink::region` (`bevy_game_region`) asks the game to summarise a box of its world — blocks by type and layer, the height profile, enclosed voids, one
sentence — and `GameLink::props` (`bevy_game_props`) lists the models and props standing in it, with cell, rotation and a box in metres. Without `min`/`max` the box is
around the first player of `Query::World`. Two tools need no game: `map_scheda` (the same summary for a `.fmap.ron`, or what changed between two) and `map_percorso`
(can the player walk from a mark to another with the game's jump and run, and which step breaks if not). All four say in their descriptions to prefer them to a screenshot;
the BRP link has none of them (`unsupported`, with the way out). Design, real runs and the cost table: Voriz's `docs/mappe-a-numeri.md`.

## Depends on the fulcrum checkout

`fulcrum-debug-channel`, `fulcrum-dev-channel`, `fulcrum-vision`, `fulcrum-trace` and `fulcrum-map-scan` (feature `doc`) are **path dependencies outside this workspace**
(`../../../../../../../engine/fulcrum/crates/core/…`): the protocol types and the client are fulcrum's and are not copied, so a
protocol change is a compile error here instead of silent drift. All of them are pure Rust (no Bevy). A clone of Arbor without the
fulcrum checkout beside it does not build `bennu-link`, hence not `bennu-be`.

## Tests

```sh
cargo nextest run -p bennu-link        # or: cargo test -p bennu-link
```

`tests/dev_link.rs` starts a real `ChannelServer` (the transport the fulcrum games use) on a loopback port with a canned
protocol and drives every call through the link: discovery, connect by id and by port, command (player and console), named
action, key with modifiers, text, mouse (click, double click, drag), screenshot, HUD, chat, mods, world, mod state, the
"not supported" sentences, the UI tree (compact keys, hidden nodes, filter), image analysis (the query reaches the game, no screenshot request does), and a game that goes away. For the real thing: `voriz --mute --dev-channel`, then
`bevy_game_connect` from an MCP client or `live_attach` from Imber.
