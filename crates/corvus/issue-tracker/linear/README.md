# corvus-issue-tracker-linear

The Linear implementation of the Corvus `IssueTracker` trait.

## Purpose

Implements `corvus_issue_tracker_api::IssueTracker` for Linear over its GraphQL
API — search / get / lookup / filter-options / transition / assign / comment /
create / image-fetch, plus the self-describing `ProviderDescriptor` (OAuth +
PAT auth methods, brand icon).

**Keyring-free.** The session (base URL + `Authorization` header) arrives through
an injected `Arc<dyn arbor_ipc::SessionProvider>`; `LinearTracker` holds an opaque
`account` path (the shell maps it to the keyring). On a `401` it asks the
provider to `refresh` and retries once. Only the shell ever reaches the keyring
or runs the OAuth dance.

## Public API: use the prelude

`corvus_issue_tracker_linear::prelude::{LinearTracker, validate_token, LINEAR_GQL}`.
`LinearTracker::new(session, account)` builds an `Arc<dyn IssueTracker>`;
`validate_token(token, base)` checks a token before the shell stores it.

## Tests

The pure pieces — identifier parsing, the search-filter builder (`#`/`~`/free
text), and the GraphQL-JSON → `Issue` mapper — are unit-tested (`cargo test -p
corvus-issue-tracker-linear`). The network round-trips are not unit-tested
(they're the injected-credential boundary).

## Depends on

`corvus-issue-tracker-api` (DTOs + trait), `arbor-ipc` (`SessionProvider`),
`arbor-core` (shared HTTP client), `async-trait`, `serde_json`, `reqwest`.

## Consumed by

`arbor` (the shell): registered in `src-tauri/src/integrations/registry.rs`, with
the keyring/OAuth adapter in `token_source.rs` and the command shim in
`linear.rs`.
