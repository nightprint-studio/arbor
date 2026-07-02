//! Canonical entry point for `bennu-java`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

// The two entry points.
pub use crate::infer::infer_receiver_type;
pub use crate::symbols::extract_symbols;

// The structural model produced by `extract_symbols`.
pub use crate::symbols::{FieldDecl, FileSymbols, Import, MethodDecl, ParamDecl, TypeDecl};

// The resolver seam the type-walk consumes + the member shapes it resolves against.
pub use crate::seam::{ClassMembers, Member, MemberKind, TypeRef, TypeResolver, Visibility};
