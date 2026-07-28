# picus-types

The vocabulary both halves of **Picus** share.

Picus is a database client *and* a maintainer of the scripts those databases are
installed from, and the two meet in the generator: the schema read from a live
connection feeds the DML written into the scripts. A handful of types therefore
have to be the same on both sides — a column's type as the server reported it *is*
the type the generated statement must respect.

| Module | What it holds |
|---|---|
| `kind` | `EngineKind` — the engine a connection speaks and the dialect a folder is written in. **One** type, because those must never drift apart. Plus `ForeignEngine` / `FolderEngine` — see below. |
| `role` | `FolderRole` — what a folder of scripts is for. Discovered by the script half, read by the generator half. |
| `schema` | `Column`, `TableInfo`, `ForeignKey`, `IndexInfo`, `SequenceInfo`, `TriggerInfo`, `SchemaSnapshot` — serialised camelCase to match `src/lib/types/picus/index.ts` field-for-field. |

## Four engine states

A folder is not simply "a Picus dialect" or "unclassified". `FolderEngine` is the one slot a
folder's engine lives in, and it has four answers:

| State | Means | What happens |
|---|---|---|
| `Some(Supported(_))` | Oracle / PostgreSQL | read, parsed, analysed, generated into |
| `Some(Generic)` | **portable** SQL, valid on both | parsed against both, counts for both, generated into with the intersection |
| `Some(Unsupported(_))` | recognised, unsupported | named on screen, **never asked about, never parsed** |
| `None` | nobody knows | the interface asks |

One wire word per value in one key: `dialect = "oracle"`, `"generic"`, `"sqlserver"`. The serde
is hand-written rather than `untagged` because a unit variant would otherwise spell itself
`null`, and `null` already means "nobody knows" here.

The same slot is a **file's** engine too. In an untidy repository the engine is on the file rather
than on the directory — `4_12_ORA.sql` beside `4_12_POS.sql` — so `picus-project`'s `ScriptFile`
carries a `FolderEngine` on exactly the same terms as `FolderNode` does. One type with four
answers, asked at two granularities, is what keeps a file and its folder from ever disagreeing
about what *portable* means.

`ForeignEngine` is a separate type from `EngineKind` on purpose: `EngineKind` is what a
driver connects with and what an emitter writes, and folding the two would give every
`match` an arm claiming Picus can emit T-SQL.

`Generic` is **never inferred** — a promise that these scripts run on both engines is something
a person makes, not something a folder name implies.

## `DialectScope`: two dual questions

`FolderEngine::scope() -> Option<DialectScope>` is the single bridge from "what a folder is" to
"what may be parsed and emitted". `DialectScope` is `One(EngineKind) | Portable` — with **no**
unsupported member and **no** unknown one, so a parse or generation target in such a folder is
unrepresentable rather than merely unchecked.

Two predicates hang off it, and the whole portable feature lives in the gap between them:

| | `covers(d)` — "does content here count for `d`?" | `permits_syntax_of(d)` — "may syntax specific to `d` appear?" |
|---|---|---|
| `One(Oracle)` | Oracle only | Oracle only |
| `Portable` | **both** | **neither** |

The first is why a portable folder is in every lane and can never be reported as a gap. The
second is why `DIA001` inverts there: a construct belonging to *either* engine is a broken
promise.

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
