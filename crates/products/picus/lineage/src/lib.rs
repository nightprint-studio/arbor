//! `picus-lineage` — where a column actually comes from, when the answer is several
//! views deep.
//!
//! ## The problem
//!
//! A database of two hundred views, many of them selecting from other views, is a
//! perfectly ordinary shape for an application schema. In one the value arrives as
//! `codsa`, three levels down it is `t.cenint`, and the only way to find that out is
//! to read `pg_get_viewdef` by hand and chase aliases. That is the question this
//! crate answers, in one call:
//!
//! ```text
//! CODSA  ←  V_WS_ELENCHI.CODSA  ←  V_TIPI.CENINT  ←  TAB_TIPI.CENINT
//! ```
//!
//! ## What this is not
//!
//! It is **not** what the server reports. PostgreSQL names the relation the
//! statement asked for — a view is reported as itself, because the origin is stamped
//! by the parser before the rewriter expands anything. That answer is authoritative
//! and it is what the result grid colours by normally.
//!
//! This is a **deduction**, made deliberately and on demand, by reading the views'
//! own SQL. It can therefore be wrong in ways the server's answer cannot, and
//! anything presenting it has to keep the two visibly apart. The whole model is
//! built around admitting that: see [`Verdict`], whose three values exist so that
//! "computed from these" and "the trail ends here" never collapse into a confident
//! table name.
//!
//! ## Pure
//!
//! No filesystem, no clock, no driver. The database is a [`Catalogue`] the caller
//! implements, which is what makes every branch of the resolution a unit test rather
//! than a live view somebody has to keep working.
//!
//! ## Public API: use the [`prelude`]

pub mod model;
pub mod prelude;
pub mod resolve;

#[cfg(test)]
mod tests;
