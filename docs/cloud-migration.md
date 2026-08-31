# Moving the cloud out of Arbor

**Goal:** the cloud panel is a plugin and its providers are extensions, with nothing about
object storage left in Arbor. Not "the cloud, better organised" — the cloud, *gone*, and Arbor
left holding only capabilities that are true of any plugin.

## Why this is not where it looked like it was

The `cloud-gcs` WASI package implements `list / stat / read / write / delete / copy / test`.
That is a small slice of what the cloud actually does, and today it is not even reached: it is
routed to by [`src-tauri/src/cloud_guest.rs`](../src-tauri/src/cloud_guest.rs), whose every
function returns `None` — "not ours, fall through" — unless the connection is GCS *with a
bearer-token credential*, the package resolves through the extension index, and its credential
slot is filled. Otherwise the built-in path runs, which is why the panel works with the
provider absent or broken.

Everything else has always been Rust inside Arbor:

| Where | What | Lines |
|---|---|---|
| `crates/wasm/cloud` (`arbor-cloud`) | opendal operators, ops, **transfers** (download / upload / sync / download_many / concat), GCS auth, Google OAuth, secrets | 3103 |
| `src-tauri/src/cloud/mod.rs`, `cloud_guest.rs` | job-registry bridge, cancellation maps, the guest route | 581 |
| `src-tauri/src/ipc/mod.rs` | 22 `__cloud_*` reverse-channel handlers | ~600 |
| `crates/platform/plugin/ns/src/cloud/` | the `arbor.cloud` Lua namespace | 533 |
| `src/lib/components/shared/CloudChunkOrderModal.svelte` | the chunk-order picker | ~400 |

## The three things in the way

1. **`cloud-provider.wit` is built on a `resource`.** The dynamic call path refuses resources
   (see the module doc of [`dynamic.rs`](../crates/platform/plugin/wasm/src/dynamic.rs)), so
   `arbor.ext.call` cannot reach the provider at all — the only caller that can is the typed
   path, which is the Arbor-side cloud code being removed. The interface has to become
   whole-call: `bucket` + `config` on every function, with the guest caching its own auth.
2. **Bytes.** JSON gives a `list<u8>` one number per byte. A 200 MB object is ~600 MB of text,
   in every process it crosses.
3. **What the plugin did not have.** Progress cards (`arbor.job` was a Corvus namespace), an
   OAuth engine (`oauth_google.rs` is Google-specific code *inside Arbor*), and a chunk-order
   dialog (a Svelte component in the shell).

## Phases

### Phase 0 — capabilities (done)

The only part that *stays* in Arbor, and none of it mentions the cloud.

- **`arbor.job` is platform, not Corvus.** Moved with `arbor.cloud` into the new
  `arbor-plugin-ns` crate; both backends with a plugin host (corvus-be, bennu-be) install them.
  A plugin can report progress wherever it is hosted.
- **`arbor.ext.call_to_file` / `call_from_file`.** Call an extension with the payload going
  straight between the guest and a local file — bytes never become JSON and never pass through
  Lua. `file_arg` (1-based) names which argument the file's bytes are lowered into; `offset` /
  `length` / `append` make chunked transfers possible without either side holding the object.
  The path goes through the caller's own `fs` permission and scope, checked in the backend that
  holds the plugin's manifest, and reaches the shell already absolute.
- **`arbor.oauth.start` / `refresh`.** The engine, never the provider: endpoints, client,
  scopes and the provider's dialect (`access_type=offline`, `prompt=consent`, …) are all data
  the plugin passes. Arbor contributes the two halves a package cannot hold — the loopback
  listener and the keychain. Tokens land in the plugin's own credential slot as a documented
  JSON document, which is also what a provider extension reads through `arbor:host/secrets`.
- **`reorder_list` form node.** A list whose order is the answer, keyboard-first (Alt+↑/↓ move
  the row). What the chunk-order picker becomes when it stops being a Svelte component in the
  shell.
- **Hook fan-out fixed.** `fire_plugin_hook_on_backends` sent every shell-raised plugin hook to
  Corvus only. A universal plugin is loaded once per product, so a plugin hosted by Bennu never
  saw its own callbacks — the panel would hang on "Loading…". It now goes to every backend that
  hosts plugins.

### Phase 1 — whole-call WIT (done)

`wit/cloud-provider.wit` has no `resource`: every function carries `at: target`. That is what
makes the provider reachable from Lua at all. The typed path went with it — `CloudGuest`,
`open_cloud` and `src-tauri/src/cloud_guest.rs` are gone, because a typed wrapper in Arbor
means Arbor carrying the interface.

### Phase 2 — the provider carries its own weight (done for GCS)

Three routes, and the package owns all three:

- **OAuth** — the package runs its own sign-in through `arbor.oauth`, holding the slot it
  writes to. The panel asks `cloud-gcs.connect` by name and never learns what Google is.
- **ADC** — the file `gcloud auth application-default login` writes. A user grant is adopted
  into the same slot and refreshed by the same engine; a service-account one takes the route
  below. Found through `arbor.fs.user_dirs`, so no environment variable is ever read.
- **Service-account key** — signed inside the component (`cloud-common::jwt`, RustCrypto), the
  assertion exchanged for a token, the token stored in the slot the component reads. The key
  lives in the package's own second slot; the panel that took it from the user keeps no copy.

`gcloud auth print-access-token` was deliberately not used: it would give a storage provider
the ability to run processes, for a token that expires in an hour with nothing to renew it.

S3 / Azure when they are wanted — they sign each request rather than carrying a bearer token,
which is work inside the guest and none in Arbor.

### Phase 3 — orchestration in Lua (done)

`cloud-storage` contains no `arbor.cloud` call at all. Listing is a paginated walk instead of
a host-driven stream; transfers are ranged reads appended to a file; the chunk merge IS the
download, so the temp directory and the second pass over the bytes are gone (and
`chunk-merger-bin`, whose whole job that was, is retired). Progress is `arbor.ui.operation`,
cancellation is a flag the loop checks, ordering is a `reorder_list` form node.

### Phase 4 — demolition

Delete `arbor-cloud`, `src-tauri/src/cloud*`, the 22 `__cloud_*` handlers, the `arbor.cloud`
namespace (i.e. `crates/platform/plugin/ns/src/cloud/`, which exists only as the bridge that
keeps the panel working meanwhile) and the chunk-order modal. `arbor-plugin-ns` is left holding
`arbor.job`.

## The bridge, and its expiry

`arbor.cloud` currently lives in `arbor-plugin-ns` so the existing plugin keeps working — and
works in Bennu too — while the phases land. It is marked as a staging post in its own module
doc. **Do not grow it**: a new cloud capability belongs in the plugin, and anything it needs
from Arbor should arrive as a generic capability like the four above, or it is a sign the line
is in the wrong place.

## The rule this is all measured against

**If the host has to learn something, it is not a plugin.** Every capability added in Phase 0
passes that test: none of them knows what a bucket is.
