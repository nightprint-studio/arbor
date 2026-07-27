//! `picus-types` — the vocabulary both halves of Picus share.
//!
//! Picus is a database client **and** a maintainer of the scripts those databases
//! are installed from, and the two halves meet in the generator: the schema read
//! from a live connection feeds the DML that is written into the scripts. So a
//! handful of types have to be the same on both sides — a column's type as the
//! server reported it *is* the type the generated statement must respect.
//!
//! This crate holds exactly those, and nothing else:
//!
//! * [`kind::EngineKind`] — the engine a connection speaks and the dialect a
//!   folder is written in. One type, because those must never drift apart.
//! * [`schema`] — what a database says about itself.
//!
//! ## What it deliberately is not
//!
//! A leaf. `serde` and nothing more: no driver, no SQL, no I/O, no async. Both
//! `picus-db-api` (the driver contract) and `picus-ast` (the script model) depend
//! on it, which is what keeps the script half free of any dependency on drivers —
//! the thing that makes Oracle a first-class engine with no Oracle driver.
//!
//! Being a leaf is also what keeps it wasm-clean, and it is the slice a "generate
//! SQL" plugin would need.
//!
//! ## Public API: use the [`prelude`]

pub mod kind;
pub mod prelude;
pub mod schema;
