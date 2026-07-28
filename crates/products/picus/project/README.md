# picus-project

What a Picus **script repository** is: which folder each SQL dialect lives in, what each folder is
for, what encoding its files are in, how its update files are named, and how a generated block is
marked.

Picus's other half is a database client; this crate never touches a database. It reads a
directory of SQL files and answers questions about the *repository*, which is a different thing
from the schema a connection reports — the two were conflated once early in the product and it
leaked immediately.

## The invariant

**The dialect belongs to the folder** — to a folder, any folder, wherever in the tree the
repository chose to put it. A folder may *declare* a dialect and a role; every folder under it
**inherits** what it did not declare, until one overrides it.

That is what makes a real repository describable:

```
AGGIORNAMENTO           role = update
AGGIORNAMENTO/2024/ORA  dialect = oracle
AGGIORNAMENTO/2024/POS  dialect = ?
```

The role is at the top, the dialect at the bottom, and neither carries the other. There is no
"branch" — the tree is the repository's own directory hierarchy, and `FolderNode` is a directory
in its real place.

`FolderNode::effective_dialect` is an `Option<EngineKind>` and `None` is a real answer: a folder
nobody classified receives no generated SQL and takes part in no cross-dialect comparison. A
default would be worse than nothing, because guessing wrong writes Oracle syntax into a
PostgreSQL file — the exact failure Picus exists to catch. `POS` above matches nothing Picus
knows *by default*, and being asked about it is the correct outcome — until the project says
what it means (below).

### …and the last link of the chain is the file

Not every repository puts the engine in a directory. An untidy one puts it in the file name —
`4_12_ORA.sql` beside `4_12_POS.sql` in one folder that can say nothing about either, because it
is honestly both. So the engine is a property of the **file**, of which the folder is the default:

```
file declaration → folder declaration → nearest ancestor's → none
```

`ScriptFile` carries `engine` (declared on this file, `None` for essentially every file in
essentially every repository) and `effective_engine` (after inheritance), read through exactly the
same `scope()` / `covers()` / `effective_dialect()` / `is_generic()` methods `FolderNode` exposes.
Same four answers, one level of granularity further down — the rules live in `FolderEngine`, and
these are the same questions asked of a smaller thing, which is what keeps a file and its folder
from ever disagreeing about what *portable* means. `resolve()` fills both in one pass, so there is
one inheritance rule in the crate and no second place for the two to drift.

The **role** stays the folder's. A directory of scripts is *for* something and the file beside this
one is for the same thing; the engine is the one axis that genuinely varies file by file.

Two consequences downstream depends on:

- `FolderNode::is_in_lane` asks the **files**, so a folder holding one `*_ORA.sql` and one
  `*_POS.sql` is in both lanes where before it was in neither, and a folder holding only other
  folders is in none. `Project::lane_files` gives the lane at the granularity content actually has.
- `Project::dialects` counts over files, so a repository whose PostgreSQL content is four scattered
  `*_POS.sql` files genuinely has a PostgreSQL side — a folder-level count said it did not, and
  every cross-dialect rule stayed silent about it.

## Four engine states

A folder is not "a Picus dialect or a question". `AGGIORNAMENTO/2024/MSQ` in that repository is
SQL Server, and `COMUNE/` holds plain SQL meant to run on both engines — and never being able to
answer a question is a different fact from not knowing the answer, which is a different fact
again from the answer being "both".

| `FolderNode::effective_engine` | Means | Behaviour |
|---|---|---|
| `Some(Supported(_))` | Oracle / PostgreSQL | parsed, analysed, compared, generated into |
| `Some(Generic)` | **portable** | parsed against both, in **every** lane, generated into with the intersection |
| `Some(Unsupported(_))` | recognised, unsupported | named on screen, **never asked about, never parsed** |
| `None` | nobody knows | the interface asks |

**One field**, because a folder has one engine. Read it through the methods rather than the
field, because the useful questions are not "which engine":

- `scope()` — what its SQL has to be valid in. `None` is the gate that keeps unsupported folders
  out of the parser and the emitter.
- `covers(dialect)` — does content here count for that dialect? True of **both** for a portable
  folder, which is what puts it in two lanes and is why `is_in_lane` asks this and not equality.
- `effective_dialect()` — the *single* dialect, if there is one. `None` for portable as well as
  for unclassified, and a caller that meant `covers` is the bug this distinction catches.
- `is_generic()` / `engine_is_unsupported()` / `engine_is_unknown()` — the three states that are
  not "an ordinary dialect".

Not parsing the unsupported ones is correctness before it is speed: a permissive
Oracle/PostgreSQL grammar does not *fail* on T-SQL, it produces a plausible-looking tree of
statements that mean nothing. Portable folders are the opposite — fully parsed, with the
acceptance rule inverted, so a construct belonging to *either* engine is reported.

`Generic` is **never inferred**. No keyword produces it; it only ever arrives from a
declaration, per path or by name in `alias`.

## Two rules the crate is built around

1. **Nothing is written without an explicit confirmation.** `discover()` returns a `Proposal`,
   never a file. The user sees what Picus concluded, corrects it, and only then does the caller
   invoke `ProjectConfig::save`. This lands in someone's repository and gets committed.
2. **An existing project file wins over every inference.** A rescan never overwrites a decision
   the user made — including the decision to *clear* a dialect, which is why a declaration is
   authoritative for the fields it leaves absent as well as for the ones it sets. It only fills
   in what the file cannot know: which files exist, and what encoding they turned out to be.

## Layout

| Module | Holds |
|---|---|
| `tree` | `Project` / `FolderNode` / `ScriptFile` — the wire shapes the interface renders, `camelCase`, serialize-only |
| `resolve` | inheritance: what a folder declares → what applies to it, and to every file in it |
| `config` | `.arbor/picus/project.toml`: the folder and file declarations, version table, naming, marker |
| `legacy` | reading a `version = 1` file and folding its branches into declarations |
| `discover` | `plan()` (pure) + `scan()` (the filesystem) + `discover()` (both) |
| `infer` | a folder's role and engine from its own name, each with the keyword that produced it — and a file's engine, from the project's vocabulary and from nothing else |
| `alias` | names that mean something in **this** repository, where they are looked for, and the rule that matches them |
| `naming` | the update-file scheme: a versioned default plus a per-project regex |
| `marker` | the comment above a generated block, its template, and recognising it again |
| `insertion` | where a generated block lands, per folder role |
| `version` | an application version that orders numerically, not lexicographically |
| `path` | project-relative paths: the parent, the last segment, the ancestry |

## Where the configuration lives, and why

`<root>/.arbor/picus/project.toml` — inside the `.arbor/` directory Arbor already owns in a
repository, namespaced per product so the other products can move in without a dotfile each.

It holds what describes *the repository*; the per-user preferences stay in the profile's
`picus/config.toml`. Nothing is in both. The reason for the split is concrete: a colleague
opening the same repository must inherit the roles and the expected encodings, or the same repo
behaves differently per person.

The folder declarations are a **flat list keyed by path**:

```toml
[[folder]]
path = "AGGIORNAMENTO"
role = "update"

[[folder]]
path = "AGGIORNAMENTO/2024/ORA"
dialect = "oracle"
encoding = "windows-1252"
```

Only folders that declare something appear — a folder that simply inherits is absent — and a
declaration survives a subdirectory being added, which the previous nested branch/folder shape
could not. Encoding and naming overrides inherit the same way the dialect does. `dialect` may
name an engine Picus does not read (`dialect = "sqlserver"`): a folder has one engine, so it
has one key.

### And, where a repository is untidy, keyed by file

```toml
[[file]]
path = "AGGIORNAMENTO/2024/4_12_POS.sql"
dialect = "postgres"
```

Same shape, same key, one level down. Only the engine, and deliberately only the engine: a role is
a fact about a directory, an encoding is measured from the bytes rather than declared. Nothing
inherits *downwards* from it — a file has nothing below it — so it is a leaf answer and it beats
everything, including the folder it sits in. Almost always empty: a `[[file]]` line is a correction
to a file Picus placed wrongly, and `discover()` never proposes one.

### The version number is derived from the content

`ProjectConfig::save` stamps **`required_version()`**, not `CURRENT_VERSION`: the lowest schema
version that can read this configuration correctly — `3` exactly when something in it classifies an
individual file (a `[[file]]` declaration, or an alias whose scope covers file names), `2`
otherwise. A version number is a claim about compatibility and the file is committed and shared, so
the honest claim depends on what the file says. Always writing the newest would lock a colleague on
an older build out of a repository using nothing their build lacks; always writing the oldest would
let that build silently ignore the declarations that decide which dialect a script is parsed as.

### And a vocabulary, for the names that repeat

A declaration answers for one path, which does not scale to a repository shipping a folder set
per delivered version — eleven folders called `POS`, and a twelfth next month. So a project also
gets to say what a **name** means:

```toml
[[alias]]
name = "POS"
engine = "postgres"

[[alias]]
name = "MSQ"
engine = "sqlserver"     # recognised, not supported: left alone from here on

[[alias]]
name = "CONSEGNE"
role = "update"
```

Three properties it is built on, each of them a trap avoided:

- It **adds to** the built-in vocabulary. Declaring one alias never costs the repository the
  defaults it was already relying on.
- It matches exactly the way a built-in keyword does — **whole word, case-insensitively**, via
  `alias::name_matches`. Substring matching is how `POS` starts claiming `POSIZIONI`, which is
  the precise reason `pos` is not in the global list.
- A bad value **degrades**: `engine` and `role` are wire strings read through typed accessors,
  exactly like `[generation.insertion]`, so a typo drops that one entry and is reported by
  `ProjectConfig::problems` instead of failing the parse and resetting the file.

Precedence, and it is the whole design: **`[[folder]]` beats `[[alias]]` beats the built-in
vocabulary**. A specific answer beats a general rule; a local fact beats a global heuristic.
Aliases apply at *discovery*, so a `POS` folder created next month is classified without anyone
touching this file.

#### Where the name is looked for — `applies_to`

Folder names by default. A repository whose engine is in the file name points the same name at file
names as well:

```toml
[[alias]]
name = "POS"
engine = "postgres"
applies_to = "both"      # "folders" (default) | "files" | "both"
```

It moves the **engine** only — a role stays a fact about a directory, and an alias declaring only a
role while pointed at file names classifies nothing and is reported as such. An unreadable value
degrades to the default rather than to nothing: `applies_to` says *where* to look, so a typo must
not un-declare an engine that was spelled correctly. `AliasScope` is a type rather than a `bool`
because the third answer is real — one alias answering for a `POS` folder *and* a `4_12_POS.sql`
without a second entry that can drift from the first.

**The built-in vocabulary never classifies a file** (`infer_file_engine_in` consults the project's
aliases and nothing else), and it is the asymmetry the whole path rests on. `ORA` is Italian for
*now*, so `AGGIORNA_ORA_INIZIO.sql` would read as Oracle; `MIGRAZIONE_DA_MYSQL.sql` is a PostgreSQL
script *about* MySQL, and reading `mysql` out of it produces not a wrong finding but **no**
findings, silently. Folder names are short, deliberate and a dozen to a repository; file names are
hundreds and nobody reviews them. `4_12_ORACLE.sql` is the case this gives up, and one `[[alias]]`
line buys it back. The extension is stripped before matching (`infer::file_stem`), so `.sql` can
never match an alias called `SQL`.

A `version = 1` file, with `[[branch]]` tables holding `[[branch.folder]]` ones, still loads:
`legacy` folds it into declarations on the way in and the resolver reproduces exactly what the
old two-level shape meant.

The same reasoning is why **where a generated block lands** can be stated here, per role, and
outranks the user's own preference:

```toml
[generation.insertion]
update = "end-of-file"           # after the last complete statement
init   = "after-last-on-table"   # grouped with the statements on the same table
```

Those two are also the defaults, so a repository that agrees with them writes nothing. Key and
value are both plain strings: an unrecognised role or rule degrades to the default and is
reported by `ProjectConfig::problems`, rather than failing the parse and resetting every other
setting in the file.

## Testing

Everything worth testing is in `plan()` and `resolve()`, both pure — a list of files and their
bytes in, a proposal out. There is not a temporary directory anywhere in the suite. `scan()` is
the thin glue that produces that list from a real directory, and is deliberately the only part
without unit tests.

Three properties are worth knowing about because downstream crates depend on them:

- **The output does not depend on the order the files arrived in.** Folder and file order is
  user-visible and must never reflect how the filesystem happened to enumerate a directory.
  Asserted directly.
- **Pure-ASCII files abstain from the encoding vote.** They are the ones being decided, so
  letting them vote would make a mostly-ASCII folder drown out its own evidence.
- **A dialect keyword has to be a whole word.** `ora` sits inside `LAVORAZIONE`, and every folder
  in the tree is now asked what it is — so roles match on substrings and engines do not. A
  project's own aliases match by the same rule, through the same function.
- **A per-path declaration beats an alias, and an alias beats the built-in vocabulary.** All
  three orderings are asserted at the discovery level, including the awkward one: a declaration
  that *clears* the engine must not be re-inferred from the alias on the next scan.
- **An alias says nothing about a file name until it is asked to,** and the built-in vocabulary
  never does at all. Both are asserted directly, because the safety of file-level classification is
  entirely that it only happens where somebody wrote it down.

```bash
cargo test -p picus-project
```
