# arbor-scheduler

Single trigger engine — FixedRate / FixedDelay / Cron — with cooperative
cancellation, focus-gating, and on-the-fly mutability (cancel / enable /
disable / swap trigger without re-registering).

## Why this crate exists

There used to be **two** schedulers in the codebase:

- `src-tauri/src/marketplace/scheduler.rs` — wakes every N minutes and
  fires the marketplace catalog refresh.
- `src-tauri/src/plugin/runtime/scheduler/mod.rs` — drives plugin-declared
  schedules (`arbor.scheduler.register` from Lua), one OS thread per
  schedule, supports `fixed_rate` / `fixed_delay` / `cron`, focus-gated.

Same loop, two implementations. This crate is the single engine both end
up calling into. Future consumers (a GC task for orphaned plugin folders,
scheduled pipeline runs, periodic plugin-update checks) inherit cancel /
focus-gating / on-the-fly mutability for free.

## Public surface (via `prelude`)

```rust
use arbor_scheduler::prelude::*;
use std::time::Duration;

let sched = Scheduler::new(
    ctx,                                  // Arc<dyn arbor_core::prelude::AppCtx>
    tokio::runtime::Handle::current(),    // runtime the per-schedule tasks live on
);

sched.register(
    ScheduleKey::new("marketplace", "auto_refresh"),
    Trigger::FixedDelay { delay: Duration::from_secs(60) },
    ScheduleOpts {
        gate: Some(Arc::new(|| /* read config, decide */ true)),
        ..Default::default()
    },
    Arc::new(FnAction(|| async {
        // refresh the catalog
    })),
)?;
```

### On-the-fly mutability

Every mutator notifies the corresponding runner so it picks up the change
on its very next loop turn (no waiting for the current sleep to elapse).

| Operation             | Method                                                            |
|-----------------------|-------------------------------------------------------------------|
| Cancel one            | `sched.cancel(&key)` → returns `bool`                             |
| Cancel a whole subsystem | `sched.cancel_namespace("plugin:foo")` (exact, no prefix match) |
| Disable without dropping the task | `sched.set_enabled(&key, false)`                      |
| Re-enable             | `sched.set_enabled(&key, true)`                                   |
| Swap trigger          | `sched.update_trigger(&key, Trigger::FixedRate { … })?`           |
| Snapshot              | `sched.list()` → `Vec<ScheduleSnapshot>`                          |
| Membership check      | `sched.contains(&key)`                                            |

A disabled schedule parks on a `tokio::sync::Notify` — no thread teardown,
no consumer-side reconfiguration. `update_trigger` revalidates the cron
expression and leaves the previous trigger in effect on parse failure.

## Module layout

| File           | Responsibility                                              |
|----------------|-------------------------------------------------------------|
| `action.rs`    | `Action` trait + `FnAction` closure adapter                 |
| `error.rs`     | `SchedulerError` (`NotFound`, `InvalidCron`)                |
| `key.rs`       | `ScheduleKey { namespace, name }`                           |
| `opts.rs`      | `ScheduleOpts` + `Gate` (per-tick sync predicate)           |
| `prelude.rs`   | canonical public re-exports                                 |
| `runner.rs`    | per-schedule async loop (private)                           |
| `scheduler.rs` | `Scheduler` — register / cancel / mutate / list             |
| `snapshot.rs`  | `ScheduleSnapshot` — frontend-friendly view                 |
| `trigger.rs`   | `Trigger` + parse-once `CompiledTrigger`                    |

## Depends on

- `arbor-core` — only for `AppCtx` (focus signal + arbor data root).

External: `tokio`, `cron`, `chrono`, `tracing`, `thiserror`,
`async-trait`, `serde`.

**Must NOT depend on `mlua`.** The Lua-bridge action lives in
`arbor-plugin-core`; this crate just exposes the `Action` trait.

## Consumed by (planned migration)

- `arbor-plugin-core` — `arbor.scheduler.register` from Lua, via an
  `Action` impl that wraps `PluginHost::fire_hook_on`.
- `arbor-plugin-marketplace` — auto-refresh task, a single `FixedDelay`
  entry with a `gate` that re-reads `marketplace.refresh_hours`.
- `arbor-pipeline-core` — scheduled pipeline runs.
- Any future host loop that wants cancel + focus-gating + on-the-fly
  reconfiguration.

## Semantics notes

- **`FixedRate` vs `FixedDelay`**: `FixedRate`'s next fire = previous
  *start* + interval; `FixedDelay`'s next fire = previous *end* + delay.
  An overrunning handler under `FixedRate` triggers the next fire
  immediately (no catch-up burst — missed ticks collapse into one).
- **Focus-gating**: `only_when_focused = true` skips fires while the
  window is in the background, but the clock keeps advancing — focus
  return doesn't trigger a burst of back-to-back fires.
- **Custom gate**: synchronous closure evaluated every tick; same "skip
  but advance the clock" semantics as focus-gating. Lets consumers express
  "feature flag" toggles without re-registering on every flip.
- **Cron**: 6-field Spring syntax (`sec min hour dom mon dow`). Parsed
  once at `register` / `update_trigger`; runtime path never re-parses.
- **Initial delay**: applies to `FixedRate` / `FixedDelay` only. For cron
  it would just push past one occurrence; ignored.
