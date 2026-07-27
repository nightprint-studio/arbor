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
| `config_cmds` | `get_picus_config` / `set_picus_config` (the studio's encoding fallbacks, write guards, emission defaults and query row limit — see [`picus-core`](../core)) |
| `selftest` | `be_ping` / `be_echo` |

Landing next, each as its own module against the same `PicusState`: the database
half (`picus-db-api` + one crate per engine, PostgreSQL first) and the script half
(parse / inventory / analyse / emit / rewrite).

## Plugins

None. When Picus does want them — custom emission rules and naming schemes are the
obvious candidates — do **not** copy sitta-be/tyto-be's `plugin.rs` a third time:
that file already carries the note to promote the host-pure wiring into a shared
`arbor-plugin-be` crate first.

## Lifecycle

Spawned **lazily** by the shell (`ipc::ensure_picus_be`) when the Picus window first
opens — the launcher and the other product windows never touch the SQL studio
backend. The shell routes the `picus` program to it via the split broker; while it
is detached every `picus` rpc method reports `BackendNotRunning`, and the frontend
falls back to its fixtures.

## Self-test

```text
rpc("picus", "be_ping", {})            → "pong"
rpc("picus", "be_echo", {message:"x"}) → "x"
```
