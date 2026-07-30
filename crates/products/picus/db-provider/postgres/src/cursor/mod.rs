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
//! twice or skipped. Picus deliberately does **not** cap the cursor at some row
//! count — a cap is exactly the silent truncation this feature exists to remove.
//!
//! ## …but it is not paid to show the first window
//!
//! It used to be, and that was the mistake. Declaring the cursor up front meant
//! **every** read waited for the whole result to be copied, and a row limit on the
//! grid bounded none of it — a table of scanned documents took minutes to show five
//! hundred rows, with nothing on screen and a Cancel that had a commit to interrupt
//! rather than a query.
//!
//! So a result begins as a **wrapped `LIMIT`** ([`first_window_query`]) and becomes
//! a cursor only when somebody asks for a row the first window did not hold. Most
//! never do. An exact count asked of one that has not, goes through
//! [`count_query`] rather than declaring one — `count(*)` reads no columns, so a
//! table of large objects is counted without touching a single one of them.
//!
//! **What that costs**: the first window and the cursor are two executions, so
//! without an `ORDER BY` a row shown in the first window may appear again — or not
//! at all — once scrolling crosses into the cursor's snapshot. Inside the cursor
//! everything is as stable as it always was. That is a weaker guarantee than
//! before, taken deliberately: the old one only held for people who waited out the
//! materialisation, and on a large table nobody did.
//!
//! ## The one place that price becomes unpayable
//!
//! Multiply the above by a `bytea` column of scanned documents and it stops being a
//! trade: `SELECT * FROM archivio` copies every megabyte of every row into a
//! tuplestore before the first row appears, to draw a grid that can show none of it.
//! That is minutes, and it looks exactly like the application having hung.
//!
//! So a result carrying large objects is read through a projection where they
//! stand for themselves ([`masked_source`]). Which columns those are comes from the
//! **server's own description** of the result — the `prepare` that already happens
//! to type the columns — not from parsing the statement, so it works for a join, a
//! union or a CTE, and it holds however the column was named. The values are read
//! one at a time when a cell is opened.
//!
//! It is deliberately not conditional on being able to open them again. A grid of
//! sizes you cannot open is a poor thing; a query that takes four minutes and
//! cannot be cancelled is a worse one, and the remedy for the first is to select
//! the key column too.

pub mod registry;
pub mod sql;

pub use registry::{CursorHandle, CursorRegistry, IDLE_TTL, MAX_OPEN};
pub use sql::{
    bounded_body, close_statement, count_query, count_statements, declare_cursor,
    explain_statement, first_window_statements, is_large_object, masked_projection, masked_source,
    orders_its_own_rows, plan_execution, plan_row_estimate, relation_query, relation_target,
    window_statements, ExecutionPlan,
};
