# sitta-core

The headless backend core for **Sitta** — Arbor's file-explorer product (Model-D).

The sitta twin of [`corvus-core`](../../corvus/core) / [`merula-core`](../../merula/core):
it owns the canonical [`SittaState`] that the `sitta-be` process holds, **Tauri-free
by construction**.

Deliberately tiny. A file manager's real work is:

- **filesystem I/O** → already extracted to [`arbor-fs`](../../foundation/fs), served
  today by the shell's `platform` broker;
- **git-awareness** (status badges, branch, stage/discard from the tree) → lives in
  [`corvus-git`](../../corvus/git), shared by `corvus-be` and `sitta-be`.

So `SittaState` holds only the BE→FE event egress (`EventSink`) and the reverse
channel back to the shell (`HostCaller`). Everything else a later wave needs already
has a `with_*` builder slot, so waves fill handlers in `sitta-be` without re-editing
this crate.

## Config

`config` holds [`SittaConfig`] — the file-explorer's own UX preferences (view/sort/
startup, sidebar + column layout, favourites, saved searches, external-link policy,
the git-awareness switch), persisted to sitta's per-profile `…/sitta/config.toml`
via `load` / `save` (infallible-by-design: a missing/corrupt file yields defaults).
Resolved through [`arbor_core::prelude::sitta_config_path`] — not pushed by the
shell. The four window/OS-integration settings the launcher consumes stay in the
shell config, not here.

## Public API

Reach the surface through the prelude (workspace convention):

```rust
use sitta_core::prelude::{SittaState, SittaConfig};
```
