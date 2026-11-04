//! Canonical entry point for `hirundo-core`'s public API.

pub use crate::secrets::{read_secret, secret_account};
pub use crate::state::HirundoState;
pub use crate::store::{clean_rel, CollectionNode, Node, Store, DIR};
