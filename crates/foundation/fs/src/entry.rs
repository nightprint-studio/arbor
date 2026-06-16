//! Serializable DTOs shared between `arbor-fs` and the shell's FS commands.
//!
//! These are the FE-facing shapes the explorer renders. They live here (not in
//! the shell) so the pure layer and the command wrappers agree on one
//! definition. `serde::Serialize` only — they're produced by the backend and
//! read by the frontend, never deserialized here.

use serde::Serialize;

/// One directory entry with the metadata the explorer shows in a single call.
#[derive(Debug, Serialize, Clone)]
pub struct FsEntry {
    pub name:     String,
    pub path:     String,
    pub is_dir:   bool,
    /// File size in bytes. `None` for directories or on error.
    pub size:     Option<u64>,
    /// Last-modified time as Unix timestamp in milliseconds. `None` on error.
    pub modified: Option<i64>,
    /// Creation time as Unix timestamp in milliseconds. `None` on error or on
    /// platforms/filesystems that don't record a birth time (e.g. many Linux FS).
    pub created:  Option<i64>,
}

/// A filesystem quick-access root (a user dir, a drive, or a WSL distro mount).
#[derive(Debug, Serialize, Clone)]
pub struct FsRoot {
    pub name: String,
    pub path: String,
    /// "home" | "desktop" | "documents" | "downloads" | "drive" | "wsl"
    pub kind: String,
}

/// One item currently in the OS trash / Recycle Bin.
#[derive(Debug, Serialize, Clone)]
pub struct TrashEntry {
    /// Opaque, stable handle (the OS trash id) used to restore / purge it.
    pub id:            String,
    pub name:          String,
    /// Original absolute path it was deleted from (parent + name).
    pub original_path: String,
    /// Deletion time as a Unix timestamp in seconds (`None` when unknown).
    pub deleted_at:    Option<i64>,
}

/// Recursive size of a path: total file bytes plus file / sub-directory counts.
#[derive(Debug, Serialize, Clone)]
pub struct DirSize {
    /// Total bytes of all files under the path (directories themselves: 0).
    pub bytes: u64,
    /// File count (excluding directories).
    pub files: u64,
    /// Sub-directory count (excluding the path itself).
    pub dirs:  u64,
}

/// Capacity / free space for one drive (Overview dashboard).
#[derive(Debug, Serialize, Clone)]
pub struct DriveUsage {
    pub name:  String,
    pub path:  String,
    /// Total capacity in bytes. `None` when the platform/volume can't report it.
    pub total: Option<u64>,
    /// Free (available-to-caller) bytes. `None` when unavailable.
    pub free:  Option<u64>,
}

/// Aggregate storage stats across all drives (Overview dashboard).
#[derive(Debug, Serialize, Clone)]
pub struct OverviewStats {
    pub drives:         Vec<DriveUsage>,
    /// Sum of known drive capacities (bytes).
    pub total_capacity: u64,
    /// Sum of known free space (bytes).
    pub total_free:     u64,
}

/// Convert a filesystem timestamp into a Unix-epoch value in milliseconds,
/// swallowing the `io::Result` and any pre-epoch / unsupported cases to `None`.
pub(crate) fn to_unix_ms(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}
