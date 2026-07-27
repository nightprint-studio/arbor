//! `picus-ast` — Picus's model of a change, with no engine attached.
//!
//! The hinge of the product. A change is described **once** — a table, an
//! operation, some rows, a comparison key — in a form that mentions no dialect.
//! Emission then turns that single description into as many correct statements as
//! there are destinations: the Oracle branch and the PostgreSQL branch, each in its
//! own syntax, from the same source of truth.
//!
//! Two rules this crate exists to keep:
//!
//! * **The model carries no dialect.** [`dml::DmlModel`] has no engine field. The
//!   dialect lives on the [`target::Target`], which gets it from the folder it
//!   belongs to. If it leaked into the model, "the same change in both branches"
//!   would stop being a guarantee and become a coincidence.
//! * **Rules belong to a destination.** A version guard makes sense on an update
//!   script and is meaningless on an initialisation one, so rules live on the
//!   target and never propagate across roles.
//!
//! Depends on `picus-types` and serde. No driver, no SQL, no I/O — the script half
//! of Picus works on an engine it cannot connect to, which is exactly Oracle's
//! situation today.
//!
//! ## Public API: use the [`prelude`]

pub mod dml;
pub mod prelude;
pub mod target;
