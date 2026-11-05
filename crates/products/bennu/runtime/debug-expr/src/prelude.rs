//! Canonical entry point for `bennu-debug-expr`'s public API.
//!
//! Workspace convention: call sites reach this crate through `bennu_debug_expr::prelude::...`. The
//! submodules stay public for rustdoc navigation, but they are not the path a host imports from.

pub use crate::ast::{BinOp, Expr, Lit, UnOp};
pub use crate::condition::parse_condition;
pub use crate::eval::{eval, eval_condition};
pub use crate::java_names::{candidate_classes, parse_imports, static_import_owners, JavaImports};
pub use crate::ops::{binary, unary};
pub use crate::parser::parse as parse_expr;
pub use crate::path::{is_path, parse as parse_path, path_text, Step};
pub use crate::syntax::{Lang, Syntax, JAVA, RUST};
pub use crate::target::DebugTarget;
pub use crate::value::{Prim, Val};
