# corvus-issues

Shared **issue-tracker domain** for Corvus (Linear + Jira). It builds the
`SessionProvider`-injected `IssueTracker` registry that both sides of the Model-D
split link:

- the **shell** injects a keyring-backed `VaultSessionProvider` (in-process);
- **`corvus-be`** injects a `ChildSessionProvider` that resolves credentials over
  the reverse channel (out-of-process).

The crate is **keyring-free** — credentials are *injected*, never read here — so
it carries no Tauri / `keyring` / `tokio` dependency and can be linked by the
headless backend.

## API (via the [`prelude`])

| Item | Role |
|------|------|
| `build_registry(session_for)` | builds `(IssueTrackerRegistry, Arc<JiraTracker>)` from a `Fn(&str) -> Arc<dyn SessionProvider>` factory. The concrete `JiraTracker` is returned alongside because the Jira shim needs its inherent methods (`download_attachment`, `current_user`) that aren't on the trait. |
| `linear_new_issue(...)` / `jira_new_issue(...)` | assemble a `NewIssue` — the single source the shell `create_issue_req` shims and the `corvus-be` create handlers both call, so the two never drift. |
| `JiraAuthStatus` | the Jira auth-status DTO. |
| `corvus_issue_tracker_api::prelude::*` (re-exported) | the whole contract — DTOs (`Issue`, `IssueComment`, `IssueFilters`, …), the `IssueTracker` trait, `NewIssue`, `IssueTrackerRegistry` / `IssueTrackerError`, `branch_name_for_issue`. |
| `JiraTracker`, `validate_token`, `LINEAR_GQL` | the concrete Jira handle + Linear's token-validation free fn / endpoint, re-used by the shell connect path. |

The `session_for` id literals (`"linear"`, `"jira"`) are the **load-bearing
routing keys**: each tracker stores its `account` and calls `session(account)`
with it; the shell's `VaultSessionProvider::for_account` maps the same literals.
Keep them identical — they are the wire contract between the two providers.

## Depends on

`corvus-issue-tracker-{api,linear,jira}` (the trait + the two impls),
`arbor-ipc` (`SessionProvider`), `serde`. Keyring-free, Tauri-free.
