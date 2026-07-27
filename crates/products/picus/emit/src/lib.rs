//! `picus-emit` — deterministic per-dialect SQL emission.
//!
//! One dialect-free [`DmlModel`](picus_ast::prelude::DmlModel) in, one correct
//! statement per destination out. Structured input → model → emission, with **no
//! language model anywhere in the flow**: that is a product requirement, not a
//! preference, and it is what makes the output diffable, testable and identical
//! every time.
//!
//! The dialect comes from the [`Target`](picus_ast::prelude::Target) — that is,
//! from the folder being written into — never from ambient state. One generation
//! produces N files, each correct on its own terms.
//!
//! ## What the engines actually differ on
//!
//! The list this crate has to encode, and all of it:
//!
//! | | Oracle | PostgreSQL |
//! |---|---|---|
//! | Block | `DECLARE … BEGIN … END;` + `/` | `DO $$ … END $$;` |
//! | Upsert | `MERGE … USING (… FROM DUAL)` | `INSERT … ON CONFLICT … DO UPDATE` |
//! | Now | `SYSDATE` | `CURRENT_TIMESTAMP` |
//! | Object exists | `USER_TABLES` count | `to_regclass(…) IS NOT NULL` |
//! | Unquoted identifiers | fold upper | fold lower |
//! | Transaction | the block commits | the block runs inside the caller's |
//!
//! ## Public API: use the [`prelude`]

pub mod block;
pub mod literal;
pub mod prelude;
pub mod statement;

use picus_ast::prelude::{DmlModel, EngineKind, Target, TargetWrap};

/// The SQL one target receives, header comment included.
///
/// Returns a comment rather than an error when there is nothing to write: an empty
/// generator is a normal state on the way to a full one, not a failure.
pub fn emit_for_target(model: &DmlModel, target: &Target) -> String {
    if model.rows.is_empty() {
        return "-- no rows yet: fill in the form, paste some INSERTs, or import a CSV".to_string();
    }

    let engine = match target.dialect {
        EngineKind::Oracle => "Oracle",
        EngineKind::Postgres => "PostgreSQL",
    };
    let role = format!("{:?}", target.role).to_lowercase();
    let header = format!("-- {} · {engine} · {role}\n", model.table);

    let body = match target.wrap {
        TargetWrap::Plain => model
            .rows
            .iter()
            .map(|row| statement::plain_statement(model, row, target.dialect))
            .collect::<Vec<_>>()
            .join("\n\n"),
        TargetWrap::Block => block::block(model, target),
    };

    header + &body
}

#[cfg(test)]
mod tests;
