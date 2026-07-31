//! Running a statement whose values travel **beside** it.
//!
//! The whole feature is one sentence: a value the user typed into a box is sent as
//! a *parameter*, not spliced into the SQL text. Splicing means somebody has to
//! quote it, and "somebody has to quote it" is how every injection and every
//! mangled apostrophe in a description field has ever happened. Here the statement
//! reaches the server with `$1` still in it and the value arrives separately, in
//! the wire's own field, where it can never be read as syntax.
//!
//! ## What is given up, stated plainly
//!
//! **A parameterised read does not open a scrollable cursor.** `DECLARE … CURSOR`
//! is a utility statement and PostgreSQL does not let it take parameters, so there
//! is nothing to declare the result over. A bound read therefore comes back as up
//! to `window` rows with `result_id: None` and an honest `end_of_result` — the same
//! shape [`run_direct`](crate::session::PgSession) already produces for a statement
//! that cannot be cursored. The grid shows what arrived and says there is more; it
//! cannot scroll into the rest, and the remedy is the user's own `LIMIT`/`OFFSET`
//! or running it without placeholders.
//!
//! ## Why the statement is wrapped, when it is
//!
//! Parameters force the **extended** protocol, and tokio-postgres asks for binary
//! results there — where the simple protocol this crate reads everywhere else hands
//! every value over as the server's own text (see [`crate::rows`] for why that
//! matters in a maintenance tool). Rather than re-implement PostgreSQL's output
//! functions on this side, the projection is cast: every column comes back as
//! `text`, produced by the very function the server would have used.
//!
//! Wrapping is exactly what [`crate::cursor`] warns about for ordering — a
//! sub-select's `ORDER BY` is dropped when PostgreSQL pulls the subquery up — so
//! the bound goes **inside** the wrapped body ([`bounded_body`]). A subquery
//! carrying `LIMIT`/`OFFSET`/`FETCH`/a locking clause is not pulled up, so its
//! ordering survives; and one of those is always present, because that is precisely
//! the case in which `bounded_body` declines to add one.

use std::error::Error;
use std::time::Instant;

use picus_db_api::prelude::{BindValue, CellValue, Column, DbError, DbResult, ExecuteResult};
// The driver's own `to_sql_checked!` macro names this same path. This crate has no
// direct `bytes` dependency, and taking one purely to spell a parameter type would
// be a dependency added for a signature.
use tokio_postgres::types::private::BytesMut;
use tokio_postgres::types::{Format, IsNull, ToSql, Type};
use tokio_postgres::{Client, Row, Statement};

use crate::cursor::{bounded_body, is_large_object, plan_execution, ExecutionPlan};
use crate::error::map_pg;
use crate::rows;
use crate::sql::{
    guard_read_only, leading_keyword, quote_ident, single_statement, strip_leading_noise,
};

/// Bind `binds` to `sql`'s placeholders and run it.
///
/// Same contract as [`DbSession::execute`](picus_db_api::prelude::DbSession::execute)
/// in every respect but the held result: a write reports what it changed, a read
/// reports its first `window` rows, and a write on a read-only connection is refused
/// here in the product's own words before the server refuses it in its own.
pub async fn execute_bound(
    client: &Client,
    sql: &str,
    binds: &[BindValue],
    window: u32,
    read_only: bool,
) -> DbResult<ExecuteResult> {
    // Courtesy check first, exactly as `execute` does it — a clear message without a
    // round trip. The server stays the authority: a read-only session runs in a
    // read-only transaction mode, so anything this misses is refused there.
    guard_read_only(sql, read_only)?;

    let started = Instant::now();
    let window = window.max(1);
    let probe = window.saturating_add(1);

    let values: Vec<Bound> = binds.iter().map(Bound::new).collect();
    let params: Vec<&(dyn ToSql + Sync)> =
        values.iter().map(|v| v as &(dyn ToSql + Sync)).collect();

    // The server's own description: how many values the statement wants, and —
    // the part nothing on this side can answer honestly — whether it produces rows
    // at all. Both are free here, and both decide what happens next.
    let described = client.prepare(sql).await.map_err(map_pg)?;
    if described.params().len() != values.len() {
        return Err(wrong_arity(described.params().len(), values.len()));
    }
    if described.columns().is_empty() {
        let affected = client.execute(&described, &params).await.map_err(map_pg)?;
        return Ok(write_result(affected, started));
    }

    let columns = describe(&described);
    let plan = plan_bound(sql);
    // Nothing is masked on the direct path: masking means rewriting the projection,
    // and the direct path is the one where the statement is *not* rewritten. Saying
    // otherwise would put a column in `masked_columns` whose value did come back.
    let masked: Vec<bool> = match plan {
        BoundPlan::Direct => vec![false; columns.len()],
        _ => columns.iter().map(|c| is_large_object(&c.data_type)).collect(),
    };
    // A masked cell holds a size, so it reads as a number — otherwise the grid
    // right-aligns every other numeric column and leaves that one on the left.
    let numeric: Vec<bool> = described
        .columns()
        .iter()
        .enumerate()
        .map(|(i, c)| masked[i] || rows::is_numeric(c.type_()))
        .collect();

    let read = match wrapper(plan, &masked, probe) {
        Some(wrapped) => client.query(wrapped.as_str(), &params).await,
        // Nothing this module understands well enough to rewrite: it runs exactly
        // as typed, so any error the server reports quotes the user's own SQL.
        None => client.query(&described, &params).await,
    };
    let mut fetched = collect(&read.map_err(map_pg)?, &numeric)?;

    let end_of_result = fetched.len() as u32 <= window;
    fetched.truncate(window as usize);

    let masked_columns = columns
        .iter()
        .zip(&masked)
        .filter(|(_, heavy)| **heavy)
        .map(|(c, _)| c.name.clone())
        .collect();

    Ok(ExecuteResult {
        // The known limit: a cursor cannot take parameters, so there is nothing to
        // scroll and the reply says so rather than naming a result that does not
        // exist.
        result_id: None,
        columns: Some(columns),
        row_count: fetched.len(),
        rows: fetched,
        // No `EXPLAIN` is issued: the scrollbar has nothing to size, because there
        // is no result to scroll. `None` renders as unknown, never as zero.
        estimated_rows: None,
        total_rows: None,
        elapsed_ms: started.elapsed().as_millis() as u64,
        end_of_result,
        affected: None,
        masked_columns,
        hidden_columns: Vec::new(),
        row_key: Vec::new(),
        effective_sql: None,
    })
}

/// The reply for a statement that produced no result set.
fn write_result(affected: u64, started: Instant) -> ExecuteResult {
    ExecuteResult {
        result_id: None,
        columns: None,
        rows: Vec::new(),
        estimated_rows: None,
        total_rows: None,
        elapsed_ms: started.elapsed().as_millis() as u64,
        row_count: 0,
        end_of_result: true,
        affected: Some(affected),
        masked_columns: Vec::new(),
        hidden_columns: Vec::new(),
        row_key: Vec::new(),
        effective_sql: None,
    }
}

/// The columns as the server described them, in the shape the grid reads.
fn describe(statement: &Statement) -> Vec<Column> {
    statement
        .columns()
        .iter()
        .map(|c| Column {
            name: c.name().to_string(),
            data_type: c.type_().name().to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        })
        .collect()
}

// ── One value, on its way to the server ───────────────────────────────────────

/// A bound value, sent as text.
///
/// Text rather than the driver's binary encoding, and that is the load-bearing
/// choice: the parameter's type is whatever the *server* inferred for that
/// placeholder — `int4` in `WHERE id = $1`, `timestamptz` in a `BETWEEN` — and
/// asking this side to produce a binary `numeric` or a binary timestamp would mean
/// re-implementing PostgreSQL's input functions in order to send a string somebody
/// typed. In text format the server parses it with the very function it would use
/// for a literal, so a bad value comes back as *its* error message
/// (`invalid input syntax for type integer: "abc"`) rather than one invented here.
///
/// [`accepts`](ToSql::accepts) is unconditional for the same reason: this type
/// carries the user's text for whatever the placeholder turned out to be, so there
/// is no type it should refuse.
#[derive(Debug)]
struct Bound(Option<String>);

impl Bound {
    /// A wire value as the text the server will parse. `Null` stays a real NULL —
    /// it is not the empty string, and in a tool that writes `UPDATE`s confusing
    /// the two is the whole bug.
    fn new(value: &BindValue) -> Self {
        Self(match value {
            BindValue::Null => None,
            BindValue::Bool(b) => Some(b.to_string()),
            BindValue::Int(n) => Some(n.to_string()),
            BindValue::Float(n) => Some(n.to_string()),
            BindValue::Text(t) => Some(t.clone()),
        })
    }
}

impl ToSql for Bound {
    fn to_sql(
        &self,
        _ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match &self.0 {
            Some(text) => {
                out.extend_from_slice(text.as_bytes());
                Ok(IsNull::No)
            }
            None => Ok(IsNull::Yes),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    fn encode_format(&self, _ty: &Type) -> Format {
        Format::Text
    }

    /// Normally the `to_sql_checked!` macro's job — the check it performs is
    /// `accepts`, and this type accepts everything, so the checked form *is* the
    /// unchecked one.
    fn to_sql_checked(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        self.to_sql(ty, out)
    }
}

// ── How a bound statement is read back ────────────────────────────────────────

/// How a statement that returns rows can be made to return them as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundPlan<'a> {
    /// A query — readable through a wrapper over a bounded subquery.
    Query(&'a str),
    /// A statement that changes data and returns rows. It cannot be a subquery,
    /// but it can be a CTE.
    Returning(&'a str),
    /// Everything else that returns rows (`EXPLAIN`, `SHOW`, a multi-statement
    /// paste): run as written, and read only what reads back as text.
    Direct,
}

/// Decide which of the three a statement is.
///
/// Conservative in the same direction as [`plan_execution`]: anything unrecognised
/// is [`BoundPlan::Direct`], which runs the user's own SQL and can at worst fail to
/// read a value back — never rewrite a statement this module does not understand.
fn plan_bound(sql: &str) -> BoundPlan<'_> {
    if let ExecutionPlan::Cursor(body) = plan_execution(sql) {
        return BoundPlan::Query(body);
    }
    let Some(body) = single_statement(sql) else { return BoundPlan::Direct };
    match leading_keyword(strip_leading_noise(body)).as_str() {
        // `WITH` reaches here only when it is data-modifying — `plan_execution`
        // already claimed the read-only ones above.
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "WITH" => BoundPlan::Returning(body),
        _ => BoundPlan::Direct,
    }
}

/// The generated name of the `n`-th column, quoted.
///
/// Positional, never the statement's own names: the alias list renames every column
/// at once, so a result with duplicate names — or with a column that has none —
/// is still addressable. That is the case the masking in [`crate::session`] has to
/// decline, and here it costs nothing because the reply's column names come from
/// the server's description rather than from the wrapper.
fn alias(n: usize) -> String {
    quote_ident(&format!("c{}", n + 1))
}

/// The alias list that renames a wrapped result positionally.
fn aliases(count: usize) -> String {
    (0..count).map(alias).collect::<Vec<_>>().join(", ")
}

/// The projection that turns every column into the server's own text — with the
/// large objects standing for themselves, as everywhere else in this crate.
fn text_projection(masked: &[bool]) -> String {
    masked
        .iter()
        .enumerate()
        .map(|(i, heavy)| {
            let id = alias(i);
            match heavy {
                true => format!("pg_column_size({id})::text AS {id}"),
                false => format!("{id}::text AS {id}"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The statement actually sent: the user's own, wrapped when a wrapper applies.
///
/// `None` is the honest answer for everything the two wrappers do not accept — and
/// it is the safe one, because it sends the statement as typed.
fn wrapper(plan: BoundPlan<'_>, masked: &[bool], probe: u32) -> Option<String> {
    match plan {
        BoundPlan::Query(body) => {
            // The bound goes INSIDE the wrapped body, or the wrapper costs the
            // statement its order. `None` means the body already bounds itself,
            // which blocks the pull-up just as well.
            let inner = bounded_body(body, probe).unwrap_or_else(|| body.to_string());
            Some(subquery_source(&inner, masked, probe))
        }
        // A write cannot be a subquery, but it can be a CTE — and a data-modifying
        // CTE runs to completion whatever the outer `LIMIT` says, so the write is
        // the write the user asked for and only its report is bounded.
        BoundPlan::Returning(body) => Some(cte_source(body, masked, probe)),
        BoundPlan::Direct => None,
    }
}

/// A query, read as text. `inner` must already carry its own bound — see the module
/// note on why the `LIMIT` cannot live out here.
fn subquery_source(inner: &str, masked: &[bool], limit: u32) -> String {
    format!(
        "SELECT {}\nFROM (\n{inner}\n) AS \"picus_bound\"({})\nLIMIT {limit}",
        text_projection(masked),
        aliases(masked.len()),
    )
}

/// A data-modifying statement's `RETURNING` rows, read as text.
fn cte_source(body: &str, masked: &[bool], limit: u32) -> String {
    format!(
        "WITH \"picus_bound\" AS (\n{body}\n)\nSELECT {}\nFROM \"picus_bound\" AS \"picus_row\"({})\nLIMIT {limit}",
        text_projection(masked),
        aliases(masked.len()),
    )
}

/// Every row, as cells.
fn collect(rows: &[Row], numeric: &[bool]) -> DbResult<Vec<Vec<CellValue>>> {
    rows.iter().map(|row| cells(row, numeric)).collect()
}

/// One row.
///
/// Everything is read as text because everything *is* text — either cast by the
/// wrapper, or (on the direct path) genuinely text, which `EXPLAIN` and `SHOW`
/// always are. A column that is neither is reported rather than guessed at: a value
/// nobody can decode must not become a plausible-looking wrong one.
fn cells(row: &Row, numeric: &[bool]) -> DbResult<Vec<CellValue>> {
    (0..row.len())
        .map(|i| {
            let text: Option<&str> = row
                .try_get(i)
                .map_err(|_| unreadable(row.columns().get(i).map_or("", |c| c.name())))?;
            Ok(rows::cell(text, numeric.get(i).copied().unwrap_or(false)))
        })
        .collect()
}

fn wrong_arity(wanted: usize, given: usize) -> DbError {
    DbError::Internal(format!(
        "this statement takes {wanted} value(s) and {given} were supplied. PostgreSQL numbers its \
         placeholders, so $1 through ${wanted} all have to be filled in — a gap in the run is a \
         value the server never receives."
    ))
}

fn unreadable(column: &str) -> DbError {
    DbError::Internal(format!(
        "the column {column} could not be read back as text. A statement like EXPLAIN or SHOW is \
         run exactly as written when it carries values, and only its text columns can be read \
         that way — run this one without placeholders to see it."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(value: &BindValue) -> Option<String> {
        let mut buf = BytesMut::new();
        match Bound::new(value).to_sql(&Type::TEXT, &mut buf).unwrap() {
            IsNull::Yes => None,
            IsNull::No => Some(String::from_utf8(buf.to_vec()).unwrap()),
        }
    }

    #[test]
    fn a_value_crosses_the_wire_as_text_and_never_as_syntax() {
        // The point of the whole module: whatever the user typed arrives as bytes in
        // the parameter's own field. Nothing here quotes anything, because nothing
        // here is building SQL.
        assert_eq!(encode(&BindValue::Text("O'Brien".into())).as_deref(), Some("O'Brien"));
        assert_eq!(encode(&BindValue::Text("1; DROP TABLE t".into())).as_deref(), Some("1; DROP TABLE t"));
        assert_eq!(encode(&BindValue::Int(42)).as_deref(), Some("42"));
        assert_eq!(encode(&BindValue::Bool(true)).as_deref(), Some("true"));
    }

    #[test]
    fn null_and_the_empty_string_stay_different() {
        // In a tool that writes UPDATE statements this is not a detail.
        assert_eq!(encode(&BindValue::Null), None);
        assert_eq!(encode(&BindValue::Text(String::new())).as_deref(), Some(""));
    }

    #[test]
    fn a_bound_value_is_sent_in_text_format() {
        // The server parses it with the input function of the type IT inferred.
        // Binary would mean re-implementing those functions on this side.
        assert!(matches!(Bound::new(&BindValue::Int(1)).encode_format(&Type::INT4), Format::Text));
        assert!(<Bound as ToSql>::accepts(&Type::TIMESTAMPTZ), "no placeholder type is refused");
    }

    #[test]
    fn a_read_is_wrapped_and_a_write_that_returns_rows_is_not_wrapped_the_same_way() {
        assert_eq!(
            plan_bound("SELECT * FROM t WHERE a = $1"),
            BoundPlan::Query("SELECT * FROM t WHERE a = $1"),
        );
        assert_eq!(
            plan_bound("INSERT INTO t (a) VALUES ($1) RETURNING id"),
            BoundPlan::Returning("INSERT INTO t (a) VALUES ($1) RETURNING id"),
        );
        assert_eq!(
            plan_bound("WITH d AS (DELETE FROM t WHERE a = $1 RETURNING *) SELECT * FROM d"),
            BoundPlan::Returning("WITH d AS (DELETE FROM t WHERE a = $1 RETURNING *) SELECT * FROM d"),
        );
        // Neither a subquery nor a CTE will take these: they run as written.
        assert_eq!(plan_bound("EXPLAIN SELECT * FROM t WHERE a = $1"), BoundPlan::Direct);
        assert_eq!(plan_bound("SHOW search_path"), BoundPlan::Direct);
        assert_eq!(plan_bound("SELECT 1; SELECT $1"), BoundPlan::Direct);
        // …and "run as typed" is what Direct means: no wrapper is built for it.
        assert!(wrapper(BoundPlan::Direct, &[false], 11).is_none());
    }

    #[test]
    fn every_column_comes_back_as_the_servers_own_text() {
        let out = subquery_source("SELECT a, b FROM t\nLIMIT 501", &[false, false], 501);
        assert!(out.contains(r#""c1"::text AS "c1", "c2"::text AS "c2""#), "{out}");
        // Renamed positionally, so a result with two columns called `id` — or one
        // with no name at all — is still addressable.
        assert!(out.contains(r#"AS "picus_bound"("c1", "c2")"#), "{out}");
    }

    #[test]
    fn the_bound_sits_inside_the_wrapper_not_outside_it() {
        // THE ordering rule of this module. PostgreSQL drops a sub-select's
        // `ORDER BY` when it pulls the subquery up, and a `LIMIT` inside is what
        // stops it pulling. A bound that lived only on the outer query would hand
        // back an ordered query's rows shuffled.
        let inner = bounded_body("SELECT * FROM t ORDER BY a", 501).unwrap();
        let out = subquery_source(&inner, &[false], 501);
        let opened = out.find("FROM (").unwrap();
        let closed = out.find("\n) AS").unwrap();
        assert!(out[opened..closed].contains("LIMIT 501"), "the bound must be inside: {out}");
    }

    #[test]
    fn a_statement_that_bounds_itself_needs_nothing_added() {
        // `bounded_body` declines exactly when the body already carries a clause
        // that blocks the pull-up, so the ordering is safe either way.
        assert_eq!(bounded_body("SELECT * FROM t ORDER BY a LIMIT 20", 501), None);
    }

    #[test]
    fn large_objects_stand_for_themselves_here_too() {
        let out = subquery_source("SELECT * FROM archivio\nLIMIT 501", &[false, true], 501);
        assert!(out.contains(r#"pg_column_size("c2")::text AS "c2""#), "{out}");
    }

    #[test]
    fn a_returning_write_is_read_through_a_cte() {
        let out = cte_source("INSERT INTO t (a) VALUES ($1) RETURNING id", &[false], 11);
        assert!(out.starts_with("WITH \"picus_bound\" AS ("), "{out}");
        assert!(out.contains(r#"FROM "picus_bound" AS "picus_row"("c1")"#), "{out}");
        // The placeholder is untouched — the value still arrives beside it.
        assert!(out.contains("VALUES ($1)"), "{out}");
    }

    #[test]
    fn a_body_ending_in_a_comment_is_not_commented_out() {
        // The newlines are load-bearing: a trailing `--` would otherwise swallow the
        // closing paren of the wrapper.
        assert!(subquery_source("SELECT 1 -- fine", &[false], 10).contains("-- fine\n)"));
        assert!(cte_source("INSERT INTO t VALUES ($1) -- fine", &[false], 10).contains("-- fine\n)"));
    }
}
