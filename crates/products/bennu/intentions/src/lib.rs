//! `bennu-intentions` — the Bennu editor's **Alt+Enter intentions** (context quick-fixes) as pure
//! Java source transforms.
//!
//! Each intention is `(&str source, usize caret_byte_offset) -> Option<Edit>`: a light,
//! string/comment-aware byte scanner finds the smallest construct under the caret and returns the
//! byte range to replace + the replacement text. No tree-sitter, no filesystem, no Tauri — so every
//! transform is exhaustively unit-tested here (there is no FE test runner), and the `bennu-be`
//! handlers are trivial wrappers.
//!
//! Current intentions:
//!   * [`log_param`] — parameterize a concatenated logging message
//!     (`logger.info("x " + v)` → `logger.info("x {}", v)`).
//!   * [`np_equals`] — flip `x.equals("lit")` to the NPE-safe `"lit".equals(x)`.
//!   * [`simplify`] — `x.size() == 0` → `x.isEmpty()`, `flag == true` → `flag`,
//!     `!(a == b)` → `a != b`.
//!
//! [`intentions::intentions_at`] is the aggregation seam: it runs every transform at the caret and
//! returns the applicable ones as [`intentions::Offer`]s (id + label + edit), so the editor makes
//! one call and adding a new intention is a one-line registration.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intentions::prelude::...`.

pub mod format;
pub mod import_class;
pub mod intentions;
pub mod log_param;
pub mod np_equals;
pub mod override_stub;
pub mod prelude;
pub mod quick_fix;
mod scan;
pub mod simplify;

/// A single-edit replacement: substitute `source[start..end]` (byte offsets) with `replacement`.
/// The shared return shape of the caret transforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}
