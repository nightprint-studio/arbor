//! Canonical entry point for `picus-parse`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_parse::prelude::...`, never through the submodules.

pub use crate::dialect::{classify_function, classify_node, ForeignConstruct};
pub use crate::dml::{Assignment, ColumnRef, DmlOperation, DmlShape, ValueCell, ValueRow};
pub use crate::error::{ParseError, ParseErrorKind};
pub use crate::literal::{decode as decode_literal, LiteralValue};
pub use crate::object::{ObjectKind, ObjectRef};
pub use crate::parser::{language, parse, SqlParser};
pub use crate::range::{line_col, ByteRange};
pub use crate::statement::{ParsedFile, Segment, Statement, StatementKind};

// `EngineKind` is re-exported because every entry point takes one, and making a
// caller name a third crate just to say which dialect a file is would be a poor
// trade for purity.
pub use picus_types::prelude::EngineKind;
