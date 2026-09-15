//! Canonical entry point for `bennu-dtd`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_dtd::prelude::...`.

pub use crate::model::{
    AttListDecl, AttrDecl, AttrKind, Content, DefaultDecl, Dtd, ElementDecl, EntityDecl, Occurs,
    Particle,
};
pub use crate::parse::parse;
