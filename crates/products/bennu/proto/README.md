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

The `CapabilitySet` bitset is *produced* by `bennu-project` (the Spike D
capability-detection ruleset); this crate only carries its serialized view.

## Usage

Reach the surface through the prelude:

```rust
use bennu_proto::prelude::*;
```
