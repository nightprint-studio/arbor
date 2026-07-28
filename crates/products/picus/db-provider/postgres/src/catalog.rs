//! Catalogue reads — the `pg_catalog` queries behind the schema browser.
//!
//! `pg_catalog` rather than `information_schema` throughout: it is markedly faster
//! on a large schema, and it is the only place some of what we need (a trigger's
//! timing bitmask, an index's expression columns, a cheap row estimate) exists at
//! all.
//!
//! Every query is parameterised on the schema name. Nothing here interpolates a
//! user string into SQL — object names go through
//! [`quote_ident`](crate::sql::quote_ident) only where a parameter is impossible.

use picus_db_api::prelude::*;
use tokio_postgres::Client;

use crate::error::map_pg;
use crate::sql::{fk_on_delete, trigger_events, trigger_for_each_row, trigger_timing};

/// Relations (tables, partitioned tables, views, materialised views) with their
/// columns, primary-key flags and row estimates — one round-trip for the lot.
///
/// Constraints and indexes are deliberately absent: a schema with hundreds of
/// tables would pay for detail nobody has opened. [`table_detail`] fetches those
/// when a tab actually opens.
pub async fn read_relations(
    client: &Client,
    schema: &str,
) -> DbResult<(Vec<TableInfo>, Vec<TableInfo>)> {
    read_relations_where(client, schema, None).await
}

/// One relation with its columns, or `None` when the schema has no such name.
///
/// The same query as [`read_relations`] with the name pinned. Worth having as its
/// own entry point rather than filtering the full read in Rust: on a catalogue of
/// several hundred relations the full read returns a row per *column* of every one
/// of them — tens of thousands of rows — to answer a question about one.
pub async fn read_relation(
    client: &Client,
    schema: &str,
    name: &str,
) -> DbResult<Option<TableInfo>> {
    let (tables, views) = read_relations_where(client, schema, Some(name)).await?;
    Ok(tables.into_iter().chain(views).next())
}

/// The shared body: every relation in `schema`, or just the one named.
async fn read_relations_where(
    client: &Client,
    schema: &str,
    only: Option<&str>,
) -> DbResult<(Vec<TableInfo>, Vec<TableInfo>)> {
    const SQL: &str = "
        SELECT c.relname,
               c.relkind::text,
               a.attname,
               format_type(a.atttypid, a.atttypmod),
               a.attnotnull,
               pg_get_expr(d.adbin, d.adrelid),
               COALESCE(pk.indisprimary, false),
               c.reltuples::bigint
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
     LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
     LEFT JOIN LATERAL (
               SELECT i.indisprimary
                 FROM pg_index i
                WHERE i.indrelid = c.oid AND i.indisprimary AND a.attnum = ANY (i.indkey)
                LIMIT 1
          ) pk ON true
         WHERE n.nspname = $1
           AND c.relkind IN ('r', 'p', 'v', 'm')
           AND ($2::text IS NULL OR c.relname = $2)
      ORDER BY c.relname, a.attnum";

    let rows = client.query(SQL, &[&schema, &only]).await.map_err(map_pg)?;

    let mut tables: Vec<TableInfo> = Vec::new();
    let mut views: Vec<TableInfo> = Vec::new();

    for row in rows {
        let rel_name: String = row.get(0);
        let relkind: String = row.get(1);
        let is_view = relkind == "v" || relkind == "m";

        let column = Column {
            name: row.get(2),
            data_type: row.get(3),
            not_null: row.get(4),
            default_value: row.get(5),
            primary_key: row.get(6),
        };
        let estimated: i64 = row.get(7);

        let bucket = if is_view { &mut views } else { &mut tables };
        match bucket.last_mut().filter(|t| t.name == rel_name) {
            Some(existing) => existing.columns.push(column),
            None => bucket.push(TableInfo {
                name: rel_name,
                kind: if is_view { RelationKind::View } else { RelationKind::Table },
                columns: vec![column],
                primary_key_name: None,
                foreign_keys: None,
                indexes: None,
                definition: None,
                // `reltuples` is -1 on a relation that has never been analysed.
                // That is "unknown", and it must not render as "empty".
                estimated_rows: (estimated >= 0).then_some(estimated),
            }),
        }
    }

    Ok((tables, views))
}

/// Sequences. `pg_sequences` (PostgreSQL 10+) exposes the live values without the
/// per-sequence `SELECT` an older catalogue would have needed.
pub async fn read_sequences(client: &Client, schema: &str) -> DbResult<Vec<SequenceInfo>> {
    const SQL: &str = "
        SELECT sequencename,
               COALESCE(last_value, start_value),
               increment_by,
               min_value,
               max_value,
               cycle,
               cache_size
          FROM pg_sequences
         WHERE schemaname = $1
      ORDER BY sequencename";

    let rows = client.query(SQL, &[&schema]).await.map_err(map_pg)?;
    Ok(rows
        .into_iter()
        .map(|row| SequenceInfo {
            name: row.get(0),
            last_value: row.get(1),
            increment_by: row.get(2),
            min_value: row.get(3),
            max_value: row.get(4),
            cycle: row.get(5),
            cache_size: row.get(6),
        })
        .collect())
}

/// User triggers (internal ones — the constraint machinery — are excluded).
pub async fn read_triggers(client: &Client, schema: &str) -> DbResult<Vec<TriggerInfo>> {
    const SQL: &str = "
        SELECT t.tgname,
               c.relname,
               t.tgtype::int2,
               t.tgenabled::text
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1
           AND NOT t.tgisinternal
      ORDER BY c.relname, t.tgname";

    let rows = client.query(SQL, &[&schema]).await.map_err(map_pg)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let tgtype: i16 = row.get(2);
            let enabled: String = row.get(3);
            TriggerInfo {
                name: row.get(0),
                table: row.get(1),
                timing: trigger_timing(tgtype).to_string(),
                events: trigger_events(tgtype),
                // 'D' means disabled; every other value is some flavour of enabled.
                enabled: enabled != "D",
                for_each_row: trigger_for_each_row(tgtype),
            }
        })
        .collect())
}

/// A view's defining SELECT, pretty-printed by the server.
pub async fn read_view_definition(
    client: &Client,
    schema: &str,
    name: &str,
) -> DbResult<Option<String>> {
    const SQL: &str = "
        SELECT pg_get_viewdef(c.oid, true)
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('v', 'm')";

    let rows = client.query(SQL, &[&schema, &name]).await.map_err(map_pg)?;
    Ok(rows.first().map(|r| r.get(0)))
}

/// Outgoing foreign keys of one relation, with the column order preserved.
///
/// `unnest(… ) WITH ORDINALITY` is what preserves it: `conkey` is an array whose
/// order is the constraint's, and aggregating without the ordinal would silently
/// reorder a composite key — which would then generate a join predicate that
/// compiles and is wrong.
pub async fn read_foreign_keys(
    client: &Client,
    schema: &str,
    table: &str,
) -> DbResult<Vec<ForeignKey>> {
    const SQL: &str = "
        SELECT con.conname,
               con.confdeltype::text,
               ref.relname,
               (SELECT array_agg(att.attname ORDER BY k.ord)
                  FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
                  JOIN pg_attribute att
                    ON att.attrelid = con.conrelid AND att.attnum = k.attnum),
               (SELECT array_agg(att.attname ORDER BY k.ord)
                  FROM unnest(con.confkey) WITH ORDINALITY AS k(attnum, ord)
                  JOIN pg_attribute att
                    ON att.attrelid = con.confrelid AND att.attnum = k.attnum)
          FROM pg_constraint con
          JOIN pg_class c ON c.oid = con.conrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_class ref ON ref.oid = con.confrelid
         WHERE n.nspname = $1 AND c.relname = $2 AND con.contype = 'f'
      ORDER BY con.conname";

    let rows = client.query(SQL, &[&schema, &table]).await.map_err(map_pg)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let del: String = row.get(1);
            ForeignKey {
                name: row.get(0),
                on_delete: del.bytes().next().and_then(|b| fk_on_delete(b as i8)),
                referenced_table: row.get(2),
                columns: row.get(3),
                referenced_columns: row.get(4),
            }
        })
        .collect())
}

/// Indexes of one relation.
///
/// Columns come from `pg_get_indexdef(oid, n, true)` rather than from `indkey`,
/// because that is the only form that renders an **expression** index readably —
/// `lower(email)` instead of a `0` standing for "not a plain column".
pub async fn read_indexes(client: &Client, schema: &str, table: &str) -> DbResult<Vec<IndexInfo>> {
    const SQL: &str = "
        SELECT i.relname,
               ix.indisunique,
               ix.indisprimary,
               am.amname,
               ARRAY(SELECT pg_get_indexdef(ix.indexrelid, k, true)
                       FROM generate_series(1, ix.indnatts) AS k)
          FROM pg_index ix
          JOIN pg_class i ON i.oid = ix.indexrelid
          JOIN pg_class c ON c.oid = ix.indrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_am am ON am.oid = i.relam
         WHERE n.nspname = $1 AND c.relname = $2
      ORDER BY i.relname";

    let rows = client.query(SQL, &[&schema, &table]).await.map_err(map_pg)?;
    Ok(rows
        .into_iter()
        .map(|row| IndexInfo {
            name: row.get(0),
            unique: row.get(1),
            primary_key: row.get(2),
            kind: Some(row.get(3)),
            columns: row.get(4),
        })
        .collect())
}

/// The name of a relation's primary-key constraint, when it has one.
pub async fn read_primary_key_name(
    client: &Client,
    schema: &str,
    table: &str,
) -> DbResult<Option<String>> {
    const SQL: &str = "
        SELECT con.conname
          FROM pg_constraint con
          JOIN pg_class c ON c.oid = con.conrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $1 AND c.relname = $2 AND con.contype = 'p'";

    let rows = client.query(SQL, &[&schema, &table]).await.map_err(map_pg)?;
    Ok(rows.first().map(|r| r.get(0)))
}
