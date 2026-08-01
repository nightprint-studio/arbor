//! The Garrulus vault index.
//!
//! Built when a vault is opened, updated per-note on save. It is a **cache**:
//! a corrupt or missing index is a rebuild, never an error, and no answer it
//! gives is authoritative over the files on disk (docs/garrulus-design.md §5.2).
//!
//! Module map:
//!
//! | module       | responsibility                                              |
//! |--------------|-------------------------------------------------------------|
//! | `note_view`  | the *only* place that reads a `garrulus_vault::Note`         |
//! | `graph`      | forward link edges, backlinks, unresolved links, mentions    |
//! | `text`       | inverted word index + snippet extraction                     |
//! | `fuzzy`      | subsequence scorer for the quick switcher                    |
//! | `query`      | `parse_query` and filter application                         |
//! | `problems`   | broken links, orphans, untyped notes, duplicate titles       |
//! | `index`      | `Index`, which owns all of the above                         |
//!
//! Consumers import from [`prelude`], never from the submodules.

pub mod fuzzy;
pub mod graph;
pub mod index;
pub mod note_view;
pub mod prelude;
pub mod problems;
pub mod query;
pub mod text;
