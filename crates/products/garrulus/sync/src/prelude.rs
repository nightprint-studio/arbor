//! Canonical entry point for `garrulus-sync`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `garrulus_sync::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but they are not the call-site path.

// `RelPath` is exported as `SyncRelPath`, and `read_note` / `write_note` /
// `classify` below carry sync-flavoured names, for the same reason `split_front`
// does: `garrulus-vault`'s prelude owns all four of those identifiers, and
// `garrulus-core` glob-merges both preludes. A consumer must never have to
// disambiguate — and a latent E0659 that only fires at the first call site is
// worse than an explicit name here.
//
// TODO(divergence): the two `RelPath`s mean the same thing and the vault's is the
// richer one (`escapes`, `glob_matches`, `stem`). Collapsing onto it would delete
// this alias and the `String` round-trips at the be seam.
pub use crate::change::{
    auto_commit_message, commit_identity, parse_name_status, slugify_device, ChangeBatch,
    ChangeKind, NoteChange, RelPath as SyncRelPath,
};
pub use crate::conflict::{
    append_merge_daily, is_daily_note, is_side_file, merge_note, side_file_name, Conflict,
    ConflictStamp, CONFLICT_MARKER,
};
pub use crate::error::{is_offline_message, SyncError, SyncResult};
pub use crate::files::{
    fnv1a64, hash_file, hash_tree, is_skipped_dir, read_note as read_note_text, walk_notes,
    write_note as write_note_atomic, MARKER_DIR,
};
pub use crate::folder::{parse_manifest, render_manifest, FolderRemote};
// Exported as `split_front` / `join_front`, NOT `split_frontmatter`:
// `garrulus-parse` owns a public `split_frontmatter` with a different signature,
// and a consumer glob-importing both preludes must not have to disambiguate.
pub use crate::frontmatter::{join as join_front, merge_frontmatter, parse_fields, split_front};
pub use crate::git::{parse_left_right, parse_log, CredentialProvider, GitRemote};
pub use crate::keyed::{keeps_one_sided, merge_keyed, render_fields, Clash, Field};
pub use crate::merge::{merge_lines3, merge_text3};
pub use crate::metadata::{is_metadata_path, merge_metadata, METADATA_DIR, TRASH_DIR};
pub use crate::remote::{
    PullOutcome, RemoteCapabilities, RemoteDescriptor, RemoteKind, Revision, SyncRemote,
};
pub use crate::state::{classify as classify_sync_state, StateInputs, SyncState};
