//! Imber's per-project context — standing notes about *this* project, beside the general ones.
//!
//! The general context (Settings › Imber) is about the user and goes to every conversation. This one is about a
//! repository — its conventions, what not to touch, the vocabulary of its domain — and travels with it, in
//! `<repo>/.arbor/bennu/config.toml` under `[assistant]`, like the other per-project preferences of Bennu. The
//! project's `CLAUDE.md` stays the place for what a whole team should read; this is for what is just yours.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::repo_config;

const SECTION: &str = "assistant";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct AssistantRepo {
    context: String,
}

/// The saved context of the project at `root`; empty when there is none.
pub(super) fn load(root: &str) -> String {
    if root.trim().is_empty() {
        return String::new();
    }
    repo_config::load::<AssistantRepo>(root, SECTION).context
}

/// The project's context, for the editor modal.
#[arbor_rpc::handler]
fn assistant_project_context_get(_ctx: &BennuState, root: String) -> Result<String, String> {
    Ok(load(&root))
}

/// Replace the project's context. Cut at the same cap as the general one, so the editor and the prompt agree.
#[arbor_rpc::handler]
fn assistant_project_context_set(_ctx: &BennuState, root: String, text: String) -> Result<(), String> {
    if root.trim().is_empty() {
        return Err("open a project first".into());
    }
    let context: String = text.chars().take(bennu_core::config::MAX_GENERAL_CONTEXT_CHARS).collect();
    repo_config::save(&root, SECTION, &AssistantRepo { context })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_is_saved_beside_the_projects_other_settings_and_read_back() {
        let dir = std::env::temp_dir().join(format!("bennu-asst-ctx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.display().to_string();
        assert_eq!(load(&root), "");
        repo_config::save(&root, "run", &AssistantRepo { context: "kept".into() }).unwrap();
        repo_config::save(&root, SECTION, &AssistantRepo { context: "Legacy Struts; never touch /vendor.".into() }).unwrap();
        assert_eq!(load(&root), "Legacy Struts; never touch /vendor.");
        assert_eq!(repo_config::load::<AssistantRepo>(&root, "run").context, "kept");
    }
}
