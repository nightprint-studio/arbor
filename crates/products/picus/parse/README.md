# picus-parse

One permissive Tree-sitter grammar covering **both** dialects Picus maintains —
Oracle (SQL + PL/SQL) and PostgreSQL (SQL + PL/pgSQL) — and a thin,
byte-range-oriented reader over it.

## Why one grammar and not two

Picus keeps repositories in which the same logical change exists twice, once in
each dialect, and its job is to notice when the two drift apart. The two
dialects diverge almost always **by addition** and almost never by collision.

With two strict grammars, an Oracle-ism inside a PostgreSQL file is a parse
failure, and the best message available is *"syntax error at line 12"*. With one
permissive superset it is **a node with a name**, and the message becomes:

> `(+)` is Oracle's outer-join marker; PostgreSQL wants `LEFT JOIN … ON`

So the grammar's rule is: **a construct that exists in only one dialect gets its
own named node**, never a fold into a generic one. `src/dialect.rs` is the table
that turns those names into advice, and `Statement::foreign` is where a caller
finds them.

The dialect is a *parameter*, never a field: `parse(source, engine)`. There is no
ambient dialect anywhere in Picus (`docs/picus-design.md` §1).

## What it parses

To **full expression depth, everywhere**, including inside `DECLARE … BEGIN …
END` and PL/pgSQL bodies. That is not thoroughness for its own sake: in a real
Oracle upgrade script the INSERT that has to be checked for duplicate keys is
three blocks deep, so a parser that treated procedural bodies as opaque would see
nothing at all in the files this product exists to maintain.

Covered: SELECT with every join form (including Oracle's `(+)`), set operations,
CTEs and `WITH RECURSIVE`, `CONNECT BY` / `START WITH`, window functions, `CASE`,
subqueries, `FETCH FIRST` / `LIMIT` / `OFFSET`, `ROWNUM`; INSERT (with and
without a column list, and `INSERT … SELECT`), UPDATE, DELETE, `MERGE … USING …
FROM DUAL`, `ON CONFLICT DO UPDATE/NOTHING`, `RETURNING`, TRUNCATE; CREATE /
ALTER / DROP for table, view, materialized view, index, sequence, trigger,
schema, synonym, type, and Oracle packages / procedures / functions plus
PostgreSQL functions with `$$`-quoted bodies; constraints (PK, FK with
`ON DELETE` / `ON UPDATE`, unique, check, not null, defaults, identity); `COMMENT
ON`; `GRANT` / `REVOKE`; the procedural layer (`IF`/`ELSIF`/`LOOP`/`FOR`/`WHILE`,
cursors, `SELECT … INTO`, `EXECUTE IMMEDIATE`, `RAISE`, `%TYPE`/`%ROWTYPE`,
exception handlers, `DO $$ … $$`, `PERFORM`, `RETURN`).

### Known holes

Documented rather than pretended away:

- **Keyword-argument function syntax** — `TRIM(LEADING 'x' FROM y)`,
  `SUBSTRING(x FROM 1 FOR 2)`, `COLLATE`, and the quantified comparison
  `= ANY (…)` are not modelled. Each is another recursive expression rule, and
  together they cost about five megabytes of generated parse table for forms that
  barely appear in install scripts. `TRIM(x)`, `SUBSTR(x, 1, 2)` and
  `SUBSTRING(x, 1, 2)` all still parse — as ordinary function calls.
- **`DO $$ … $$` bodies are one token.** The dollar-quoted string is a literal,
  so the DML inside a PostgreSQL anonymous block is not visible from the outside.
  A caller that wants it re-parses the inner range with the same parser; the test
  suite demonstrates that flow.
- **SQL\*Plus directives** — `PROMPT`, `SPOOL`, `@file`, `DEFINE` — are not
  modelled. `SET` is, in a deliberately loose form.
- **`GRANT`/`REVOKE` object references** are parsed structurally but not
  extracted into `references`.
- **`(+)` must be written without spaces**; `( + )` is not accepted.
- `1.` (a number with a trailing bare dot) is not a number: accepting it would
  make the PL/SQL range `1..10` lex as `1.` `.10`.
- `NOT DEFERRABLE` on a constraint is not accepted, because a constraint keeps
  its trailing attributes greedily and admitting it would break `DEFAULT 0 NOT
  NULL` — the commonest column definition there is.
- Words that are keywords here cannot be used as unquoted identifiers unless they
  are in the `UNRESERVED` list in `grammar/keywords.js`. The list is kept short on
  purpose (every entry widens the lookahead set of every state); `COMMENT`,
  `OWNER`, `FILTER` and the sequence-option words are the plausible casualties.

## Layout

```
grammar.js              the grammar's entry point; merges the modules below
grammar/keywords.js     case-insensitive keyword tokens + the UNRESERVED list
grammar/lexical.js      names, literals, types — and the word token, declared LAST
grammar/expression.js   the expression ladder
grammar/query.js        SELECT, clauses, joins, set operations, CTEs
grammar/dml.js          INSERT / UPDATE / DELETE / MERGE / TRUNCATE
grammar/ddl.js          tables, views, indexes, sequences, constraints, DROP
grammar/routine.js      functions, procedures, packages, triggers, types
grammar/plsql.js        PL/SQL and PL/pgSQL blocks and statements
grammar/session.js      COMMENT ON, GRANT/REVOKE, SET, transaction control
src/scanner.c           the external scanner (hand-written)
src/parser.c            GENERATED, committed
src/*.rs                the Rust reader
test/corpus/*.txt       the Tree-sitter corpus (~230 cases)
tests/*.rs              the Rust API tests and the corpus-wide properties
```

## Two things that are load-bearing and easy to break

1. **`lexical` is merged LAST in `grammar.js`, and `identifier` is the last rule
   inside it.** SQL keywords are case-insensitive, so they are character-class
   patterns rather than string literals — which means tree-sitter's
   keyword-extraction machinery (`word`) cannot capture them, and `identifier`
   competes with every keyword in the main lexer. Tree-sitter breaks such a tie
   by, in order: explicit precedence, match **length**, then **declaration
   order**. Explicit precedence is unusable (it outranks length, so a `DATA`
   keyword would beat the longer identifier `DATA_MOD`), so declaration order is
   the tie-break, and every keyword must be declared before `identifier`. Move
   either and `SELECT 1 FROM t` silently reads `FROM` as a column alias.

2. **The statement terminator is optional**, because Picus parses live editor
   buffers. The cost is that "where does this statement stop" is not decided by a
   token, which is why so many rules carry `prec.right`: the policy everywhere is
   that a construct keeps going as long as it can, since stopping early
   truncates silently.

## Build workflow (Tree-sitter)

`grammar.js` is the source of truth. The generated `src/parser.c`,
`src/grammar.json`, `src/node-types.json` and `src/tree_sitter/` are **committed**,
and `build.rs` compiles them together with `src/scanner.c` through the `cc` crate.
A plain `cargo build` therefore needs no Node and no Tree-sitter CLI — only a C
compiler, which the workspace already requires for vendored libgit2 and mlua.
Same arrangement as `merula-lang`.

After editing the grammar:

```sh
tree-sitter generate      # regenerates src/parser.c and friends
tree-sitter test          # runs test/corpus
cargo test -p picus-parse # runs the Rust layer and the corpus-wide properties
```

and commit the regenerated files.

`src/parser.c` is large (tens of megabytes). That is the honest cost of a full
SQL + PL/SQL grammar with expression depth; it was brought down from 60 MB by
shrinking the unreserved-keyword list, collapsing the three parenthesised
expression forms into one, and dropping the keyword-argument function syntaxes.

## The invariant the rest of the script half depends on

`ParsedFile` is a **map of a string the caller still owns**. Nothing here stores
the source and nothing reconstructs text: every position is a `ByteRange` into
the original bytes. `ParsedFile::segments` enumerates statements and the gaps
between them, and

> concatenating every segment reproduces the input **byte for byte**

is asserted over the entire corpus, in both dialects, in `tests/corpus.rs`. It is
what lets `picus-rewrite` splice a statement and guarantee the rest of the file
survives untouched.

## Public API

Through `picus_parse::prelude` (workspace convention):

```rust
use picus_parse::prelude::*;
use picus_types::prelude::EngineKind;

let mut parser = SqlParser::new();          // reusable; keep one per folder scan
let parsed = parser.parse(source, EngineKind::Oracle);

for statement in &parsed.statements {
    statement.kind;         // Select / Insert / Create / Block / …
    statement.range;        // exact bytes, terminator included
    statement.defines;      // objects CREATE/ALTER defines
    statement.references;   // every other object it names
    statement.dml;          // every INSERT/UPDATE/DELETE/MERGE, including nested
    statement.foreign;      // constructs from the other dialect, with advice
}
parsed.errors;              // parse errors as data — never a panic, never a loop
```

Consumed next by `picus-inventory`, `picus-analyze` and `picus-rewrite`.
