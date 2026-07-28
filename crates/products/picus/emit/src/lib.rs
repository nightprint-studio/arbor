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
//! ## Portable destinations: the intersection, or a refusal
//!
//! A folder declared **portable** is one file that has to run on both engines,
//! which is the point — the user's simple inserts stop being written twice. The
//! emitter answers it by taking a [`DialectScope`](picus_ast::prelude::DialectScope)
//! rather than an `EngineKind`, so there is no single engine to quietly default to
//! and every row of the table above had to grow a portable answer:
//!
//! | | Portable |
//! |---|---|
//! | Block | **refused** — no spelling runs on both |
//! | Upsert | **refused** — `MERGE` and `ON CONFLICT` are each foreign to the other |
//! | Now | `CURRENT_TIMESTAMP`, which is standard and both accept |
//! | Unquoted identifiers | left exactly as written; the engines fold oppositely |
//! | Version guard | **refused** — it needs the block to return from |
//!
//! Two of those are `Err`, and that is deliberate rather than unfortunate: a
//! caller cannot forget to check something the type system hands it.
//!
//! ## Public API: use the [`prelude`]

pub mod block;
pub mod literal;
pub mod prelude;
pub mod statement;

use picus_ast::prelude::{DmlModel, Target, TargetWrap};

use crate::statement::EmitResult;

/// The SQL one target receives, header comment included.
///
/// Returns a comment rather than an error when there is nothing to write: an empty
/// generator is a normal state on the way to a full one, not a failure.
///
/// It **does** return an error when the target cannot receive this model at all —
/// a portable destination asked for an upsert, or wrapped in a block. That is not
/// a nicety: the whole guarantee of writing into a portable folder is that nothing
/// engine-specific can land there, and the way to guarantee it is for the emitter
/// to have no way to produce one. There is no `EngineKind` to fall back on in a
/// portable scope, so every dialect-dependent decision below either has a portable
/// answer or is this `Err`.
pub fn emit_for_target(model: &DmlModel, target: &Target) -> EmitResult {
    if model.rows.is_empty() {
        return Ok("-- no rows yet: fill in the form, paste some INSERTs, or import a CSV".to_string());
    }

    let role = format!("{:?}", target.role).to_lowercase();
    let header = format!("-- {} · {} · {role}\n", model.table, target.dialect.label());

    let body = match target.wrap {
        TargetWrap::Plain => model
            .rows
            .iter()
            .map(|row| statement::plain_statement(model, row, target.dialect))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n\n"),
        TargetWrap::Block => block::block(model, target)?,
    };

    Ok(header + &body)
}

#[cfg(test)]
mod tests;
