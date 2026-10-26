//! `bennu-mybatis` — a mapper XML coloured for what it is.
//!
//! To the XML grammar a mapper is elements and attributes, and the part that matters — the SQL in a
//! statement, the `#{}` / `${}` bindings in it, the dynamic tags that decide which pieces of it exist
//! — is one flat run of text. So a misspelled keyword, an unquoted `${}` that should have been a
//! `#{}`, and a `test="…"` condition all read exactly like the prose around them.
//!
//! ## What is coloured
//!
//! * the **SQL** of a statement or `<sql>` fragment: keywords, functions, strings, numbers, comments;
//! * the **bindings**: `#{name}` (a bound parameter) and `${name}` (spliced into the text — drawn
//!   differently on purpose, because it is the one that can be an injection);
//! * the **dynamic tags** (`if`, `where`, `foreach`, …) and their OGNL `test` / `value` expressions;
//! * **declarations** (`id="…"`), **references** (`refid`, `resultMap`, `extends`, `select`), the
//!   **Java types** the mapper names, and the **property** each result column maps to.
//!
//! Everything is a per-buffer question — the text says what it is — so there is no project model and
//! `reindex` only records that the host has been through once.

// The extension itself — what a host registers.
pub mod ext;
// Which spans of a mapper get which colour.
mod highlight;
pub mod prelude;
// The SQL inside a statement, lexed.
mod sql;
// A forgiving scan of the mapper's markup: tags, attributes, character data.
mod xml_scan;
