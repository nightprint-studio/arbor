//! `arbor-sql-abbrev` — an Emmet-like abbreviation language for SQL.
//!
//! ```text
//! s#localstrings(keycode,value)[keycode='ita']
//! → SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'
//! ```
//!
//! ## The grammar, in one place
//!
//! ```text
//! verb # table  chain?  changes?  ( columns )?  [ conditions ]?  *n?  { row template }?
//! ```
//!
//! | Verb | Means | Example |
//! |---|---|---|
//! | `s`  | SELECT | `s#ordini>clienti(nome)[evaso=false]` |
//! | `i`  | INSERT | `i#ordini(id,codice)*3{$, 'COD_$'}` |
//! | `u`  | UPDATE | `u#ordini(evaso=true)[id=7]` |
//! | `d`  | DELETE | `d#ordini[id=7]` |
//! | `m`  | upsert | `m#ordini[id]` |
//! | `a`  | ALTER  | `a#ordini+nota:varchar(200)~importo:number(12,2)` |
//! | `fc` | cursor loop | `fc#ordini[evaso=false]` |
//!
//! * `>table` follows a **foreign key**; `>table:column` picks which one.
//! * `(…)` are columns — with `=value` where the verb assigns.
//! * `[…]` are conditions — except after `m#`, where they are the key columns.
//! * `*n` repeats a row; `{…}` gives each repetition its own values, with `$`
//!   standing for the row number (see [`numbering`]).
//! * `+col:type` adds a column and `~col:type` retypes one.
//!
//! The point is not the keystrokes. A snippet engine can save keystrokes and
//! needs no crate. The point is that the host **has the schema**, so the
//! expansion knows things a snippet cannot:
//!
//! * **where the quotes go**, from the column's type — `007` in a `varchar`
//!   account code keeps its leading zeros, `15` in a `numeric` does not gain
//!   quotes, and a column the host did not classify is quoted, because that is
//!   the answer that fails safely;
//! * **what a join is `ON`**, from the foreign key — `s#ordini>clienti` reads the
//!   condition out of the constraint and refuses, naming the candidates, when
//!   there is more than one to read;
//! * **that a name is wrong**, and often which name was meant.
//!
//! An expansion that a text snippet could have produced is not worth a crate. One
//! that only a schema-aware tool could produce is the whole of this one.
//!
//! ## Refuse rather than guess
//!
//! Every failure is a sentence a person can act on, and there is no such thing
//! here as a plausible approximation. No foreign key between two tables is a
//! refusal, not a `1=1`. An `UPDATE` with no `WHERE` is a refusal, not a
//! statement that touches every row. A column in two of the chain's tables is a
//! refusal naming both, not a binding to whichever the user happened to type
//! first. See [`error::AbbrevError`].
//!
//! ## Two entry points, one parse
//!
//! * [`expand`](expand::expand) — abbreviation + schema → a resolved
//!   [`Statement`](statement::Statement).
//! * [`context_at`](context::context_at) — abbreviation + caret → what to offer
//!   for completion there.
//!
//! Both go through [`parse`](parse::parse), which never fails: it records a
//! syntax error and keeps a slot at every position it reached, so `s#ordini>` has
//! an answer for the caret at the end of it. A second, more forgiving parser
//! written for completion is the failure mode this design exists to prevent —
//! two parsers drift, and the day they disagree the editor offers a column for a
//! table the expansion will not use.
//!
//! ## What the crate returns, and why it is not a string
//!
//! [`expand`](expand::expand) returns an **intent**: tables and columns spelled as
//! the schema spells them, joins carrying the foreign key's columns, values paired
//! with the [`ValueKind`](schema::ValueKind) that decides their quoting. Rendering
//! is the host's, because a host may already own a deterministic emitter it cannot
//! be asked to bypass — Picus routes `INSERT`/`UPDATE` through its own
//! `DmlModel` → `picus-emit` so that identifier casing and the Oracle/PostgreSQL
//! differences stay in one place. [`render`](render::render) is provided for hosts
//! without one.
//!
//! ## Adopting it
//!
//! Build a [`SchemaView`](schema::SchemaView) from whatever the host already knows
//! about the database, **once per connection**, and keep it: completion runs on
//! every keystroke and this crate caches nothing. Then call the two functions.
//! Nothing else is required, and there is no trait to implement.
//!
//! ## Public API: use the [`prelude`]

pub mod context;
pub mod error;
pub mod expand;
pub mod join;
pub mod numbering;
pub mod parse;
pub mod prelude;
pub mod render;
pub mod resolve;
pub mod schema;
pub mod span;
pub mod statement;
pub mod syntax;

#[cfg(test)]
mod tests;
