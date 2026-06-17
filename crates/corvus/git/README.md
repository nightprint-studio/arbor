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
| `encoding` | encoding-aware decode/encode (CP1252 ↔ UTF-8 ↔ UTF-16, BOM round-trip) |

**Decoupling, not dependency.** Two things stash would otherwise drag in stay
out of the crate:
- **git invocation** → the explicit [`cli::GitCli`] (no global state).
- **recovery snapshots** → a `snapshot: &dyn Fn(&Repository, &str)` callback the
  caller passes; the shell binds it to its `recovery::try_snapshot`, so
  `recovery` (and its `config` dependency) stays shell-side.

Likewise **hooks** are not here: stash fires no hooks itself — the shell handler
fires `on_stash_push` / `on_stash_pop` around the call.

**Next:** `reset` (hard-reset snapshot + tags). Serving stash/reset from
`corvus-be` additionally needs `recovery` extracted (so the headless process can
take the snapshot) — that's the next extraction.

## Public API: use the prelude

`corvus_git::prelude::{GitCli, GitError, bisect_*, *_session, BisectState, …}`.

## Tests

`cargo test -p corvus-git` covers the pure output parsers (`Bisecting: N …`,
`<sha> is the first bad commit`).

## Depends on

`arbor-process-ext` (the `NoWindowExt` console-suppression), `serde`,
`serde_json`, `thiserror`. No Tauri, no git2 (yet), no shell types.
