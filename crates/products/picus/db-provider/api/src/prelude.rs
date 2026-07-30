//! Canonical entry point for `picus-db-api`'s public API.
//!
//! Workspace convention: call sites (in `picus-be` and in each engine crate) reach
//! this crate's surface through `picus_db_api::prelude::...`. The submodules stay
//! `pub` for rustdoc navigation, but the prelude is the canonical call-site path.

// The shared vocabulary, re-exported so a driver-side call site imports one
// prelude rather than two. Defined in `picus-types`, the leaf both halves share.
pub use picus_types::prelude::{
    Column, EngineKind, ForeignKey, IndexInfo, RelationKind, SchemaSnapshot, SequenceInfo,
    TableInfo, TriggerDetail, TriggerInfo,
};

pub use crate::activity::{ActivitySnapshot, BlockEdge, SessionActivity, StopKind};
pub use crate::capability::{EngineCapabilities, SchemaGroup};
pub use crate::depends::{DependencyEdge, DependencyGraph, DependencyKind, DependencyNode};
pub use crate::connection::{ConnectionSpec, ConnectionState, ConnectionStatus};
pub use crate::descriptor::{
    ConnectionField, DbProviderDescriptor, EmissionTraits, FieldKind, IdentifierCase, SelectOption,
};
pub use crate::error::{DbError, DbResult};
pub use crate::plan::{PlanNode, PlanRequest, QueryPlan};
pub use crate::provider::{DbProvider, DbSession};
pub use crate::query::{
    BindSlot, BindValue, CellValue, ExecuteResult, ResultCount, ResultWindow, DEFAULT_WINDOW_ROWS,
};
pub use crate::tx::{TxCapability, TxOutcome, TxState};
pub use crate::registry::DbProviderRegistry;
pub use crate::secret::{NoSecrets, Secret, SecretResolver};
