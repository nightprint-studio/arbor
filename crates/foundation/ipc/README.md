# arbor-ipc

Transport-agnostic IPC for Arbor's Model D (1 FE + N BE).

## Purpose

In Model D the shell process owns the single WebView and talks to headless
product backends (`corvus-be` / `merula-be` / `sitta-be`) over two channels
(LSP-style): method+JSON request/response, and a one-way push-event channel.
This crate is the contract for both, written so the same client runs in-process
(loopback) or out-of-process (`ChildClient`, framed JSON over child stdio), and
so the byte-stream can later be hardened to a named pipe / unix socket without
touching the router (principle #6).

Full design: [`docs/ipc-design.md`](../../../docs/ipc-design.md).

## Public API: use the prelude

Reach the surface through `arbor_ipc::prelude::...`:
`BrokerClient`, `LoopbackBroker`, `ChildClient`, `serve_stdio`, `Bytes`,
`Event`, `IpcError`, `SessionProvider`, `AuthSession`, `CredentialError`.

## Contents

- **`client`** — `BrokerClient` (the transport-agnostic request/response trait
  the shell router speaks to) + `LoopbackBroker` (in-process dispatch).
- **`transport`** — `ChildClient` (shell side: spawns the backend, reads its
  `Hello`, demuxes responses / events / host-calls over framed JSON on the
  child's stdio) + `serve_stdio` (backend side of the same frame protocol).
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

Hardening the byte-stream under `ChildClient` from child stdio to a named pipe
(Windows) / unix socket (`0600` + `SO_PEERCRED`) with a nonce/ACL handshake. The
router, the handlers and the frame protocol stay put — only the
listener/connector changes.

## Depends on

`serde`, `serde_json`, `thiserror`.

## Consumed by

`arbor-shell-common` (the router); future product `*-ipc` clients and `*-be`
backends.
