//! Encoding helpers — re-exported from the Tauri-free `corvus-git` crate.
//!
//! The implementation moved to `corvus_git::encoding` (so the headless backend
//! shares it). This shim keeps the `crate::git::encoding::*` paths the ~17
//! in-shell consumers (studio backends, diff, blame, merge, …) already use.

pub use corvus_git::encoding::*;
