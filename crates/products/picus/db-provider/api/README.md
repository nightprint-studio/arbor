# picus-db-api

The contract every database engine implements for **Picus**. The direct model is
[`corvus/git-provider/api`](../../../corvus/git-provider/api), which does the same
job for GitHub and GitLab.

```text
picus/db-provider/api        ← this crate: traits + wire types + descriptor
picus/db-provider/postgres   ← the first implementation
picus/db-provider/oracle     ← later, and additive
```

## What is in here

| Module | What it holds |
|---|---|
| `provider` | `DbProvider` (an engine) and `DbSession` (one live connection) |
| `descriptor` | `DbProviderDescriptor` — connection fields, capabilities, emission traits, labels: the per-engine UI as **data** |
| `capability` | `EngineCapabilities` — what the engine has, so the UI stops asking "is this Oracle?" |
| `schema` / `query` | the wire types, serialised camelCase to match `src/lib/types/picus/index.ts` field-for-field |
| `connection` | `ConnectionSpec` — safe to persist, safe to log: it never holds a secret |
| `secret` | `SecretResolver` + `Secret` (zeroed on drop, `Debug` prints `***`) |
| `registry` | engine → provider lookup, so adding one is registering rather than editing |

## What this crate refuses to know

No driver, no SQL, no keychain, no Tauri. A provider crate brings its own driver;
the password arrives through `SecretResolver`, which `picus-be` implements over the
shell's credential broker. That is what makes this crate testable without a
database — and what stops a driver from ever seeing the keychain.

## Two invariants

- **The engine is never ambient.** `EngineKind` travels as a parameter, attached to
  the connection or the folder being acted on. A backend-wide "current dialect"
  would break the product's reason to exist — see
  [`docs/picus-design.md`](../../../../../docs/picus-design.md) §1.
- **Read-only is enforced server-side.** `DbSession::execute` refuses a write on a
  read-only connection with `DbError::ReadOnly`, and the session is opened in a
  read-only transaction mode so the *server* is the one saying no. Hiding the
  button is not the mechanism; it is the courtesy on top of it.

## Public API

Reach this crate through the [`prelude`](src/prelude.rs):
`picus_db_api::prelude::*`.
