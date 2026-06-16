# corvus-provider-descriptor

The shared, **domain-agnostic** provider-connection contract for Corvus.

One vocabulary spoken by *both* connection layers (issue trackers and git hosts)
and by the *single* generic frontend — so the UI carries **zero** per-provider
knowledge (no `linear`/`jira`/`github`/`gitlab` branches, no per-provider IPC).

- **`ProviderDescriptor`** — what the FE renders to connect a provider: `id`,
  `domain` (issue tracker vs git host), display name, icon, brand color, and the
  ordered `auth_methods` (`auth_methods[0]` = recommended).
- **`AuthMethod`** — either `OAuth { flow: Device | Redirect }` or
  `Fields { fields, hints }`.
- **`AuthField`** — `key`, `label`, `widget` (text/secret/url), `required`, and
  the declarative **`required_when`** rule (e.g. *email required only when the
  Jira domain ends with `.atlassian.net`*).
- **`FieldRule` / `FieldMatch` / `FieldHint`** — the small declarative rule model
  the FE interprets, so *all* conditional form behavior lives in the
  backend-authored descriptor, never in UI code.
- **`AuthStatus`** — `authenticated`, optional `user`, `account_label` (Jira
  tenant / self-hosted host), active `method`.
- **`OAuthStart`** — `Redirect { url }` or `Device { user_code, … }`, the return
  of a generic `*_provider_start_oauth`. Completion is signalled by the single
  Tauri event `arbor://provider-oauth-done` whose payload carries the provider
  `id` (so concurrent OAuth flows update the correct card).

```rust
use corvus_provider_descriptor::prelude::*;
```

The descriptor describes *how to connect*; the imperative connection actions
(`connect_fields` / `start_oauth` / `disconnect` / `auth_status`) live in the
shell-side connector that owns each descriptor. The data-layer crates
(`corvus-issue-tracker-*`, `corvus-git-provider-*`) stay focused on issues / MRs
and do not depend on this crate.
