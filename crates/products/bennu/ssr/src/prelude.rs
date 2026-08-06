//! Canonical entry point for `bennu-ssr`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_ssr::prelude::...`.
//! The submodules stay `pub` for rustdoc navigation, but the prelude is the canonical call-site
//! path.

// The language: parsing a query, and everything it parses to.
pub use crate::query::{
    parse as parse_query, Alternative, Ask, Constraint, Denotes, GroupBy, NamedConstraint, Query,
    QueryError,
};

// Running one: compiling the alternatives, searching a file, and the name resolution this crate
// asks the caller for.
pub use crate::engine::{
    compile, glob_matches, search_file, Denotation, Hit, HitCapture, NoTypes, Subject, TypeOracle,
};

// The table `group` asks for.
pub use crate::report::{build as build_report, module_of, Report, Row};

// The replacement half.
pub use crate::replace::{
    apply as apply_edits, check as check_replacement, edits_for, render, Edit, ReplaceError,
};
