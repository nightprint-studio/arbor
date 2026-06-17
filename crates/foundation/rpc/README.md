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

## Public API: use the prelude

`arbor_rpc::prelude::{handler, registry, decode_field, Entry, CallFn}`.

## Tests

`cargo test -p arbor-rpc` exercises the macro end-to-end: registration,
arg-decode (incl. omitted `Option`), wrong-context error, bad-arg error.

## Depends on

`arbor-rpc-macros`, `inventory`, `serde`, `serde_json`. Product-agnostic — no
Tauri, no keyring, no product types.

## Consumed by

`arbor` (the shell): `src-tauri/src/ipc/corvus/*` annotates its handlers and
builds the registry. Future `*-be` backends share it unchanged.
