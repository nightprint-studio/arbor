# hirundo-model

What a Hirundo request *is*, and every rule that needs no network or disk. **No I/O**: everything is a function of
its inputs, so every rule is a test.

| Module | What it holds |
|---|---|
| `method` | HTTP methods; which of them change things (`is_mutating`) |
| `kv` | key/value rows (params, headers, form fields, variables) and their **bulk-edit text** (`key: value`, `// ` = off, ` # ` = description; `k=v&k2=v2` and browser header blocks read too) |
| `auth` | Bearer / Basic / API key; `effective` picks the innermost level that is not `Inherit`; `NoAuth` cuts inheritance |
| `env` | environment **types** (id, name, colour, `confirm_mutating`, a variable set), environments, `needs_confirmation`, the layer order |
| `vars` | layered resolution (collection → folders → type → environment → local → runtime), `@secret` lookups, `{{interpolation}}` |
| `request` | `HttpRequest`, `Body`, `Settings` |
| `prepare` | request + inherited auth/headers + variables → `Prepared`, exactly what goes on the wire |
| `curl` | `curl …` → request (bash quoting, `$'…'`, `-G`, `-u`, combined short flags), request → `curl …` |
| `format` | the one-file-per-request TOML, `collection.toml` / `folder.toml`, environments, types, `local.toml` |
| `tomlw` | the small writer that keeps bodies and header blocks multi-line |

Layering of variables (later wins): collection, its folders, the environment **type**, the **environment**, local
(this machine), runtime (a test script). A variable with value `@secret` has no value in any file; it is looked up
in the keyring under the layer that declares it, and a missing one does not fall back to an earlier plain value.

Not here yet: OAuth 2 / JWT refresh, multipart bodies, assertions and Lua tests, OpenAPI / Postman / Bruno import.
