//! Canonical entry point for `corvus-plugin-ns`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_plugin_ns::prelude::...` (or a single
//! `use corvus_plugin_ns::prelude::*;`). The submodules stay `pub` for rustdoc
//! navigation, but the canonical path goes through here.

pub use crate::brp::BrpInstaller;
pub use crate::ci::CiInstaller;
pub use crate::cloud::CloudInstaller;
pub use crate::issues::IssuesInstaller;
pub use crate::job::JobInstaller;
pub use crate::linked_worktrees::LinkedWorktreesInstaller;
pub use crate::mr::MrInstaller;
pub use crate::notes::NotesInstaller;
pub use crate::nshost::{NsHost, NsHostHandle};
pub use crate::pipeline::PipelineInstaller;
pub use crate::repo::RepoInstaller;
pub use crate::security::SecurityInstaller;
pub use crate::tabs::TabsInstaller;
pub use crate::terminal::TerminalInstaller;
pub use crate::toolchain::ToolchainInstaller;
pub use crate::ui_branding::UiBrandingInstaller;
pub use crate::workspace::WorkspaceInstaller;
