# Hirundo — the HTTP client of the Bennu suite

Spec: the approved mock `docs/mockups/hirundo-http-client.html`. This file is what is built, how, and what is not yet.

## Shape

Model D product, same as Picus: `hirundo-model` (pure) → `hirundo-core` (state + `.hirundo/` store) → `hirundo-be`
(headless process). Shell wiring: router program `hirundo` (`pure_oop`), `ensure_hirundo_be`, the `__hirundo_secret`
reverse-channel door, `hirundo_store_secret` / `hirundo_delete_secret` / `hirundo_has_secret` commands, `hirundo`
in `ensure_product_backend` (another window may borrow it) and in the profile-switch recycle list, `hirundo-be` in
`backends:dev|release` and `stage-backends.mjs`, `window/hirundo.rs` + `capabilities/hirundo.json`.

## On disk

```
<project>/.hirundo/
  collections/<collection>/collection.toml    name, auth, shared headers, variables
                          /<request>.toml     one readable file per request
                          /<folder>/folder.toml, …
  environments/<id>.toml
  env-types.toml                              absent → local / dev / staging / prod
  local.toml (+ .gitignore)                   active environment, this machine's variables — not committed
```

Params, headers, form fields and variables are the **bulk-edit text** (`key: value`, `// ` off, ` # ` description)
inside a multi-line string, so a diff reads like the change. Bodies are `'''` literal blocks. A file that does not
parse is still listed, with its error.

## Environments have types

`EnvType { id, name, color, confirm_mutating, vars }` — `local`, `dev`, `staging`, `prod` ship (prod asks before
POST/PUT/PATCH/DELETE); the user edits, removes and adds (`qa`, `customer-acme`…). `Environment { id, name,
type_id, vars }` is an instance. Variables resolve in layers, later wins: collection → folders → **type** →
**environment** → local → runtime. An environment whose type is gone has no colour and no protection (not an error).
Protection is enforced in the backend: `request_send` answers `needs_confirmation` until the caller says
`confirmed`.

## Secrets

A variable whose value is `@secret` has no value in any file. Its value is in the keyring under
`hirundo/<project>|<layer>:<id>|<name>`; the window stores it with `hirundo_store_secret` (never through the
backend), the backend asks for it per request over the reverse channel. A secret that is missing is *missing* —
an earlier plain value does not stand in for it — and the request is not sent.

## Sending

`context::build` folds request + collection/folders (auth, headers, variables) + environment + type + local +
secrets into `hirundo-model`'s `Prepared`. Preview, "copy as cURL" and the sender share that one value. A request
with an unresolved `{{variable}}` is not sent. The sender (reqwest, already in the workspace) is async and
cancellable (`request_cancel`), caps the body at 8 MiB, keeps binary bodies as base64.

## Done (phase 1 — backend)

Data model and file format; auth inheritance (Bearer / Basic / API key); bulk-edit text; environments, types,
protection; variables and `{{interpolation}}`; request preparation; cURL import/export (Chrome's "Copy as cURL"
bash flavour); the store; handlers for tree, environments, preview/send/cancel, conversions; shell wiring;
FE types and IPC (`src/lib/types/hirundo`, `src/lib/ipc/hirundo`). Unit tests in all three crates.

## Done (phase 2 — the window)

FE under `src/lib/components/hirundo/`, `stores/hirundo/`, `ipc/hirundo/`, `types/hirundo/`: activity rail and sidebar
(collections tree with fuzzy filter and hover trash, environments, history, runner), request tabs with dirty dot and
preview tabs, request bar tinted by the environment's type, icon toolbar, Params / Headers / Auth / Body / Tests / Docs,
bulk text edit, response viewer (Pretty / Raw / Headers / Tests), environments manager (environments, types, local values,
secrets), collection/folder settings, cURL, generate code, resolved-variables preview, response diff, command palette,
shortcuts (`hirundo-shortcuts.ts`), docs, gallery scenes and fixtures. The window's own preference (last and recent
projects) is in the profile: `hirundo/config.toml`, served by `get_hirundo_config` / `hirundo_project_opened`.

## Done (phase 3 — partly)

* Declarative assertions (`hirundo-model::assertion`) and sandboxed Lua (`hirundo-be::tests_run`, mlua from the
  workspace; only string / table / math / utf8, an instruction budget, `hirundo.response`, `expect`, `env.set`, `log`).
* Runner (`stores/hirundo/runner.svelte.ts`): sequential, chained through `env.set`, one confirmation up front for a
  protected environment.

## Done (phase 3 — second part)

* **Layout**: sidebar (collections in the shared `Tree` with a context menu and F2 / Del, detected endpoints, environments), request
  area with the environment selector in its header, and a resizable **bottom panel** (response, runner, history, schedules).
* **Workspaces** like Bennu's and Corvus's: `config.toml` holds workspaces (name, colour, projects) and the active one; the title bar has the switcher.
* **Detected from code**: `bennu-be` serves `endpoints_detected(root)` (the Spring model the HTTP-client generator already reads); Hirundo asks
  only for the Bennu projects chosen in the panel (`detected_roots`), default the project it is on. An endpoint opens as an unsaved request.
* **Schedules** (`hirundo-be/schedules.rs`): data in the profile (`state/<project-hash>/schedules.json`), kept as `arbor-scheduler::jobs` jobs (recurrence / next run / missed-run policy / last result) whose action is "run this collection in this environment"; a file in the earlier format (a bare array) is still read;
  the window ticks every 30 s (`schedules_due`), runs with the runner it has and reports (`schedule_ran`). Nothing fires with the window closed.
* **Runner history** (`runs.rs`): the last 50 runs per project, in the profile.
* Toolbar: Go to the controller (only for a request that matches a detected endpoint). Save / Save as live in the title bar, bulk edit in each table, Copy Path in the tab and tree menus. Ask Imber was removed (it only opened Bennu with a prompt on the clipboard).

## Done (phase 4 — protocols and import)

* **SOAP**: `BodyKind::Soap` + `SoapSettings { version, action }`, protocol headers, WS-Security `UsernameToken` (an auth type applied to the envelope), fault
  extraction and XML indenting (`hirundo-model::soap`); WSDL / envelope import reusing Bennu's `bennu_generate::soap` reader (`hirundo-be/soap.rs`).
* **OpenAPI 3 (JSON)**: `hirundo-model::openapi` (pure: refs, examples, auth mapping, tags) + `openapi_read` / `openapi_apply` and the choice dialog. YAML is refused in words.
* **.http / .rest**: `hirundo-model::httpfile` (parse, write, dynamic variables, `http-client.env.json`) + import / export handlers. Dynamic variables are generated
  per send as the last layer of the cascade.
* **Multipart**: `BodyKind::Multipart` on the form rows (`@path` = file), framed by the sender; cURL `-F` both ways; `.http` export in IntelliJ's shape.
* **Cookie jar**: `hirundo-model::cookies` (RFC 6265 matching and expiry), one jar per environment in the profile, sent and kept by `request_send`, a panel to read and delete.
* **SSE**: `hirundo-model::sse` (incremental parser) + `sse_start` / `sse_stop` (a task on the runtime, events as `hirundo:sse`, reconnection with `Last-Event-ID`) + the stream panel.
* **Bennu endpoints**: `bennu-be` serves `endpoints_detected`; the panel reads only the projects chosen.

## Done (phase 5 — Bennu runs .http files)

`hirundo-be/httprun.rs` (`http_outline`, `http_envs`, `http_check`, `http_run`) is the engine Bennu borrows (`ensureBackend('hirundo')`). Bennu side: `components/bennu/http/`
(stream-parser language, completion, panel, controller calls) and `stores/bennu/http.svelte.ts`. The `hirundo:sse` event reaches every window: the shell re-emits each backend topic globally.

## Not yet

* Detected endpoints are Spring's only (Struts and JAX-RS rows are in other Bennu catalogs); a project Bennu has not indexed lists nothing.
* Schedules that fire with the window closed (they would need a resident process), and runs as a Corvus pipeline step (pipeline steps are Lua plugin
  work, not a built-in kind).
* Imber inside the Hirundo window (today: a prompt handed to Bennu), and opening the controller at its line in Bennu (today: the OS default editor).
* Bennu's `.http` files: done are the language (colouring, completion), the ▶ and Ctrl+Enter, the HTTP panel, environments, diagnostics, a call arrow on Spring controller methods (run / generate). Not done: "Open in Hirundo" from a controller (no way to hand a request to another window yet), a `.http` entry in the New File dialog (the palette has "HTTP: New .http file"), environments of the Hirundo project (only `http-client.env.json` is read), request history persisted across sessions, multipart bodies built from `< file` parts.
* Importing a multipart body from a `.http` file; code generation for multipart / SOAP in the languages of the generator (cURL carries them); cookie jar toggle per collection; a file picker for multipart in the cURL dialog.
* OAuth 2 and JWT refresh, multipart, cookie jar, client certificates / proxy, OpenAPI / Postman / Bruno / Insomnia /
  HAR import, WebSocket / SSE / GraphQL / gRPC.
