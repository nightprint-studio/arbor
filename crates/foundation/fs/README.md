# arbor-fs

Pure, Tauri-free filesystem operations for Arbor.

## Purpose

The file-explorer logic used to live entirely in
`src-tauri/src/commands/fs_commands.rs` (~1900 lines), mixing genuine FS I/O
with Tauri/OS glue. This crate holds the **pure** half so it can be reused by
the generic FS commands today and a headless `sitta-be` tomorrow (Model D,
`docs/migration-roadmap.md` M1a / M5). The `#[tauri::command]` functions are now
thin wrappers over it.

## Public API: use the prelude

Workspace convention — reach the surface through `arbor_fs::prelude::...` (or a
single `use arbor_fs::prelude::*;`). Operations are grouped by verb-domain:
`arbor_fs::prelude::read::read_dir(...)`, `copy::copy(...)`,
`mutate::rename_many(...)`, `trash::trash_list()`, `zip::unzip(...)`,
`roots::list_roots()`, `size::dir_size(...)`, `pathexpand::expand_path(...)`.

## Contents

- **`read`** — directory listing, recursive name search (glob/substring),
  text-file read.
- **`mutate`** — create dir/file, rename (single + two-phase batch), write,
  delete (single + many).
- **`copy`** — copy / move / duplicate with an injected `ProgressSink` and a
  cooperative `CancelToken`. The shell supplies the sink that throttles and
  emits the progress event; arbor-fs only reports facts.
- **`trash`** — move-to-trash + the Recycle Bin view (list / restore / purge /
  empty), backed by `trash::os_limited` (Windows/Linux) or `~/.Trash` (macOS).
- **`zip`** — compress several sources into one archive; extract with zip-slip
  sanitisation.
- **`roots`** — quick-access roots (user dirs + drives + WSL distros) and
  per-drive storage usage (Overview dashboard).
- **`size`** — recursive directory size + multi-selection totals.
- **`pathexpand`** — address-bar `~` / `%VAR%` / `$VAR` / `${VAR}` expansion.
- **`encoding`** — encoding-aware decode/encode for file content, plus
  detection that reports **how** it decided. The single canonical home for this
  — `corvus-git` re-exports it, the shell's studio backends decode/encode
  through it, and Picus's script diagnostics build on it. Backed by
  `encoding_rs`. See [Encoding detection](#encoding-detection).
- **`entry`** — the serializable DTOs (`FsEntry`, `FsRoot`, `TrashEntry`,
  `DirSize`, `DriveUsage`, `OverviewStats`).
- **`error`** — `FsError`, shaped so the shell maps it back to `AppError` with
  the exact same wire string the explorer showed before the split.

## Encoding detection

Real-world SQL / Java / `.properties` repositories are still `windows-1252`
with CRLF. A file silently rewritten as UTF-8 by an outside editor turns
accented characters into mojibake and installs wrong data, so detection has to
be explicit about what it knows and what it merely assumes.

The chain, in order:

| # | Evidence | `EncodingSource` |
|---|---|---|
| 1 | A BOM (UTF-8, UTF-16 LE/BE) | `bom` |
| 2 | Valid UTF-8 with **at least one multibyte sequence** | `utf8` |
| 3 | Pure ASCII → ambiguous, resolved from the folder | `inherited` |
| 4 | Anything else → the legacy single-byte encoding | `heuristic` |
| — | Pinned by the user or by config | `forced` |

Rung 2 requires a multibyte sequence on purpose: valid UTF-8 that is *pure
ASCII* is equally valid windows-1252, so it is not evidence of anything.

### Inheritance is explicit

Rung 3 is resolved by an `EncodingContext` the **caller** builds — this crate
never scans a folder by itself, because which files may vote is the caller's
policy (a Picus script folder votes with its `.sql` files, not its `README`).

```rust
use arbor_fs::prelude::*;

let ctx = EncodingContext::from_samples(folder_contents);   // decidable files vote
let (text, detection) = encoding::decode_in_context(&bytes, &ctx);
detection.label();   // "windows-1252"
detection.source;    // EncodingSource::Inherited
```

Ambiguous files cast no vote (they are the ones being decided). The winner is a
**plurality**; ties break to the legacy encoding when it is among the tied
candidates, then by canonical name — never by directory-iteration order.

### Writing

`encode_strict` / `encode_for_disk_strict` fail with an `UnrepresentableChar`
(character, line, column) instead of substituting, and are the only functions
here that can actually write UTF-16 — `encoding_rs` has no UTF-16 encoder and
`Encoding::encode` quietly falls back to UTF-8 for those labels.

### Frozen half

`detect`, `decode_bytes`, `decode_bytes_full`, `encode_for_disk`,
`encode_for_disk_with_bom` predate the above and are **unchanged**: they claim
pure-ASCII files for UTF-8, they leave a BOM in the decoded string as a leading
U+FEFF, and they substitute on unmappable characters. Bennu, Corvus and the
studio backends depend on exactly that; characterisation tests pin it.

## What stays in the shell

OS/Tauri shell-integration that is **not** pure FS I/O: open-with-default,
reveal-in-file-manager, open-terminal, the native Properties dialog,
set-wallpaper, native icons, and the per-window watcher (it's
`WebviewWindow` + `emit_to`-coupled). Those remain `#[tauri::command]`s in
`fs_commands.rs`.

## Depends on

`serde`, `thiserror`, `dirs`, `regex`, `trash`, `zip`, `encoding_rs`;
`arbor-process-ext` (for `no_window` on the WSL enumeration); `windows-sys`
(drive enumeration + disk usage, Windows only). No Tauri, no Tokio — the blocking
calls run on the shell's `spawn_blocking`.

## Consumed by

`arbor` (the shell, via `fs_commands.rs` + the studio backends' encoding);
`corvus-git` (re-exports `encoding`); future `sitta-be`.
