//! Bringing another project into the conversation when Imber asks Bennu about it.
//!
//! There is no tool for it: the door is any catalogue call whose `root` is a project the conversation does not
//! work in — `bennu_project_summary { root }` being the natural one, since it opens a project and says how far
//! its index is. Before such a call runs, the gate decides (`bennu_assistant::open_projects::decide`):
//!
//! * a project the user has open in the window is already indexed: within the reach it joins without a word,
//!   outside it the access card asks;
//! * an external one outside the reach goes through the access card alone, whose text says that a yes also
//!   opens and indexes it — one question for one thing;
//! * an external one inside the reach follows Settings › Imber › Access › *Opening other projects*.
//!
//! What joins stays in the conversation for its later runs (`runs::remember_opened_project`) and counts toward
//! the cap on projects. A project opened here is opened as the agent surface opens one — in the background, on
//! the lean profile — so it is released by the idle reaper like any project nobody is looking at.
//!
//! Any folder can be brought in; what gets **indexed** is only what can be: the Java index for a Maven or Gradle
//! project, and every language server whose root marker is at the root (`Cargo.toml`, `package.json`,
//! `.luarc.json`, `go.mod`, a server of your own…). A folder with neither joins as plain files — Bennu's tools
//! there answer only where a language server serves the file.
//!
//! Never from a scheduled run, an agent, or an isolated copy; never the root of a disk or the home folder itself,
//! which nobody means as one project.

use std::path::Path;

use bennu_assistant::prelude::{decide_open_project, AccessMode, Decision, Op, OpenProjects, OpenStep};

use super::ToolHost;
use crate::assistant::projects::{self, project_at, same_root, DEFAULT_CAP, MAX_CAP};
use crate::assistant::runs;
use crate::lsp_registry::SessionOrigin;

/// What the project is, as the card and the thread name it: its build kinds, and the language servers that start for it.
fn kind_words(modules: &[projects::Module], servers: &[String]) -> String {
    let mut kinds: Vec<&str> = modules.iter().map(|m| m.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    match (kinds.is_empty(), servers.is_empty()) {
        (true, true) => "folder".to_string(),
        (false, true) => kinds.join(" + "),
        (true, false) => format!("{} (language server)", servers.join(", ")),
        (false, false) => format!("{} ({})", kinds.join(" + "), servers.join(", ")),
    }
}

/// The root of a disk, or the home folder itself: never one project.
fn too_wide(path: &Path) -> bool {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from);
    path.parent().is_none() || home.is_some_and(|h| h == path)
}

impl ToolHost {
    /// Settle a catalogue call aimed at a project the conversation does not work in: bring it in, ask, or refuse.
    /// `Ok` with nothing to do when the call names no such project.
    pub(super) fn open_project_gate(&self, input: &serde_json::Value) -> Result<(), String> {
        let flat = self.catalog_args(input);
        let Some(root) = self.foreign_root(&flat) else { return Ok(()) };
        let path = Path::new(&root);
        if self.session.unattended {
            return Err(format!("`{root}` is not one of the projects in this conversation, and a scheduled run does not open others"));
        }
        if !self.session.may_open_projects {
            return Err(format!(
                "`{root}` is not one of the projects in this conversation, and this conversation cannot open others (an agent works in its parent's projects; an isolated copy sees only the copy)"
            ));
        }
        if !path.is_dir() {
            return Err(format!("`{root}` is not a folder on this machine"));
        }
        if too_wide(path) {
            return Err(format!("`{root}` is a whole disk or the home folder: name the project folder inside it"));
        }
        let modules = projects::modules_of(path);
        let servers = crate::lsp_registry::LspRegistry::global().servers_for_root(&root);
        let config = bennu_core::config::load().assistant;
        let cap = match config.max_projects as usize {
            0 => DEFAULT_CAP,
            n => n.min(MAX_CAP),
        };
        let current = self.session.projects();
        if current.len() >= cap {
            return Err(format!(
                "this conversation already works across {} projects, the most Settings › Imber › Access allows: finish with one, or ask the user to raise the limit",
                current.len()
            ));
        }

        let in_window = self.session.window_roots.iter().any(|w| same_root(w, &root));
        let decision = self.session.policy().check(path, Op::List);
        let permitted = matches!(decision, Decision::Allow);
        let setting = OpenProjects::parse(&config.open_projects);
        let kind = kind_words(&modules, &servers);
        let indexable = !servers.is_empty() || modules.iter().any(|m| matches!(m.kind, "Maven" | "Gradle"));
        let what = if indexable {
            "Opening it indexes it in the background, so Imber can use Bennu's tools there (symbols, references, problems). "
        } else {
            "There is nothing to index there (no Java build, no language server for it): Imber works with its files. "
        };
        match decide_open_project(setting, in_window, permitted, self.session.mode) {
            OpenStep::Refuse(why) => return Err(why),
            OpenStep::Join | OpenStep::Open => {}
            OpenStep::AskToOpen => {
                let autonomous_note = if self.session.mode == AccessMode::Autonomous { "" } else { " In Autonomous mode it would open without asking (Settings › Imber › Access)." };
                self.confirm(
                    "Open another project for Imber?",
                    &format!(
                        "{root}\n{kind}, not open in Bennu.\n\n{what}It stays part of this conversation and is closed when it has been idle for a while.{autonomous_note}"
                    ),
                    false,
                )?;
            }
            OpenStep::AskAccess { indexes } => match decision {
                Decision::Deny(why) => return Err(why),
                Decision::Ask(ask) => {
                    let what = if indexes {
                        format!("Imber wants to use Bennu on this {kind}, which is not open. Allowing access also opens it for this conversation{}.", if indexable { " and indexes it" } else { "" })
                    } else {
                        format!("Imber wants to use Bennu on this {kind}, which you have open but which is outside its reach.")
                    };
                    match ask.grant.clone() {
                        Some(folder) => self.ask_access(&folder, path, &what, ask.upgrade, Op::List)?,
                        None => self.confirm("Allow this?", &format!("{what}\n\n{}", ask.message), true)?,
                    }
                }
                Decision::Allow => {}
            },
        }
        self.bring_in(&root, !in_window, &kind)
    }

    /// Open `root` when it is not open yet, and add it to the conversation for this run and the next ones.
    fn bring_in(&self, root: &str, open: bool, kind: &str) -> Result<(), String> {
        if open {
            crate::project::open_and_start(&self.session.ctx, root, SessionOrigin::Background, true)
                .map_err(|e| format!("`{root}` could not be opened: {e}"))?;
        }
        let project = project_at(root, &self.session.projects());
        let name = project.name.clone();
        self.session.add_project(project);
        runs::remember_opened_project(&self.session.conversation_id, root);
        let how = if open { "was opened for this conversation (indexed in the background where it can be)" } else { "joined this conversation (already open in Bennu)" };
        self.note("info", &format!("`{name}` ({root}, {kind}) {how}."));
        Ok(())
    }
}
