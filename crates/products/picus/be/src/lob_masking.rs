//! `lob_masking` — deciding, before a read runs, how its large objects are handled.
//!
//! A masked large-object cell shows its size and is read whole only when clicked —
//! which needs the row it sits in to be *addressable*. This module works out whether
//! it is, and rewrites the statement when a small change makes it so:
//!
//! * **A keyed table whose key the query already selected** (or `SELECT *`) — mask,
//!   nothing to do. The key is in the result, so a cell can be read back by it.
//! * **A keyed table whose key the query left out** — splice the key columns into the
//!   projection as **hidden** columns, so the fetch has them without the user seeing
//!   an id they did not ask for.
//! * **A table with no primary key** — splice its `ctid`, the engine's own internal
//!   row address, in as the hidden key. The masking stays on and the grid stays
//!   coherent; nothing is materialised.
//! * **Anything with no single physical row** — a join, a view, a CTE, a computed or
//!   aggregated result — is genuinely not addressable, so masking is turned **off**
//!   and the value is shown instead of a size that could never be opened.
//!
//! Everything the parser is unsure about lands in that last bucket (masking off),
//! which is always safe: the only cost is a value shown where a size would have been.
//! Injecting into a shape that could not take a column is the thing that must never
//! happen, and [`picus_parse::prelude::SelectShape`] is conservative for exactly that
//! reason.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::LobMasking;
use picus_emit::prelude::ident;
use picus_parse::prelude::{parse, DialectScope, EngineKind, SelectShape, StatementKind};
use picus_rewrite::prelude::{apply_splices, Splice};

use crate::connections::find_spec;
use crate::source_relation::source_names;

/// The engine's internal row address — the key of last resort, for a table that has
/// no primary key of its own.
const CTID: &str = "ctid";

/// How a read should be run: the (possibly rewritten) SQL, whether large objects may
/// be masked, which injected columns to hide, and the key a masked cell is read by.
pub(crate) struct LobPlan {
    pub sql: String,
    pub masking: LobMasking,
    /// Injected key columns, hidden from the grid. Sit at the end of the projection.
    pub hidden: Vec<String>,
    /// The columns that address one row, for reading a masked cell back.
    pub row_key: Vec<String>,
}

impl LobPlan {
    /// Run it as written, masking on — the behaviour before this module existed, for
    /// everything that is not a single-source SELECT.
    fn passthrough(sql: &str) -> Self {
        Self { sql: sql.to_string(), masking: LobMasking::Auto, hidden: Vec::new(), row_key: Vec::new() }
    }

    /// Do not mask — the rows are not addressable, so a size would be a dead end.
    fn off(sql: &str) -> Self {
        Self { sql: sql.to_string(), masking: LobMasking::Off, hidden: Vec::new(), row_key: Vec::new() }
    }
}

/// Plan the large-object handling for a statement about to run on a connection.
pub(crate) fn plan_lob_read(sql: &str, connection_id: &str, state: &PicusState) -> LobPlan {
    let scope = find_spec(connection_id)
        .map(|spec| DialectScope::One(spec.engine))
        .unwrap_or(DialectScope::One(EngineKind::Postgres));

    let parsed = parse(sql, scope);
    // Exactly one statement, a read, no syntax error. A write, a paste of several
    // statements, or something the parser choked on runs exactly as it did before.
    let [statement] = parsed.statements.as_slice() else {
        return LobPlan::passthrough(sql);
    };
    if statement.kind != StatementKind::Select || statement.has_error {
        return LobPlan::passthrough(sql);
    }

    // Exactly one source, and the catalogue must call it a base table — a view or a
    // CTE has no row of its own to key on.
    let names = source_names(statement);
    let [name] = names.as_slice() else {
        return LobPlan::off(sql);
    };
    let Some(pk) = table_pk(state, connection_id, name) else {
        return LobPlan::off(sql);
    };

    // Every SELECT carries a shape; the default is the conservative "do not inject".
    let shape = statement.select.clone().unwrap_or_default();
    plan_injection(sql, scope, &shape, &pk)
}

/// The pure decision, given a single base table's primary key (empty = the table has
/// none) and the projection shape. Separated from the catalogue lookup so it can be
/// tested without a connection.
fn plan_injection(sql: &str, scope: DialectScope, shape: &SelectShape, pk: &[String]) -> LobPlan {
    if !pk.is_empty() {
        // A keyed table. If the key is already in the result, mask and read cells
        // back by it — no rewrite.
        if shape.star || pk.iter().all(|k| output_has(shape, k)) {
            return LobPlan {
                sql: sql.to_string(),
                masking: LobMasking::Auto,
                hidden: Vec::new(),
                row_key: pk.to_vec(),
            };
        }
        if shape.not_injectable {
            return LobPlan::off(sql);
        }
        // Splice the columns the query left out, hidden.
        let missing: Vec<String> =
            pk.iter().filter(|k| !output_has(shape, k)).cloned().collect();
        let refs: Vec<String> = missing.iter().map(|k| ident(k, scope, false)).collect();
        LobPlan {
            sql: inject(sql, shape.select_list_end, &refs),
            masking: LobMasking::Auto,
            hidden: missing,
            row_key: pk.to_vec(),
        }
    } else {
        // No key of its own — address rows by the engine's internal `ctid`.
        if shape.not_injectable {
            return LobPlan::off(sql);
        }
        // `ctid` is a system column, so `*` never includes it; inject unless the
        // query somehow already selected it.
        if output_has(shape, CTID) {
            return LobPlan {
                sql: sql.to_string(),
                masking: LobMasking::Auto,
                hidden: Vec::new(),
                row_key: vec![CTID.to_string()],
            };
        }
        LobPlan {
            sql: inject(sql, shape.select_list_end, &[CTID.to_string()]),
            masking: LobMasking::Auto,
            hidden: vec![CTID.to_string()],
            row_key: vec![CTID.to_string()],
        }
    }
}

/// Is a column already one of the projection's output names? Case-insensitive, which
/// inherits the same alias-shadowing gap `editability` has and does not widen it.
fn output_has(shape: &SelectShape, name: &str) -> bool {
    shape.outputs.iter().any(|o| o.eq_ignore_ascii_case(name))
}

/// Splice `, <ref>` for each ref at the end of the projection. The columns are
/// unqualified: the statement reads from one table, so the names are unambiguous
/// whether or not the `FROM` aliased it.
fn inject(sql: &str, at: usize, refs: &[String]) -> String {
    let text: String = refs.iter().map(|r| format!(", {r}")).collect();
    let splice =
        Splice::insert(at, text, "picus: inject row key so a large-object cell can be addressed");
    apply_splices(sql, &[splice]).unwrap_or_else(|_| sql.to_string())
}

/// The primary-key column names of a single base table, or `None` when the name is
/// not a table on this connection (a view, a CTE, an unread schema).
fn table_pk(state: &PicusState, connection_id: &str, name: &str) -> Option<Vec<String>> {
    let schema = state.schemas().get(connection_id)?;
    let info = schema.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name))?;
    Some(info.columns.iter().filter(|c| c.primary_key).map(|c| c.name.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(select_list_end: usize, star: bool, outputs: &[&str], not_injectable: bool) -> SelectShape {
        SelectShape {
            select_list_end,
            star,
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            not_injectable,
        }
    }

    fn pg() -> DialectScope {
        DialectScope::One(EngineKind::Postgres)
    }

    #[test]
    fn a_star_over_a_keyed_table_masks_and_keys_by_the_pk() {
        let s = shape(8, true, &[], false);
        let plan = plan_injection("SELECT * FROM t", pg(), &s, &["id".to_string()]);
        assert_eq!(plan.masking, LobMasking::Auto);
        assert!(plan.hidden.is_empty());
        assert_eq!(plan.row_key, vec!["id".to_string()]);
        assert_eq!(plan.sql, "SELECT * FROM t");
    }

    #[test]
    fn a_key_already_projected_is_not_injected_again() {
        let s = shape(20, false, &["ID", "NOME"], false);
        let plan = plan_injection("SELECT id, nome FROM t", pg(), &s, &["id".to_string()]);
        assert!(plan.hidden.is_empty());
        assert_eq!(plan.row_key, vec!["id".to_string()]);
        assert_eq!(plan.sql, "SELECT id, nome FROM t");
    }

    #[test]
    fn a_missing_key_is_spliced_in_hidden() {
        // "SELECT nome, allegato FROM documenti" — select_list ends after "allegato".
        let sql = "SELECT nome, allegato FROM documenti";
        let end = "SELECT nome, allegato".len();
        let s = shape(end, false, &["NOME", "ALLEGATO"], false);
        let plan = plan_injection(sql, pg(), &s, &["id".to_string()]);
        assert_eq!(plan.masking, LobMasking::Auto);
        assert_eq!(plan.hidden, vec!["id".to_string()]);
        assert_eq!(plan.row_key, vec!["id".to_string()]);
        assert_eq!(plan.sql, "SELECT nome, allegato, id FROM documenti");
    }

    #[test]
    fn a_keyless_table_is_addressed_by_ctid() {
        let sql = "SELECT nome, allegato FROM aperta";
        let end = "SELECT nome, allegato".len();
        let s = shape(end, false, &["NOME", "ALLEGATO"], false);
        let plan = plan_injection(sql, pg(), &s, &[]);
        assert_eq!(plan.masking, LobMasking::Auto);
        assert_eq!(plan.hidden, vec!["ctid".to_string()]);
        assert_eq!(plan.row_key, vec!["ctid".to_string()]);
        assert_eq!(plan.sql, "SELECT nome, allegato, ctid FROM aperta");
    }

    #[test]
    fn a_keyless_star_still_injects_ctid_because_star_excludes_it() {
        let s = shape("SELECT *".len(), true, &[], false);
        let plan = plan_injection("SELECT * FROM aperta", pg(), &s, &[]);
        assert_eq!(plan.hidden, vec!["ctid".to_string()]);
        assert_eq!(plan.row_key, vec!["ctid".to_string()]);
        assert_eq!(plan.sql, "SELECT *, ctid FROM aperta");
    }

    #[test]
    fn a_shape_that_cannot_take_a_column_turns_masking_off() {
        let s = shape(0, false, &["NOME"], true);
        // Keyed but not injectable, and keyless but not injectable, both off.
        assert_eq!(plan_injection("x", pg(), &s, &["id".to_string()]).masking, LobMasking::Off);
        assert_eq!(plan_injection("x", pg(), &s, &[]).masking, LobMasking::Off);
    }

    #[test]
    fn multiple_missing_keys_are_all_injected() {
        let sql = "SELECT nome FROM t";
        let end = "SELECT nome".len();
        let s = shape(end, false, &["NOME"], false);
        let plan = plan_injection(sql, pg(), &s, &["a".to_string(), "b".to_string()]);
        assert_eq!(plan.sql, "SELECT nome, a, b FROM t");
        assert_eq!(plan.hidden, vec!["a".to_string(), "b".to_string()]);
    }
}
