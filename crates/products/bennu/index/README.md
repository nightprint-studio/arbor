# bennu-index

★ **The product** (docs §1). A multi-source symbol / relation index over a lean,
low-RAM **mmap'd store**.

**Leaf crate**: no Bennu dependencies.

## Store

- **`fst::Map`** — name → `u64` offset (mmap'd). `levenshtein` feature gives fuzzy
  and prefix queries for free. Several maps per lookup axis (per-simple-name,
  per-fqn, per-file — docs §3) over one blob.
- **Framed `rkyv` blob** (mmap'd) — records accessed **zero-copy** on demand, with
  `bytecheck` validation on access (a corrupt mmap = a loud error, never UB).

Ported from the proven `bennu-spike-index` (2M symbols = 251 MiB on disk, queried at
~15 MiB working set — docs §10), with the two production hardenings it surfaced:

1. **16-byte-aligned records** — the writer pads every record to a 16-byte boundary
   (`store.rs`); a misaligned rkyv slice fails `bytecheck`.
2. **Format-version header** (`b"BNNUIDX0"` + `u64`) — `BlobReader::open` rejects a
   mismatch with `StoreError::VersionMismatch`, so the caller rebuilds from sources
   (rkyv 0.8.x has no migration — docs §4).

## Schema (docs §3)

Every `Symbol` / `Relation` carries a `source` tag (`ProjectSource`, `JdkBytecode`,
`TargetClasses`, `DepBytecode`, `StrutsAction`, `TldTag`, `SpringBean`, …): adding a
fonte is a new variant feeding the same table, not a rewrite.

Each `Symbol` also carries an opaque `members_json` string: for a type symbol it's the
analyzer-owned serialized member surface (supertypes + methods + fields), so a consumer
resolves a project type's members straight from the index without re-parsing its source.
The index stays a leaf crate — it treats the blob as an opaque string; only `bennu-intel`
knows the shape (a serialized `bennu_java::ClassMembers`). Adding it bumped the on-disk
`FORMAT_VERSION` to 2 (an old index is rebuilt on open, not misread).

## Query

`SymbolIndex` binds one fst map + one blob → typed `Symbol` reads:

- `get_exact(name)` — zero-copy access + `bytecheck` + deserialize.
- `prefix(prefix)` — every key with that prefix.
- `fuzzy(term, distance)` — Levenshtein-tolerant.

## Builder + incremental patch

`IndexBuilder` ingests `IndexRecord`s (a `Symbol` + the fst keys it's reachable under —
e.g. a class under both its simple and binary name) **grouped by source file**, then
flattens them to the mmap'd store:

- `set_file(path, records)` — set/replace one file's contribution.
- `patch_file(path, Some(records))` / `patch_file(path, None)` — replace or drop one
  file's rows and re-persist (incremental edit / delete) — no whole-project re-parse.
- `persist()` — write the fst + blob.

`PersistedIndex` is the read view the completion query serves from: `get(key)` (exact) +
`prefix(prefix)`, returning full `Symbol` records off the mmap. Leaf-clean: the builder
knows only records + keys; the analyzer above turns Java sources into them.

## Usage

```rust
use bennu_index::prelude::*;
```
