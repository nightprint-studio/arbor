# arbor-feedback

Window-agnostic core for Arbor's user-feedback systems. Holds the Tauri-free
pieces that any window (main, grove, …) can host, so progress/status surfaces
are no longer locked to the main-window shell.

## What's here

- **`jobs`** — `JobRegistry` + `JobInfo` / `JobStatus`, the pure in-memory job
  model and bookkeeping. The process-spawning glue (`spawn_job`, the output
  `LineBatcher`) needs `AppHandle` + the plugin host and therefore stays in the
  shell crate (`src-tauri`); only the data model lives here.
- **`notify`** — the `plugin:notification` payload (`NotificationPayload`) and
  `emit_notification(&dyn AppCtx, …)`.
- **`operations`** — the `arbor://plugin-operation-*` event-name contract.

Toasts are frontend-only (each window owns its own store), so they have no
backend counterpart here.

## Routing (`target`)

Backend events broadcast to every window. Payloads carry an optional `target`
window id; each window mounts a feedback host with an id and filters by it. The
`main` host also accepts untagged (`target == None`) items, so existing call
sites keep their original behavior with no changes. The filtering happens on the
frontend — this crate just makes `target` part of the contract
(`JobInfo.target`, `NotificationPayload.target`, operation `start` payload).

## Public API

Reach everything through the prelude:

```rust
use arbor_feedback::prelude::*; // JobRegistry, JobInfo, JobStatus, kill_process,
                                // NotificationPayload, emit_notification, EVENT_*
```

## Dependencies

`arbor-core` (the `AppCtx` emit trait), `arbor-process-ext` (`no_window` for the
Windows kill path), `serde` / `serde_json`. No Tauri.
