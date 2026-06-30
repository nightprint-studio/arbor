# corvus-git-provider-api

The provider-agnostic **git-host contract** for Corvus.

This crate is the leaf the concrete provider impls
(`corvus-git-provider-github`, `corvus-git-provider-gitlab`) build on. It holds
only the *vocabulary* of the domain — no client logic, no credential store:

- **DTOs** — merge/pull requests, CI runs & jobs, releases, repo-native issues,
  webhooks, branch protection, security findings & summaries, and the shared
  request/filter/id types. Their serde shape is the contract with the frontend.
- **`GitProvider`** — the async, object-safe trait describing a remote host's
  REST surface. Held as `Arc<dyn GitProvider>`; stub methods return
  `ProviderError::Unsupported`.
- **`GitProviderRegistry`** — host-keyed (`github.com`, `gitlab.com`,
  self-hosted GitLab) `Arc<dyn GitProvider>` lookup.
- **`ProviderError`** — the unified error type. Impls `From<reqwest::Error>` /
  `From<serde_json::Error>` so impl crates can `?` HTTP/JSON errors into it
  (the orphan rule forces those conversions to live here).

Import through the prelude:

```rust
use corvus_git_provider_api::prelude::*;
```

## Layout

| Module | Holds |
|---|---|
| `kind` | `ProviderKind` (github/gitlab/gitea/bitbucket) |
| `capability` | `Capabilities` matrix |
| `auth` | `ProviderUser`, `OAuthHandle`, `ProviderAuth` |
| `repo` | `RemoteRepo`/`RemoteRepoInfo`, `RepoRef`, repo CRUD requests, tree/file payloads |
| `mr` | merge-request payloads + `MrId`/`MrFilter`/`MergeOpts`/… |
| `ci` | `CiRun`/`CiJob`/`CiWorkflow`/`CiProviderInfo` + filters |
| `release`, `issue`, `webhook`, `branch` | the remaining trait surfaces |
| `security` | severity ladder, findings, summary, filters |
| `provider` | the `GitProvider` trait |
| `registry` | `GitProviderRegistry` + `host_from_url` |
| `error` | `ProviderError` |

Provider-agnostic computation helpers (median age, host-side risk score, filter
application) and the GitLab/GitHub fetch logic do **not** live here — they belong
to the impl crates / shell, which depend on these types.
