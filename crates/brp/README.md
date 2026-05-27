# arbor-brp

Bevy Remote Protocol client: JSON-RPC over HTTP plus an SSE watch stream
for incremental updates.

## Purpose

Today `src-tauri/src/brp/{client,sse}.rs` ships a small client used by an
internal feature targeting Bevy game projects. It doesn't depend on
anything else in `src-tauri/` beyond `AppError`, so it carves out cleanly.

It also has a clear **future**: the Lua plugin runtime will eventually
gain enough `arbor.http` + `arbor.sse` surface that this client could be
rewritten as a plugin. Isolating it as a crate today means that day is a
one-shot delete, not a refactor.

## Contents (planned)

- `BrpClient` — JSON-RPC over HTTP. Endpoints: query, get, get_resource,
  list, mutate. Auto-reconnects on transient transport errors.
- `BrpWatch` — Server-Sent Events stream subscribed to a BRP server.
  Disconnect-handling follows the project rule: silent retry, single
  toast on first failure after retry, single regain toast, single
  give-up toast at end of MAX_RETRIES (`disconnect_notified` bool gates
  both).
- DTOs for the Bevy-side responses (`Entity`, `Component`, `QueryResult`).

## Depends on

- `arbor-core` — `http::builder`, `AppCtx` (for emit on watch events),
  `AppError`.

External: `reqwest`, `serde`, `serde_json`, `futures-util`, `tokio`,
`tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — for now, until the Lua port lands. The shell
  wires the BRP commands and forwards watch events to the frontend.

## Notes

- This crate is **earmarked for deletion** once the Lua runtime gains
  full BRP coverage. Treat any new API surface here as throwaway:
  prefer minimal cosmetic changes over deep refactoring.
- The current `connect_timeout = 5s` + no overall timeout on SSE is
  intentional — once connected the stream may stay open for hours.
