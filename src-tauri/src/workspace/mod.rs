// ---------------------------------------------------------------------------
// Workspace subsystem — GitKraken-style grouping of registered repos.
//
// Three pieces of persistent state live side-by-side under ~/.config/arbor/:
//
//   * repos.json              — RepoRegistry (UUID → path/url/display name).
//                               Central truth for "which repos has Arbor ever
//                               known about".  Referenced by workspace
//                               members and tab snapshots.
//
//   * workspaces.json         — WorkspaceStore (ordered list of
//                               WorkspaceDef + active_workspace_id).  Each
//                               workspace carries its repo membership by
//                               UUID, a colour index, an order, plus a
//                               handful of reserved extensibility fields
//                               (metadata, settings_override, git_identity)
//                               so future customisation does not require a
//                               schema migration.
//
//   * workspace-state/<id>.json  — TabSnapshot per workspace (list of open
//                                  repo-ids, active one, cross-workspace
//                                  tabs).  Kept in a separate file so a
//                                  corrupted snapshot never takes down the
//                                  registry.
//
// A fixed-ID "scratch" workspace exists as a fall-back for ad-hoc opens.
// ---------------------------------------------------------------------------

pub mod registry;
pub mod store;
pub mod snapshot;
pub mod migration;

pub use registry::{RepoRegistry, RepoRegistryEntry};
pub use store::{WorkspaceDef, WorkspaceGroup, WorkspaceStore, SCRATCH_ID};
pub use snapshot::{CrossWsTabRef, TabMeta, TabSnapshot};
