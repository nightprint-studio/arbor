# arbor-scheduler

Generic trigger engine: FixedRate / FixedDelay / Cron with cooperative
cancel and optional focus-gating.

## Purpose

Today there are **two** schedulers in the codebase:

- `src-tauri/src/marketplace/scheduler.rs` — wakes every N minutes, fires
  the marketplace catalog refresh.
- `src-tauri/src/plugin/runtime/scheduler/mod.rs` — drives plugin-declared
  schedules (`arbor.scheduler.register` from Lua), one OS thread per
  schedule, supports `fixed_rate` / `fixed_delay` / `cron`, focus-gated.

They solve the same problem with two implementations. This crate is the
**single engine** both will end up calling into. Future consumers (a GC
task for orphaned plugin folders, periodic plugin-update checks, future
remote pre-fetches) inherit cancel/focus-gating for free.

## Contents (planned)

- `Trigger` enum: `FixedRate { interval_sec }`, `FixedDelay { delay_sec }`,
  `Cron { expr }`.
- `Action` — abstract handle for what to fire. Two concrete shapes:
  - `RustAction(Box<dyn Fn() -> BoxFuture + Send + Sync>)` — for host code
    (e.g. marketplace refresh).
  - `LuaHookAction { plugin_name, action_name }` — bridge that the plugin
    runtime registers; the runtime knows how to dispatch to the right
    mlua VM.
- `Scheduler` — runtime struct that owns the cancel tokens
  (`HashMap<key, Arc<AtomicBool>>`) and the focus state. Methods:
  `register(key, trigger, action)`, `cancel(key)`, `cancel_all_matching(prefix)`.
- `RunLoop` — the per-schedule loop (currently lives in the plugin
  runtime as `run_scheduler_loop`); becomes the canonical implementation
  here.

## Depends on

- `arbor-core` — for `AppCtx` (the focus signal comes from there) and
  base error types.

External: `tokio`, `cron`, `chrono`, `tracing`, `thiserror`, `async-trait`.

**Must NOT depend on `mlua`.** The Lua-bridge action lives in
`arbor-plugin-core`; this crate just exposes the `Action` shape.

## Consumed by

- `arbor-plugin-core` — drives `arbor.scheduler.register` from Lua.
- `arbor-plugin-marketplace` — auto-refresh task (one `FixedDelay` entry).
- `arbor-pipeline-core` — scheduled pipeline runs.
- Future: any host loop that wants cancel + focus-gating semantics.

## Notes

- The current marketplace scheduler `continue`s when refresh is disabled
  instead of cancelling the task — that's intentional (config can change
  at runtime). The redesign preserves this behaviour: `register` with a
  trigger whose interval = `None` parks the entry until next reconfigure,
  no thread teardown needed.
- Focus-gating: a schedule marked `only_when_focused` skips fires while
  the app window is in the background. The clock keeps advancing
  (FixedRate doesn't "catch up" on missed ticks).
