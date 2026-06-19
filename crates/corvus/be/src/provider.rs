//! Shared git-provider registry for the OOP REST domains (repo-browser today;
//! avatar / security / MR / CI next).
//!
//! Built once from the reverse channel, this is the `issues` pattern one tier
//! up: where `issues` injects a [`ChildSessionProvider`] into a registry of
//! *trackers*, this injects one into a registry of *providers*. Each provider is
//! the keyring-free `corvus-git-provider-{github,gitlab}` impl; its `session` /
//! `refresh` marshal back over the reverse channel to the shell's
//! `VaultSessionProvider` (the sole keyring holder), so behaviour — including
//! the on-401 refresh-and-retry baked into each provider's HTTP layer — is
//! identical to in-process. The shell seeds the same two hosted providers at
//! startup with the in-process vault; here the only difference is the
//! `SessionProvider` impl.

use std::sync::{Arc, Mutex, OnceLock};

use arbor_ipc::prelude::{ChildSessionProvider, HostCaller, SessionProvider};
use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    CiProviderInfo, GitProvider, GitProviderRegistry, MrId, ProviderError, ProviderKind, RepoRef,
};
use corvus_git_provider_github::prelude::GithubProvider;
use corvus_git_provider_gitlab::prelude::GitlabProvider;

/// The registry plus the host caller it was built from. The registry is behind a
/// `Mutex` because tab-keyed resolution ([`provider_for_tab`]) auto-registers
/// self-hosted GitLab instances on demand, exactly like the shell's
/// `lookup_or_register`. The host caller is kept for that auto-registration's
/// `ChildSessionProvider` and for the proactive [`maybe_refresh`] host call.
struct Providers {
    registry: Mutex<GitProviderRegistry>,
    host:     Arc<dyn HostCaller>,
}

static PROVIDERS: OnceLock<Providers> = OnceLock::new();

/// The resolved `(provider, repo_ref, info)` for a tab — the OOP twin of the
/// shell's `git_provider::helpers::ResolvedProvider`. `repo` is shaped for the
/// provider's REST conventions (GitHub: owner+name; GitLab: full project path).
pub struct Resolved {
    pub provider: Arc<dyn GitProvider>,
    pub repo:     RepoRef,
    pub info:     CiProviderInfo,
}

/// A fresh reverse-channel `SessionProvider` bound to `host`. Account-agnostic:
/// it forwards whatever `account` the provider passes (`"github.com"` /
/// `"https://gitlab.com"`) to the shell, which routes it via
/// `VaultSessionProvider::for_account`.
fn child(host: &Arc<dyn HostCaller>) -> Arc<dyn SessionProvider> {
    Arc::new(ChildSessionProvider::new(Arc::clone(host))) as Arc<dyn SessionProvider>
}

/// Wire the provider registry to the reverse channel. Called once from `main`
/// after the `FrameHostCaller` is built; idempotent via `OnceLock::set`. Seeds
/// the two hosted providers, mirroring the shell's startup seeding but with a
/// reverse-channel session instead of the in-process vault.
pub fn init(host: Arc<dyn HostCaller>) {
    let mut registry = GitProviderRegistry::new();
    registry.register(Arc::new(GithubProvider::new(child(&host), "github.com")));
    registry.register(Arc::new(GitlabProvider::new(child(&host))));
    let _ = PROVIDERS.set(Providers { registry: Mutex::new(registry), host });
}

fn providers() -> &'static Providers {
    PROVIDERS.get().expect("provider::init must run before dispatch")
}

/// Resolve the hosted provider for a browser `provider` string
/// (`"github"` → `github.com`, `"gitlab"` → `gitlab.com`). The remote browser
/// targets the hosted instances only; self-hosted browsing is not exposed here.
/// Same error wire strings as the shell's in-process `repo_browser::provider_for`.
pub fn for_host(provider: &str) -> Result<Arc<dyn GitProvider>, String> {
    let host = match provider {
        "github" => "github.com",
        "gitlab" => "gitlab.com",
        other    => return Err(format!("Unknown provider: {other}")),
    };
    let reg = providers().registry.lock().map_err(|_| "provider registry poisoned".to_string())?;
    reg.for_host(host)
        .ok_or_else(|| format!("No provider registered for {host}"))
}

/// The registered hosted providers tagged by their browser string — the OOP
/// twin of the shell's `rb_list_accounts` provider enumeration.
pub fn hosted() -> Vec<(String, Arc<dyn GitProvider>)> {
    let Ok(reg) = providers().registry.lock() else {
        return Vec::new();
    };
    [("github", "github.com"), ("gitlab", "gitlab.com")]
        .iter()
        .filter_map(|(p, h)| reg.for_host(h).map(|prov| (p.to_string(), prov)))
        .collect()
}

/// Resolve `(provider, repo_ref, info)` for the repo the shell registered for
/// `tab_id` — the OOP twin of the shell's `provider_for_tab`. Opens the repo by
/// the pushed path, lists its remotes, detects the provider (pure URL parsing),
/// then looks it up (auto-registering a self-hosted GitLab instance on demand).
/// Error wire strings match the shell's `provider_for_remotes` /
/// `lookup_or_register`.
pub fn provider_for_tab(state: &CorvusState, tab_id: &str) -> Result<Resolved, String> {
    let repo = crate::repo::open(state, tab_id)?;
    let remotes: Vec<(String, String)> = corvus_git::remote::list_remotes(&repo)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.name, r.url))
        .collect();

    let mut info = CiProviderInfo::detect_from_remotes(&remotes)
        .ok_or_else(|| "No GitHub or GitLab remote detected for this repository".to_string())?;

    let provider = lookup_or_register(&info)?;
    // Fill the keyring-coupled `has_token` from the resolved provider (which
    // probes through the reverse channel), keeping it out of the pure detector.
    info.has_token = provider.has_token();

    let repo = match info.provider.as_str() {
        "github" => RepoRef::github(
            info.owner.clone().unwrap_or_default(),
            info.repo_name.clone().unwrap_or_default(),
        ),
        "gitlab" => RepoRef::gitlab(info.project_path.clone().unwrap_or_default()),
        other => return Err(format!("Unknown provider: {other}")),
    };

    Ok(Resolved { provider, repo, info })
}

/// Look up the provider for `info.remote_url`, auto-registering a self-hosted
/// GitLab instance on demand (so callers never see "no provider for host X").
/// Mirrors the shell's `lookup_or_register`.
fn lookup_or_register(info: &CiProviderInfo) -> Result<Arc<dyn GitProvider>, String> {
    let p = providers();
    {
        let reg = p.registry.lock().map_err(|_| "provider registry poisoned".to_string())?;
        if let Some(prov) = reg.for_remote_url(&info.remote_url) {
            return Ok(prov);
        }
    }
    if info.provider == "gitlab" {
        if let Some(base_url) = info.gitlab_base_url.as_deref() {
            let mut reg = p.registry.lock().map_err(|_| "provider registry poisoned".to_string())?;
            // Re-check under the write lock — another thread may have raced ahead.
            if let Some(prov) = reg.for_remote_url(&info.remote_url) {
                return Ok(prov);
            }
            let provider: Arc<dyn GitProvider> =
                Arc::new(GitlabProvider::new_self_hosted(child(&p.host), base_url));
            reg.register(Arc::clone(&provider));
            return Ok(provider);
        }
    }
    Err(format!("No GitProvider registered for remote {}", info.remote_url))
}

/// Build an `MrId` from a [`Resolved`] + numeric MR/PR id — the OOP twin of the
/// shell's `git_provider::helpers::mr_id_from`. Branches on the detected
/// provider so GitHub gets `(owner, repo_name)` and GitLab gets `(path, None)`.
pub fn mr_id_from(resolved: &Resolved, number: u64) -> MrId {
    match resolved.info.provider.as_str() {
        "github" => MrId {
            provider:      ProviderKind::GitHub,
            owner_or_path: resolved.info.owner.clone().unwrap_or_default(),
            repo_name:     Some(resolved.info.repo_name.clone().unwrap_or_default()),
            number,
        },
        _ => MrId {
            provider:      ProviderKind::GitLab,
            owner_or_path: resolved.info.project_path.clone().unwrap_or_default(),
            repo_name:     None,
            number,
        },
    }
}

/// Proactive provider-keyed token refresh over the reverse channel — the OOP
/// twin of the shell's `maybe_refresh_for_provider` pre-call. Best-effort: a
/// failure is swallowed (the subsequent REST surfaces 401 if the token really is
/// dead, and the provider's HTTP layer refreshes reactively then), exactly as
/// in-process. `provider` is the browser string (`"github"` | `"gitlab"`).
pub fn maybe_refresh(provider: &str) {
    let _ = providers().host.call("__maybe_refresh", serde_json::json!(provider));
}

/// Map a [`ProviderError`] to the EXACT wire string the shell's in-process path
/// produces. The shell wraps it as `AppError::Other(e.to_string())`; on the OOP
/// path the `SplitBroker` re-wraps our returned `String` as `AppError::Other`
/// too, so returning `e.to_string()` yields a byte-identical message to the FE.
pub fn pe(e: ProviderError) -> String {
    e.to_string()
}
