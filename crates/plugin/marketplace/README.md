# arbor-plugin-marketplace

Curated and custom plugin / theme catalog, installer (ZIP extraction +
manifest validation), and on-disk cache.

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

## Public API

Everything is reached through [`crate::prelude`] per the workspace
convention. Sub-modules are `pub` for rustdoc navigation, but call sites
use the prelude.

## Contents

| Module          | Responsibility                                                                                |
|-----------------|-----------------------------------------------------------------------------------------------|
| `types`         | DTOs (`MarketplaceCatalog`, `MarketplacePlugin`, `MarketplaceTheme`, `RegistryEntry`, …)      |
| `paths`         | Canonical FS locations for every on-disk state file / dir (single source of truth)            |
| `error`         | `MarketplaceError` with domain variants (`PinMismatch`, `InvalidArchive`, `InstallCollision`) |
| `host`          | `MarketplaceHost` trait — local-plugin / dev-dir reflection injected by the shell             |
| `github_api`    | Shared HTTP client + URL helpers + `resolve_ref_sha` / `verify_pinned_sha`                    |
| `index`         | `index.json` shape, External vs Internal entries, `fetch_catalog`                             |
| `fetch`         | Leaf `fetch_plugin` / `fetch_theme` (+ icon / HTML doc resolution)                            |
| `custom`        | 3-mode resolver for user-added GitHub sources                                                 |
| `installer`     | Zipball extraction, theme JSON writer, uninstall                                              |
| `cache`         | TTL-checked community cache + custom-source cache (read / write / invalidate)                 |
| `installs`      | `marketplace_installed.json` ledger (per-name install + enable state)                         |
| `user_registry` | `user_registry.toml` source pointers (composite key `repo + subpath`)                         |
| `registry`     | `MarketplaceRegistry` — in-memory state + `catalog()` merge with local sources                |
| `refresh`       | Async helpers: `refresh_community`, `add_custom_source`, `remove_custom_source`               |

## Depends on

- `arbor-core` — `paths`, `CoreError`.
- `arbor-plugin-types` — `Manifest`, `Permissions`, `Dependency`.

External: `reqwest`, `serde`, `serde_json`, `tokio`, `futures-util`,
`zip`, `toml`, `semver`, `tracing`, `thiserror`.

The crate intentionally does **not** depend on `arbor-plugin-core`, on
the Tauri shell, or on `arbor-scheduler`. Local-plugin discovery and
enable-state reads happen through the [`MarketplaceHost`](crate::host)
trait so the shell can wire those in without dragging the runtime into
the catalog layer. Auto-refresh scheduling stays in the shell crate
(`AppHandle` access).

## Consumed by

- `arbor` (Tauri shell) — wires `MarketplaceHost`, exposes Tauri
  commands for the FE's Marketplace modal, installs the auto-refresh
  trigger against `arbor-scheduler`.

## Notes

- The installer does NOT load the installed plugin into the runtime.
  It writes files to disk; the host re-scans on the next reload /
  restart. That's on purpose — keeps the install path testable without
  spinning up an mlua VM.
- Doc HTML and icon SVG are inlined into the community cache as part of
  every `MarketplacePlugin` — the modal renders them without re-fetching.
- Per-entry failures during catalog resolution are logged and dropped
  rather than propagated: a single broken submission can't blank the
  catalog.
