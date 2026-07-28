# picus-core

The headless backend core for **Picus** (the SQL studio) — the picus twin of
[`bennu-core`](../../bennu/core) / [`tyto-core`](../../tyto/core). Owns the
canonical `PicusState` the `picus-be` process holds; **Tauri-free by
construction**.

`PicusState` holds the BE→FE event egress, the reverse channel back to the shell,
and the two things whose lifetime *is* the backend process: the open database
sessions and the script repositories read so far. The studio's real work — driver
sessions' protocol, statement parsing, per-dialect emission, script rewriting —
lives in the leaf crates the picus-be domain handlers drive.

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
fail the file's parse and silently reset every other setting. `InsertionRule`
itself is defined in [`picus-project`](../project) — the same setting can be stated
by the repository, and the repository's answer wins — and re-exported here so the
two tiers cannot disagree about what `"end-of-file"` means.

## The script read cache

`ScriptCache` (`src/scripts.rs`) holds one `ScriptSnapshot` per repository: the
tree, its configuration, and **every script decoded once**, each entry carrying the
SHA-256 of the bytes it came from (`src/digest.rs`).

Three properties are load-bearing:

- **Nothing expires on its own.** A snapshot lives until an explicit refresh or a
  write replaces it. A consistency report that changes while nobody changed
  anything is a report people stop believing.
- **The parse is not in here.** A `ParsedFile` is a map of a string the caller
  owns, so caching one beside its own source is self-referential; parsing happens
  inside the call that needs it, in one function in picus-be.
- **Entries are content-addressed.** The digest is what a future on-disk parse tier
  would key on, and the reason nothing in this module would have to change for it
  to exist.

The digest is also the staleness signal the two-call apply rests on, which is why
it is SHA-256 and not a cheap hash.

## Public API

Reach this crate through the [`prelude`](src/prelude.rs): `picus_core::prelude::*`.
