//! [`PgSession`] — one live PostgreSQL connection.
//!
//! ## How a value becomes a cell
//!
//! Statement execution goes through the **simple query protocol**
//! (`Client::simple_query`), which hands every value back as text exactly as the
//! server would print it. That is deliberate, and it is the right trade for a
//! maintenance tool:
//!
//! * a `timestamptz`, a `numeric(38,10)` and a domain type come back looking the
//!   way they will look in the script the user is about to write — no client-side
//!   reformatting silently changing what they see;
//! * an unknown or exotic type can never fail a whole result set, because nothing
//!   is being decoded into a Rust type;
//! * `NULL` stays distinguishable from the empty string, which in a tool that
//!   writes UPDATE statements is not a detail.
//!
//! The cost is that the simple protocol carries no type information. So we ask for
//! it separately with a `prepare` (best-effort: `SET` and multi-statement input
//! aren't preparable, and then we simply show untyped columns) and use it for one
//! thing only — deciding whether a column is numeric, so the grid can right-align
//! it and the value survives as a number rather than a string. Text columns are
//! never parsed as numbers: an account code of `007` must not become `7`.

use std::sync::Mutex;

use async_trait::async_trait;
use picus_db_api::prelude::*;
use tokio_postgres::types::Type;
use tokio_postgres::{Client, SimpleQueryMessage};

use crate::catalog;
use crate::error::map_pg;
use crate::sql::{guard_read_only, quote_ident, quote_qualified};
use crate::tls::TlsChoice;

/// A live session. Shared behind an `Arc`: the handler serving a query and the one
/// serving its cancellation run concurrently, which is the entire point of being
/// able to cancel.
pub struct PgSession {
    client: Client,
    /// Cancellation key issued by the server at connect time. Cancelling opens a
    /// *second*, short-lived connection — which is why it works while the first one
    /// is busy.
    cancel_token: tokio_postgres::CancelToken,
    tls: TlsChoice,
    spec: ConnectionSpec,
    server_version: String,
    /// Set when the connection is torn down, so `status()` stops claiming health.
    closed: Mutex<bool>,
}

impl PgSession {
    pub(crate) fn new(
        client: Client,
        tls: TlsChoice,
        spec: ConnectionSpec,
        server_version: String,
    ) -> Self {
        let cancel_token = client.cancel_token();
        Self { client, cancel_token, tls, spec, server_version, closed: Mutex::new(false) }
    }

    /// The schema this session browses: the configured one, or PostgreSQL's default.
    fn schema(&self) -> &str {
        if self.spec.schema.is_empty() {
            "public"
        } else {
            &self.spec.schema
        }
    }

    fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Column types for a statement, when it is preparable. `None` is not a
    /// failure — it means "show the columns untyped", which several perfectly good
    /// statements (`SET`, a multi-statement paste) legitimately produce.
    async fn column_types(&self, sql: &str) -> Option<Vec<(String, Type)>> {
        let stmt = self.client.prepare(sql).await.ok()?;
        Some(stmt.columns().iter().map(|c| (c.name().to_string(), c.type_().clone())).collect())
    }
}

#[async_trait]
impl DbSession for PgSession {
    async fn status(&self) -> ConnectionStatus {
        let state = if self.is_closed() || self.client.is_closed() {
            ConnectionState::Disconnected
        } else if self.spec.read_only {
            ConnectionState::ReadOnly
        } else {
            ConnectionState::Connected
        };
        ConnectionStatus {
            id: self.spec.id.clone(),
            state,
            server_version: self.server_version.clone(),
            db_version: String::new(),
            message: String::new(),
        }
    }

    async fn read_schema(&self) -> DbResult<SchemaSnapshot> {
        let schema = self.schema();
        let (tables, views) = catalog::read_relations(&self.client, schema).await?;
        let sequences = catalog::read_sequences(&self.client, schema).await?;
        let triggers = catalog::read_triggers(&self.client, schema).await?;
        Ok(SchemaSnapshot { tables, views, sequences, triggers })
    }

    async fn table_detail(&self, name: &str) -> DbResult<TableInfo> {
        let schema = self.schema();
        let (tables, views) = catalog::read_relations(&self.client, schema).await?;
        let mut info = tables
            .into_iter()
            .chain(views)
            .find(|t| t.name == name)
            .ok_or_else(|| DbError::NotFound(format!("relation {name}")))?;

        match info.kind {
            RelationKind::View => {
                info.definition = catalog::read_view_definition(&self.client, schema, name).await?;
            }
            RelationKind::Table => {
                info.primary_key_name =
                    catalog::read_primary_key_name(&self.client, schema, name).await?;
                info.foreign_keys =
                    Some(catalog::read_foreign_keys(&self.client, schema, name).await?);
                info.indexes = Some(catalog::read_indexes(&self.client, schema, name).await?);
            }
        }
        Ok(info)
    }

    async fn fetch_page(&self, name: &str, offset: u64, limit: u32) -> DbResult<RowPage> {
        // The relation name cannot be a bind parameter, so it is quoted — the one
        // place in this file where a user string reaches SQL as text. `offset` and
        // `limit` are integers and formatted as such.
        let relation = if name.contains('.') {
            quote_qualified(name)
        } else {
            format!("{}.{}", quote_ident(self.schema()), quote_ident(name))
        };
        let sql = format!("SELECT * FROM {relation} OFFSET {offset} LIMIT {limit}");

        let types = self.column_types(&sql).await;
        let (columns, rows) = self.run_simple(&sql, types, limit).await?;

        // The row estimate, not a `count(*)`: drawing a page number is not worth
        // scanning a hundred-million-row table.
        let total = catalog::read_relations(&self.client, self.schema())
            .await
            .ok()
            .and_then(|(t, v)| t.into_iter().chain(v).find(|r| r.name == name))
            .and_then(|r| r.estimated_rows);

        Ok(RowPage { columns, rows, offset, total })
    }

    async fn execute(&self, sql: &str, limit: u32) -> DbResult<QueryResult> {
        // Courtesy check first — a clear product message without a round-trip. The
        // server is still the authority: the session runs in a read-only
        // transaction mode, so anything this misses is refused there.
        guard_read_only(sql, self.spec.read_only)?;

        let started = std::time::Instant::now();
        let types = self.column_types(sql).await;
        let (columns, rows) = self.run_simple(sql, types, limit).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let row_count = rows.len();
        Ok(QueryResult {
            columns,
            rows,
            elapsed_ms,
            row_count,
            truncated: row_count as u32 >= limit,
            command_tag: String::new(),
        })
    }

    async fn cancel(&self) -> DbResult<()> {
        match &self.tls {
            TlsChoice::Plain(connector) => {
                self.cancel_token.cancel_query(connector.clone()).await.map_err(map_pg)
            }
            TlsChoice::Rustls(connector) => {
                self.cancel_token.cancel_query(connector.clone()).await.map_err(map_pg)
            }
        }
    }

    async fn read_db_version(
        &self,
        table: &str,
        column: &str,
        filter: &str,
    ) -> DbResult<Option<String>> {
        if table.is_empty() || column.is_empty() {
            return Ok(None);
        }
        let where_clause =
            if filter.trim().is_empty() { String::new() } else { format!(" WHERE {filter}") };
        let sql = format!(
            "SELECT {}::text FROM {}{where_clause} LIMIT 1",
            quote_ident(column),
            quote_qualified(table)
        );

        match self.client.simple_query(&sql).await {
            Ok(messages) => Ok(messages.into_iter().find_map(|m| match m {
                SimpleQueryMessage::Row(r) => r.get(0).map(|s| s.to_string()),
                _ => None,
            })),
            // A database that is not this project's simply has no version table.
            // That is an ordinary state, not an error to shout about.
            Err(e) => match map_pg(e) {
                DbError::NotFound(_) => Ok(None),
                other => Err(other),
            },
        }
    }

    async fn close(&self) -> DbResult<()> {
        *self.closed.lock().unwrap_or_else(|p| p.into_inner()) = true;
        Ok(())
    }
}

impl PgSession {
    /// Run a statement over the simple protocol and shape the result.
    ///
    /// `types` (from a best-effort `prepare`) decides only which columns are
    /// numeric; the values themselves are always the server's own text.
    async fn run_simple(
        &self,
        sql: &str,
        types: Option<Vec<(String, Type)>>,
        limit: u32,
    ) -> DbResult<(Vec<Column>, Vec<Vec<CellValue>>)> {
        let messages = self.client.simple_query(sql).await.map_err(map_pg)?;

        let mut columns: Vec<Column> = Vec::new();
        let mut numeric: Vec<bool> = Vec::new();
        let mut rows: Vec<Vec<CellValue>> = Vec::new();

        for message in messages {
            match message {
                SimpleQueryMessage::Row(row) => {
                    if columns.is_empty() {
                        // The first row is where the simple protocol reveals the
                        // column names; types (if any) come from the prepare.
                        columns = row
                            .columns()
                            .iter()
                            .enumerate()
                            .map(|(i, c)| Column {
                                name: c.name().to_string(),
                                data_type: types
                                    .as_ref()
                                    .and_then(|t| t.get(i))
                                    .map(|(_, ty)| ty.name().to_string())
                                    .unwrap_or_default(),
                                primary_key: false,
                                not_null: false,
                                default_value: None,
                            })
                            .collect();
                        numeric = (0..columns.len())
                            .map(|i| {
                                types
                                    .as_ref()
                                    .and_then(|t| t.get(i))
                                    .is_some_and(|(_, ty)| is_numeric(ty))
                            })
                            .collect();
                    }
                    if rows.len() as u32 >= limit {
                        continue;
                    }
                    let cells = (0..row.len())
                        .map(|i| cell(row.get(i), numeric.get(i).copied().unwrap_or(false)))
                        .collect();
                    rows.push(cells);
                }
                SimpleQueryMessage::CommandComplete(_) => {}
                _ => {}
            }
        }

        Ok((columns, rows))
    }
}

/// Turn one text value into a cell.
///
/// Numeric columns become numbers so the grid can right-align them with tabular
/// figures; everything else stays the server's text. A `numeric` too wide for an
/// `f64` deliberately stays text rather than being silently rounded — losing
/// precision in a tool that writes SQL is worse than losing the alignment.
fn cell(value: Option<&str>, numeric: bool) -> CellValue {
    let Some(text) = value else { return CellValue::Null };
    if !numeric {
        return CellValue::Text(text.to_string());
    }
    if let Ok(i) = text.parse::<i64>() {
        return CellValue::Int(i);
    }
    match text.parse::<f64>() {
        // Round-trip check: only take the float when it prints back identically,
        // so a high-precision decimal keeps every digit as text.
        Ok(f) if format!("{f}") == text => CellValue::Float(f),
        _ => CellValue::Text(text.to_string()),
    }
}

/// Is this a type the grid should treat as a number?
fn is_numeric(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::INT2 | Type::INT4 | Type::INT8 | Type::FLOAT4 | Type::FLOAT8 | Type::NUMERIC | Type::OID
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_and_empty_string_stay_different() {
        assert_eq!(cell(None, false), CellValue::Null);
        assert_eq!(cell(Some(""), false), CellValue::Text(String::new()));
    }

    #[test]
    fn text_columns_are_never_parsed_as_numbers() {
        // An account code must survive with its leading zeros.
        assert_eq!(cell(Some("007"), false), CellValue::Text("007".to_string()));
    }

    #[test]
    fn numeric_columns_become_numbers() {
        assert_eq!(cell(Some("42"), true), CellValue::Int(42));
        assert_eq!(cell(Some("-1"), true), CellValue::Int(-1));
        assert_eq!(cell(Some("1.5"), true), CellValue::Float(1.5));
    }

    #[test]
    fn a_decimal_too_precise_for_f64_stays_text() {
        let wide = "0.12345678901234567890123456789";
        assert_eq!(
            cell(Some(wide), true),
            CellValue::Text(wide.to_string()),
            "precision matters more than alignment in a tool that writes SQL"
        );
    }
}
