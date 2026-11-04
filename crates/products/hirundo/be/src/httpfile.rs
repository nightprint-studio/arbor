//! `.http` files in and out, and IntelliJ's environment files in.
//!
//! The format is `hirundo-model::httpfile` (pure, tested). Here is the I/O: a file given by path or URL or
//! pasted, a body taken from the file a `< ./body.json` line names, requests written to a file, and the
//! `http-client.env.json` / `http-client.private.env.json` pair read from a folder and turned into environments.

use std::path::Path;

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{parse_http_env, parse_http_file, to_http_file, HttpEntry};
use serde::Serialize;

use crate::context::store_for;
use crate::openapi::source_text;

/// The largest body file read.
const MAX_BODY_FILE: u64 = 8 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct HttpImport {
    pub entries: Vec<HttpEntry>,
}

/// A `.http` text, or the path of one, as requests. `< ./file` bodies are read relative to the file when the
/// source is a path; from pasted text they stay a warning.
#[arbor_rpc::handler]
async fn http_import(_state: &HirundoState, source: String) -> Result<HttpImport, String> {
    let text = source_text(&source).await?;
    let dir = {
        let s = source.trim();
        Path::new(s).is_file().then(|| Path::new(s).parent().map(Path::to_path_buf)).flatten()
    };
    let mut entries = parse_http_file(&text);
    if entries.is_empty() {
        return Err("no request found: a request is a line like `GET https://host/path`".into());
    }
    for entry in &mut entries {
        let Some(file) = entry.body_file.clone() else { continue };
        let resolved = dir.as_ref().map(|d| d.join(&file));
        match resolved.filter(|p| p.is_file() && p.metadata().map(|m| m.len() <= MAX_BODY_FILE).unwrap_or(false)) {
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(body) => {
                    entry.request.body.text = body;
                    if entry.request.body.kind == hirundo_model::prelude::BodyKind::Empty {
                        entry.request.body.kind = hirundo_model::prelude::BodyKind::Text;
                    }
                }
                Err(e) => entry.warnings.push(format!("Could not read the body file {file}: {e}")),
            },
            None => entry.warnings.push(format!("The body file {file} was not read: import the .http file by its path, or paste the body.")),
        }
    }
    Ok(HttpImport { entries })
}

/// Requests of the project as a `.http` text; with `file`, also written there. `paths` are request files.
#[arbor_rpc::handler]
fn http_export(_state: &HirundoState, project: String, paths: Vec<String>, file: Option<String>) -> Result<String, String> {
    let store = store_for(&project)?;
    let requests = paths.iter().map(|p| store.read_request(p)).collect::<Result<Vec<_>, _>>()?;
    if requests.is_empty() {
        return Err("there is no request to export".into());
    }
    let text = to_http_file(&requests);
    if let Some(file) = file.filter(|f| !f.trim().is_empty()) {
        std::fs::write(&file, &text).map_err(|e| format!("could not write {file}: {e}"))?;
    }
    Ok(text)
}

/// The environments of an IntelliJ `http-client.env.json` (and its private twin) found in `dir`, saved into the
/// project. An environment that already exists is left alone. Returns how many were added.
#[arbor_rpc::handler]
fn http_env_import(_state: &HirundoState, project: String, dir: String) -> Result<usize, String> {
    let dir = Path::new(&dir);
    let public = std::fs::read_to_string(dir.join("http-client.env.json")).map_err(|_| format!("there is no http-client.env.json in {}", dir.display()))?;
    let private = std::fs::read_to_string(dir.join("http-client.private.env.json")).ok();
    let store = store_for(&project)?;
    let existing: Vec<String> = store.list_environments()?.into_iter().map(|e| e.id).collect();
    let mut added = 0;
    for env in parse_http_env(&public, private.as_deref())? {
        if existing.contains(&env.id) {
            continue;
        }
        store.write_environment(&env)?;
        added += 1;
    }
    Ok(added)
}
