//! Canonical entry point for `arbor-rpc`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_rpc::prelude::...`.

pub use crate::{
    async_registry_for, decode_field, handler, object_schema, registry, registry_for, schema_of,
    tools, tools_for, AsyncCallFn, Builder, CallFn, Entry, HandlerEntry, Kind, RpcBundle, Safety,
    ToolDescriptor, ToolField, ToolMeta, ToolOutput,
};
