//! Canonical entry point for `corvus-brp`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_brp::prelude::...` (or a single `use arbor_brp::prelude::*;`). The
//! submodules stay `pub` for rustdoc navigation but are not the canonical
//! call-site path.

pub use crate::client::{BrpClient, BrpError};
pub use crate::dialect::{MethodMap, Op};
pub use crate::registry::{
    skeleton, variant_skeleton, FieldShape, Shape, TypeFilter, TypeInfo, TypeRegistry, VariantEntry,
    VariantShape,
};
pub use crate::sse::{run_watch_stream, WatchEvent};
pub use crate::world::{
    components_set_changed, entity_ids, entity_index, parse_entity_ref, preferred_type_paths,
    short_type_name, type_paths, unwrap_components, unwrap_resource,
    ComponentsReply, EntityRow, Page, PageQuery, RowView, Snapshot,
};
pub use crate::{
    methods, probe_capabilities, BrpCapabilities, BrpRegistry, BrpSession, BrpStatus, WatchSub,
    DEFAULT_ENDPOINT,
};
