# arbor-shell-common

The Arbor shell runtime for Model D (1 FE + N BE).

## Purpose

The shell process owns the single WebView and is the only thing that talks to
both the product backends and the OS keyring. This crate holds the two
shell-side responsibilities, kept separate from any one product:

- **Router** — maps a FE `invoke` to the right backend over `arbor-ipc`.
- **Credential broker** — the *sole* keyring holder, with an in-memory
  access-token cache (Model D §D.5).

See [`docs/crate-refactor-round2.md`](../../../docs/crate-refactor-round2.md)
§D.5 and [`docs/migration-roadmap.md`](../../../docs/migration-roadmap.md) (M1c).

## Public API: use the prelude

Reach the surface through `arbor_shell_common::prelude::...`:
`Router`, `RouterError`, `CredentialBroker`, `BrokerError`.

## Contents (M1c skeleton)

- **`router`** — `Router`: a registry of one `BrokerClient` per product
  (keyed by id, e.g. `"corvus"`) plus dispatch. In-process today, pipe/socket
  later — unchanged across the flip because it only sees `BrokerClient`.
- **`broker`** — `CredentialBroker`: keyring-backed, caches short-lived access
  tokens in memory (refresh secrets stay in the keyring) with a TTL,
  invalidation on 401/403, and `zeroize`-on-drop. Tokens never leave the broker.

## Not here yet

The host WebView2 / window-management / single-instance / deep-link pieces, and
relaying backend push events to the FE as Tauri events, fold in as the shell
takes over from `src-tauri` (M3). The broker's real refresh→access exchange
(an HTTP round-trip) plugs into `access_token` where the skeleton currently
returns the stored secret directly.

## Depends on

`arbor-ipc` (router), `keyring` + `zeroize` (broker), `thiserror`.

## Consumed by

`bins/arbor` (the shell binary), from M3.
