//! `arbor-fs` — pure filesystem operations for Arbor.
//!
//! Tauri-free FS I/O: read/write/copy/move/delete/list/search/trash/zip, plus
//! quick-access roots, recursive sizes and address-bar path expansion. The
//! `#[tauri::command]` functions in `src-tauri/src/commands/fs_commands.rs` are
//! thin wrappers over this crate: they map [`prelude::FsError`] to the host
//! `AppError`, drive the blocking calls on `spawn_blocking`, and inject the
//! Tauri-specific glue the pure layer exposes via traits — the
//! [`prelude::ProgressSink`] (progress emit) and [`prelude::CancelToken`]
//! (the op-id registry).
//!
//! OS/Tauri shell-integration that is *not* pure FS I/O (open-with-default,
//! reveal-in-file-manager, the per-window watcher, native icons, the native
//! Properties dialog, set-wallpaper, open-terminal) intentionally stays in the
//! shell — see `docs/migration-roadmap.md` (M1a).
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public surface
//! through a `prelude` module. Reach types through `arbor_fs::prelude::...`
//! (or a single `use arbor_fs::prelude::*;`) rather than the per-feature
//! submodule paths.

pub mod copy;
pub mod entry;
pub mod error;
pub mod mutate;
pub mod pathexpand;
pub mod prelude;
pub mod read;
pub mod roots;
pub mod size;
pub mod trash;
pub mod zip;
