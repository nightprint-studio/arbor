# bennu-proto

The **Bennu IPC contract** — the Phase-0 request/response types shared by the
`bennu-be` backend and the Bennu frontend.

Pure serde, **Tauri-free by construction** (depends only on `serde` +
`serde_json`). No logic lives here: it is the single source of truth for the wire
shapes of the Phase-0 methods.

## Phase-0 methods → payloads

| Method (snake_case)   | Result type       |
|-----------------------|-------------------|
| `bennu_open_project`  | `ProjectInfo`     |
| `bennu_project_tree`  | `TreeNode`        |
| `bennu_read_file`     | `FileContents`    |
| `bennu_capabilities`  | `CapabilitySet`   |
| `bennu_completion`    | `Vec<CompletionItem>` (Phase-0 stub → `[]`) |
| `bennu_diagnostics`   | `Vec<Diagnostic>`     (Phase-0 stub → `[]`) |
| `bennu_get_run_config`| `RunConfigSet` (per-repo `[bennu.run]`; fresh repo → `{ configs: [], active_id: null }`) |
| `bennu_set_run_config`| `()` (persists `RunConfigSet` into `<repo>/.arbor/config.toml`) |
| `bennu_main_classes`  | `Vec<MainClassEntry>` (types declaring `public static void main(String[])`) |
| `bennu_index_entries` | `Vec<IndexEntry>` (index-inspector per-kind list: members / jars / jdk / beans / actions / relations) |
| `bennu_get_debug_config` | `DebugConfig` (per-repo `[bennu.debug]`: breakpoints + exception breakpoints + watches) |
| `bennu_set_debug_config` | `()` (persists it, and pushes the breakpoints to any live session) |
| `bennu_debug_variables`  | `Vec<DebugValue>` (what is in scope at one frame of the stopped thread) |
| `bennu_debug_expand`     | `Vec<DebugValue>` (an object's fields, or an array's elements) |
| `bennu_debug_watch`      | `DebugValue` (a watch path evaluated against a frame) |

The debugger's **events** carry `DebugStatus` (`arbor://bennu/debug-status`), `DebugPause`
(`…/debug-paused`, with its `StackFrame`s) and `BreakpointStatus` (`…/debug-breakpoints`) —
a breakpoint is identified by **file and line**, which is what the user set and what survives
a rebuild; turning it into a location a VM understands is `bennu-be`'s job, redone per launch.

The `CapabilitySet` bitset is *produced* by `bennu-project` (the Spike D
capability-detection ruleset); this crate only carries its serialized view.

## Usage

Reach the surface through the prelude:

```rust
use bennu_proto::prelude::*;
```
