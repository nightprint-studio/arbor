# arbor-rpc

The Model-D backend dispatch core: self-registering RPC handlers.

## Purpose

A product backend (`corvus-be`, `merula-be`, `sitta-be`) exposes its commands as
plain functions annotated with `#[handler]`:

```rust
#[arbor_rpc::handler]   // registers as "stash_save" (the fn name); pass "x.y" to override
fn stash_save(state: &AppState, tab_id: String, message: Option<String>, include_untracked: bool)
    -> Result<StashEntry, AppError> { /* … */ }
```

The method name is optional — it defaults to the function's own name, so a
handler named after its endpoint never repeats the string.

### Program namespaces

Each handler belongs to a **program** — the router's product label. A bare
`#[handler]` registers under the default (empty) program; tag a handler with a
program to put it in another backend's slice:

```rust
#[arbor_rpc::handler(program = "platform")]            // method = fn name
fn get_app_info(state: &AppState) -> Result<AppInfo, AppError> { /* … */ }

#[arbor_rpc::handler(program = "platform", name = "theme.get")]   // both set
fn theme_get(state: &AppState) -> Result<Theme, AppError> { /* … */ }
```

`registry()` returns every handler regardless of program (a single `*-be`
binary only links its own program's handlers, so that's already its exact
method set). `registry_for(program)` returns just one program's handlers — used
by the **shell**, which links several backends' handlers into one inventory
while they await their out-of-process split, so each program's in-process
dispatcher serves only its own methods (no cross-program leak, no same-name
collision).

### Sync vs async handlers

A handler is **sync** (a plain `fn`) or **async** (an `async fn`) — the macro
reads its `async`-ness and registers it as `Kind::Sync` / `Kind::Async`:

```rust
#[arbor_rpc::handler]                          // CPU-bound git — Kind::Sync
fn get_status(state: &AppState, tab_id: String) -> Result<RepoStatus, AppError> { /* … */ }

#[arbor_rpc::handler]                          // network/credential — Kind::Async
async fn list_mrs(state: &AppState, tab_id: String) -> Result<Vec<Mr>, AppError> { /* … */ }
```

`registry_for(program)` returns only the **sync** handlers (the host runs them
on `spawn_blocking`, off the runtime workers — right for libgit2). The
disjoint `async_registry_for(program)` returns the **async** handlers (the host
awaits them on the runtime — the network round-trip yields the thread instead
of blocking a pool thread). An async handler's future borrows the context and
must be `Send`, so it must not hold a `MutexGuard` across an `.await` (lock
briefly, drop, then await).

The macro reads the signature and generates:

- the JSON-argument decode (one `decode_field` per parameter, by name);
- the result serialization;
- an `inventory::submit!` so the handler self-registers.

`registry()` then returns every annotated handler as a `method → CallFn` map —
**no central list, no per-command `match`, no arg-struct**. It's the same trick
`#[tauri::command]` uses, retargeted at the generic Model-D dispatch.

## How it crosses the seam

- The **first parameter is the backend context** (a shared reference, e.g.
  `&AppState`). The dispatcher passes it type-erased as `&dyn Any`; the generated
  thunk downcasts it back — so this crate stays free of any product's concrete
  `AppState` / error type.
- Handler **errors cross as their `Display` string** (the wire string the FE
  matches on); the success value crosses as a `serde_json::Value`.

## Two crates, one dependency

A proc-macro must live in its own `proc-macro = true` crate, so the macro is in
[`arbor-rpc-macros`](../rpc-macros) and re-exported here. Consumers depend only on
`arbor-rpc` (the macro + `inventory` ride along) — the serde / serde_derive
pattern.

## Composing generic handlers — bundles (Bevy-like)

`#[handler]` is ideal for a backend's **own** concrete handlers, but it can't
register **generic** ones (the macro bakes a concrete context downcast, and
`inventory` entries are non-generic and link-local). The `Builder` adds that:

```rust
let (sync, asyncs) = arbor_rpc::Builder::<MyCtx>::new()
    .add_inventory("")                 // this binary's #[handler] set
    .add(some_lib::SomeBundle)         // a reusable RpcBundle<MyCtx>, monomorphised here
    .into_maps();
```

An `RpcBundle<C>` returns `HandlerEntry`s whose bodies are **non-capturing**
closures referencing only the type `C` (to downcast `&dyn Any` back) — so they
coerce to the same `CallFn` fn-pointers the macro emits, with no `inventory` and
no per-handler glue in the product. This is the "hybrid" model: concrete handlers
via the macro, reusable generic ones via bundles (see `arbor-plugin-rpc`).

## Public API: use the prelude

`arbor_rpc::prelude::{handler, registry, registry_for, async_registry_for, decode_field, Builder, RpcBundle, HandlerEntry, Entry, Kind, CallFn, AsyncCallFn}`.

## Tests

`cargo test -p arbor-rpc` exercises the macro end-to-end: registration,
arg-decode (incl. omitted `Option`), wrong-context error, bad-arg error, and
per-program partitioning (`registry_for`).

## Depends on

`arbor-rpc-macros`, `inventory`, `serde`, `serde_json`. Product-agnostic — no
Tauri, no keyring, no product types.

## Consumed by

`arbor` (the shell): `src-tauri/src/ipc/corvus/*` (default program) and
`src-tauri/src/ipc/platform/*` (`program = "platform"`) annotate their handlers;
each dispatcher builds its slice with `registry_for(...)`. Future `*-be`
backends share it unchanged.
