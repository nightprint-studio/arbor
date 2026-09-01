# arbor-brp

Bevy Remote Protocol client — JSON-RPC over HTTP + SSE watch streams.
**Generic Bevy tooling, not git/Corvus-specific** (the old `corvus-` prefix was
organizational only); now lives under `crates/wasm/`.

## Purpose

Talks to a running Bevy game's `RemoteHttpPlugin` endpoint (BRP 0.18): a
read-only JSON-RPC client, a connect-time capability probe
(`rpc.discover` + `registry.schema`), and long-lived `*+watch` SSE
subscriptions. The Tauri command + plugin-namespace layers consume it.

Earmarked to become a **WASM plugin**: its long-lived SSE streaming and tokio
task lifecycle (`AbortHandle` cancellation) need native async that a Lua plugin
can't provide, so it sits under `crates/wasm/` — where `arbor-cloud` sat until the
cloud left Arbor for good. Until it makes the same trip it's used in-process by the
launcher; isolating it as a crate keeps that cut clean.

## Public API: use the prelude

Reach the surface through `arbor_brp::prelude::...`: `BrpClient`, `BrpError`,
`BrpSession`, `BrpRegistry`, `BrpStatus`, `BrpCapabilities`, `WatchSub`,
`WatchEvent`, `run_watch_stream`, `probe_capabilities`, `DEFAULT_ENDPOINT`,
`methods`.

## Contents

- **`client`** — `BrpClient`: one `call(method, params)` JSON-RPC round-trip;
  `BrpError` failure modes (kept separate from the host `AppError`, mapped at
  the command boundary).
- **`sse`** — `run_watch_stream`: a hand-parsed SSE client for `*+watch`
  methods; `WatchEvent` (`Open` / `Data` / `RpcError` / `Error` / `Close`).
- **lib root** — `BrpSession` + `BrpCapabilities` (the capability matrix and
  its `ingest_discover` / `ingest_schema`), `BrpRegistry` (singleton session +
  watch bookkeeping), `BrpStatus` (FE/Lua status payload), `probe_capabilities`,
  the `methods` constants, `DEFAULT_ENDPOINT`.

## Tests

Pure logic is unit-tested (`cargo test -p arbor-brp`): capability ingestion
(`rpc.discover` shapes, `registry.schema` short-name classification + one-level
recursion), SSE `data:` frame parsing, status-from-no-session, truncation. The
HTTP/SSE transport itself needs a live BRP endpoint and isn't unit-tested.

## Depends on

`serde`, `serde_json`, `thiserror`, `tracing`, `tokio` (`AbortHandle`),
`futures-util`, `reqwest`. No Arbor-internal deps — fully self-contained.

## Consumed by

`arbor` (the shell): `commands/brp_commands.rs` (Tauri commands) and
`plugin/ns_shell/brp.rs` (the `arbor.brp.*` Lua namespace).
