# picus-diff

The comparison engine. Two schemas, two sets of counts or two sets of rows in — one `DiffReport`
out.

```rust
use picus_diff::prelude::*;

let config = DiffTemplates::builtin().config_for("structure");
let mut report = DiffReport::new("production", "staging");

report.schema      = Some(compare_schema(&snapshot_a, &snapshot_b, &config));
report.indexes     = Some(compare_indexes(&snapshot_a, &snapshot_b, &config));
report.constraints = Some(compare_constraints(&snapshot_a, &snapshot_b, &config));
if !config.contents.enabled {
    report.skip(CheckKind::Contents, SkipReason::Disabled, "contents are off in this template");
}

let report = report.finish();   // ← computes the verdict
```

## Why it is pure

It opens nothing, reads nothing and waits for nothing: it is handed structures that have already
been read. That is not tidiness, it is the requirement — the same engine has to answer three
questions:

1. **database against database** — two snapshots read over two connections;
2. **database against the scripts that install it** — one snapshot from a server, the other
   *derived from a repository of SQL files* that no connection was ever opened to;
3. **one query's results against another's** — two `RowSet`s that never had a schema.

An engine that knew how to connect could only do the first. It is also what makes the crate
testable: composite keys, `1` versus `1.0`, a threshold that has just been crossed — all cases
that would otherwise need a database, and none of which do here.

The only dependency that is not `serde`/`thiserror` is `picus-types`, the leaf crate, for the
schema shapes. Deliberately **not** `picus-db-api`: that is the driver contract, and dragging it
in would tie a crate that must compare-without-a-connection to the trait that needs one.

## Typed cells, not rendered strings

The obvious way to diff two result sets is to render every cell to a string and compare the
strings. It is wrong in both directions at once, and which one depends on the driver: two drivers
rendering the same `NUMBER(10,2)` as `1.5` and `1.50` produce a difference that does not exist; a
driver rendering the integer `1`, the float `1.0` and the string `'1'` all as `1` hides two that
do; and `NULL` and `''` become one cell, which in a tool whose job is to help someone write an
`UPDATE` is the most expensive confusion available.

So `DiffValue` keeps the type, and the comparison never crosses variants. The one hand-written
piece of it is float equality: a stored `NaN` on both sides is two databases that **agree**, and a
column reported as different forever, with no edit that could make it stop, is a report people
learn to close.

## A skipped check is part of the verdict

The reason `DiffReport` is a type and not a bag of comparisons. A diff is used to decide whether
something is safe to ship, and *"identical"* is the sentence that decision is made on. If contents
were never compared because they are off, or a relation's indexes were not in the snapshot, then
"identical" is a lie of omission and the reader has no way to know.

Every check that did not run leaves a `SkippedCheck` behind, every comparison that ran over a
partial input reports `not_read`, and `finish()` produces one of three verdicts:

| Verdict | Means |
|---|---|
| `identical` | everything asked for was compared, and it all matched |
| `identicalWhereChecked` | nothing compared differs — but something was not compared |
| `different` | at least one difference |

Row differences follow the same rule: they are **counted in full** and *listed* up to the cap, so
`only_in_a.len()` is what you can show and `only_in_a_total` is what there is.

## What it compares

| Check | Reports |
|---|---|
| `schema` | relations only in A / only in B / changed; per relation, columns added, removed and changed (type, nullability, default, position); table↔view; optionally the view definition |
| `indexes` | presence and definition, keyed by `(relation, name)`; `not_read` for a snapshot that did not carry them |
| `constraints` | primary keys (by their columns) and foreign keys — **matched by definition** by default, because a constraint created without a name gets a generated one that differs in every database it was installed into |
| `sequences` | presence, drift in `last_value` against a threshold, and the definition attributes |
| `triggers` | timing, events (as a set), row-vs-statement, and **enabled** — the difference a schema dump will not show you |
| `counts` | row counts with warning/error percentage thresholds |
| `contents` | rows matched by key (or by position), with the differing cells |

## Configuration and templates

The relation filters are top-level, so every check looks through the same window: a per-check copy
would let a run compare the columns of a table whose rows it excluded and then call the pair
identical. Globs are `*` and `?` and nothing else — implemented here rather than pulled in, so
what an old template matches never changes under it.

Every check is separately switchable and every check has a `NameFilter` over its own objects
(`*_pkey` is an index name, not a table). A tuned configuration is not something anybody writes
twice, so it is nameable and storable: `DiffTemplate` / `DiffTemplates`, four shipped, no I/O —
where they are stored is the caller's decision.

By default the run reads the **catalogue only**. Counts and contents touch data and are opt-in: a
"quick diff" that quietly scans two hundred tables of somebody's production database is not quick
and was not asked for.
