//! # picus-parse
//!
//! One permissive Tree-sitter grammar covering **both** dialects Picus
//! maintains — Oracle (SQL + PL/SQL) and PostgreSQL (SQL + PL/pgSQL) — and a
//! thin, byte-range-oriented reader over it.
//!
//! ## Why one grammar and not two
//!
//! Picus keeps repositories where the same logical change exists twice, once in
//! each dialect. Its job is to notice when the two drift apart. With two strict
//! grammars an Oracle-ism inside a PostgreSQL file is a parse failure and the
//! best message available is "syntax error at line 12". With one permissive
//! superset it is **a node with a name**, and the message becomes
//! "`(+)` is Oracle's outer-join marker; PostgreSQL wants LEFT JOIN … ON".
//!
//! So the grammar's rule is: every construct that exists in only one dialect
//! gets its own named node. [`dialect`] is the table that turns those names into
//! advice, and [`Statement::foreign`] is where a caller finds them.
//!
//! ## What it parses
//!
//! To **full expression depth, everywhere** — including inside `DECLARE … BEGIN
//! … END` and PL/pgSQL bodies. That is not thoroughness for its own sake: in a
//! real Oracle upgrade script the INSERT that has to be checked for duplicate
//! keys is three blocks deep, so a parser that treated procedural bodies as
//! opaque would see nothing at all in the files this product exists to maintain.
//!
//! ## What it hands back
//!
//! [`ParsedFile`] is a **map of a string the caller still owns**. Nothing here
//! stores the source and nothing reconstructs text: every position is a
//! [`ByteRange`] into the original bytes, which is what lets `picus-rewrite`
//! splice a statement and guarantee the rest of the file survives byte for byte.
//! [`ParsedFile::segments`] enumerates statements and the gaps between them, and
//! the invariant is that concatenating them reproduces the input exactly.
//!
//! ```no_run
//! use picus_parse::prelude::*;
//! use picus_types::prelude::{DialectScope, EngineKind};
//!
//! let sql = "INSERT INTO PARAMETRI (COD, VAL) VALUES ('A', '1');";
//! let parsed = parse(sql, DialectScope::One(EngineKind::Postgres));
//!
//! assert_eq!(parsed.reassemble(sql), sql);
//! for statement in &parsed.statements {
//!     for shape in &statement.dml {
//!         println!("{:?} into {}", shape.operation, shape.table.folded_qualified());
//!     }
//! }
//! ```
//!
//! ## Build workflow
//!
//! `grammar.js` (plus the modules under `grammar/`) is the source of truth; the
//! generated `src/parser.c`, `src/grammar.json` and `src/node-types.json` are
//! **committed**, and `build.rs` compiles them together with the hand-written
//! `src/scanner.c`. A plain `cargo build` therefore needs no Node and no
//! Tree-sitter CLI — only a C compiler, which the workspace already requires.
//! After editing the grammar, run `tree-sitter generate` and `tree-sitter test`
//! in this directory and commit the regenerated files. See `README.md`.
//!
//! ## Public API: use the [`prelude`]
//!
//! [`Statement::foreign`]: statement::Statement::foreign
//! [`ByteRange`]: range::ByteRange
//! [`ParsedFile`]: statement::ParsedFile
//! [`ParsedFile::segments`]: statement::ParsedFile::segments

pub mod dialect;
pub mod dml;
pub mod error;
pub mod literal;
pub mod object;
pub mod parser;
pub mod prelude;
pub mod projection;
pub mod range;
pub mod select;
pub mod statement;

mod walk;
