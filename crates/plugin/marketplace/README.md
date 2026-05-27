# arbor-plugin-marketplace

Curated and custom plugin / theme catalog, installer, and auto-refresh.

## Purpose

The marketplace is a self-contained subsystem that:

1. Fetches the curated `arbor-extensions` `index.json` from GitHub raw +
   each entry's `plugin.toml`, builds a catalog.
2. Resolves user-added custom sources (single plugin at root, single
   plugin at subpath, or multi-plugin `index.json` at root).
3. Optionally verifies entries pinned with `pinned_sha` against the
   GitHub commits API to defend against tag-hijack on third-party repos.
4. Installs entries by extracting a downloaded ZIP into the plugin /
   theme dir, validating the manifest, and writing an entry to
   `marketplace_installed.json`.
5. Wakes periodically to refresh the cache (or every modal-open
   triggers a refresh).

Most of the duplication discussed at the start of the refactor lives
here: two anonymous `struct Resp { sha: String }` for the GitHub commits
endpoint, six `dirs::config_dir().…join("arbor")` callsites, an HTTP
client built ad-hoc next to the same one in `integrations/`, and an
entire scheduling loop reimplemented from the plugin runtime.

After the split:

- the SHA helpers come from `arbor-core::gh_api::resolve_commit_sha`,
- the HTTP client from `arbor-core::http::builder`,
- the paths from `arbor-core::paths`,
- the scheduler from `arbor-scheduler`.

## Contents (planned)

- `types` — `Catalog`, `Plugin`, `Theme`, `RegistryEntry`,
  `MarketplaceSource`, `ThemeVariant`.
- `fetcher` — index.json + `plugin.toml` resolution. Currently 581 lines
  in `src-tauri/src/marketplace/fetcher.rs`; will be split into
  `index.rs` + `fetch.rs` + `custom.rs` here.
- `installer` — ZIP extract + manifest validation + install state file.
- `cache` — disk-backed list snapshot (post-refactor: **list-level
  fields only**, no inlined SVG icons, no doc HTML — those fetch on
  detail open). Per the discussion: offline = blank modal is acceptable;
  no need for MBs of local cache.
- `scheduler` — thin wrapper that registers one `FixedDelay` entry with
  `arbor-scheduler` when `marketplace.refresh_hours` is set.
- `MarketplaceConfig` — `refresh_hours`, `poll_minutes` (deprecated by
  the scheduler refactor), `registry_repo` override for forks,
  `custom_sources` (also persisted in `user_registry.toml`).
- `MarketplaceError` — `Network`, `Parse`, `PinMismatch`, `NotFound`,
  `IndexTooLarge`, `Other`. Maps to `AppError` at the `arbor` boundary.

## Depends on

- `arbor-core` — `paths`, `http::builder`, `gh_api`, `AppError`,
  `AppCtx`.
- `arbor-scheduler` — refresh trigger.
- `arbor-plugin-types` — `Manifest`, `Permissions`, `Dependency`.

External: `reqwest`, `serde`, `serde_json`, `tokio`, `futures-util`,
`dirs`, `zip`, `toml`, `semver`, `tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — exposes `marketplace_*` Tauri commands; the
  Marketplace modal in the frontend talks to those.
- (none else internally)

## Notes

- The installer does NOT depend on `arbor-plugin-core`. It writes files
  to disk; the runtime loads them on next restart or hot-reload. That's
  on purpose — keeps the install path testable without spinning up an
  mlua VM.
- The "list cache" / "detail-on-open" rule is a deliberate UX choice:
  offline users see only the plugins they have installed. The
  marketplace tab assumes connectivity.
- Doc HTML and icon SVG are NOT cached on disk anymore. Icons stay
  inline in RAM only (after the first fetch). Docs fetch on click.
