//! `picus-db-postgres` — the PostgreSQL implementation of the Picus database
//! contract.
//!
//! Implements [`picus_db_api::provider::DbProvider`] over `tokio-postgres`: pure
//! Rust, so nothing has to ship a `libpq`, and the only driver that offers a real
//! server-side cancellation key — which is what makes the query editor's Cancel
//! actually stop a running statement instead of abandoning it.
//!
//! ## The four decisions worth knowing before reading the code
//!
//! * **Values come back as the server's own text** (the simple query protocol), and
//!   only numeric columns are turned into numbers. A maintenance tool must show a
//!   `timestamptz` and a wide `numeric` the way the server prints them, and must
//!   never turn the string `007` into `7`. See [`rows`].
//! * **A read is a held cursor, not a page of `OFFSET`.** Scrolling asks a
//!   `SCROLL CURSOR WITH HOLD` for a window over one fixed snapshot, so no row is
//!   ever shown twice or skipped. [`cursor`] documents what `WITH HOLD` costs, why
//!   the alternative is unavailable here, and what closes a cursor.
//! * **Read-only is enforced by the server.** A read-only session is opened with
//!   `SESSION CHARACTERISTICS AS TRANSACTION READ ONLY`; the lexical check in
//!   [`sql::guard_read_only`] exists only to give a better message sooner.
//! * **Object names are quoted, never interpolated raw.** They cannot be bind
//!   parameters, so [`sql::quote_ident`] is the one thing standing between a
//!   hostile table name and an executed statement.
//!
//! ## Public API: use the [`prelude`]

pub mod activity;
pub mod bind;
pub mod catalog;
pub mod cursor;
pub mod depends;
pub mod descriptor;
pub mod error;
pub mod plan;
pub mod prelude;
pub mod provider;
pub mod rows;
pub mod session;
pub mod sql;
pub mod tls;
pub mod tx;
