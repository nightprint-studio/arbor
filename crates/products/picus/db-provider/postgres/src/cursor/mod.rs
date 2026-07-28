//! Held results: the SQL that makes one, and the bookkeeping that ends one.
//!
//! ## Why a cursor at all
//!
//! A grid that scrolls has to ask for row 12 000. Serving that by re-running the
//! statement with a larger `OFFSET` is only correct when the statement has a total
//! order, and almost none do: without an explicit `ORDER BY`, PostgreSQL is free to
//! return the same rows in a different sequence the second time — a different plan,
//! a concurrent update, a parallel scan that finished in a different order — so
//! `OFFSET` paging can show a row twice in one window and never in the next, while
//! the user does nothing but scroll. A cursor fixes one snapshot and answers every
//! window from it.
//!
//! ## Why `WITH HOLD`, and what it costs
//!
//! The alternative is a cursor inside a transaction left open for as long as the
//! tab lives. It avoids materialising anything, and it is not available here:
//!
//! * **one connection, many tabs.** A session is one PostgreSQL backend, shared by
//!   every query tab bound to that connection. An open transaction is a property of
//!   the connection, not of the tab that started it — every other tab's statement
//!   would run inside it, and one syntax error would abort the transaction and take
//!   every open result down with it;
//! * **there is no second connection to put it on.** Picus holds no password: the
//!   secret is fetched from the shell's keychain at connect time and zeroed
//!   immediately. The session cannot open another connection later even if it
//!   wanted one;
//! * **an open snapshot blocks vacuuming** for its whole life, and a tab lives for
//!   hours or days. That is a cost paid by everyone else on that server.
//!
//! So: `DECLARE … SCROLL CURSOR WITH HOLD`. The **consequence, stated plainly**: at
//! the moment the declaring transaction commits, the server runs the query to
//! completion and copies the entire result into a tuplestore — in memory up to
//! `work_mem`, in a temporary file beyond it. For `SELECT * FROM a_huge_table` that
//! means the first window waits for the whole scan, and the server holds a second
//! copy of the result until the cursor is closed.
//!
//! That price is not hidden, and it buys the rest: after it, every window — forward,
//! backward, a jump to the end — costs the window, the exact count is a walk over
//! storage that already exists rather than a second scan, and no row is ever shown
//! twice or skipped. The mitigation is the one the product should encourage anyway:
//! ask a narrower question. Picus deliberately does **not** cap the cursor at some
//! row count — a cap is exactly the silent truncation this feature exists to remove.

pub mod registry;
pub mod sql;

pub use registry::{CursorHandle, CursorRegistry, IDLE_TTL, MAX_OPEN};
pub use sql::{
    close_statement, count_statements, declare_cursor, explain_statement, plan_execution,
    plan_row_estimate, relation_query, window_statements, ExecutionPlan,
};
