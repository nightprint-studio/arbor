# arbor-plugin-ns

The `arbor.*` Lua namespaces that belong to the **platform** rather than to one product.

## Purpose

A namespace lands here when two things are true: the state it drives lives in the shell — so a
headless backend can only reach it over the reverse channel — and nothing about it is one
product's business.

Both were true of `arbor.job` and `arbor.cloud` while they sat in `corvus-plugin-ns`, and the
cost was concrete rather than aesthetic: a plugin hosted by Bennu could not report progress or
browse a bucket, for a reason that was purely about which file a namespace was written in.
Corvus was simply the first product to grow a plugin host.

## How a product uses it

```rust
use arbor_plugin_ns::prelude::{CloudHostOps, CloudInstaller, JobHostOps, JobInstaller};

let mut namespaces = vec![];
namespaces.push(Arc::new(JobInstaller::new(JobHostOps::new(app.host_caller()))));
namespaces.push(Arc::new(CloudInstaller::new(CloudHostOps::new(app.host_caller()))));
app.api_installer(product_api_installer(namespaces));
```

The installers run through the product's `LuaApiInstaller` (i.e. after `register_lua_api`'s
host-pure namespaces), like every other product namespace. Both Corvus and Bennu install this
exact pair.

## Layout

| Path | What |
|---|---|
| `proxy.rs` | `HostProxy` — the one reverse-channel round-trip helper, with the reply shapes (`call` / `text` / `flag` / `unit`). |
| `job/` | `arbor.job` — spawn, list, cancel, dismiss, clear. The shell owns the one `JobRegistry`. |
| `cloud/` | `arbor.cloud` — object storage. **On its way out**, see below. |

Each domain is a pair: `host.rs` holds the `__<domain>_*` forwards, `ns.rs` the Lua surface
built on them. A new domain adds its vocabulary and nothing else — the round-trip is written
once.

## `cloud` is a staging post

The cloud is being moved out of Arbor entirely, into the `cloud-storage` plugin and its WASI
providers (`docs/cloud-migration.md`). This module exists so the panel keeps working — in Bennu
too — while that lands, and it goes away with Phase 4.

**Do not grow it.** A new cloud capability belongs in the plugin; if it needs something from
Arbor, that something should arrive as a generic capability (`arbor.ext.call_to_file`,
`arbor.oauth`, `arbor.job`) or the line is in the wrong place.

## Gotchas

* **The error strings are the contract.** They cross the seam as `Display` and a plugin matches
  on nothing else, so a message shaped here must stay byte-identical to what the shell sends.
* **Every call blocks** on the shell's reply, so nothing here may run on a runtime worker the
  shell might in turn be waiting on (landmine #1 in `docs/backend-architecture.md`).
* **Hooks fan out to every plugin host.** A universal plugin is loaded once per product, so the
  shell's `fire_plugin_hook_on_backends` sends to all of them — a plugin enabled twice sees
  both, and drops what it did not ask for.
