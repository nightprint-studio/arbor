//! Re-export of the workspace-shared `NoWindowExt` extension trait.
//!
//! The trait lives in the `arbor-process-ext` crate so other workspace
//! members (`arbor-cloud`, future splits) can use it without depending on
//! the main `arbor` binary. Existing call sites in this crate keep working
//! unchanged because the re-export preserves the `crate::process_ext::NoWindowExt`
//! path.

pub use arbor_process_ext::NoWindowExt;
