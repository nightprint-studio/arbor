//! Canonical entry point for `arbor-process-ext`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_process_ext::prelude::...`. The trait also stays re-exported at the
//! crate root for existing call sites.

pub use crate::NoWindowExt;

// Children that die with their parent — see [`crate::job`].
pub use crate::job::{
    kill_descendants_on_exit, run_watchdog_if_requested, track_child_group, untrack_child_group,
};

// Finding a tool on this machine — see [`crate::locate`] for why a `PATH` lookup is not enough.
pub use crate::locate::{executable_in, generic_bin_dirs, locate_executable};
