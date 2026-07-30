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
//!
//! ## Nothing here may return NULL into a non-nullable field
//!
//! `Row::get` **panics** when the value is NULL and the target type is not an
//! `Option`. A panic in a handler used to mean the request was simply never
//! answered — the studio sat on "reading schema…" forever with no error anywhere
//! (`arbor_ipc::serve_stdio` now catches it, which makes such a fault legible
//! rather than silent). The cheaper half of that lesson is here: where a catalogue
//! function can return NULL, the SQL coalesces it, so the value the row carries is
//! the value the type promises. `COALESCE` in the query rather than a default in
//! Rust, because the invariant belongs where the data is produced.

use std::collections::HashMap;

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
               a.attnum,
               -- NULL when the type OID no longer resolves (a dropped extension
               -- leaves columns behind). The column still exists and still has to
               -- be listed; what it has lost is its type name.
               COALESCE(format_type(a.atttypid, a.atttypmod), '?'),
               a.attnotnull,
               pg_get_expr(d.adbin, d.adrelid),
               -- A row on the `pk` side means this column is part of the key.
               (pk.attnum IS NOT NULL),
               COALESCE(c.reltuples, -1)::bigint
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
          JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
     LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
     -- The primary-key flag, as ONE pass over `pg_index` hash-joined to the
     -- columns.
     --
     -- This was a `LEFT JOIN LATERAL … WHERE a.attnum = ANY (i.indkey) LIMIT 1`,
     -- which is a correlated subquery evaluated **once per column of the schema**.
     -- `indkey` is an `int2vector`, so `= ANY` cannot use an index and each one is
     -- a scan of `pg_index`. On a small schema that is invisible; on a real one —
     -- several hundred tables, tens of thousands of columns — it is tens of
     -- thousands of scans, and reading the catalogue stops returning in any time
     -- the user is willing to wait. The studio then sits on its reading-schema
     -- spinner forever, which is indistinguishable from a hang and was one.
     --
     -- Unnesting first turns it into a single sequential scan of a small table:
     -- one row per key column in the database, joined by `(relation, attnum)`.
     LEFT JOIN (
               SELECT i.indrelid, unnest(i.indkey) AS attnum
                 FROM pg_index i
                WHERE i.indisprimary
          ) pk ON pk.indrelid = c.oid AND pk.attnum = a.attnum
         WHERE n.nspname = $1
           AND c.relkind IN ('r', 'p', 'v', 'm')
           -- Partitions are not objects anybody browses: the partitioned table is
           -- the thing, and its thousand monthly children are an implementation
           -- detail of it. Listing them multiplies this query's result by the
           -- partition count — the same columns, over and over — which on a
           -- partitioned schema is the difference between a catalogue read and a
           -- read that does not come back.
           AND NOT c.relispartition";

    // The name filter is **appended**, not passed as a nullable parameter.
    //
    // `AND ($2::text IS NULL OR c.relname = $2)` reads well and plans badly: the
    // statement is prepared, so the planner builds one plan for both shapes and
    // cannot use the equality it might be given. Two statements, each with only the
    // parameters it actually uses, each get the plan they deserve — and the
    // whole-schema read stops depending on how a generic plan happens to come out.
    // **No `ORDER BY`.** The ordering is done in Rust, below, and that is not a
    // preference — it is the difference between a catalogue read that returns and
    // one that does not.
    //
    // Measured on a real schema (15 224 column-rows): unsorted, the server streams
    // the whole thing in 45 ms. With *any* `ORDER BY` — by name, by name with
    // `COLLATE "C"`, or by `oid`, which has no collation at all — it does not come
    // back at all. Sorting forces the result to be materialised before a single row
    // is sent, and on this catalogue the plan the server picks to do that never
    // finishes. Since the cost is the same for an integer key, it is not the
    // comparison: it is the plan, and a tool cannot choose its user's planner.
    //
    // Fifteen thousand rows sorted in Rust are microseconds, so nothing is lost by
    // taking the ordering back — and the query becomes something the server can
    // stream instead of something it has to finish.
    let mut sql = SQL.to_string();
    if only.is_some() {
        sql.push_str(" AND c.relname = $2");
    }

    let began = std::time::Instant::now();
    let rows = match only {
        Some(name) => client.query(&sql, &[&schema, &name]).await,
        None => client.query(&sql, &[&schema]).await,
    }
    .map_err(map_pg)?;
    // Said for the whole-schema read, always.
    //
    // This is the one query in the product that has been slow enough to look like a
    // hang, and the two possible reasons — a slow plan, or a catalogue with far more
    // in it than anybody expected — are told apart by exactly these two numbers.
    // Printing them costs nothing and has already saved one round of guessing.
    if only.is_none() {
        eprintln!(
            "picus: catalogue of `{schema}` — {} column-rows in {}ms",
            rows.len(),
            began.elapsed().as_millis()
        );
    }

    // Grouped by name rather than by adjacency, and ordered afterwards.
    //
    // The old loop assumed a relation's columns arrived together, which is what the
    // `ORDER BY` was really buying — the ordering was load-bearing, not cosmetic.
    // Collecting into a map makes the grouping true whatever order the server
    // streams in, and carrying `attnum` alongside each column is what lets the
    // declared column order be restored without asking the server to sort.
    let mut collected: HashMap<String, (bool, i64, Vec<(i16, Column)>)> = HashMap::new();

    for row in rows {
        let rel_name: String = row.get(0);
        let relkind: String = row.get(1);
        let is_view = relkind == "v" || relkind == "m";
        let attnum: i16 = row.get(3);
        let estimated: i64 = row.get(8);

        let column = Column {
            name: row.get(2),
            data_type: row.get(4),
            not_null: row.get(5),
            default_value: row.get(6),
            primary_key: row.get(7),
        };

        collected
            .entry(rel_name)
            .or_insert_with(|| (is_view, estimated, Vec::new()))
            .2
            .push((attnum, column));
    }

    let mut tables: Vec<TableInfo> = Vec::new();
    let mut views: Vec<TableInfo> = Vec::new();
    for (name, (is_view, estimated, mut columns)) in collected {
        // A table's columns have an order of their own — the one every `INSERT`
        // written by hand assumes — and it is `attnum`, never the name.
        columns.sort_by_key(|(attnum, _)| *attnum);
        let info = TableInfo {
            name,
            kind: if is_view { RelationKind::View } else { RelationKind::Table },
            columns: columns.into_iter().map(|(_, c)| c).collect(),
            primary_key_name: None,
            foreign_keys: None,
            indexes: None,
            definition: None,
            // `reltuples` is -1 on a relation that has never been analysed. That is
            // "unknown", and it must not render as "empty".
            estimated_rows: (estimated >= 0).then_some(estimated),
        };
        if is_view { views.push(info) } else { tables.push(info) }
    }

    // By name, because that is the order the object tree lists them in.
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    views.sort_by(|a, b| a.name.cmp(&b.name));
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
            min_value: bounded(row.get(3), i64::MIN),
            max_value: bounded(row.get(4), i64::MAX),
            cycle: row.get(5),
            cache_size: row.get(6),
        })
        .collect())
}

/// A bound the sequence never actually reaches is not a bound.
///
/// A bigint sequence that was never given a `MAXVALUE` reports `i64::MAX`, and
/// printing 9,223,372,036,854,775,807 as a fact is wrong twice: it is the type's
/// limit rather than this sequence's, and it does not survive the trip — JSON
/// numbers are doubles, so the browser receives 9,223,372,036,854,776,000 and
/// would show a number no one ever wrote. `None` reads as "no limit", which is
/// both true and representable.
fn bounded(value: i64, sentinel: i64) -> Option<i64> {
    (value != sentinel).then_some(value)
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

/// One trigger's `CREATE TRIGGER` and the source of the routine it fires.
///
/// `pg_get_functiondef` **raises** on a routine written in C or in `internal`, so it
/// is reached only through a `CASE` that has already looked at the language —
/// `CASE` evaluates only the branch it takes, which is the whole reason the check
/// can live in the same round trip as the answer.
///
/// A trigger name is unique per table, not per schema, and the schema snapshot keys
/// triggers by name alone; two tables carrying a trigger of the same name resolve to
/// the first by table name, deterministically rather than arbitrarily.
pub async fn read_trigger_detail(
    client: &Client,
    schema: &str,
    name: &str,
) -> DbResult<Option<TriggerDetail>> {
    const SQL: &str = "
        SELECT pg_get_triggerdef(t.oid),
               COALESCE(p.proname, ''),
               COALESCE(
                 CASE WHEN l.lanname NOT IN ('c', 'internal')
                      THEN pg_get_functiondef(t.tgfoid)
                 END, '')
          FROM pg_trigger t
          JOIN pg_class c ON c.oid = t.tgrelid
          JOIN pg_namespace n ON n.oid = c.relnamespace
          LEFT JOIN pg_proc p ON p.oid = t.tgfoid
          LEFT JOIN pg_language l ON l.oid = p.prolang
         WHERE n.nspname = $1
           AND t.tgname = $2
           AND NOT t.tgisinternal
      ORDER BY c.relname
         LIMIT 1";

    let rows = client.query(SQL, &[&schema, &name]).await.map_err(map_pg)?;
    Ok(rows.first().map(|row| TriggerDetail {
        definition: row.get(0),
        function_name: row.get(1),
        function_body: row.get(2),
    }))
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
    // `and_then`, not `map`: a row exists but its value may be NULL, and the two
    // absences collapse to the same `None` the caller already handles.
    Ok(rows.first().and_then(|r| r.get(0)))
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
               -- `array_agg` over no rows is NULL, not an empty array. A key with
               -- no readable columns is a key we cannot describe, not a reason to
               -- fail the whole read.
               COALESCE((SELECT array_agg(att.attname::text ORDER BY k.ord)
                  FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
                  JOIN pg_attribute att
                    ON att.attrelid = con.conrelid AND att.attnum = k.attnum), '{}'),
               COALESCE((SELECT array_agg(att.attname::text ORDER BY k.ord)
                  FROM unnest(con.confkey) WITH ORDINALITY AS k(attnum, ord)
                  JOIN pg_attribute att
                    ON att.attrelid = con.confrelid AND att.attnum = k.attnum), '{}')
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
               am.amname::text,
               -- `pg_get_indexdef` answers NULL for a column position it cannot
               -- render, and a NULL *inside* the array is what would fail the
               -- decode — the array itself being empty is fine.
               --
               -- `ORDER BY k` is not decoration: these are the index's columns in
               -- the index's order, and an index reported as (b, a) when it is
               -- (a, b) is a wrong answer to the question people open this panel
               -- to ask.
               ARRAY(SELECT s.d
                       FROM generate_series(1, ix.indnatts) AS k,
                            LATERAL (SELECT pg_get_indexdef(ix.indexrelid, k, true) AS d) s
                      WHERE s.d IS NOT NULL
                      ORDER BY k)
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
    Ok(rows.first().and_then(|r| r.get(0)))
}
