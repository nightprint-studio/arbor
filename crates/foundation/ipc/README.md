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
`BrokerClient`, `LoopbackBroker`, `Bytes`, `Event`, `IpcError`,
`SessionProvider`, `AuthSession`, `CredentialError`.

## Contents (M1b skeleton)

- **`client`** — `BrokerClient` (the transport-agnostic request/response trait
  the shell router speaks to) + `LoopbackBroker` (in-process dispatch, used by
  M3's in-process-first step and by the ping round-trip test here).
- **`credential`** — `SessionProvider`, the async keyring-free credential
  contract a backend depends on (`session` / `refresh`, yielding an
  `AuthSession { base_url, auth_header }`), so the coupled domains (issue
  trackers, git providers) can be extracted holding only the contract, not the
  keyring-holding broker. Implemented by per-provider shell adapters (keyring
  read + provider OAuth refresh). One session shape covers fixed-endpoint
  `Bearer` (Linear), per-tenant `Bearer`/`Basic` (Jira), and self-hosted bases.
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
