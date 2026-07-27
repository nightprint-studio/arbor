//! `emit` domain — deterministic SQL generation.
//!
//! The handlers are thin on purpose: all the logic is in `picus-emit`, where it is
//! testable without a process, and where the golden tests live. What crosses the
//! seam here is a dialect-free model in and one string per target out.
//!
//! This is the backend half of the product requirement that generation involves **no
//! language model anywhere in the flow**. Same input, byte-identical output, every
//! time — which is what makes a generated block reviewable in a diff.

use picus_ast::prelude::{Column, DmlModel, Target};
use picus_core::prelude::PicusState;
use picus_emit::prelude::{emit_for_target, validate_value};
use serde::Serialize;

/// One target's generated SQL, plus anything about the target's own rules that
/// would make it wrong.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedTarget {
    pub target_id: String,
    pub sql: String,
    /// Why this target's rules can't all apply — e.g. a version guard on a target
    /// that writes bare statements, which has nothing to return from. Reported
    /// rather than silently dropped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_conflict: Option<String>,
}

/// Generate the SQL for every target of one model.
///
/// One call, N results: the point of the product is that a change is described once
/// and lands correctly in each destination, so emitting per target in a loop on the
/// frontend would be re-implementing that guarantee client-side.
#[arbor_rpc::handler]
fn picus_emit(
    _state: &PicusState,
    model: DmlModel,
    targets: Vec<Target>,
) -> Result<Vec<EmittedTarget>, String> {
    Ok(targets
        .iter()
        .map(|t| EmittedTarget {
            target_id: t.id.clone(),
            sql: emit_for_target(&model, t),
            rule_conflict: t.rule_conflict().map(str::to_string),
        })
        .collect())
}

/// One cell that cannot be written as typed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueProblem {
    /// Index into the model's `rows`.
    pub row: usize,
    pub column: String,
    /// Why, in the user's terms — never a rule identifier.
    pub reason: String,
}

/// Check every supplied value against its column.
///
/// Batched rather than per keystroke: the whole grid is validated in one round
/// trip, so the caller can mark every offending cell at once instead of discovering
/// them one at a time.
#[arbor_rpc::handler]
fn picus_validate_rows(
    _state: &PicusState,
    model: DmlModel,
) -> Result<Vec<ValueProblem>, String> {
    let mut out = Vec::new();
    for (i, row) in model.rows.iter().enumerate() {
        for column in &model.columns {
            let value = row.get(&column.name).map(String::as_str).unwrap_or("");
            if let Some(reason) = validate_value(value, column) {
                out.push(ValueProblem { row: i, column: column.name.clone(), reason });
            }
        }
    }
    Ok(out)
}

/// Check one value — for the live per-cell feedback the value grid gives while
/// typing, where sending the whole model would be wasteful.
#[arbor_rpc::handler]
fn picus_validate_value(
    _state: &PicusState,
    value: String,
    column: Column,
) -> Result<Option<String>, String> {
    Ok(validate_value(&value, &column))
}
