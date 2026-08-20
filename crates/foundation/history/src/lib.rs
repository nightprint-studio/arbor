//! `arbor-history` — local history: a content store of file revisions, independent of
//! any version-control system.
//!
//! It exists because git protects you from the moment you commit onwards, and most of
//! the ways a file is lost happen before that: a save over something you wanted, a
//! refactor that went wide, a delete of a file that was never committed at all, a tool
//! that rewrote the buffer while you were looking elsewhere. This is the layer under
//! that — the one an IDE's undo reaches for when the undo stack is not enough.
//!
//! ## What it speaks
//!
//! **Paths and bytes.** It has no notion of a source file, a project kind, a language or
//! a VCS, and no dependency on Tauri or on any product. Bennu is the first consumer;
//! Garrulus and Picus are the reason the boundary is drawn here rather than inside it.
//! Anything that looks like policy — should this file be recorded at all? is it ignored
//! by git? — belongs to the caller, which is the only side that can know.
//!
//! ## Shape
//!
//! One [`HistoryStore`](store::HistoryStore) per project. Content lands in
//! content-addressed [`blobs`], and each file gets an append-only revision
//! [`log`]. Names are hashes, so a [`log::Index`] maps them back to paths — which is
//! also what lets a **deleted** file still have a readable history, since its identity
//! stops depending on it existing.

pub mod blobs;
pub mod diff;
pub mod error;
pub mod log;
pub mod model;
pub mod prelude;
pub mod store;
