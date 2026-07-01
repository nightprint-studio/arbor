//! Canonical entry point for `arbor-studio-types`' public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_studio_types::prelude::...` (or a single
//! `use arbor_studio_types::prelude::*;`). The submodules stay `pub` for
//! rustdoc navigation but are not the canonical call-site path.

pub use crate::descriptor::{
    CrossRefScope, FormatDescriptor, IconRef, KindStyle, KindTone, NullPolicy, QuerySyntax,
    SaveWarningKind, SchemaSourceKind,
};
pub use crate::dto::{
    BulkEditAction, BulkEditFailure, BulkEditLiteral, BulkEditOpenDoc, BulkEditPreview,
    BulkEditResult, BulkEditScope, BulkEditSite, BulkEditValueSource, DiffHunk, DiffLine,
    DiffLineKind, DiffStatus, DiffTreeNode, DocSnapshot, EncodingInfo, FileEntry, MutateResult,
    NodeView, ParseResult, QueryHit, RenameCollision, RenameDirtyBlocker, RenameFailure,
    RenameOpenDoc, RenamePreview, RenameResult, RenameSite, RenameSiteScope, SchemaHint,
    SchemaHintOrigin, StudioMutation, UpdateResult,
};
pub use crate::errors::{to_ipc, StudioError, StudioResult};
pub use crate::schema::{
    CandidateKind, CrateProbe, FieldDef, ResolvedType, RootCandidate, Schema, SchemaStats,
    TypeDef, TypeSource, VariantDef, VariantShape,
};
