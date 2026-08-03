//! Canonical entry point for `bennu-ext`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_ext::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// What an extension is given, and what it contributes.
pub use crate::model::{
    ExtEntry, ExtGutterMark, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx, ProjectScan,
    ScannedFile,
};

// The trait a framework plugin implements + the capability-gated registry over it.
pub use crate::registry::{ExtensionRegistry, FrameworkExtension};
