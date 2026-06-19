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

use std::sync::{Arc, OnceLock};

use arbor_ipc::prelude::{ChildSessionProvider, HostCaller, SessionProvider};
use corvus_git_provider_api::prelude::{GitProvider, GitProviderRegistry, ProviderError};
use corvus_git_provider_github::prelude::GithubProvider;
use corvus_git_provider_gitlab::prelude::GitlabProvider;

/// The registry plus the host caller it was built from — the caller is kept for
/// the proactive [`maybe_refresh`] host call (the OOP twin of the shell's
/// `maybe_refresh_for_provider`).
struct Providers {
    registry: GitProviderRegistry,
    host:     Arc<dyn HostCaller>,
}

static PROVIDERS: OnceLock<Providers> = OnceLock::new();

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
    let _ = PROVIDERS.set(Providers { registry, host });
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
    providers()
        .registry
        .for_host(host)
        .ok_or_else(|| format!("No provider registered for {host}"))
}

/// The registered hosted providers tagged by their browser string — the OOP
/// twin of the shell's `rb_list_accounts` provider enumeration.
pub fn hosted() -> Vec<(String, Arc<dyn GitProvider>)> {
    let reg = &providers().registry;
    [("github", "github.com"), ("gitlab", "gitlab.com")]
        .iter()
        .filter_map(|(p, h)| reg.for_host(h).map(|prov| (p.to_string(), prov)))
        .collect()
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
