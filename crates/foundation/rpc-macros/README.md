# arbor-rpc-macros

The proc-macro behind [`arbor-rpc`](../rpc). **Don't depend on this directly** —
use `arbor_rpc::handler`, which re-exports it (the serde / serde_derive split).

## `#[handler]`

Annotating a backend handler turns it into a self-registering RPC entry. The
macro reads the function signature and generates:

- the JSON-argument decode (`arbor_rpc::decode_field` per parameter, by name);
- the result serialization (`serde_json::to_value`);
- an `arbor_rpc::inventory::submit!` of an `arbor_rpc::Entry`.

Expected shape: `fn(&Ctx, arg1: T1, …) -> Result<R, E>` where the **first
parameter is the backend context** (a shared reference, recovered by downcasting
the dispatcher's `&dyn Any`), `R: Serialize`, and `E: Display`.

### Attribute forms

| Form | method name | program |
|---|---|---|
| `#[handler]` | the fn's own name | default (empty) |
| `#[handler("custom.name")]` | `"custom.name"` | default (empty) |
| `#[handler(program = "platform")]` | the fn's own name | `"platform"` |
| `#[handler(program = "platform", name = "theme.get")]` | `"theme.get"` | `"platform"` |

The `program` is the router's product label; it lands in `Entry.program` and
drives `arbor_rpc::registry_for(program)`. Keys (`program`, `name`) take string
literals, in any order.

## Why a separate crate

Rust requires proc-macros to live in a `proc-macro = true` crate, which can't
also export the runtime types (`Entry`, `decode_field`, …). Those live in
`arbor-rpc`, which re-exports this macro so consumers see a single dependency.

## Depends on

`syn` (full), `quote`. Emits paths rooted at `::arbor_rpc` / `::serde_json` /
`::core` — so the generated code needs only `arbor-rpc` in scope.
