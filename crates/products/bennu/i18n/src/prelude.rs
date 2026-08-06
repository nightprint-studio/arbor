//! Canonical entry point for `bennu-i18n`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_i18n::prelude::...`. In practice a host needs [`MessagesExtension`] and nothing else —
//! everything below is reached through the [`FrameworkExtension`] trait.
//!
//! [`FrameworkExtension`]: bennu_ext::prelude::FrameworkExtension

// The extension itself — what a host registers.
pub use crate::ext::MessagesExtension;

// One bundle file and its keys.
pub use crate::bundle::{Bundle, Entry};

// Every bundle in the project.
pub use crate::catalog::{BundleCatalog, Declaration};

// Where a key is read.
pub use crate::refs::{is_scannable, key_at, key_prefix_at, keys_in, KeyRef};
