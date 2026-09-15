//! Canonical entry point for `bennu-xsd`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_xsd::prelude::...`.

pub use crate::model::{
    local, ComplexType, Group, SimpleType, Xsd, XsdAttribute, XsdElement,
};
pub use crate::parse::parse;
