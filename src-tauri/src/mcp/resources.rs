//! What Arbor offers to read: the projects the user currently has open.
//!
//! This exists because of the question every session opens with — *what is this person
//! working on* — and because neither of the other two shapes answers it well. As a tool
//! it would be a call spent before any real work; as text in the server's `instructions`
//! it would be fixed at connection time and wrong the moment they opened something else.
//! A resource is re-read on demand and attached by the client, which is exactly the
//! lifetime this fact has.
//!
//! Read-only and un-prompted, and that is safe for the same reason it is useful: it
//! reports *that* a project is open and where, never its contents. Reading a file is a
//! tool call, and goes through scope and policy like every other one.

use arbor_mcp::prelude::{Resource, ResourceCatalog, ResourceContents};
use async_trait::async_trait;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// The URI prefix every resource here carries, so a URI is traceable to what made it.
const PREFIX: &str = "arbor://project/";

/// Arbor's read-only context.
pub struct ShellResources {
    app: AppHandle,
}

impl ShellResources {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// The open projects, newest first, deduplicated by path.
    ///
    /// Two sources because neither is complete: Corvus knows its open repositories, and
    /// every other product reports into the shared recents list as it opens something.
    /// A user with Bennu on a Maven project and no repo in Corvus is invisible to the
    /// first; a repo opened in this session but never recorded is invisible to the second.
    fn projects(&self) -> Vec<(String, String, String)> {
        let state = self.app.state::<AppState>();
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<(String, String, String)> = Vec::new();

        if let Ok(cfg) = state.lock_config() {
            let mut recents = cfg.recents.clone();
            recents.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
            for entry in recents {
                if seen.insert(entry.path.clone()) {
                    out.push((entry.path, entry.name, entry.product));
                }
            }
        }
        for path in crate::ipc::open_repo_paths(state.inner()) {
            if seen.insert(path.clone()) {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                out.push((path, name, "corvus".to_string()));
            }
        }
        out
    }
}

#[async_trait]
impl ResourceCatalog for ShellResources {
    async fn list(&self) -> Vec<Resource> {
        self.projects()
            .into_iter()
            .map(|(path, name, product)| Resource {
                uri: format!("{PREFIX}{path}"),
                name: name.clone(),
                title: Some(format!("{name} — open in {product}")),
                description: Some(format!(
                    "A project the user has open in Arbor ({product}), rooted at {path}."
                )),
                mime_type: Some("text/markdown".to_string()),
            })
            .collect()
    }

    async fn read(&self, uri: &str) -> Result<Vec<ResourceContents>, String> {
        let Some(path) = uri.strip_prefix(PREFIX) else {
            return Err(format!("`{uri}` is not an Arbor project resource"));
        };
        let found = self.projects().into_iter().find(|(p, _, _)| p == path);
        let Some((path, name, product)) = found else {
            // Not an empty success: a stale URI from a cached listing must read as a
            // mistake to correct, not as a project with nothing in it.
            return Err(format!(
                "No project is open at `{path}`. Call resources/list again — the set changes \
                 as the user opens and closes things."
            ));
        };

        let text = format!(
            "# {name}\n\n\
             - **Root**: `{path}`\n\
             - **Open in**: {product}\n\n\
             This is the project's identity, not its contents. To read its files, use the \
             product's own tools — for a Java or Rust project that is `bennu_project_summary` \
             first, then `bennu_read_file`, which decodes each file in the encoding the \
             project actually declares.\n"
        );

        Ok(vec![ResourceContents {
            uri: uri.to_string(),
            mime_type: Some("text/markdown".to_string()),
            text,
        }])
    }
}
