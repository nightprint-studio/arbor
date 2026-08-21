# Distributing extensions — what the marketplace and the manifest would have to become

Companion to `plugin-surface-analysis.md`, which argued that wasm belongs in Arbor as a
**packaging format for interfaces the host defines** (`cloud`, `studio`, `brp`) and not as a
language for plugin authors. This one asks the next question: if that is right, what has to
change in the manifest, the registry and the Plugin Manager?

Measured against the tree on `feature/bennu`. Inferences are marked.

**The short version: less than it looks, and later than it looks — except one thing, which is
already broken for Lua and should be fixed regardless.**

---

## 0. What exists today

**The manifest** (`crates/platform/plugin/types/src/manifest.rs`) describes exactly one kind
of thing. Identity, compatibility (`min_arbor_version`, `arbor_api: u32`, `os`, `targets`),
sections (`permissions`, `sandbox`, `hooks`, `scheduler`, `dependencies`) — and:

```rust
#[serde(default = "default_entry")]
pub entry: String,          // "main.lua"
```

One entry point, one artifact kind, no binary payloads.

**The registry** is an `index.json` at the root of a GitHub repo, listing `plugins` and
`themes`. An entry is *internal* (a `subpath` in the registry repo) or *external* (another
repo, with an optional `pinned_sha`). Resolution fetches the raw `plugin.toml` over HTTPS.

**Installing** downloads `https://github.com/{owner}/{repo}/archive/{ref}.zip`, strips the
archive root and the subpath, and dumps the contents into `plugins_dir/{name}/`.

**Versions do not exist.**

```rust
/// We pin to `main` per design decision — tag-based resolution will land
/// once `arbor-extensions` has its first tagged release.
pub const REGISTRY_REF: &str = "main";
```

`version` in `plugin.toml` is displayed and nothing resolves against it. There is no way to
install an older version, no pin, no lockfile, no rollback.

**Integrity is provenance only.** `verify_pinned_sha` checks that a git ref resolves to an
expected commit — a defence against tag-hijack. It is not a hash of what was downloaded.
There is **no `sha256`, no checksum, no digest anywhere in the marketplace crate**: the
zipball is fetched and unpacked unverified.

---

## 1. The observation that reframes it

A cloud connector shipped as a product is not one kind of thing. It is:

- **wasm** — the provider: `list` / `stat` / `download` / `upload` / `copy` / `delete`, opendal
  underneath;
- **Lua** — the UI around it: the settings panel that configures a connection, the command
  that adds one, the entry point of the OAuth flow, the hooks.

So one distributable holds parts of two kinds. That corrects something I said in the previous
analysis: it is **not two shelves**. It is one package whose contents can be of either kind,
and sometimes are of both.

That single fact is what makes `entry: String` the wrong shape. Not because a wasm field is
missing, but because the manifest describes *the package as being* a Lua plugin, rather than
describing *what the package contains*.

---

## 2. The four things that break

### a. One entry point, where a set is needed

The manifest would have to go from "this is a Lua plugin" to "this package provides these
parts". Sketch — **a shape to argue about, not a proposal to merge** (`CLAUDE.md` is explicit
that manifest fields get asked about first):

```toml
[lua]
entry = "main.lua"              # now optional

[[provides]]
interface = "cloud-provider"    # which host interface
version   = 1                   # its contract version
module    = "provider.wasm"
sha256    = "…"
```

A list rather than a field, because one package can genuinely provide several: two format
backends (`json` and `json5`), or a provider plus its UI.

**The design trap here is over-reach.** The temptation is a full VS Code-style `contributes`
block covering every extension point in the app. Arbor deliberately does not work that way:
contributions are declared **at runtime from Lua** (`arbor.ui.contribute`,
`contribution_point`, `list_contributions`), not in the manifest.

That asymmetry is justified and worth preserving: a Lua contribution can be dynamic because
the plugin is already running when it registers one. A wasm interface cannot — the host has to
know what a package provides **before instantiating it**, to decide whether to load it at all
and to show it in the marketplace. So `[[provides]]` is static of necessity, and it should
stay as small as that necessity: which interface, which version, which file, which hash.
Anything a plugin could declare at runtime should keep being declared at runtime.

### b. A source zipball cannot carry a build artifact

Installing downloads the **source archive of a git ref**. That works because a Lua plugin *is*
its source. A `.wasm` is a build output.

| | |
|---|---|
| **commit the `.wasm`** | zero marketplace change — and the registry's vetting model is *"vetting happens via PR review"*. **A reviewer cannot review a binary.** It also puts multi-MB blobs in git history forever. |
| **GitHub Releases** | assets on a tag: source repo stays clean, provenance is real, and it fits the instinct `pinned_sha` already shows. Costs a second fetch path in `installer.rs` and requires authors to have a release pipeline. |
| **build on the user's machine** | needs a wasm toolchain on every machine. No. |

Releases is the answer, and it is a real change to the installer rather than a field.

### c. No content integrity — and now it matters

Today an unverified zipball of Lua source is a moderate risk: the user *could* read what
landed. A `.wasm` is code nobody can inspect. Shipping opaque binaries through a channel with
no content hash is not defensible.

**A per-artifact `sha256`, mandatory for any binary part, is a hard precondition** for this
whole direction — not a later hardening pass. It is the one item in this document I would call
non-negotiable.

### d. Versions do not exist, and the extensions make that load-bearing

For Lua, riding `main` is survivable: a bad update is a broken feature and the user disables
the plugin.

For something implementing a host interface it is not. An extension compiled against
`cloud-provider@1` must not be instantiated by a host that has moved to `@2`, and "the update
broke my buckets, roll back" has no mechanism at all today — no pin, no previous version, no
lockfile.

**This gap is pre-existing.** The extension work does not create it; it removes the ability to
keep ignoring it. Which makes it the strongest argument for reworking the marketplace — and
notably an argument that has nothing to do with wasm.

---

## 3. The scoping fact that changes the plan

**The first three extensions do not need the marketplace at all.**

`cloud`, `studio` and `brp` are not community contributions. They are parts of Arbor that
would be sandboxed and made swappable — they live in this repo, they are built by this repo's
pipeline, and they ship with the app. First-party artifacts need no registry entry, no
zipball, no `index.json`, and no third-party integrity story, because their integrity is the
application binary's.

And the benefits arrive anyway: a malformed-input parser bug can no longer take down the app,
the format backends become individually replaceable, and the interface gets forced into a
shape that survives a boundary.

So the sequence inverts from the obvious one:

1. the runtime and **one** interface, with first-party artifacts and no distribution at all;
2. the marketplace rework, driven by the first genuine third-party extension.

The one thing that should not wait for either is **(d)** — versioning — because it is already
a problem for the Lua plugins that exist today.

---

## 4. What a composite repo would look like, when it comes

```
plugins/cloud-gcs/
  plugin.toml
  main.lua            ← the UI half, reviewable in a PR
  doc.html
  provider/           ← the wasm half's SOURCE
    Cargo.toml
    src/lib.rs
```

with the built `provider.wasm` attached to a release and referenced by hash — never committed.

That makes `arbor-extensions` partly a cargo workspace, which is a real cost: a repo that was
scripts and JSON acquires a build. An alternative is that any package with a wasm part lives
in its own repo and the registry points at it externally — the `repo` + `pinned_sha` shape
already supports exactly that, and it keeps the curated repo reviewable by humans.

*(Inference — I have not weighed these against a real third-party author's workflow, because
there isn't one yet. That is precisely why this decision should wait for one.)*

**One genuine piece of good news:** wasm is platform-independent, so none of this brings the
n×m artifact matrix that sinks native plugin systems. `os = []` keeps meaning what it means.
The only variant that matters is the **wasm target itself** — `wasm32-unknown-unknown` vs
`wasip1` vs `wasip2`/the component model — and that is a compatibility contract as binding as
`arbor_api`. Either the manifest names it or the host fixes one forever; deciding it late is
how you end up supporting two.

---

## 5. The Plugin Manager

An extension is not a plugin in the UI either, and the differences are not cosmetic:

- it has **no hooks**, so the `[hooks]` filter means nothing;
- it has **no UI of its own** — the Studio FE draws from a backend's data, the explorer draws
  from a provider's;
- **disabling it is not "turn off a feature"**: switching off a cloud provider breaks every
  connection using it, and switching off a format backend makes those files unopenable. That
  is a different sentence from "this plugin no longer runs", and the same toggle should not
  produce both.

So: a separate section, a different row, and a disable that says what it will break. Reusing
the plugin row because both come from a manifest is how the manager stops telling the truth
about either.

---

## 6. Order

1. **Content integrity (`sha256`) and version resolution.** Both are gaps today, both hurt Lua
   plugins today, and neither needs a single decision about wasm to be worth doing.
2. **The runtime plus one interface, first-party.** `cloud`, because Lua genuinely cannot do
   it and its interface is already coarse enough to survive a boundary.
3. **`[[provides]]` in the manifest** — kept as small as the "host must know before
   instantiating" requirement forces it, and no larger.
4. **Release-asset installs** in the marketplace.
5. **A separate Plugin Manager section** for extensions.
6. **Repo structure for composite third-party packages** — deferred until a third party
   actually wants one, because that is the only thing that can settle it.

Steps 1 and 2 are independent of each other and of everything below. That is the useful shape
of this answer: the marketplace rework is real, but almost none of it blocks the work that
prompted the question.
