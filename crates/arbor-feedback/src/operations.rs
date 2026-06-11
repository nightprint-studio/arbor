//! Operations overlay contract: the `arbor://plugin-operation-*` event names.
//!
//! Operations are multi-step progress cards (the floating overlay used by
//! single-repo Pull, workspace Fetch-all / Pull-all, linked-worktree sync, and
//! plugin-driven long-running work via `arbor.ui.operation.*`). The payloads
//! are shaped inline at each emit site (the frontend `operations-bridge`
//! normalizes them); centralizing the event names here keeps the binding sites
//! and the frontend contract pinned to a single source.
//!
//! Routing: a `start` payload may carry an optional `target` window id. The
//! frontend remembers `operation id → target` and filters the subsequent
//! `update` / `finish` events (which only carry the id) by it, mirroring the
//! job-routing scheme. `None`/absent → main window.

/// Fired once when an operation begins. Carries `{ id, title, steps, … }` and
/// the optional `target`.
pub const EVENT_OP_START: &str = "arbor://plugin-operation-start";

/// Fired on each step transition (`set_current` / `update_step`). Carries the
/// operation `id` + the patch.
pub const EVENT_OP_UPDATE: &str = "arbor://plugin-operation-update";

/// Fired once when an operation ends. Carries the operation `id` + a summary or
/// error.
pub const EVENT_OP_FINISH: &str = "arbor://plugin-operation-finish";
