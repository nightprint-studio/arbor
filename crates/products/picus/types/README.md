# picus-types

The vocabulary both halves of **Picus** share.

Picus is a database client *and* a maintainer of the scripts those databases are
installed from, and the two meet in the generator: the schema read from a live
connection feeds the DML written into the scripts. A handful of types therefore
have to be the same on both sides — a column's type as the server reported it *is*
the type the generated statement must respect.

| Module | What it holds |
|---|---|
| `kind` | `EngineKind` — the engine a connection speaks and the dialect a folder is written in. **One** type, because those must never drift apart. |
| `schema` | `Column`, `TableInfo`, `ForeignKey`, `IndexInfo`, `SequenceInfo`, `TriggerInfo`, `SchemaSnapshot` — serialised camelCase to match `src/lib/types/picus/index.ts` field-for-field. |

## What it deliberately is not

A leaf: `serde` and nothing more. No driver, no SQL, no I/O, no async.

That is the point. [`picus-db-api`](../db-provider/api) (the driver contract) and
[`picus-ast`](../ast) (the script model) both depend on this, and on each other not
at all — which is what keeps the script half free of any dependency on drivers.
That independence is not academic: it is what lets Oracle be a first-class engine
for scripts while having no Oracle driver.

Being a leaf also keeps it wasm-clean, and it is the slice a "generate SQL" plugin
would need.

## Public API

Reach it through the [`prelude`](src/prelude.rs) — though most code sees these
types re-exported from `picus_db_api::prelude` or `picus_ast::prelude`, whichever
half it is already working in.
