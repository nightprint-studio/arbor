# corvus-git

Local-git logic for **Corvus**, extracted Tauri-free so the in-process shell
**and** the headless `corvus-be` process run the exact same code (see
[`docs/corvus-be-bringup.md`](../../../docs/corvus-be-bringup.md)).

## Design

- **No hidden global state.** Git invocation goes through an explicit
  [`cli::GitCli`] (the resolved git program path). The shell builds one from its
  detection state; `corvus-be` builds its own — nothing to synchronize across the
  process boundary. This is deliberate: a shared mutable git-path global would be
  exactly the kind of cross-process coupling that bites later.
- **Local error.** Operations return [`error::GitError`] (`Io` / `Other`). Each
  consumer maps it: the shell to `AppError` variant-for-variant (so the frontend
  wire string is unchanged from before the extraction), `corvus-be` to the IPC
  error string.

## Domains

| Module | What |
|--------|------|
| `bisect` | `git bisect` in `--no-checkout` mode: start / mark / reset / undo + state read from `.git/BISECT_*` |
| `bisect_sessions` | paused & completed sessions persisted under `<repo>/.arbor/bisect` |
| `stash` | save / apply / pop / drop / rename / force-apply / abort + stash-file content |
| `recovery` | snapshot-based safety net for destructive ops (journal under `.git/arbor-recovery`, pin/restore/prune) |
| `explorer` | File-Explorer git awareness (overlay badges + branch info + light inline actions: status/changes/branches/remote-url/stage/unstage/discard/ignore/checkout) on arbitrary paths. Pure git2; `discard` takes an injected `(GitCli, SnapshotPolicy)` for an optional recovery snapshot. Consumed by `sitta-be`. |
| `reset` | `git reset --soft/mixed/hard` via CLI + lightweight/annotated tag create/delete |
| `encoding` | re-export of `arbor_fs::prelude::encoding` (CP1252 ↔ UTF-8 ↔ UTF-16, BOM round-trip) — the canonical impl lives in the foundation crate |

**Decoupling, not dependency.** Things these domains would otherwise drag in
stay out of the crate, passed in by the caller instead:
- **git invocation** → the explicit [`cli::GitCli`] (no global state).
- **snapshot policy / retention** → `recovery` takes the [`recovery::SnapshotPolicy`]
  / `retention_days` as a parameter; the shell loads them from the app config and
  forwards them, so `recovery` here drags in neither `git_cli` globals nor the
  app config.
- **recovery from stash** → `stash` takes a `snapshot: &dyn Fn(&Repository, &str)`
  callback; the shell binds it to `recovery::try_snapshot` (now also crate-side),
  keeping `stash` independent of `recovery`.

Likewise **hooks** are not here: these domains fire no hooks themselves — the
shell handler fires `corvus:stash_push` / `corvus:stash_pop` (and, for reset,
`corvus:tag_create` / `corvus:tag_delete`) around the call.

The shell keeps the hard-reset recovery snapshot (config-loading) and the OID
validation around `reset::run_reset`, and fires `corvus:tag_create` / `corvus:tag_delete`
around the tag calls — none of which belong in the crate.

## Public API: use the prelude

`corvus_git::prelude::{GitCli, GitError, bisect_*, *_session, BisectState, …}`.

## Tests

`cargo test -p corvus-git` covers the pure output parsers (`Bisecting: N …`,
`<sha> is the first bad commit`) and the recovery `SnapshotPolicy` exclusion
rules (size cap, case-insensitive extension deny-list, ref-slug stability).

## Depends on

`arbor-process-ext` (the `NoWindowExt` console-suppression), `arbor-fs`
(re-exported `encoding`), `git2` (vendored-libgit2), `encoding_rs` (the
diff/merge/stash domains reference `encoding_rs::Encoding` directly), `serde`,
`serde_json`, `thiserror`, `tracing`. No Tauri, no shell types.
