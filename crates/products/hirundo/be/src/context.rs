//! Everything a request needs from around it, gathered once: the collection and folders above it, the
//! environment it is sent in (and that environment's type), this machine's local values, and the secrets
//! the variables ask for. The result is `hirundo-model`'s [`Prepared`] — the one value that preview,
//! "copy as cURL" and the sender all share.

use std::path::Path;

use hirundo_core::prelude::{read_secret, HirundoState, Store};
use hirundo_model::prelude::*;
use serde::{Deserialize, Serialize};

/// A request and where it stands: which project, where in the tree, in which environment.
#[derive(Debug, Deserialize)]
pub struct Target {
    pub project: String,
    pub request: HttpRequest,
    /// Where it lives in the tree (`bandi/auth/login.toml`). Absent for a request not saved yet, which
    /// then inherits nothing.
    pub path: Option<String>,
    /// The environment to send in; the active one (`local.toml`) when absent or empty.
    pub env_id: Option<String>,
}

/// The environment a request is about to go to, for the bar that names it.
#[derive(Debug, Clone, Serialize)]
pub struct EnvBrief {
    pub id: String,
    pub name: String,
    pub type_id: String,
    pub type_name: String,
    pub color: String,
    pub confirm_mutating: bool,
}

pub struct Built {
    pub prepared: Prepared,
    pub resolved: ResolvedVars,
    pub env: Option<EnvBrief>,
    /// The environment's type asks before this method is sent.
    pub needs_confirmation: bool,
}

/// Unix milliseconds and a seed that differs between calls (the model has neither a clock nor randomness).
fn clock() -> (i64, u64) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let tick = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    (now.as_millis() as i64, (now.as_nanos() as u64) ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

pub fn store_for(project: &str) -> Result<Store, String> {
    let dir = Path::new(project);
    if project.trim().is_empty() || !dir.is_dir() {
        return Err(format!("'{project}' is not a folder"));
    }
    Ok(Store::for_project(dir))
}

/// Fold the request with its surroundings. With `resolve` off no variable is looked up (the secrets are not
/// asked for), and the result keeps its `{{placeholders}}` — what "copy as cURL, unresolved" prints.
pub fn build(state: &HirundoState, target: &Target, resolve: bool) -> Result<Built, String> {
    let store = store_for(&target.project)?;
    let chain = match &target.path {
        Some(path) => store.chain(path)?,
        None => Vec::new(),
    };
    let types = store.read_types()?;
    let environments = store.list_environments()?;
    let local = store.read_local();
    let env_id = target.env_id.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| local.active_env.clone());
    let env = environments.iter().find(|e| e.id == env_id);
    let ty = env.and_then(|e| type_of(&types, e));

    let inherited_vars: Vec<VarLayer> = chain
        .iter()
        .enumerate()
        .map(|(depth, (rel, meta))| VarLayer { source: VarSource::new(if depth == 0 { VarScope::Collection } else { VarScope::Folder }, rel), vars: meta.vars.clone() })
        .collect();
    // `{{$uuid}}`, `{{$timestamp}}`…: generated per send, the last layer of the cascade.
    let (now_ms, entropy) = clock();
    let dynamic = dynamic_vars(&target.request, now_ms, entropy);
    let layers = env_layers(inherited_vars, ty, env, &local.vars, &dynamic);
    let resolved = if resolve { resolve_vars(&layers, &|source, name| read_secret(state, &target.project, source, name)) } else { ResolvedVars::default() };

    let inherited = Inherited { auth: chain.iter().map(|(_, m)| &m.auth).collect(), headers: chain.iter().map(|(_, m)| m.headers.as_slice()).collect() };
    let mut prepared = prepare(&target.request, &inherited, &resolved);
    // A collection or folder that switches the jar off decides for everything below it.
    if chain.iter().any(|(_, m)| m.no_cookies) {
        prepared.cookie_jar = false;
    }

    Ok(Built {
        prepared,
        resolved,
        env: env.map(|e| EnvBrief {
            id: e.id.clone(),
            name: e.name.clone(),
            type_id: e.type_id.clone(),
            type_name: ty.map(|t| t.name.clone()).unwrap_or_default(),
            color: ty.map(|t| t.color.clone()).unwrap_or_default(),
            confirm_mutating: ty.is_some_and(|t| t.confirm_mutating),
        }),
        needs_confirmation: needs_confirmation(ty, target.request.method),
    })
}
