# hirundo-be

The headless **HTTP-client** backend process for Model D — Hirundo, the Bennu suite's Postman/Bruno. Serves
collections, environments (with types), request preparation, sending and cURL conversion over framed-stdio IPC.
Hosts no plugins; resolves everything from the active profile and the project path each call names. Secrets are
asked of the shell's keyring per request (`__hirundo_secret`), never held.

| Module | Methods |
|---|---|
| `selftest` | `be_ping` / `be_echo` |
| `tree` | `collections_tree`, `request_get`, `request_save`, `request_duplicate`, `node_delete`, `collection_create`, `folder_create`, `meta_get`, `meta_save` |
| `envs` | `env_state`, `env_types_save`, `environment_save`, `environment_delete`, `local_save` |
| `send` | `request_preview`, `request_send` (async, cancellable, protected environments ask first), `request_cancel` |
| `convert` | `curl_import`, `curl_export`, `kv_parse`, `kv_format` |

`context::build` is the one place a request meets its surroundings (collection and folders above it, the
environment and its type, local values, secrets); preview, copy-as-cURL and send all use the `Prepared` it
produces, so what the window shows is what is sent.
