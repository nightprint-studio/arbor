//! `bennu-learn` — what Bennu learns from how the user treats its suggestions.
//!
//! ## What is in here
//!
//! * [`edit`] — what became of a ghost text: kept, cut short, written on, rewritten (and from which kind of thing to
//!   which), or undone. Pure functions on two strings.
//! * [`tally`] — counters whose weight fades with time, and the rates read off them (right as drawn, useful as far as
//!   it went, corrected).
//! * [`style`] — how a project lays out its code (indent, braces, spacing), measured from its files, and the text conformed to it.
//! * [`store`] — the learner: counters per (origin, context) in a folder, behind an `enabled` switch, with a
//!   `keep_text` switch for the raw corrections and a [`store::Learner::reset`] that forgets everything.
//!
//! ## What is not
//!
//! The editor that watches the ghost text, the ranking that reads the rates, and the model. This crate takes the
//! three strings of an event and a clock, and gives back numbers; it knows no Java, no project and no UI, which is
//! what lets both the popup (`bennu-query`) and the full-line completion (`bennu-flc`) read the same counters
//! without one depending on the other.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_learn::prelude::...`.

pub mod edit;
pub mod prelude;
pub mod store;
pub mod style;
pub mod tally;
