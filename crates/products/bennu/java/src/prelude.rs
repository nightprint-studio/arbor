//! Canonical entry point for `bennu-java`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

// The type-inference entry points: the one-off caret query (parses + extracts) and the
// reuse-an-existing-tree variant for the hot reference-walk path.
pub use crate::infer::{infer_receiver_type, infer_receiver_type_at};
pub use crate::symbols::{extract_symbols, extract_symbols_from_root};

// The structural model produced by `extract_symbols`.
pub use crate::symbols::{FieldDecl, FileSymbols, Import, MethodDecl, ParamDecl, TypeDecl};

// The resolver seam the type-walk consumes + the member shapes it resolves against.
pub use crate::seam::{ClassMembers, Member, MemberKind, TypeRef, TypeResolver, Visibility};
