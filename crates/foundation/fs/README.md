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
- **`entry`** — the serializable DTOs (`FsEntry`, `FsRoot`, `TrashEntry`,
  `DirSize`, `DriveUsage`, `OverviewStats`).
- **`error`** — `FsError`, shaped so the shell maps it back to `AppError` with
  the exact same wire string the explorer showed before the split.

## What stays in the shell

OS/Tauri shell-integration that is **not** pure FS I/O: open-with-default,
reveal-in-file-manager, open-terminal, the native Properties dialog,
set-wallpaper, native icons, and the per-window watcher (it's
`WebviewWindow` + `emit_to`-coupled). Those remain `#[tauri::command]`s in
`fs_commands.rs`.

## Depends on

`serde`, `thiserror`, `dirs`, `regex`, `trash`, `zip`; `arbor-process-ext` (for
`no_window` on the WSL enumeration); `windows-sys` (drive enumeration + disk
usage, Windows only). No Tauri, no Tokio — the blocking calls run on the shell's
`spawn_blocking`.

## Consumed by

`arbor` (the shell, via `fs_commands.rs`); future `sitta-be`.
