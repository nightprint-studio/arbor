# arbor-rpc-macros

The proc-macro behind [`arbor-rpc`](../rpc). **Don't depend on this directly** —
use `arbor_rpc::handler`, which re-exports it (the serde / serde_derive split).

## `#[handler("method.name")]`

Annotating a backend handler turns it into a self-registering RPC entry. The
macro reads the function signature and generates:

- the JSON-argument decode (`arbor_rpc::decode_field` per parameter, by name);
- the result serialization (`serde_json::to_value`);
- an `arbor_rpc::inventory::submit!` of an `arbor_rpc::Entry`.

Expected shape: `fn(&Ctx, arg1: T1, …) -> Result<R, E>` where the **first
parameter is the backend context** (a shared reference, recovered by downcasting
the dispatcher's `&dyn Any`), `R: Serialize`, and `E: Display`.

## Why a separate crate

Rust requires proc-macros to live in a `proc-macro = true` crate, which can't
also export the runtime types (`Entry`, `decode_field`, …). Those live in
`arbor-rpc`, which re-exports this macro so consumers see a single dependency.

## Depends on

`syn` (full), `quote`. Emits paths rooted at `::arbor_rpc` / `::serde_json` /
`::core` — so the generated code needs only `arbor-rpc` in scope.
