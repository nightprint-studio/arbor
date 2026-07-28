# picus-inventory

An index of every database object a script repository names: **where it is defined, where it is
referenced, and how many statements touch it in each folder**.

It is what the inventory view renders, and it is where the first consistency rule comes from.
The cell that matters is the **zero**: `PARAMETRI` covered once in `ORACLE/AGGIORNAMENTO` and
zero times in `POSTGRES/AGGIORNAMENTO` means one dialect does something the other does not, and
that is the entire reason Picus exists.

## Pure by construction

Files in — already read, already decoded, already parsed — an index out. No filesystem, no
clock, no database. Reading belongs to the caller, which is what lets the same code index a
repository on disk and an unsaved editor buffer, and what keeps the crate wasm-clean
(`docs/picus-design.md` §6.4).

```rust
use picus_inventory::prelude::*;
use picus_parse::prelude::{EngineKind, SqlParser};

let mut parser = SqlParser::new();
let parsed = parser.parse(&source, EngineKind::Oracle);

let scripts = vec![ParsedScript { path: "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                                  source: &source, parsed: &parsed }];
let joined    = ParsedProject::new(&project, scripts);   // `project` from picus-project
let inventory = Inventory::build(&joined);

inventory.wire();        // the shape `InventoryObject` in the frontend reads
inventory.objects;       // the same, plus every site — what picus-analyze reads
joined.orphans();        // parses whose path is not in the tree: a caller bug, never silent
```

`ParsedProject` is the join of "what was parsed" to "where it sits", and it is deliberately the
input type of `picus-analyze` too. A rule that had to re-derive a file's folder — and so its
dialect and its role — would be a rule
that could derive it differently.

A `Placement` answers from **two levels**, and which is which matters: the **role** is the folder's
(a directory of scripts is *for* something, and the file beside this one is for the same thing),
the **engine** is the file's — which for all but a handful of files is its folder's anyway. The
exceptions are the untidy repositories where both engines share a directory and only the file name
knows which is which.

## The identifier-case rule

**Unquoted names fold to UPPER CASE; quoted names keep their contents exactly; the schema
qualifier is dropped.**

This is the one decision the whole crate rests on, so it is worth the paragraph.

Oracle folds an unquoted identifier to upper case and PostgreSQL folds it to lower case, which
means the *same* table is written `PARAMETRI` in one dialect and `parametri` in the other and is
one object all the same. Comparing the written form would report every object in the repository
as missing from one side — the tool would produce nothing but noise on its first run. So both
fold to a single case, and **upper** is the one, because that is what Oracle's own data
dictionary stores and Oracle is the side whose names are written by hand.

A **quoted** name is not folded, in either direction. `"parametri"` and `parametri` are
genuinely different objects to both engines, and collapsing them would hide a real bug rather
than reveal one. (The fold itself lives in `picus-parse`'s `ObjectRef::folded_name`, so the
parser, the inventory and the rules cannot disagree about it.)

The **schema qualifier is dropped** for the same reason the case is folded: the Oracle scripts
qualify with the owning user, the PostgreSQL ones with `public`, both inconsistently and
usually not at all. Keying on `APP.PARAMETRI` versus `public.parametri` would make one object
into two and every row would look half-missing.

## What is indexed, and what is not

`InventoryKind` is exactly the frontend's `ObjectKind` union: table, view, sequence, package,
procedure, function, trigger. Indexes, types, constraints, columns, tablespaces and the rest are
**not** rows — they are parts of something else, and giving each one a row would bury the four
hundred that matter under four thousand that do not.

Two foldings, and one refusal to fold:

| Written | Row | Why |
|---|---|---|
| `MATERIALIZED VIEW` | `view` | The two dialects spell the same object differently; a maintainer wants one row |
| `PACKAGE BODY` | `package` | A spec and a body of the same name are one object to a human |
| `PACKAGE` spec vs body | **distinct** on the site | `ObjectSite::declared_kind` keeps the exact kind, so "spec here, body there" is not read as one object defined twice |

## Counting

A coverage cell is **the number of statements** that touch the object in that folder, not the
number of times it is mentioned. A statement naming `PARAMETRI` four times has done one thing to
it, and counting mentions would make a verbose folder look better covered than a terse one.

Both definitions and references count as touching: an `INSERT INTO PARAMETRI` in an update
folder is exactly the coverage `CONS001` is looking for.

A column is keyed by the folder's **project-relative path** (`AGGIORNAMENTO/2024/ORA`), which is
a folder's identity everywhere else in Picus too. Every folder that holds scripts gets one and
every one of them is seeded to zero before anything is counted, including folders whose files
were never parsed — a column that only appeared once something was found would make *"this
folder has none"* indistinguishable from *"nothing looked here"*, which is the one failure a
consistency tool must not have. Folders that hold only other folders get no column: a cell that
can never be anything but zero is noise in a table whose point is that a zero means something.

**Except where one folder holds more than one engine.** With the engine on the file, a directory
can hold `4_12_ORA.sql` beside `4_12_POS.sql`, and keying that on the path alone would add the
Oracle statements to the PostgreSQL ones and destroy the one comparison the table exists to make.
Such a folder yields **one column per engine**, with the engine named in the header —
`AGG · Oracle`, `AGG · PostgreSQL`, and `AGG · unclassified` for the files nobody has classified
yet (named rather than blank: a column headed with nothing reads like a rendering bug). Files in an
engine Picus does not read are left out of the count that decides this, so a stray T-SQL script
does not split the Oracle folder around it.

Tidy repositories — the overwhelming majority, and every repository that existed before a file
could carry an engine — are unaffected: one engine per folder means one column per folder, spelled
byte-identically to before. `Placement::coverage_key` and `ParsedProject::coverage_keys` go through
one function for that rule, because two implementations of it would give the table a column nothing
counts towards and lose one that does.

`ALTER` defines an object but does not **create** it (`ObjectSite::creating`). A table created
in the initialisation folder and altered by three update scripts is an ordinary repository, not
a table defined four times.

## Testing

```bash
cargo test -p picus-inventory
```

The tests are about the false positives, not the happy path: an Oracle `PARAMETRI` and a
PostgreSQL `parametri` becoming one row, a quoted name staying separate, a qualified name not
splitting a row in two, a four-mention statement counting once, and an INSERT three blocks deep
inside `DECLARE … BEGIN … END` still reaching the index.
