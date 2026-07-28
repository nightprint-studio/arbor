# arbor-sql-abbrev

An Emmet-like abbreviation language for SQL.

```
s#localstrings(keycode,value)[keycode='ita']
→ select keycode, value from localstrings where keycode = 'ita'
```

## Why this is not a snippet engine

The keystrokes are not the point. A snippet engine saves keystrokes, needs no crate, and every
editor already has one. The point is that the host **has the schema**, so the expansion knows
things a snippet cannot:

- **where the quotes go**, from the column's type — `007` in a `varchar` account code keeps its
  leading zeros, `15` in a `numeric` column does not gain quotes, and a column the host never
  classified is quoted, because that is the answer that fails safely;
- **what a join is `ON`**, from the foreign key — `s#ordini>clienti` reads the condition out of the
  constraint, and refuses when there is more than one to read;
- **that a name is wrong**, and often which name was meant.

An expansion a text snippet could have produced is not worth a crate. One only a schema-aware tool
could produce is the whole of this one. Every feature below is there because it needs the schema;
nothing here is a formatting preference.

## `expand` returns an intent, not text — and that is the whole shape of the crate

`expand()` gives back a **`Statement`**: fully resolved, every identifier spelled the way the schema
spells it, every join carrying the foreign key's columns, every value paired with the `ValueKind`
that decides its quoting. Turning that into characters is the **host's** job.

That split is why the crate lives in `foundation/` rather than inside a product. A host may already
own a deterministic emitter it cannot be asked to bypass: **Picus routes `i#` and `u#` through
`DmlModel` → `picus-emit`**, so that quoting, identifier casing and the Oracle/PostgreSQL
differences stay in the one place that already owns them, and one abbreviation can produce both
dialects. If this crate returned a `String`, Picus could not use it for two of its four verbs, and
the second-best version of the feature would be the only one available.

`render(&Statement, &RenderStyle)` is provided for hosts that have no emitter — Picus uses it for
`s#` and `d#` and ignores it for the other two. `RenderStyle` covers keyword case, identifier case,
the quote character, the `INSERT` placeholder and an optional terminator, and **nothing about
dialect**: a host that needs `SYSDATE`-versus-`now()` has an emitter and is not reading this.

## Refuse rather than guess

Every failure is a sentence a person can act on, and there is no such thing here as a plausible
approximation. That posture is what makes the feature safe to point at somebody's production
database:

| Situation | What happens |
|---|---|
| No foreign key between two tables | refused, naming both — **never** a `1=1` or a name-matching heuristic |
| Two foreign keys between them | refused, naming the candidate **columns** and the syntax that picks one |
| A column in two of the chain's tables | refused, naming both — not bound to whichever was typed first |
| `u#`/`d#` with no `[...]` | refused: four characters is far too few to have typed to mean *every row* |
| An unknown table or column | refused, with the nearest name **when there is exactly one** near enough |

The last row is the whole rule in miniature: a suggestion that ties with another is not offered,
because a suggestion that might be wrong costs more than none — the user reads it, tries it, and is
now two mistakes from where they started.

There is deliberately **no opt-in spelling** for an `UPDATE` or `DELETE` over every row. `d#t[]` was
considered and rejected: two spellings for the same danger, and the way to write that statement is
to write it.

## The grammar

```
<verb> '#' <table> <chain>* <cols>? <preds>? <mult>?

<chain> ::= '>' <table> (':' <column>)?
<cols>  ::= '(' name ('=' value)? (',' ...)* ')'
<preds> ::= '[' name op value (',' ...)* ']'
<mult>  ::= '*' digits
```

| Verb | Means | `>` chain | `(...)` | `[...]` | `*n` |
|---|---|---|---|---|---|
| `s` | SELECT | yes | column names, no values; `*` if absent | optional, any operator | — |
| `i` | INSERT | — | names, value optional per column; **all columns** if absent | — | rows, 1..=1000 |
| `u` | UPDATE | — | `name=value`, **required** | **required**, any operator | — |
| `d` | DELETE | — | — | **required**, any operator | — |

Operators: `=`, `!=` / `<>`, `<`, `<=`, `>`, `>=`, and `~` for `LIKE`. `>` is only a chain arrow
outside `[...]` and only an operator inside it, which is why the chain is parsed before the
brackets are ever opened.

`i#t(a='x',b)` mixes a value and a placeholder on purpose — `InsertColumn::value` is an `Option`, so
"no value given" and "given, and empty" are different answers. With `*n`, **every row carries the
same values**; that sounds like a bug and is not, it is what a seed-data user types before editing
the rows apart.

An `UPDATE`'s `[...]` takes **any** operator. "The `WHERE` must be a key equality" is a fact about
one host's model — Picus's `DmlModel` — and belongs to that host, not to the language: it refuses
`u#ordini(codice='x')[quantita>5]` where it maps this, and the next host is not made to inherit a
restriction it does not have.

A value is quoted or bare. **A quoted one is left alone** — quoting was an explicit statement of
intent and outranks the column's type in both directions. A bare one is decided by the type, with a
closed list of SQL keywords (`NULL`, `SYSDATE`, `NOW()`, `CURRENT_TIMESTAMP`, …) that are never
quoted whatever the column is. A bare value may contain balanced parentheses, so `now()` and
`coalesce(a, upper(b))` survive inside `(...)`, where `)` would otherwise close the column list on
the most obvious default anybody could type.

Aliases are generated when — and only when — there is more than one table: first letter,
deduplicated, so `ORDINI`, `CLIENTI`, `CLIENTI_FATT` become `O`, `C`, `C2`. Deterministic by
construction, because an alias appears in text the user reads and edits, and one that moved between
two runs of the same abbreviation would be worse than no alias at all. It follows the table's own
case, so a lower-case schema does not get SQL with one capital letter in it.

## One parse, two questions

`expand()` and `context_at()` go through the same `parse()`, and that is a structural decision
rather than an economy. The parser **never fails**: it records a syntax error and keeps a slot at
every position it reached, so `s#ordini>` has an empty table slot at offset 9 and the caret there
has an answer. A second, more forgiving parser written for completion is the failure mode this
crate is shaped to prevent — two parsers drift, and the day they disagree the editor offers a column
for a table the expansion is not going to use.

`context_at` consults **no schema**. It answers with the text as typed (`JoinTable { from: "ordini",
prefix: "cli" }`) and the caller resolves it, because the caller already has the schema and keeping
it out of here is what lets a completion run on every keystroke.

## What this does not do

Limits worth meeting here rather than in an error message:

- **No schema-qualified table names.** `SchemaView` is a flat list, so `public.localstrings` is a
  syntax error. Accepted as a limit — a host visible on several schemas folds the qualifier away
  before building the view, which is what Picus does everywhere else already.
- **No dialect, anywhere.** No `SYSDATE`-versus-`now()`, no upsert, no engine-specific quoting.
  Identifiers and values come out as the schema and the user spelled them.
- **The keyword list is closed**, so a function call that is not on it is *quoted* against a text
  column rather than passed through — `to_date(…)` into a `varchar` comes out as a string literal.
  It is the safe direction and it is visible in the preview; deciding whether something "looks like"
  an expression is how a user's literal text eventually gets written unquoted.
- **A chain link joins its immediate predecessor**, never an earlier table. `a>b>c` joins `c` to
  `b`, and refuses if those two are unrelated even when `c` relates to `a`.
- **`Foo` beside `foo`** in one schema resolves to the exact match if there is one, and to the first
  case-insensitive match otherwise. Deterministic, not refused.

## Adopting it: what a host has to do

1. Build a `SchemaView` from what it already knows about the connection — table names, column names,
   a `ValueKind` per column, and the foreign keys. There is **no trait to implement**; it is a
   `map` over the host's own schema type.
2. Call `expand(input, &schema)` and either read the `Statement` or hand it to `render`.
3. Call `context_at(input, caret)` from the completion handler and turn the answer into a list.

That is the whole contract. Mapping a native type name (`character varying(30)`, `NUMBER(10,2)`,
`timestamptz`) to one of five `ValueKind`s is deliberately the host's problem: the host is the one
that knows which engine reported the type, and a type *name* is exactly the thing that differs
between engines. What the crate promises in exchange is that quoting depends on nothing else.

**Build the `SchemaView` once per connection and keep it.** Nothing here mutates it and nothing
caches on the host's behalf, so a host that rebuilds it per keystroke pays for the whole schema per
keystroke. That is the one performance note this crate has.

## Layout

| Module | Holds |
|---|---|
| `schema` | `SchemaView` / `TableMeta` / `ColumnMeta` / `ForeignKeyMeta` / `ValueKind` — what the host supplies |
| `syntax` | the abbreviation as typed: `Verb`, the blocks, the slots, nothing resolved |
| `parse` | the one parser — tolerant, never fails, spans everywhere |
| `resolve` | names → schema entries, alias generation, the near-miss suggestion |
| `join` | `>` → a foreign key, and the two refusals when it cannot be one |
| `expand` | parsed + schema → `Statement` |
| `statement` | the resolved intent, and `Value::needs_quotes` — the question the crate exists to answer |
| `render` | the default renderer, for hosts without an emitter |
| `context` | `CursorContext` — what is under the caret |
| `error` | `AbbrevError`, whose `Display` strings are the contract |
| `span` | byte spans and slots, including the empty ones |

## Public API: use the prelude

Workspace convention — reach the surface through `arbor_sql_abbrev::prelude::...`. In practice a
host needs four names: `SchemaView`, `expand`, `context_at`, `AbbrevError`.

## Testing

Everything is pure — text and a data struct in, a statement or a refusal out — so there is nothing
to mock and no fixture on disk. Two things are asserted as contracts rather than as behaviour:

- **the refusal messages**, because the message *is* the contract — it crosses the host's seam as
  text and lands in front of the person typing;
- **the JSON**, because `Statement` and `CursorContext` reach a TypeScript frontend, so a renamed
  variant is a breaking change to code this crate cannot see.

The fixture schema is Italian on purpose: the abbreviations this has to survive are the ones real
users type, which means accented values, account codes with leading zeros, and two foreign keys from
the same table to the same other one.

```bash
cargo test -p arbor-sql-abbrev
```
