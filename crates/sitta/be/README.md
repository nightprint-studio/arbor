# sitta-be

The headless **Sitta** backend process (Model-D) — Arbor's file explorer.

The sitta twin of [`corvus-be`](../../corvus/be) / [`merula-be`](../../merula/be):
it serves the sitta domains over framed-stdio IPC, loads **host-pure** Lua plugins,
and has **no credentials and no pushed config**. It resolves its own `sitta_*`
config / data dirs once `init_active_profile()` has run.

A file manager keeps almost no backend domain state of its own:

- **filesystem I/O** → [`arbor-fs`](../../foundation/fs), served by the shell's
  `platform` broker;
- **git-awareness** → [`corvus-git`](../../corvus/git), shared with `corvus-be`.

## Domains served

| Module | Methods |
|--------|---------|
| `fs_git` | the File Explorer git awareness (`fs_git_status` / `_changes` / `_branches` / `_remote_url` / `_stage` / `_unstage` / `_discard` / `_ignore` / `_checkout`) over [`corvus_git::explorer`] |
| `config_cmds` | `get_sitta_config` / `set_sitta_config` (the explorer's own UX preferences, see [`sitta-core`](../core)) |
| `selftest` | `be_ping` / `be_echo` |

## Plugins

`src/plugin.rs` wires a **host-pure** plugin host (the `arbor.*` base namespaces, no
product namespaces, no vetoable hooks). Host-pure Lua plugins placed under sitta's
installed pool load on boot. The Plugin-Manager RPC surface (FE enable/disable/reload
for sitta plugins) is not wired yet — there are no sitta plugins to manage.

## Lifecycle

Spawned **lazily** by the shell (`ipc::ensure_sitta_be`) when an explorer window
first opens — the launcher and the other product windows never touch the explorer
backend. The shell routes the `sitta` program to it via the split broker; while it
is detached every `sitta` rpc method falls through to a loopback `UnknownMethod`
sink.

## Self-test

```text
rpc("sitta", "be_ping", {})            → "pong"
rpc("sitta", "be_echo", {message:"x"}) → "x"
```
