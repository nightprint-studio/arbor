# Credential Architecture (Model D)

How a headless backend obtains an authenticated session without ever touching
the OS keyring, and how that contract survives the in-process → out-of-process
(OOP) split. This is the design that gates the OOP extraction of the
credential-coupled domains — MR/PR, security, issue trackers, the git provider
layer, and CI.

## Problem

The credential-coupled backends (issue trackers, git providers, CI, security,
MR/PR) need a `base_url` + an `Authorization` header to make HTTP calls. Today
those backends run **in-process** inside `src-tauri` and reach a keyring read +
OAuth refresh directly through shell-side code. Tomorrow they extract into
`corvus-*` crates and (eventually) a separate `corvus-be` process.

The keyring is an OS-level secret store. The moment a backend lives in its own
process, it **must not** carry the keyring, the OAuth client secrets, or the
long-lived refresh tokens — those stay with the shell (the launcher), the single
trusted custodian. The backend can only ever be handed a **short-lived access
token**, scoped to the request it is about to make, and a way to ask for a fresh
one when the provider rejects the old one.

So the architecture has to answer two questions at once:

1. **What contract** does a backend depend on, such that the *same* code works
   in-process (direct keyring read) and out-of-process (round-trip to the shell)?
2. **What channel** carries that round-trip once the backend is OOP — given that
   today's IPC transport is one-directional for backend→shell?

## Current state (in-process)

The contract already exists: **`SessionProvider`**
(`crates/foundation/ipc/src/credential.rs`).

```rust
#[async_trait]
pub trait SessionProvider: Send + Sync {
    async fn session(&self, account: &str) -> Result<AuthSession>;
    async fn refresh(&self, account: &str) -> Result<AuthSession>;
    fn has_credentials(&self, account: &str) -> bool { true }
}

pub struct AuthSession {
    pub base_url:    String,         // per-tenant or fixed
    pub auth_header: String,         // full "Bearer …" / "Basic …" value
    pub web_base:    Option<String>, // user-facing web host for link building
}
```

A backend crate holds an `Arc<dyn SessionProvider>` and asks for a session;
on a `401`/`403` it asks for a `refresh()` and retries once. It never sees a
keyring, an OAuth flow, or a `client_id`. `account` is an **opaque path** the
backend passes through (e.g. `"linear"`, or — for GitLab — the instance host
root `"https://gitlab.example.org"`); the shell-side impl maps it to the real
keyring entry.

`AuthSession` carries a *session*, not a bare token, because providers diverge:
Linear is a fixed endpoint + `Bearer`; Jira is a per-tenant base URL + either
`Bearer` (OAuth) or `Basic` (API token); self-hosted GitLab/GitHub bring their
own base URL. `{ base_url, auth_header, web_base }` covers them all with one
abstraction, and keeps the trait free of `keyring`/HTTP types so the coupled
domains can be extracted into `corvus-*` crates.

Today the trait is implemented by **shell-side adapters** that read the keyring
directly and work synchronously inside the process:

- `src-tauri/src/git_provider/session.rs` — `GithubSessionProvider`,
  `GitlabSessionProvider`.
- `src-tauri/src/integrations/token_source.rs` — `LinearSessionProvider`,
  `JiraSessionProvider`.

**The in-process migration runs entirely on these adapters — no new channel is
needed.** A handler reached in-process holds an `Arc<dyn SessionProvider>` whose
backing reads the keyring locally; `session()`/`refresh()` resolve synchronously
without leaving the process.

### The adapters are thin — and that matters

Read `GithubSessionProvider::read()`: it is a keyring lookup, a `Bearer {token}`
format, and a fixed `api.github.com` base. `refresh()` delegates to
`oauth::github_flow::try_refresh()`, which is itself a thin wrapper over the
**already-generic** `arbor_auth::oauth2::refresh_token(token_url, client_id,
…, body_format)`. The same shape repeats for GitLab, Linear, Jira. What differs
between the four adapters is almost entirely **data**:

| Aspect            | GitHub            | GitLab                       | Linear            | Jira                         |
|-------------------|-------------------|------------------------------|-------------------|------------------------------|
| keyring account   | fixed             | the instance host root       | fixed             | fixed                        |
| header scheme     | `Bearer`          | `Bearer`                     | `Bearer`          | `Bearer` or `Basic`          |
| base_url          | fixed             | from account                 | fixed             | per-tenant (from store)      |
| refresh           | OAuth (`Form`)    | OAuth (gitlab.com only)      | OAuth             | OAuth; none for API token    |

The provider-specific *code* is a thin shell over generic machinery. The
provider-specific *content* is a small bundle of values.

## The reverse-channel requirement

`crates/foundation/ipc/src/transport.rs` makes the transport **asymmetric**:

- **shell → backend** is request/response: the shell writes a `Frame::Request`
  and blocks on the matching `Frame::Response` (`BrokerClient::call`, demuxed by
  id on a reader thread).
- **backend → shell** is one-way only: `Event::Notify { topic, payload }`
  pushed through the `EventSink` (`crates/foundation/ipc/src/event.rs`),
  re-emitted to the FE. There is **no** backend-initiated request/response.

A `SessionProvider` impl that lives shell-side but is *invoked from an OOP
backend* needs exactly the missing direction: the backend, mid-request, must
call **back** into the shell ("give me a session for `linear`", "refresh it")
and **block on a reply**. That is a backend→shell **request/response** channel —
full-duplex and **reentrant** (the shell may be in the middle of handling a
shell→backend request when the backend calls back).

This channel **does not exist yet, and is not needed in-process** — in-process a
handler has `&AppState` and reaches the keyring synchronously. It is needed
**only at the OOP split**, and it has exactly **two consumers**:

1. **Credential resolution — the priority.** It gates the OOP split of MR/PR,
   security, issues, the git provider layer, and CI. Without it, none of those
   backends can run OOP, because none of them can obtain a session.
2. **`arbor.ui.*` plugin UI round-trips.** A plugin running behind the seam that
   pops a form / settings panel and waits for the user's submission is the same
   shape: backend asks the shell, blocks on a reply.

Both are reentrant RPC in the same missing direction; the channel is built once
and serves both. **Credential resolution is the driving consumer** — it is what
unblocks the bulk of the OOP roadmap.

## Decision: launcher = vault + semaphore + generic OAuth engine

The launcher (shell) is the credential authority. It is **not** a host of
provider-specific code. It is three generic things:

- **Vault** — the sole keyring custodian. It reads/writes secrets; no backend
  ever sees a keyring entry.
- **Semaphore** — the single, centralized rotation lock. GitHub and GitLab
  "expiring user tokens" use **single-use** refresh tokens: two concurrent
  refreshes with the same refresh token race, only the first wins, the rest see
  "invalid refresh token" and would surface as spurious 401s. The existing
  `REFRESH_LOCK` in `git_provider/oauth/github_flow.rs` already serializes +
  coalesces this (first caller refreshes; the rest re-read the freshly-stored
  token and short-circuit). That lock is the semaphore — centralized launcher-side.
- **Generic OAuth engine** — `arbor_auth::oauth2::refresh_token(token_url,
  client_id, …, body_format)` is already provider-agnostic. The launcher drives
  it from descriptor data; it holds no per-provider refresh code.

**Refresh stays launcher-side, always.** This is the security crux: the
long-lived **refresh token never crosses the process boundary**. Only short-lived
**access tokens** flow to the backend, inside an `AuthSession`. The backend asks
"refresh"; the launcher runs the (single-use, lock-guarded) rotation against its
own vault and hands back a fresh access token. The refresh secret and the OAuth
`client_id`/`client_secret` stay home.

Since the adapter *code* is a thin shell over generic machinery (vault +
semaphore + OAuth engine), and the differences are **data**, the launcher needs
**no provider-specific code at all** — it needs the data.

## Descriptors as data + the per-BE `__credential_descriptors` IPC

The provider-specific content becomes one declarative value per provider, in the
shared crate `corvus-provider-descriptor`
(`crates/corvus/provider-descriptor/`):

```rust
struct ProviderCredentialDescriptor {
    keyring_account: KeyringAccount,   // fixed key, or "the account string is the key"
    header_scheme:   HeaderScheme,     // Bearer | Basic
    base_url:        BaseUrlRule,      // Fixed(url) | FromAccount
    refresh:         RefreshConfig,    // OAuth { token_url, client_id, body_format } | Static
}
```

- `header_scheme` — `Bearer` (GitHub/GitLab/Linear, Jira-OAuth) or `Basic`
  (Jira API-token).
- `base_url` — `Fixed(url)` (GitHub `api.github.com`, Linear `LINEAR_GQL`) or
  `FromAccount` (GitLab instance root, Jira per-tenant).
- `refresh` — `OAuth { token_url, client_id, body_format }` (drives the generic
  `refresh_token` engine) or `Static` (a PAT / API token — nothing to refresh;
  the original 401 propagates as the usual auth error).

A new provider is **one descriptor**. Zero provider code in the launcher.

### Each backend owns its descriptors; the launcher collects them

A backend is the source of truth for *its* providers' credential shape. Each
backend exposes an ad-hoc IPC method — **`__credential_descriptors`** — returning
its list of `ProviderCredentialDescriptor`s. The launcher calls it at backend
**registration** and aggregates the results into one generic, descriptor-driven
`VaultSessionProvider`.

The collection path uses the channel that **already exists** (shell → backend
request/response). No reverse channel is needed to *collect* descriptors —
registration is a normal shell-initiated call. The reverse channel is needed
**only for runtime resolution** (`session`/`refresh`) once the backend is OOP.

```
COLLECTION  (at BE registration — existing shell → backend channel)
┌─────────┐   __credential_descriptors (Request)    ┌──────────────┐
│ Launcher│ ─────────────────────────────────────▶  │  Backend     │
│ (vault) │ ◀─────────────────────────────────────  │  (corvus-be) │
└─────────┘   Vec<ProviderCredentialDescriptor>      └──────────────┘
      │  builds ONE generic VaultSessionProvider from all descriptors


RESOLUTION  (per request, at runtime — needs the REVERSE channel, OOP only)
┌─────────┐                                          ┌──────────────┐
│ Backend │   session("linear") / refresh("linear")  │  Launcher    │
│         │ ─────────────────────────────────────▶   │  vault +     │
│ (Child  │ ◀─────────────────────────────────────   │  semaphore + │
│  Session│      AuthSession { base_url, header, … }  │  OAuth engine│
│ Provider)                                           └──────────────┘
└─────────┘  (only short-lived access tokens cross — never the refresh token)
```

The two arrows ride two different channels: **collection** on the existing
forward channel at registration time; **resolution** on the new reverse channel,
per request, only when the backend is OOP.

## `VaultSessionProvider` + `ChildSessionProvider`

Two `SessionProvider` implementations, picked by where the backend runs:

- **`VaultSessionProvider`** (launcher-side) — the one generic, descriptor-driven
  impl. Given `account`, it looks up the descriptor, reads the keyring per
  `keyring_account`, formats the header per `header_scheme`, resolves `base_url`,
  and — on `refresh()` — runs the semaphore-guarded generic OAuth engine per the
  descriptor's `refresh` config (or returns "nothing to refresh" for `Static`).
  It **replaces all four hand-written adapters**. It is what an in-process backend
  holds directly, and what the launcher exposes over the reverse channel.

- **`ChildSessionProvider`** (backend-side, OOP only) — a thin `SessionProvider`
  whose `session()`/`refresh()` marshal `account` over the **reverse channel** to
  the launcher's `VaultSessionProvider` and await the `AuthSession` reply. The
  backend holds an `Arc<dyn SessionProvider>` and **cannot tell which impl it
  is** — the call site never changes.

This is the payoff of the trait being `async` and keyring-free: the *same*
backend code, holding the *same* `Arc<dyn SessionProvider>`, runs in-process
(backed by `VaultSessionProvider`, resolving locally) or OOP (backed by
`ChildSessionProvider`, resolving over the reverse channel). Only the wiring
differs.

## Refactor sequencing

The work decomposes into three increments, ordered so each lands independently
and the channel comes last (it is the hardest piece, and the first two are
useful on their own, in-process):

1. **Collapse 4 adapters → 1 + the descriptor type.** Add
   `ProviderCredentialDescriptor` (and `KeyringAccount` / `HeaderScheme` /
   `BaseUrlRule` / `RefreshConfig`) to `corvus-provider-descriptor`. Write the
   single generic `VaultSessionProvider` driven by a descriptor, and replace
   `Github`/`Gitlab`/`Linear`/`Jira` `SessionProvider` impls with four
   descriptor values. Pure in-process refactor — no behavior change, no channel.
   This proves the descriptor model captures every provider's real shape.

2. **Per-BE `__credential_descriptors` IPC.** Each backend exposes the method;
   the launcher collects descriptors at registration and assembles its
   `VaultSessionProvider` from them, over the existing forward channel. Still
   in-process — the launcher just stops hard-coding the descriptor set and reads
   it from the backends instead.

3. **The reverse channel, with `SessionProvider` as its first consumer.** Build
   the backend→shell reentrant request/response channel (the missing transport
   direction). Wire `ChildSessionProvider` as its first client so an OOP backend
   resolves credentials over it. `arbor.ui.*` round-trips follow as the second
   consumer on the same channel. This is the increment that actually unblocks the
   OOP split of MR/PR, security, issues, the git provider layer, and CI.
