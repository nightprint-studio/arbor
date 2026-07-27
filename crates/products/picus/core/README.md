# picus-core

The headless backend core for **Picus** (the SQL studio) — the picus twin of
[`bennu-core`](../../bennu/core) / [`tyto-core`](../../tyto/core). Owns the
canonical `PicusState` the `picus-be` process holds; **Tauri-free by
construction**.

`PicusState` is transport-only: BE→FE event egress + the reverse channel back to
the shell. The studio's real work — driver sessions, statement parsing, per-dialect
emission, script rewriting — lives in the leaf crates the picus-be domain handlers
drive; this crate keeps no such state.

The reverse channel is load-bearing here, not incidental: **Picus stores no
password**. A connection's secret is resolved through the shell's credential broker
at the moment of use, over that channel.

## The invariant

**The dialect is a property of the folder, never a global "current dialect."**
Nothing in this crate holds an ambient dialect, and nothing in `picus-be` should:
every function that parses, analyses, generates or rewrites SQL takes it as an
explicit parameter. See [`docs/picus-design.md`](../../../../docs/picus-design.md) §1.

## Config

`PicusConfig` (encoding fallbacks, write guards, emission defaults, query row
limit) persists to the per-profile `arbor/profiles/<active>/picus/config.toml`,
round-tripped through `toml`. The path is resolved by picus-be itself via
`arbor_core::prelude::picus_config_path` — not pushed by the shell. `load()` is
infallible-by-design (defaults on a missing/corrupt file).

A **script project's** own settings — its declared encoding, line ending and
version table — deliberately do *not* live here. They belong to the project, so a
colleague opening the same repository inherits them.

The insertion rules are stored as wire strings rather than as a serde enum, and
read through the typed accessors: an unknown value must degrade to the default, not
fail the file's parse and silently reset every other setting.

## Public API

Reach this crate through the [`prelude`](src/prelude.rs): `picus_core::prelude::*`.
