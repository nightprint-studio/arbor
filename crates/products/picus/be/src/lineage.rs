//! `lineage` domain — where a column comes from, when the answer is several views
//! deep.
//!
//! The resolution itself is [`picus_lineage`], which is pure and knows nothing about
//! databases. What is here is the two things it cannot do: read the catalogue and
//! read the views' SQL, and hold on to both so a second trace costs nothing.
//!
//! ## Every definition in one round trip
//!
//! A trace does not know which views it will need until it is already walking them,
//! so fetching them one at a time would be a round trip per level per column. On a
//! schema of a couple of hundred views that is the difference between an answer and
//! a progress bar. So [`DbSession::view_definitions`] brings all of them at once and
//! they are cached for the connection — view definitions change when somebody
//! deploys, not while somebody reads.
//!
//! ## The answer is a deduction and says so
//!
//! Unlike `column_sources` on a result — which is the **server's** own statement
//! about a relation and cannot be wrong — this is read out of the views' SQL and can
//! be. That is why it is asked for explicitly, never computed behind a query, and
//! why every verdict that is not `resolved` carries its reason. Anything rendering
//! it has to keep the two visibly apart.
//!
//! The wire key is `connectionId`, and `#[handler]` decodes each argument by its own
//! identifier — so the wire contract wins over the naming convention, as it does in
//! [`crate::query`]. Hence the module-wide allow; it is the only reason for it.
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::sync::Arc;

use picus_core::prelude::PicusState;
use picus_lineage::prelude::{trace_relation, trace_statement, Catalogue, Lineage};
use picus_parse::prelude::EngineKind;
use picus_db_api::prelude::SchemaSnapshot;

use crate::connections::{find_spec, require_session};

/// The catalogue as the resolver needs it: names folded, definitions in hand.
///
/// Built per call from a snapshot that is already cached and a definition map that
/// is cached beside it, so building one is a walk over what is in memory rather than
/// anything the user waits for.
struct Live {
    /// Folded relation name → its folded column names, in declaration order.
    columns: HashMap<String, Vec<String>>,
    /// Folded view name → its defining `SELECT`.
    definitions: HashMap<String, String>,
}

impl Live {
    fn build(schema: &SchemaSnapshot, definitions: &[(String, String)]) -> Self {
        let mut columns = HashMap::new();
        for relation in schema.tables.iter().chain(schema.views.iter()) {
            columns.insert(
                fold(&relation.name),
                relation.columns.iter().map(|c| fold(&c.name)).collect(),
            );
        }
        Self {
            columns,
            definitions: definitions.iter().map(|(n, sql)| (fold(n), sql.clone())).collect(),
        }
    }
}

impl Catalogue for Live {
    fn is_view(&self, relation: &str) -> Option<bool> {
        let name = unqualified(relation);
        // A relation the catalogue holds a definition for is a view; one it merely
        // knows the columns of is a table. Anything else is genuinely unknown, and
        // the resolver reports it as such rather than treating it as a table — which
        // would end a trail on something that may not be the end at all.
        if self.definitions.contains_key(name) {
            return Some(true);
        }
        self.columns.contains_key(name).then_some(false)
    }

    fn definition(&self, view: &str) -> Option<String> {
        self.definitions.get(unqualified(view)).cloned()
    }

    fn columns(&self, relation: &str) -> Option<Vec<String>> {
        self.columns.get(unqualified(relation)).cloned()
    }
}

/// `SCHEMA.NAME` → `NAME`.
///
/// A connection reads one schema and the catalogue is keyed by bare name, so a
/// qualified reference in a view's SQL has to lose its qualifier to be found. A
/// reference to a *different* schema then looks like a local one — which is a real
/// limit, and the honest place for it is here, next to the reason: the catalogue has
/// nothing else to check it against.
fn unqualified(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// The comparison form: quoted names verbatim, everything else upper-cased — the
/// same rule [`picus_parse`] folds by, so the two agree about what a name is.
fn fold(written: &str) -> String {
    if written.len() >= 2 && written.starts_with('"') && written.ends_with('"') {
        written[1..written.len() - 1].replace("\"\"", "\"")
    } else {
        written.to_uppercase()
    }
}

/// Read the catalogue and every view definition, from cache when they are there.
async fn catalogue_for(state: &PicusState, id: &str) -> Result<Live, String> {
    let session = require_session(state, id)?;

    let schema = match state.schemas().get(id) {
        Some(held) => held,
        None => {
            let read = session.read_schema().await.map_err(|e| e.to_string())?;
            let held = Arc::new(read);
            state.schemas().put(id, Arc::clone(&held));
            held
        }
    };

    let definitions = match state.schemas().definitions(id) {
        Some(held) => held,
        None => {
            let read = session.view_definitions().await.map_err(|e| e.to_string())?;
            let held = Arc::new(read);
            state.schemas().put_definitions(id, Arc::clone(&held));
            held
        }
    };

    Ok(Live::build(&schema, &definitions))
}

/// The engine a connection speaks, for the dialect the definitions are parsed as.
fn engine_of(id: &str) -> EngineKind {
    find_spec(id).map(|spec| spec.engine).unwrap_or(EngineKind::Postgres)
}

/// Trace every column of one relation back to the tables behind it.
///
/// For a view. A table answers with nothing traced, which is true — there is nothing
/// behind a table — and is what lets the interface offer this on any relation
/// without first asking what kind it is.
#[arbor_rpc::handler]
async fn picus_relation_lineage(
    state: &PicusState,
    connectionId: String,
    relation: String,
) -> Result<Lineage, String> {
    let catalogue = catalogue_for(state, &connectionId).await?;
    Ok(trace_relation(&catalogue, &fold(&relation), engine_of(&connectionId)))
}

/// Trace the columns one statement projects.
///
/// What the result on screen is traced by: the statement's own `FROM` is the first
/// step, so a `SELECT` over a view is followed through it and onward.
#[arbor_rpc::handler]
async fn picus_statement_lineage(
    state: &PicusState,
    connectionId: String,
    sql: String,
) -> Result<Lineage, String> {
    let catalogue = catalogue_for(state, &connectionId).await?;
    Ok(trace_statement(&catalogue, &sql, engine_of(&connectionId)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_db_api::prelude::{Column, RelationKind, TableInfo};

    fn relation(name: &str, kind: RelationKind, columns: &[&str]) -> TableInfo {
        TableInfo {
            name: name.to_string(),
            kind,
            columns: columns
                .iter()
                .map(|c| Column {
                    name: c.to_string(),
                    data_type: "text".into(),
                    primary_key: false,
                    not_null: false,
                    default_value: None,
                })
                .collect(),
            primary_key_name: None,
            foreign_keys: None,
            indexes: None,
            definition: None,
            estimated_rows: None,
        }
    }

    #[test]
    fn a_qualified_reference_is_found_by_its_bare_name() {
        // A view's SQL may write `public.tab_tipi`; the catalogue is keyed bare.
        let schema = SchemaSnapshot {
            tables: vec![relation("tab_tipi", RelationKind::Table, &["cenint"])],
            ..Default::default()
        };
        let live = Live::build(&schema, &[]);
        assert_eq!(live.is_view("PUBLIC.TAB_TIPI"), Some(false));
        assert_eq!(live.columns("PUBLIC.TAB_TIPI"), Some(vec!["CENINT".to_string()]));
    }

    #[test]
    fn a_relation_with_a_definition_is_a_view_and_one_without_is_a_table() {
        let schema = SchemaSnapshot {
            tables: vec![relation("gare", RelationKind::Table, &["ngara"])],
            views: vec![relation("v_gare", RelationKind::View, &["n"])],
            ..Default::default()
        };
        let live = Live::build(&schema, &[("v_gare".into(), "SELECT ngara AS n FROM gare".into())]);
        assert_eq!(live.is_view("V_GARE"), Some(true));
        assert_eq!(live.is_view("GARE"), Some(false));
        // Not in the catalogue at all is a third answer, and the resolver needs it
        // to say "another schema, or newer than the catalogue" instead of "a table".
        assert_eq!(live.is_view("ALTRO"), None);
    }

    #[test]
    fn a_quoted_name_keeps_its_case_and_a_bare_one_does_not() {
        assert_eq!(fold("tab_tipi"), "TAB_TIPI");
        assert_eq!(fold("\"Mixed Case\""), "Mixed Case");
        assert_eq!(fold("\"has\"\"quote\""), "has\"quote");
    }
}
