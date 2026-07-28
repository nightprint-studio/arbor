# picus-be

The headless **Picus** backend process (Model-D) — Arbor's SQL studio: a client for
live databases and a maintainer for the per-dialect script repository those
databases are installed from.

The picus twin of [`bennu-be`](../../bennu/be) / [`tyto-be`](../../tyto/be), on the
slim path: it serves the picus domains over framed-stdio IPC, has **no plugin host,
no pushed config, and no stored password**, and resolves its own `picus_*` config /
data dirs once `init_active_profile()` has run.

## Two rules this binary keeps as it grows

- **No language model, anywhere in the flow.** Generation is structured input →
  model → per-dialect emission: deterministic, testable, diffable. This is a product
  requirement, not a preference.
- **No ambient dialect.** The dialect is a property of the *folder* being written,
  never backend-wide state; every parse / emit / rewrite entry point takes it
  explicitly. See [`docs/picus-design.md`](../../../../docs/picus-design.md) §1.

Credentials follow from the first rule's sibling: Picus keeps no password. A
connection's secret is resolved through the shell's credential broker over the
reverse channel, at the moment of use.

## Domains served

| Module | Methods |
|--------|---------|
| `selftest` | `be_ping` / `be_echo` |
| `config_cmds` | `get_picus_config` / `set_picus_config` (the studio's encoding fallbacks, write guards, emission defaults and query row limit — see [`picus-core`](../core)) |
| `providers` | `picus_providers` — every engine, connectable or not |
| `connections` | the configured list, open / close / test, `picus_read_db_version` |
| `schema` | `picus_read_schema` / `picus_table_detail` / `picus_fetch_page` |
| `query` | `picus_execute` / `picus_cancel` |
| `emit` | `picus_emit` / `picus_validate_rows` / `picus_validate_value` |
| `project` | `picus_open_project` / `picus_confirm_project` / `picus_is_project` / `picus_propose_update_file` |
| `scripts` | `picus_open_scripts` / `picus_refresh_scripts` / `picus_analyze_scripts` / `picus_script_text` |
| `apply` | `picus_preview_apply` / `picus_apply` |

`secrets` is the reverse-channel resolver rather than a domain: the only module that
knows how a password is fetched, so the driver crates see a trait.

## The script half

`scripts` is the seam between the finished script crates and the running
application. It reads a repository **once**, decodes every file once, and holds the
result on `PicusState` (`picus_core::scripts`); `picus-parse`, `picus-inventory`
and `picus-analyze` then run over that held text. Three things are worth knowing
before changing it:

- **Invalidation is by hand.** A refresh or a write, and nothing else. Nothing
  watches the filesystem, on purpose.
- **The parse lives inside the call.** `ParsedFile` borrows its source, so it is
  produced by one isolated function (`scripts::parse_all`) — which is also the only
  place a future on-disk parse cache would touch.
- **A write is two calls.** `picus_preview_apply` returns the exact bytes plus a
  digest per file; `picus_apply` re-prepares and refuses if any of those digests
  moved, naming the file. What was approved is what gets written, or nothing is.

Where a generated block lands is a stated rule, per folder role, resolved
repository → user → built-in default and written into the diff's hunk header.
`picus-rewrite`'s refusal to write a file it cannot reproduce byte for byte is left
exactly as it is.

## Plugins

None. When Picus does want them — custom emission rules and naming schemes are the
obvious candidates — do **not** copy sitta-be/tyto-be's `plugin.rs` a third time:
that file already carries the note to promote the host-pure wiring into a shared
`arbor-plugin-be` crate first.

## Lifecycle

Spawned **lazily** by the shell (`ipc::ensure_picus_be`) when the Picus window first
opens — the launcher and the other product windows never touch the SQL studio
backend. The shell routes the `picus` program to it via the split broker; while it
is detached every `picus` rpc method reports `BackendNotRunning`.

## Self-test

```text
rpc("picus", "be_ping", {})            → "pong"
rpc("picus", "be_echo", {message:"x"}) → "x"
```
