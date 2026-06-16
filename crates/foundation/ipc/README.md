# arbor-ipc

Transport-agnostic IPC for Arbor's Model D (1 FE + N BE).

## Purpose

In Model D the shell process owns the single WebView and talks to headless
product backends (`corvus-be` / `merula-be` / `sitta-be`) over two channels
(LSP-style): typed request/response via `tarpc`, and a one-way push-event
channel. This crate is the contract for both, written so the same client runs
in-process today and over a named pipe / unix socket tomorrow — swapping only
the transport (principle #6).

Full design: [`docs/ipc-design.md`](../../../docs/ipc-design.md).

## Public API: use the prelude

Reach the surface through `arbor_ipc::prelude::...`:
`BrokerClient`, `LoopbackBroker`, `Bytes`, `Event`, `IpcError`.

## Contents (M1b skeleton)

- **`client`** — `BrokerClient` (the transport-agnostic request/response trait
  the shell router speaks to) + `LoopbackBroker` (in-process dispatch, used by
  M3's in-process-first step and by the ping round-trip test here).
- **`event`** — `Event`, the one-way BE→shell push enum (the shell re-emits
  `Event::Notify { topic, payload }` to the FE as a Tauri event).
- **`error`** — `IpcError`.

## Not here yet

The `tarpc` codegen, the per-product service traits, the named-pipe /
unix-socket transport, and the spawn + nonce + ACL handshake all land at the M3
in-process→IPC flip. `tarpc` is parked in `[workspace.dependencies]` but not
wired, so this crate has no `tarpc`/`tokio` dependency yet.

## Depends on

`serde`, `serde_json`, `thiserror`.

## Consumed by

`arbor-shell-common` (the router); future product `*-ipc` clients and `*-be`
backends.
