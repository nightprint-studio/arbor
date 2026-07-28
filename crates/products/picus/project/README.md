# picus-project

What a Picus **script repository** is: its per-dialect branches, what each folder is for, what
encoding its files are in, how its update files are named, and how a generated block is marked.

Picus's other half is a database client; this crate never touches a database. It reads a
directory of SQL files and answers questions about the *repository*, which is a different thing
from the schema a connection reports — the two were conflated once early in the product and it
leaked immediately.

## The invariant

**The dialect belongs to the folder.** `Branch::dialect` is an `Option<EngineKind>`, and `None`
is a real answer: a branch nobody could identify receives no generated SQL. A default would be
worse than nothing, because guessing wrong writes Oracle syntax into a PostgreSQL file — the
exact failure Picus exists to catch.

## Two rules the crate is built around

1. **Nothing is written without an explicit confirmation.** `discover()` returns a `Proposal`,
   never a file. The user sees what Picus concluded, corrects it, and only then does the caller
   invoke `ProjectConfig::save`. This lands in someone's repository and gets committed.
2. **An existing project file wins over every inference.** A rescan never overwrites a decision
   the user made; it only fills in what the file cannot know — which files exist, and what
   encoding they turned out to be.

## Layout

| Module | Holds |
|---|---|
| `tree` | `Project` / `Branch` / `ScriptFolder` / `ScriptFile` — the wire shapes the interface renders, `camelCase`, serialize-only |
| `config` | `.arbor/picus/project.toml`: roles, encodings, version table, naming, marker |
| `discover` | `plan()` (pure) + `scan()` (the filesystem) + `discover()` (both) |
| `infer` | a folder's role and a branch's engine, each with the keyword that produced it |
| `naming` | the update-file scheme: a versioned default plus a per-project regex |
| `marker` | the comment above a generated block, its template, and recognising it again |
| `insertion` | where a generated block lands, per folder role |
| `version` | an application version that orders numerically, not lexicographically |

## Where the configuration lives, and why

`<root>/.arbor/picus/project.toml` — inside the `.arbor/` directory Arbor already owns in a
repository, namespaced per product so the other products can move in without a dotfile each.

It holds what describes *the repository*; the per-user preferences stay in the profile's
`picus/config.toml`. Nothing is in both. The reason for the split is concrete: a colleague
opening the same repository must inherit the roles and the expected encodings, or the same repo
behaves differently per person.

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

Everything worth testing is in `plan()`, which is pure — a list of files and their bytes in, a
proposal out. There is not a temporary directory anywhere in the suite. `scan()` is the thin
glue that produces that list from a real directory, and is deliberately the only part without
unit tests.

Two properties are worth knowing about because downstream crates depend on them:

- **The output does not depend on the order the files arrived in.** Branch, folder and file
  order is user-visible and must never reflect how the filesystem happened to enumerate a
  directory. Asserted directly.
- **Pure-ASCII files abstain from the encoding vote.** They are the ones being decided, so
  letting them vote would make a mostly-ASCII folder drown out its own evidence.

```bash
cargo test -p picus-project
```
