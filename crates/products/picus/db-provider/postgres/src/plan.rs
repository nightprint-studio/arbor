//! `EXPLAIN` — what the server says it will do, and what it did.
//!
//! Two shapes of the same answer, because they are read by two different people at
//! two different moments: the **structure** is what the interface indents and marks
//! up, the **text** is what gets pasted into a ticket. A tool that can only show you
//! its own rendering of a plan is a tool nobody can ask a colleague about.
//!
//! ## `ANALYZE` runs the statement, so it is guarded here
//!
//! `EXPLAIN` describes. `EXPLAIN ANALYZE` **executes**. On a `SELECT` that is merely
//! slow; on a `DELETE` it is the delete, and the user asked for a plan. So the
//! analysed form is refused for anything that is not a read — and on a read-only
//! connection it is refused as a write, which is the fact the user can act on.
//!
//! The plain form is deliberately **not** guarded: `EXPLAIN UPDATE …` plans without
//! executing, and it is exactly the thing somebody wants to look at before they run
//! the update on a read-only connection they are about to switch.
//!
//! ## Why the analysed form has no second read
//!
//! Without `ANALYZE` the plan is asked for twice — once as JSON for the structure,
//! once as text for the paste — and both are planning-only round trips that cost
//! nothing. With `ANALYZE` a second read would be a second **execution**, so there
//! is exactly one, and its JSON is what [`QueryPlan::text`] carries. That is still
//! the server's own output verbatim; it is simply the server's output in the one
//! format that could also be parsed.

use std::time::Instant;

use picus_db_api::prelude::{DbError, DbResult, PlanNode, PlanRequest, QueryPlan};
use serde_json::Value;
use tokio_postgres::{Client, SimpleQueryMessage};

use crate::cursor::explain_statement;
use crate::error::map_pg;
use crate::sql::{guard_read_only, single_statement, statement_kind, StatementKind};

/// The plan for one statement.
///
/// `read_only` is the connection's flag, not the statement's: it only changes which
/// refusal an analysed request gets, never whether the plain form is allowed.
pub async fn explain(
    client: &Client,
    sql: &str,
    request: PlanRequest,
    read_only: bool,
) -> DbResult<QueryPlan> {
    let started = Instant::now();
    // One statement, without its terminator. A paste of several is not one plan, and
    // `EXPLAIN` of it would plan the first and run nothing of the rest — an answer
    // that looks complete and is about a third of what was asked.
    let body = single_statement(sql).ok_or_else(|| refused(NOT_ONE_STATEMENT))?;

    if request.analyze {
        // Order matters. On a read-only connection this is a write in the only sense
        // that counts, and "this connection is read-only" is the sentence the user
        // can do something about; the generic refusal below would bury that.
        guard_read_only(body, read_only)?;
        if statement_kind(body) != StatementKind::Read {
            return Err(refused(ANALYZE_WOULD_RUN_IT));
        }
    }

    let (text, json) = match request.analyze {
        // One execution, and its own output is the text. See the module note.
        true => {
            let measured = read_plan(client, &analyzed_statement(body)).await?;
            (measured.clone(), measured)
        }
        false => {
            let structure = read_plan(client, &json_statement(body)).await?;
            let text = read_plan(client, &explain_statement(body)).await?;
            (text, structure)
        }
    };

    let root: Value = serde_json::from_str(&json)
        .map_err(|e| refused(&format!("the server's plan could not be read: {e}")))?;
    Ok(assemble(text, &root, request.analyze, started.elapsed().as_millis() as u64))
}

// ── The statements ─────────────────────────────────────────────────────────────

/// `EXPLAIN (FORMAT JSON, VERBOSE) <body>` — planning only.
///
/// `VERBOSE` is what puts the output column list and the schema-qualified relation
/// names in the answer; it costs the planner nothing and it is the difference
/// between a node that says `Index Scan` and one that says which index, on what.
///
/// The newline is the comment case: a body opening with `--` would otherwise
/// comment out its own `EXPLAIN`.
pub fn json_statement(body: &str) -> String {
    format!("EXPLAIN (FORMAT JSON, VERBOSE)\n{body}")
}

/// `EXPLAIN (ANALYZE, …) <body>` — **this executes the statement**.
///
/// `BUFFERS` is unconditional here, which is why [`PlanRequest::buffers`] does not
/// reach this function: the accounting is a by-product of an execution that is
/// already happening, so asking for it costs nothing, and "how much of this came off
/// disk" is the second question anybody asks about a slow node. The flag stays on
/// the request for engines where it is not free.
pub fn analyzed_statement(body: &str) -> String {
    format!("EXPLAIN (ANALYZE, FORMAT JSON, VERBOSE, BUFFERS)\n{body}")
}

/// Read a plan out of the server, as one string.
///
/// `FORMAT JSON` answers with a single row holding the whole document; the text
/// format answers with one row per line. Joining covers both without the caller
/// having to know which it asked for.
async fn read_plan(client: &Client, statement: &str) -> DbResult<String> {
    let messages = client.simple_query(statement).await.map_err(map_pg)?;
    let lines: Vec<String> = messages
        .iter()
        .filter_map(|m| match m {
            SimpleQueryMessage::Row(row) => row.get(0).map(str::to_string),
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return Err(refused(NO_PLAN));
    }
    Ok(lines.join("\n"))
}

// ── The tree, flattened ────────────────────────────────────────────────────────

/// How deep the walk will follow a plan.
///
/// PostgreSQL plans are shallow — a dozen levels is a lot — and this is not a limit
/// on anything real. It is there because the walk is recursive and the document
/// comes off a socket: a malformed or pathological one must not be able to recurse
/// the stack away.
const MAX_DEPTH: u32 = 64;

fn assemble(text: String, root: &Value, analyzed: bool, elapsed_ms: u64) -> QueryPlan {
    // `EXPLAIN (FORMAT JSON)` answers with a one-element array; the fallback is for
    // the shape a future server version might use rather than for anything seen.
    let head = root.get(0).unwrap_or(root);
    let plan = head.get("Plan");

    let mut nodes = Vec::new();
    if let Some(plan) = plan {
        flatten(plan, 0, analyzed, &mut nodes);
    }

    QueryPlan {
        text,
        startup_cost: plan.and_then(|p| num(p, "Startup Cost")),
        total_cost: plan.and_then(|p| num(p, "Total Cost")),
        // The root node's own time is per loop and the root runs once, but the
        // server states the total separately and that is the number to trust.
        actual_ms: analyzed.then(|| num(head, "Execution Time")).flatten(),
        analyzed,
        nodes,
        elapsed_ms,
    }
}

/// Pre-order: parent, then its children. That is the order `EXPLAIN` prints, and
/// the order the tree reads in — a child feeds the node above it.
fn flatten(node: &Value, depth: u32, analyzed: bool, out: &mut Vec<PlanNode>) {
    out.push(one_node(node, depth, analyzed));
    if depth >= MAX_DEPTH {
        return;
    }
    let Some(children) = node.get("Plans").and_then(Value::as_array) else { return };
    for child in children {
        flatten(child, depth + 1, analyzed, out);
    }
}

fn one_node(node: &Value, depth: u32, analyzed: bool) -> PlanNode {
    let rows = num(node, "Plan Rows");
    // Both of these are **per loop**, exactly as `Plan Rows` is, so they compare
    // directly with the estimate beside them. A node inside a nested loop is
    // therefore reporting one iteration, and `detail` says so when there is more
    // than one — multiplying here instead would make the estimate look wrong by the
    // loop count on every plan that has one.
    let actual_rows = analyzed.then(|| num(node, "Actual Rows")).flatten();
    let actual_ms = analyzed.then(|| num(node, "Actual Total Time")).flatten();

    PlanNode {
        depth,
        label: label(node),
        relation: text_of(node, "Relation Name"),
        startup_cost: num(node, "Startup Cost"),
        cost: num(node, "Total Cost"),
        rows,
        actual_rows,
        actual_ms,
        detail: detail(node),
        warning: warning(node, rows, actual_rows),
    }
}

/// `Seq Scan`, `Index Scan using orders_pkey`, `Hash Join`.
fn label(node: &Value) -> String {
    let kind = text_of(node, "Node Type").unwrap_or_else(|| "Node".to_string());
    match text_of(node, "Index Name") {
        Some(index) => format!("{kind} using {index}"),
        None => kind,
    }
}

/// Conditions and choices worth showing under a node, in the server's own words.
const CONDITIONS: [&str; 10] = [
    "Index Cond",
    "Filter",
    "Join Filter",
    "Hash Cond",
    "Merge Cond",
    "Recheck Cond",
    "One-Time Filter",
    "Join Type",
    "Sort Method",
    "Subplan Name",
];

/// Keys the server states as a list.
const KEY_LISTS: [&str; 3] = ["Sort Key", "Group Key", "Presorted Key"];

/// Counts that mean something only when they are not zero.
const COUNTS: [&str; 3] = ["Rows Removed by Filter", "Heap Fetches", "Workers Launched"];

fn detail(node: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in CONDITIONS {
        if let Some(value) = text_of(node, key) {
            out.push(format!("{key}: {value}"));
        }
    }
    for key in KEY_LISTS {
        let joined = node
            .get(key)
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        if !joined.is_empty() {
            out.push(format!("{key}: {joined}"));
        }
    }
    for key in COUNTS {
        match num(node, key) {
            Some(n) if n > 0.0 => out.push(format!("{key}: {}", n.round() as i64)),
            _ => {}
        }
    }
    // Said explicitly because it is the one thing about `EXPLAIN ANALYZE` everybody
    // misreads: the rows and the time above are one iteration's, not the total.
    if let Some(loops) = num(node, "Actual Loops").filter(|n| *n > 1.0) {
        out.push(format!("Loops: {} — the rows and time above are per loop", loops.round() as i64));
    }
    out.extend(buffers(node));
    out
}

/// What this node read, when `BUFFERS` was asked for.
///
/// Hit versus read is the distinction that matters: the same node doing the same
/// work is a different problem when the pages were already in the cache.
fn buffers(node: &Value) -> Option<String> {
    let of = |key: &str| num(node, key).unwrap_or(0.0).round() as i64;
    let (hit, read, written) =
        (of("Shared Hit Blocks"), of("Shared Read Blocks"), of("Shared Written Blocks"));
    if hit == 0 && read == 0 && written == 0 {
        return None;
    }
    let mut parts = vec![format!("hit {hit}"), format!("read {read}")];
    if written > 0 {
        parts.push(format!("written {written}"));
    }
    Some(format!("Buffers: {}", parts.join(", ")))
}

// ── Remarks ────────────────────────────────────────────────────────────────────

/// How many estimated rows make a sequential scan worth remarking on.
///
/// A sequential scan is not a fault — under a few thousand rows it is the *right*
/// plan, and flagging it there would train people to ignore the marks. The number
/// is where "the planner chose to read everything" stops being ordinary.
const BIG_SCAN_ROWS: f64 = 50_000.0;

/// How far an estimate has to miss before it is worth saying so.
///
/// An order of magnitude, and not less: estimates are estimates, and a plan marked
/// up because the planner said 900 and got 1 400 is a plan full of marks that mean
/// nothing.
const MISESTIMATE_FACTOR: f64 = 10.0;

/// The remark this node deserves, in prose, or nothing.
///
/// Two of them, and no more. Every line here has to say *why*, because advice with
/// no reasoning attached is noise the reader cannot check — and a screen of
/// unjustified advice is what makes people stop reading the justified line.
fn warning(node: &Value, rows: Option<f64>, actual_rows: Option<f64>) -> Option<String> {
    let mut said: Vec<String> = Vec::new();

    // `Parallel Seq Scan` ends with the same two words and is the same fact.
    let sequential = text_of(node, "Node Type").is_some_and(|k| k.ends_with("Seq Scan"));
    if let Some(estimate) = rows.filter(|n| sequential && *n >= BIG_SCAN_ROWS) {
        let what = text_of(node, "Relation Name").unwrap_or_else(|| "this relation".to_string());
        said.push(format!(
            "Every row of {what} is read here — about {} of them — because no index answers the \
             condition on it. At this size that is usually where the statement's time goes; an \
             index on the columns filtered here is what removes it.",
            estimate.round() as i64,
        ));
    }

    if let (Some(estimate), Some(actual)) = (rows, actual_rows) {
        // Clamped to one: a zero on either side is a division, and "the planner
        // expected 0" is not a distinction worth a special case.
        let (expected, got) = (estimate.max(1.0), actual.max(1.0));
        let factor = if got > expected { got / expected } else { expected / got };
        if factor >= MISESTIMATE_FACTOR {
            said.push(format!(
                "The planner expected {} row(s) here and got {} — {} by a factor of about {:.0}. \
                 Every choice above this node was made on the wrong number, so the plan may be \
                 the wrong plan rather than merely a slow one; statistics that have not been \
                 gathered since the data changed are the usual cause.",
                expected.round() as i64,
                got.round() as i64,
                if got > expected { "an underestimate" } else { "an overestimate" },
                factor,
            ));
        }
    }

    (!said.is_empty()).then(|| said.join(" "))
}

// ── Small readers ──────────────────────────────────────────────────────────────

fn num(node: &Value, key: &str) -> Option<f64> {
    node.get(key).and_then(Value::as_f64)
}

fn text_of(node: &Value, key: &str) -> Option<String> {
    node.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty())
}

/// A refusal Picus itself makes, phrased as the sentence the user reads.
///
/// [`DbError::Sql`] rather than [`DbError::Internal`] purely for how it prints:
/// `Internal` renders as `internal error: …`, and a deliberate, correct refusal that
/// reads like a crash is worse than a slightly wrong variant. The contract has no
/// "the product refused this" variant; if one is ever added, these are its call
/// sites.
fn refused(message: &str) -> DbError {
    DbError::Sql { message: message.to_string(), code: None, position: None }
}

const NOT_ONE_STATEMENT: &str = "a plan is about one statement — select the one you want \
    explained, or remove the others.";

const ANALYZE_WOULD_RUN_IT: &str = "measuring a plan runs the statement, and this one is not a \
    read — measuring it would perform it. Ask for the estimated plan instead.";

const NO_PLAN: &str = "the server returned no plan for this statement";

#[cfg(test)]
mod tests {
    use super::*;

    fn node(json: &str) -> Value {
        serde_json::from_str(json).expect("fixture")
    }

    #[test]
    fn the_plain_form_never_carries_analyze() {
        // The whole safety property of the unanalysed path, asserted rather than
        // assumed: this string is sent for statements that must not run.
        let out = json_statement("DELETE FROM t");
        assert!(!out.contains("ANALYZE"), "{out}");
        assert!(out.starts_with("EXPLAIN (FORMAT JSON, VERBOSE)\n"), "{out}");
    }

    #[test]
    fn a_body_opening_with_a_comment_is_not_commented_out() {
        assert!(json_statement("-- note\nSELECT 1").contains(")\n-- note\nSELECT 1"));
        assert!(analyzed_statement("-- note\nSELECT 1").contains(")\n-- note\nSELECT 1"));
    }

    #[test]
    fn the_measured_form_asks_for_buffers_because_it_is_running_anyway() {
        let out = analyzed_statement("SELECT 1");
        assert!(out.contains("ANALYZE") && out.contains("BUFFERS"), "{out}");
    }

    #[test]
    fn the_tree_flattens_parent_first_with_its_depth() {
        let root = node(
            r#"[{"Plan":{"Node Type":"Hash Join","Startup Cost":12.0,"Total Cost":42.5,"Plan Rows":10,
                "Plans":[
                  {"Node Type":"Seq Scan","Relation Name":"orders","Total Cost":18.5,"Plan Rows":8},
                  {"Node Type":"Hash","Total Cost":4.0,"Plan Rows":2,
                   "Plans":[{"Node Type":"Index Scan","Index Name":"customers_pkey","Plan Rows":2}]}
                ]}}]"#,
        );
        let plan = assemble("text".into(), &root, false, 7);

        let shape: Vec<(u32, &str)> =
            plan.nodes.iter().map(|n| (n.depth, n.label.as_str())).collect();
        assert_eq!(
            shape,
            vec![
                (0, "Hash Join"),
                (1, "Seq Scan"),
                (1, "Hash"),
                (2, "Index Scan using customers_pkey"),
            ],
        );
        assert_eq!(plan.total_cost, Some(42.5));
        assert_eq!(plan.startup_cost, Some(12.0));
        assert_eq!(plan.nodes[0].startup_cost, Some(12.0));
        // A node the document gives no startup cost for reports none rather than zero:
        // "the engine didn't say" and "free to start" are different claims.
        assert_eq!(plan.nodes[1].startup_cost, None);
        assert_eq!(plan.nodes[1].relation.as_deref(), Some("orders"));
        assert!(!plan.analyzed, "nothing was run, so nothing may claim to have been measured");
        assert_eq!(plan.actual_ms, None);
        assert_eq!(plan.elapsed_ms, 7);
    }

    #[test]
    fn an_unanalysed_plan_carries_no_measurements_even_if_the_document_has_them() {
        // Defence in depth against the one mislabelling that matters: an estimate
        // shown as a measurement.
        let root = node(
            r#"[{"Plan":{"Node Type":"Seq Scan","Plan Rows":1,"Actual Rows":900,
                "Actual Total Time":12.5},"Execution Time":13.0}]"#,
        );
        let plan = assemble(String::new(), &root, false, 0);
        assert_eq!(plan.nodes[0].actual_rows, None);
        assert_eq!(plan.nodes[0].actual_ms, None);
        assert_eq!(plan.actual_ms, None);
        assert_eq!(plan.nodes[0].warning, None, "there is nothing to compare against");
    }

    #[test]
    fn a_measured_plan_reports_what_it_measured() {
        let root = node(
            r#"[{"Plan":{"Node Type":"Index Scan","Index Name":"orders_pkey","Plan Rows":12,
                "Actual Rows":11,"Actual Total Time":0.4,"Total Cost":8.3},
                "Execution Time":1.75}]"#,
        );
        let plan = assemble(String::new(), &root, true, 3);
        assert!(plan.analyzed);
        assert_eq!(plan.actual_ms, Some(1.75));
        assert_eq!(plan.nodes[0].actual_rows, Some(11.0));
        assert_eq!(plan.nodes[0].actual_ms, Some(0.4));
        assert_eq!(plan.nodes[0].warning, None, "an estimate that was close is not a remark");
    }

    #[test]
    fn a_large_sequential_scan_is_remarked_on_and_a_small_one_is_not() {
        let big = node(r#"{"Node Type":"Seq Scan","Relation Name":"archivio","Plan Rows":900000}"#);
        let remark = warning(&big, num(&big, "Plan Rows"), None).expect("a remark");
        assert!(remark.contains("archivio"), "{remark}");
        assert!(remark.contains("index"), "the remark has to say what would remove it");

        let small = node(r#"{"Node Type":"Seq Scan","Relation Name":"stati","Plan Rows":40}"#);
        assert_eq!(
            warning(&small, num(&small, "Plan Rows"), None),
            None,
            "on a small relation a sequential scan is the right plan",
        );
    }

    #[test]
    fn a_parallel_sequential_scan_is_the_same_fact() {
        let n = node(r#"{"Node Type":"Parallel Seq Scan","Plan Rows":900000}"#);
        assert!(warning(&n, Some(900_000.0), None).is_some());
    }

    #[test]
    fn an_estimate_that_missed_by_an_order_of_magnitude_is_named_as_one() {
        let n = node(r#"{"Node Type":"Index Scan"}"#);
        let under = warning(&n, Some(10.0), Some(4200.0)).expect("a remark");
        assert!(under.contains("underestimate"), "{under}");
        let over = warning(&n, Some(9000.0), Some(3.0)).expect("a remark");
        assert!(over.contains("overestimate"), "{over}");
        // Within an order of magnitude is an estimate doing its job.
        assert_eq!(warning(&n, Some(100.0), Some(430.0)), None);
        // A zero on either side must not divide.
        assert_eq!(warning(&n, Some(0.0), Some(1.0)), None);
    }

    #[test]
    fn the_detail_lines_say_what_the_node_was_given() {
        let n = node(
            r#"{"Node Type":"Seq Scan","Filter":"(stato = 'EV'::text)",
                "Rows Removed by Filter":1204,"Sort Key":["a","b"],"Actual Loops":24,
                "Shared Hit Blocks":300,"Shared Read Blocks":12}"#,
        );
        let lines = detail(&n);
        assert!(lines.iter().any(|l| l.starts_with("Filter: (stato")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Rows Removed by Filter: 1204"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Sort Key: a, b"), "{lines:?}");
        // The per-loop caveat, said where the numbers it qualifies are.
        assert!(lines.iter().any(|l| l.contains("per loop")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Buffers: hit 300, read 12"), "{lines:?}");
    }

    #[test]
    fn a_zero_count_is_not_a_line() {
        let n = node(r#"{"Node Type":"Seq Scan","Rows Removed by Filter":0,"Actual Loops":1}"#);
        assert!(detail(&n).is_empty(), "{:?}", detail(&n));
    }

    #[test]
    fn a_pathological_document_cannot_recurse_the_stack_away() {
        // Built as VALUES rather than as text to be parsed: the point is `assemble`'s own depth
        // guard, and a document this deep trips serde_json's parser recursion limit first — which
        // failed the fixture before the thing under test ever ran.
        let mut deepest = serde_json::json!({"Node Type": "Leaf"});
        for _ in 0..200 {
            deepest = serde_json::json!({"Node Type": "Nested", "Plans": [deepest]});
        }
        let root = serde_json::json!([{"Plan": deepest}]);
        let plan = assemble(String::new(), &root, false, 0);
        assert_eq!(plan.nodes.len() as u32, MAX_DEPTH + 1);
    }
}
