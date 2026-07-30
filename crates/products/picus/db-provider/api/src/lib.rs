//! `picus-db-api` — the contract every database engine implements for Picus.
//!
//! The model is `corvus/git-provider/api`, which does the same job for GitHub and
//! GitLab: one trait, one registry, one set of wire types, and one **descriptor**
//! that turns per-engine differences into data the UI reads instead of branches the
//! UI hardcodes.
//!
//! ```text
//! picus/db-provider/api        ← you are here: traits + types + descriptor
//! picus/db-provider/postgres   ← the first implementation
//! picus/db-provider/oracle     ← later, and additive: nothing above it changes
//! ```
//!
//! ## What this crate refuses to know
//!
//! No driver, no SQL, no keychain, no Tauri. A provider crate brings its own
//! driver; the secret arrives through [`secret::SecretResolver`], which `picus-be`
//! implements over the shell's credential broker. That is what keeps this crate
//! honest — and what makes it testable without a database.
//!
//! ## Two invariants worth restating here
//!
//! * **The engine is never ambient.** [`kind::EngineKind`] travels as a parameter,
//!   attached to the connection or the folder being acted on. See
//!   `docs/picus-design.md` §1.
//! * **A read-only connection is enforced server-side**, in
//!   [`provider::DbSession::execute`] — never by hiding a button.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate through
//! `picus_db_api::prelude::...`.

pub mod activity;
pub mod capability;
pub mod connection;
pub mod depends;
pub mod descriptor;
pub mod error;
pub mod plan;
pub mod prelude;
pub mod provider;
pub mod query;
pub mod registry;
pub mod secret;
pub mod tx;

// The engine kind and the schema shapes live in `picus-types`, the leaf both
// halves of the product share — a column's type as the server reported it IS the
// type a generated statement must respect, so there is one of each in the whole
// product. Re-exported here so a driver-side call site never has to name a third
// crate.
pub use picus_types::{kind, schema};
