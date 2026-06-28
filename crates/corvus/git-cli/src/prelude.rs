//! The crate's single public entry point. Consumers import via
//! `corvus_git_cli::prelude::*` (or fully-qualified `corvus_git_cli::prelude::…`).

pub use crate::error::{GitCliError, Result};
pub use crate::{
    clear_override, command, detect, download_portable, download_supported, portable_dir,
    request_download_cancel, set_path, set_portable_dir_override, snapshot, verify,
    DownloadProgress, GitCliState,
};
