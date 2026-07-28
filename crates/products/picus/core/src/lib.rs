//! `picus-core` — the headless backend core for Picus (the SQL studio: a client
//! for live databases and a maintainer for the per-dialect script repository those
//! databases are installed from).
//!
//! The picus twin of `bennu-core` / `tyto-core`: the canonical [`PicusState`] the
//! `picus-be` process owns, **Tauri-free by construction**. Deliberately small — a
//! SQL studio's heavy lifting (driver sessions, statement parsing, per-dialect
//! emission, script rewriting) lives in the leaf crates the domain handlers drive
//! (`picus-db-api` + one crate per engine, then `picus-ast` / `picus-parse` /
//! `picus-emit` / …); this state holds only the BE→FE event egress, the reverse
//! channel back to the shell, and the two things whose lifetime *is* the process:
//! the open database sessions ([`connections::SessionPool`]) and the script
//! repositories read so far ([`scripts::ScriptCache`]).
//!
//! ## The structural invariant, restated for the backend
//!
//! **The dialect is a property of the folder, never a global "current dialect".**
//! Nothing in this crate holds an ambient dialect, and nothing in `picus-be` should
//! either: every function that parses, analyses, generates or rewrites SQL takes it
//! as an explicit parameter. See `docs/picus-design.md` §1.
//!
//! Modules here depend only on [`arbor_ipc`], `arbor_core` (path resolution for the
//! typed config) and serde — no `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites (in `picus-be`) reach this crate's surface
//! through `picus_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod config;
pub mod connections;
pub mod digest;
pub mod prelude;
pub mod scripts;
pub mod state;
