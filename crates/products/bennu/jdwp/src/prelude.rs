//! Canonical entry point for `bennu-jdwp`'s public API.
//!
//! Workspace convention: call sites reach this crate through `bennu_jdwp::prelude::...` (or
//! one `use bennu_jdwp::prelude::*;`). The submodules stay public for rustdoc navigation, but
//! they are not the path a host should import from.
//!
//! A host needs [`Client`] to attach, the [`command`](crate::command) functions to ask
//! things, and [`Event`] to be told when the program stopped.

pub use crate::client::Client;
pub use crate::codec::{Id, IdSizes, Location, Tag, Value};
pub use crate::command::{
    array_length, array_values, class_name, class_signature, classes_by_signature, clear_event,
    dispose, fields, frame_this, frame_values, frames, id_sizes, line_table, location_of_line,
    methods, object_type, object_values, request_class_prepare, request_exception, request_step,
    resume_thread, resume_vm, set_breakpoint, string_value, superclass, suspend_vm, thread_name,
    type_signature, variable_table, version, ClassRef, Field, Frame, LineEntry, Local, Method,
    StepDepth, MOD_STATIC,
};
pub use crate::error::{error_name, JdwpError, Result};
pub use crate::event::{kind, parse_composite, Composite, Event, SuspendPolicy};
pub use crate::packet::{Packet, PacketKind};
