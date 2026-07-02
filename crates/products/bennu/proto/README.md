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

The `CapabilitySet` bitset is *produced* by `bennu-project` (the Spike D
capability-detection ruleset); this crate only carries its serialized view.

## Usage

Reach the surface through the prelude:

```rust
use bennu_proto::prelude::*;
```
