//! The crate's public surface, in one import.
//!
//! Per the workspace convention: submodules stay `pub` so rustdoc reads well, but
//! every call site goes through here.

pub use crate::edit::{apply, render, render_with, TextEdit};
pub use crate::error::SyntaxError;
pub use crate::outline::{
    node_path_at, node_path_at_with, outline, outline_with, Injection, OutlineOptions, SyntaxNode,
    SyntaxTree,
};
pub use crate::pattern::{Arity, Capture, Match, Pattern};
pub use crate::range::ByteRange;
