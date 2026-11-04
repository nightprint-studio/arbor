//! Running the requests of a `.http` file — the engine behind Bennu's HTTP files.
//!
//! Bennu's editor shows the file; this is what it asks: the **outline** (where each request is, for the gutter),
//! the **environments** of the `http-client.env.json` beside the file, the **diagnostics**, and **run** — one
//! request, picked by the caret line, sent through the same sender Hirundo's own requests use. The file's
//! `@name = value` variables, the selected environment, the dynamic variables and the environment's protection
//! (a type that asks before POST / PUT / PATCH / DELETE, guessed from the environment's name like an import
//! does) are all applied here.

use std::path::{Path, PathBuf};

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::*;
use serde::{Deserialize, Serialize};

use crate::send::{execute, ResponseOut, IN_FLIGHT};

/// How many folders up from the file the environment files are looked for.
const ENV_SEARCH_DEPTH: usize = 6;
const MAX_BODY_FILE: u64 = 8 * 1024 * 1024;

fn clock() -> (i64, u64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (now.as_millis() as i64, now.as_nanos() as u64)
}

/// The environments of the nearest `http-client.env.json` at or above `dir`, with its private twin merged in.
fn load_envs(dir: Option<&str>) -> Vec<Environment> {
    let Some(start) = dir.map(PathBuf::from) else { return Vec::new() };
    for folder in start.ancestors().take(ENV_SEARCH_DEPTH) {
        if let Ok(public) = std::fs::read_to_string(folder.join("http-client.env.json")) {
            let private = std::fs::read_to_string(folder.join("http-client.private.env.json")).ok();
            return parse_http_env(&public, private.as_deref()).unwrap_or_default();
        }
    }
    Vec::new()
}

#[derive(Debug, Serialize)]
pub struct OutlineEntry {
    pub title: String,
    pub method: Method,
    pub url: String,
    pub start_line: usize,
    pub request_line: usize,
    pub end_line: usize,
}

/// Where the requests of a text are.
#[arbor_rpc::handler]
fn http_outline(_state: &HirundoState, text: String) -> Result<Vec<OutlineEntry>, String> {
    Ok(parse_http_file(&text)
        .into_iter()
        .map(|e| OutlineEntry { title: e.title, method: e.request.method, url: e.request.url, start_line: e.start_line, request_line: e.request_line, end_line: e.end_line })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct EnvBriefOut {
    pub name: String,
    pub type_id: String,
    /// Asks before POST / PUT / PATCH / DELETE.
    pub protected: bool,
    /// The names of its variables, for completion.
    pub vars: Vec<String>,
}

/// The environments available to a file in `dir`.
#[arbor_rpc::handler]
fn http_envs(_state: &HirundoState, dir: Option<String>) -> Result<Vec<EnvBriefOut>, String> {
    let types = default_types();
    Ok(load_envs(dir.as_deref())
        .into_iter()
        .map(|e| EnvBriefOut { protected: type_of(&types, &e).is_some_and(|t| t.confirm_mutating), vars: e.vars.iter().map(|v| v.key.clone()).collect(), name: e.name, type_id: e.type_id })
        .collect())
}

/// What is wrong in the text, with the variables of the selected environment counted as defined.
#[arbor_rpc::handler]
fn http_check(_state: &HirundoState, text: String, dir: Option<String>, env: Option<String>) -> Result<Vec<HttpDiag>, String> {
    let known: Vec<String> = load_envs(dir.as_deref()).into_iter().filter(|e| env.as_deref().map_or(true, |n| e.name == n)).flat_map(|e| e.vars.into_iter().map(|v| v.key)).collect();
    Ok(diagnose_http(&text, &known))
}

#[derive(Debug, Deserialize)]
pub struct RunArgs {
    /// The whole file: its \`@variables\` apply to every request.
    pub text: String,
    /// The caret line (1-based); the request whose block holds it is run.
    pub line: usize,
    pub dir: Option<String>,
    pub env: Option<String>,
    pub request_id: String,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RunResult {
    Sent { name: String, method: Method, url: String, response: ResponseOut, tests: String, warnings: Vec<String> },
    Unresolved { name: String, names: Vec<String> },
    NeedsConfirmation { name: String, env: String, method: Method, url: String },
}

/// Which request a caret line belongs to: the block that holds it, else the nearest one above.
fn pick(entries: &[HttpEntry], line: usize) -> Option<&HttpEntry> {
    entries.iter().find(|e| e.start_line <= line && line <= e.end_line).or_else(|| entries.iter().rev().find(|e| e.request_line <= line))
}

#[arbor_rpc::handler]
async fn http_run(_state: &HirundoState, args: RunArgs) -> Result<RunResult, String> {
    let entries = parse_http_file(&args.text);
    let mut entry = pick(&entries, args.line).cloned().ok_or("put the caret in a request")?;

    // A body taken from a file (`< ./body.json`), beside the .http file.
    if let (Some(file), Some(dir)) = (entry.body_file.clone(), args.dir.as_deref()) {
        let path = Path::new(dir).join(&file);
        if path.is_file() && path.metadata().map(|m| m.len() <= MAX_BODY_FILE).unwrap_or(false) {
            entry.request.body.text = std::fs::read_to_string(&path).map_err(|e| format!("could not read the body file {file}: {e}"))?;
            if entry.request.body.kind == BodyKind::Empty {
                entry.request.body.kind = BodyKind::Text;
            }
        } else {
            entry.warnings.push(format!("The body file {file} was not found beside the .http file."));
        }
    }

    let envs = load_envs(args.dir.as_deref());
    let env = args.env.as_deref().and_then(|n| envs.iter().find(|e| e.name == n));
    let types = default_types();
    let ty = env.and_then(|e| type_of(&types, e));

    let file_layer = VarLayer { source: VarSource::new(VarScope::Collection, "file"), vars: file_variables(&args.text) };
    let (now_ms, entropy) = clock();
    let dynamic = dynamic_vars(&entry.request, now_ms, entropy);
    let layers = env_layers(vec![file_layer], ty, env, &[], &dynamic);
    let resolved = resolve_vars(&layers, &|_, _| None);
    let prepared = prepare(&entry.request, &Inherited::default(), &resolved);

    if !prepared.unresolved.is_empty() {
        return Ok(RunResult::Unresolved { name: entry.title, names: prepared.unresolved });
    }
    if needs_confirmation(ty, prepared.method) && !args.confirmed {
        return Ok(RunResult::NeedsConfirmation { name: entry.title, env: env.map(|e| e.name.clone()).unwrap_or_default(), method: prepared.method, url: prepared.url });
    }

    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());
    IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).insert(args.request_id.clone(), std::sync::Arc::clone(&cancel));
    let outcome = tokio::select! {
        r = execute(&prepared) => r,
        _ = cancel.notified() => Err("cancelled".to_string()),
    };
    IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).remove(&args.request_id);
    let response = outcome?;
    Ok(RunResult::Sent { name: entry.title, method: prepared.method, url: prepared.url, response, tests: entry.request.tests, warnings: entry.warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "@host = http://x\n\n### One\nGET {{host}}/a\n\n### Two\nPOST {{host}}/b\n";

    #[test]
    fn the_caret_picks_the_block_that_holds_it_or_the_nearest_above() {
        let entries = parse_http_file(FILE);
        assert_eq!(pick(&entries, 4).unwrap().title, "One");
        assert_eq!(pick(&entries, 5).unwrap().title, "One", "the blank line after a request is still its block");
        assert_eq!(pick(&entries, 8).unwrap().title, "Two");
        assert!(pick(&entries, 1).is_none(), "above the first request there is nothing to run");
    }

    #[test]
    fn no_env_files_means_no_environments() {
        assert!(load_envs(None).is_empty());
        assert!(load_envs(Some("/definitely/not/a/folder")).is_empty());
    }
}
