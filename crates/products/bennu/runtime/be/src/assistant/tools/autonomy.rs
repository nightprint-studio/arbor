//! The host's half of Autonomous mode: remembering which files the run's builds and checks reported errors in,
//! and asking the pure rule ([`bennu_assistant::prelude::decide_grant`]) whether one of them may be opened
//! without a prompt. The rule itself, and every row of its table, live in `bennu-assistant`.

use std::path::{Path, PathBuf};

use bennu_assistant::prelude::{build_error_ask, decide_grant, diagnostic_files, Ask, GrantQuery, Op, Verdict};
use serde_json::Value;

use super::ToolHost;

impl ToolHost {
    /// Remember the files a build or check of this run just reported an error in. Cheap and always on: in
    /// the other modes the same list only changes the wording of the prompt.
    pub(super) fn note_diagnostics(&self, tool: &str, root: &str, result: &Value) {
        let files = diagnostic_files(tool, result, Path::new(root));
        if !files.is_empty() {
            self.session.diagnostics.lock().unwrap_or_else(|p| p.into_inner()).note(files);
        }
    }

    /// Open `path` for this run, with no prompt, when the mode allows it and the build named the file.
    /// Returns whether it did; if not, the ordinary prompt follows.
    pub(super) fn open_if_the_build_named_it(&self, path: &Path, op: Op) -> bool {
        if !self.session.mode.may_raise_access() {
            return false;
        }
        let diagnostics: Vec<PathBuf> = self.session.diagnostics.lock().unwrap_or_else(|p| p.into_inner()).files().to_vec();
        let others: Vec<PathBuf> =
            self.session.projects.iter().map(|p| PathBuf::from(&p.root)).filter(|r| r.as_path() != Path::new(&self.session.root)).collect();
        let used = self.session.run_grants.lock().unwrap_or_else(|p| p.into_inner()).used();
        let scope = self.session.policy().scope;
        let verdict = decide_grant(&GrantQuery {
            mode: self.session.mode,
            scope,
            op,
            path,
            diagnostics: &diagnostics,
            project_root: Path::new(&self.session.root),
            other_projects: &others,
            used,
        });
        if verdict != Verdict::Grant {
            return false;
        }
        self.session.open_file(path, &self.display_path(&path.display().to_string()));
        true
    }

    /// The sentence put to the user for a folder outside the allowed area. When the build named the file it
    /// says so, because "outside what you allowed" alone is a puzzle to someone who never mentioned that file.
    pub(super) fn access_question(&self, path: &Path, folder: &Path, ask: &Ask) -> String {
        let named = self.session.diagnostics.lock().unwrap_or_else(|p| p.into_inner()).names(path);
        if !named || ask.upgrade {
            return ask.message.clone();
        }
        build_error_ask(path, folder, self.session.policy().scope_words())
    }
}
