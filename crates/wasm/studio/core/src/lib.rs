//! `arbor-studio-core` — the `StudioFormatBackend` trait plus (from
//! Stage 2 onward) the format-agnostic engines shared by every
//! per-format backend.
//!
//! Stage 1 ships only the trait; `history`, `diff`, `query`,
//! `edit_expr`, `refactor`, `persist` are empty skeletons that Stage 2
//! fills by lifting the logic currently copy-pasted across the 5
//! `*_studio/backend_impl.rs`. Depends only on `arbor-studio-types`;
//! no Tauri, no launcher coupling.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `arbor_studio_core::prelude::...`.

pub mod backend;
pub mod prelude;

// ── Generic engines (Stage 2 — empty skeletons for now) ──────────────
pub mod diff;
pub mod edit_expr;
pub mod history;
pub mod persist;
pub mod query;
pub mod refactor;

// ── Stage 3 — generic scaffolding for the simple formats ─────────────
pub mod default_backend;
pub mod schema;
pub mod simple;
