# picus-analyze

The fourteen rules over a repository of SQL install scripts, plus the declared suppressions that
silence a finding **without hiding it**.

Pure: a `ParsedProject` (from `picus-inventory`), a `ProjectConfig` (from `picus-project`) and an
`Inventory` in — a `Report` out. No filesystem, no clock, no database.

```rust
use picus_analyze::prelude::*;
use picus_inventory::prelude::{Inventory, ParsedProject};

let joined    = ParsedProject::new(&project, scripts);
let inventory = Inventory::build(&joined);
let report    = analyze(&joined, &config, &inventory);

report.findings;              // suppressed ones included, and marked
report.skipped;               // rules that could not run, and why
report.rejected_suppressions; // comments somebody wrote that silence nothing
```

The inventory is a **parameter**, not something this crate rebuilds: the interface renders the
coverage table too, and building it twice would let the table and the findings be computed from
two different reads of the same repository.

## Two things every rule is held to

**`consequence` says what goes wrong in practice.** Never a restatement of the rule.

> *"It applies its changes and leaves VERSIONE_DB on the old value, so the next update refuses to
> start and the installation stalls one version behind — with the changes already applied."*

not *"update blocks should carry the version forward"*. A report whose messages are rule names is
a report people learn to close. There is a test that fails if a consequence repeats its title or
reaches for "should be".

**A rule that cannot run says so.** It goes into `skipped` with a reason. A rule that quietly
passes for lack of input is indistinguishable, in a report, from a rule that passed — and that is
the failure mode a consistency tool cannot have. `VER003` on a project whose filenames record only
the version they install is the case this list exists for.

## The rules

Two axes, and keeping them apart is the point. `CONS001` and `CONS004` compare **one dialect
against the other**; `CONS002` and `CONS003` compare **one dialect's initialisation against its own
updates**. `DIA001` compares nothing at all — it is one script that will not run.

The unit on both axes is a **lane**: `(dialect, role)` — the folders that play one role for one
dialect, wherever in the tree they sit and however many of them there are. `INIZIALIZZAZIONE/2024/ORA`
and `INIZIALIZZAZIONE/2025/ORA` are one lane and one install story; reading either alone would
report the other as a gap. A folder no ancestor declares a dialect for is in no lane and takes
part in nothing cross-dialect.

| Rule | Severity | Fires when | Deliberately does **not** fire when |
|---|---|---|---|
| `CONS001` | blocking | An object is touched by one dialect's lane at some role and by the other's not at all | The object is a **package** (Oracle-only); the folder has **no dialect**; the role exists for only one dialect |
| `CONS002` | blocking | A datum the `init`/`data` folders write is written by no `update` script of the same dialect | The updates never load that table at all; a value is **computed**; a row has no column list; the difference is only in a column one half never writes |
| `CONS003` | blocking | A datum an `update` script writes is written by no `init`/`data` script of the same dialect | Same three, mirrored |
| `CONS004` | blocking | Both dialects load the same table at the same role, with different columns or different rows | Any value is **computed** (`SYSDATE`, `now()`, a sequence) — the rows are then incomparable, not different |
| `DIA001` | blocking | A statement uses a construct belonging to the other dialect | The file was parsed as something other than its folder's dialect; the same construct repeats in one statement (one finding, not four) |
| `VER001` | blocking | An `update` file changes something and never **reads** the version table | The folder is not `update`; the file changes nothing; the file only **writes** the version table (that is `VER002`'s job, not a guard) |
| `VER002` | blocking | An `update` file changes something and never writes the version table | The folder is not `update`; the file changes nothing |
| `VER003` | blocking | Two update files leave a hole, overlap, or both install the same version | Files that are not update scripts under the project's pattern |
| `DUP001` | blocking | The same row is inserted twice in one script | The rows differ anywhere; a value is computed; a named row would have to be matched against a positional one; the two INSERTs are in different files |
| `DUP002` | review | The same object is **created** twice for one dialect | The two creations are for **different dialects** (that is the point of the repository); one of them is an `ALTER`; they are a package spec and its body |
| `ENC001` | review | A file's encoding differs from what its folder expects | The encoding was **pinned** by the user |
| `ENC002` | blocking | A character in the file cannot be represented in the folder's encoding | The character does have a byte there (every accented Italian character does) |
| `DML001` | review | A `DELETE` or an `UPDATE` has no `WHERE` | It is a `TRUNCATE`; it has a `WHERE`; it is the closing `UPDATE` on the project's **version table**, which has no `WHERE` by design |
| `DML002` | review | An `INSERT` has no column list | It names its columns. A `MERGE` whose insert branch has none counts — same hazard, other spelling |

### What `ENC002` turned out to be

`ENC001` says a file is no longer in the encoding its folder expects, and offers to convert it
back. **`ENC002` is the check that makes that offer safe.** It reports a character the folder's
encoding has no byte for — the case where taking `ENC001`'s fix replaces it with a question mark
and loses text nobody would notice was gone.

That is why it is blocking while `ENC001` is only worth a look, and why the two fire together on
a UTF-8 file that has picked up a character windows-1252 cannot hold. It is not "a second encoding
rule"; it is the guard on the first one's corrective action.

### What `CONS002` and `CONS003` take "a datum" to be

One **row of an INSERT**, reduced to `(column, value)` pairs — the same comparison form the
cross-dialect rules use, so `1.50` and `1.5` are one number and `'X'` and `'x'` are two strings.
`UPDATE` and `DELETE` are not data here: they change a row that is already there, and the question
these two ask is whether the row is there at all.

The install half is **cumulative** and the update half is a **chain of deltas**, so "in one and not
the other" read literally reports the entire seed dataset. Three gates keep that out of the report:

1. **The table must be loaded by both halves.** A table only the initialisation ever inserts into
   is seed data from before the update folder existed. Nothing in the tree dates a row, so this is
   the only honest signal there is — and it is why a project whose updates never touch `PARAMETRI`
   gets nothing from `CONS002`, however much is in there.
2. **Only the columns both halves write are compared.** An update carrying one extra column is the
   same datum written twice, and comparing the full rows would report it once in each direction.
3. **An unreadable statement stands the whole table down** — a computed cell, an `INSERT … SELECT`,
   or a row with no column list. Same abstention as everywhere else in this crate.

Both anchor at the **statement**, not at the folder, and that is deliberate: `-- picus: ignore
CONS002 — this row predates the update folder` is a fact about one INSERT, and there has to be
somewhere to write it. What the analysis cannot do without history is date a row; a repository that
needs finer than "does the update folder maintain this table" wants either the git log or a
per-table declaration in the project settings.

### Where a rule stops, and why

`VER001` and `VER002` judge the **file**, not the statement. In these repositories an update
script *is* the block: one transition, applied whole, with the guard at the top. Reporting per
statement would produce fifty findings for one missing `IF`, all with the same fix.

`CONS001` reports **once per object per dialect**, at the most significant role where the gap
shows. A table absent from a whole dialect would otherwise be three rows — init, data, update —
for one problem.

`CONS001` anchors at the **folder**, not at a file inside it: no file in it is the one that should
have had the statement, and naming one would be a guess dressed up as advice. The jump the user
wants is `alsoAt`, which points at the dialect that does do it.

## Suppressions

```sql
-- picus: ignore DML001 — full reload of the parameter table on install
DELETE FROM PARAMETRI;
```

Three decisions:

1. **The reason is mandatory.** A comment with nothing after the rule id silences nothing, and is
   reported back in `rejected_suppressions` — a suppression that looks like it works and does not
   is worse than none. Same for an unknown rule id.
2. **A suppressed finding stays in the report**, with the reason attached; the interface hides it
   behind a toggle. Deleting it would make the reason unreadable, which defeats point 1.
3. **Scope is where the comment sits.** Above a statement, or trailing on the same line, it
   belongs to that statement. In the **header** it is ambiguous by nature — it sits both at the
   top of the file and above the first statement — so the reading depends on the rule:
   `ignore ENC001` in a header can only be about the file, `ignore DML001` in the same position is
   about the `DELETE` underneath it. `RuleId::is_file_scoped` is that distinction.

The comment scanner is a small state machine over strings and block comments, not a line scan: a
`--` inside a string literal is not a comment, and a suppression that only worked outside quotes
would be a subtlety nobody should have to know about.

## Testing

```bash
cargo test -p picus-analyze
```

The suite is about the **false positives**. Every rule here is easy to make fire; what is hard is
keeping it quiet on a repository that is doing the right thing, and a false positive costs more
than a missed finding — people stop reading a report that is wrong about things they know are
fine. So most of the cases assert that nothing is produced: an Oracle package that has no
PostgreSQL counterpart, a `COMMON/` folder with no dialect, `SEQ.NEXTVAL` in an Oracle script, a
table created once per dialect, a create followed by an alter, two rows stamped with `SYSDATE`, an
accented character windows-1252 can hold perfectly well, a table only the initialisation seeds, an
update that carries one column more than the initialisation does, and the closing version bump —
an `UPDATE` with no `WHERE` that is on every correctly written update script there is.
