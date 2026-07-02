//! Canonical entry point for `bennu-intel`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intel::prelude::...`. The submodule stays `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::provider::{
    DocumentSymbol, IntelError, IntelProvider, Location, LspClientProvider, NativeJavaProvider,
    Position, TextEdit,
};

// The wire types the provider produces, re-exported so a consumer (bennu-be) reaches
// them through the intel prelude it already imports.
pub use bennu_proto::prelude::{CompletionItem, Diagnostic};

// The Phase-1 completion machinery, for the be layer to build a project's provider.
pub use crate::completion::completion;
pub use crate::java_index::{
    build_project_index, collect_java, file_records_from_source, project_type_map,
};
pub use crate::jdk::JdkMemberIndex;
pub use crate::resolver::{convert_members, IndexResolver};
