# Extension repos, releases and manifests — a proposal

`cloud`, `studio` and `brp` leave the workspace and become external repos. This proposes the
three shapes that decision forces: how a repo is laid out, how a release is cut, and what the
manifest says.

A proposal, not a decision — manifest fields get agreed before they get written
(`CLAUDE.md`). Everything measured against the tree on `feature/bennu`.

---

## 1. Two levels, and both already half-exist

```
repo  ──has──▶  package  ──has──▶  a Lua part, and/or one or more provides
```

- A **repo** holds one or more packages. The registry already expresses this: an entry is
  `{repo, subpath}`, so two entries pointing at the same repo with different subpaths is
  today's shape, not a new concept.
- A **package** is a directory with a `plugin.toml`. It may have a Lua part, one or more wasm
  parts, or both.
- A **provide** is one wasm module implementing one host interface.

Nothing above needs inventing. What is missing is only the third row: a package cannot
currently say it provides anything but Lua.

---

## 2. Repo layouts

### Shape A — one package, one provide (`arbor-brp`)

The simple case. The repo *is* the extension.

```
arbor-brp/
├── plugin.toml                 ← the package lives at the repo root
├── main.lua                    ← the UI half (connect dialog, status)
├── doc.html
├── icon.svg
├── provider/                   ← the wasm half, SOURCE ONLY
│   ├── Cargo.toml
│   └── src/lib.rs
├── Cargo.toml                  ← workspace: [members] = ["provider"]
└── .github/workflows/release.yml
```

Registry entry: `{ "repo": "…/arbor-brp", "tag": "v1.2.0", … }` — no `subpath`.

### Shape B — one repo, several packages (`arbor-studio`)

Studio is eight crates today (`types`, `core`, `api`, plus five format backends). Externalised,
each format becomes **its own package**: five packages, five artifacts, five version lines.

```
arbor-studio/
├── Cargo.toml                  ← workspace spanning BOTH trees below
├── crates/
│   ├── studio-sdk/             ← types + core: the PUBLIC interface crate
│   ├── studio-json/
│   ├── studio-toml/
│   ├── studio-yaml/
│   ├── studio-ron/
│   └── studio-properties/
├── packages/                   ← one per format; what the registry points at
│   ├── studio-json/
│   │   ├── plugin.toml
│   │   ├── doc.html
│   │   ├── icon.svg
│   │   └── backend/            ← depends on studio-sdk + studio-json
│   │       ├── Cargo.toml
│   │       └── src/lib.rs
│   ├── studio-toml/
│   ├── studio-yaml/
│   ├── studio-ron/
│   └── studio-properties/
└── .github/workflows/release.yml
```

**Why five and not one.** The shorter argument is duplication: `studio-sdk` is ~6350 lines
(`core` 5260 + `types` 1090) against formats ranging from 1251 (toml) to 5744 (ron), so the
shared half weighs about as much as a format and five artifacts embed five copies of it.

The longer argument wins anyway, and it is not about bytes:

- **A blob makes the interface public only on paper.** If Arbor's five formats ship as one
  artifact and a third party's `.ini` ships as a single, the two are visibly different classes
  of thing. Five peers is what makes `studio-format@1` an interface rather than an internal
  detail with a door cut into it.
- **Shipped as one, externalising Studio buys almost nothing.** You would take on the whole
  distribution cost — releases, hashes, registry entries, a published SDK — and get back a
  monolith that still moves in lockstep. The modularity *is* the reason to leave the
  workspace.
- **Loading is per-format.** Somebody who never opens a `.properties` file never instantiates
  that guest.

**The real cost is release friction, not size.** A fix in `studio-sdk` means five tags and five
updated registry entries instead of one. Both halves are mechanisable: a single release
workflow cuts all five tags from one commit, and the registry side is **one PR touching five
entries in one `index.json`** — not five PRs. Worth building that workflow before the second
release, not after the fifth.

**And the SDK has to be published regardless.** A third-party format needs `studio-sdk` from
crates.io to compile against `studio-format@1`. That cost belongs to *having a public
interface*, not to splitting into five — which makes the marginal cost of the split smaller
than it first looks.

### Shape C — composite (`arbor-cloud`)

The same rule applied consistently reshapes cloud too. A bucket connector needs a Lua half —
the settings panel that configures a connection, the command that adds one, the OAuth entry
point — and that half is genuinely shared by all three backends. So: **one UI package, three
provider packages that depend on it.**

```
arbor-cloud/
├── Cargo.toml
├── crates/
│   └── cloud-sdk/              ← opendal setup, auth, the shared error map
├── packages/
│   ├── cloud/                  ← the UI package: Lua only, no provides
│   │   ├── plugin.toml
│   │   ├── main.lua
│   │   ├── lua/{connections,oauth}.lua
│   │   ├── doc.html
│   │   └── icon.svg
│   ├── cloud-gcs/              ← one provide, depends on `cloud`
│   ├── cloud-s3/
│   └── cloud-azblob/
└── .github/workflows/release.yml
```

The dependency mechanism for this already exists — `Dependency { name, version, optional }`
with a semver requirement and a topo-sort — so a provider package declares:

```toml
[[dependencies]]
name    = "cloud"
version = ">=1.4.0"
```

Somebody who only uses S3 installs `cloud` + `cloud-s3` and never carries GCS's OAuth code.

### The rule the three shapes share

**A repo never contains a built artifact.** `provider/` and `backend/` hold source; the
`.wasm` exists only as a release asset. A repo with binaries in it has a history that grows
forever and a diff nobody can read.

---

## 3. The manifest

### The change

`entry: String` becomes a `[lua]` section, and a `[[provides]]` list appears. `entry` is not
kept at the top level as an alias — `CLAUDE.md` allows breaking plugin-API changes precisely
so shapes do not accumulate compatibility barnacles, and two spellings of the entry point is
exactly such a barnacle.

### Worked example — a provider package

```toml
name        = "cloud-gcs"
version     = "1.4.0"
description = "Google Cloud Storage buckets in the file explorer."
author      = "nightprint-studio"
license     = "MIT"
repository  = "https://github.com/nightprint-studio/arbor-cloud"
category    = "data"
icon        = "icon.svg"
doc_file    = "doc.html"

min_arbor_version = "0.9.0"
arbor_api         = 1              # the LUA contract version — unchanged meaning
targets           = ["sitta"]

[permissions]
network = ["storage.googleapis.com", "oauth2.googleapis.com"]

[[dependencies]]
name    = "cloud"                  # the UI package
version = ">=1.4.0"

[[provides]]
interface = "cloud-provider"
version   = 1                      # the INTERFACE contract, not the package's
id        = "gcs"
module    = "cloud_gcs.wasm"

[wasm]
target = "wasm32-wasip2"
```

No `[lua]` section: this package is a provider and nothing else. Its UI lives in the package
it depends on.

### Worked example — the UI half

```toml
name    = "cloud"
version = "1.4.0"
targets = ["sitta"]

[lua]
entry = "main.lua"                 # omit the section entirely for a Lua-less package
```

No `[[provides]]`: this one is an ordinary Lua plugin, and it installs down the ordinary
source-archive channel (§4).

### One package, one provide

`[[provides]]` stays a **list** because nothing forces a package to have exactly one — but in
practice every package here has one, and that is the shape to design for. A package with
several provides only makes sense when they genuinely cannot be separated, and neither Studio
nor cloud is such a case once the shared half is its own package.

What is deliberately **not** supported is several `[[provides]]` entries pointing at the *same*
module with different ids. It was the mechanism that would have let five formats live in one
artifact, that decision went the other way, and a capability with no user is exactly the kind
of thing that survives long enough for somebody to build on it by accident.

### The four decisions inside that shape

**`interface` + `version` are not `arbor_api`.** `arbor_api` versions the Lua surface; an
interface versions itself. `studio-format@1` and `cloud-provider@3` move independently, and
collapsing them into one number means every interface change invalidates every extension.

**`target` is declared, once per package.** `wasm32-unknown-unknown` / `wasip1` / `wasip2`
are different worlds. Naming it now costs a line; deciding it late is how a host ends up
supporting two runtimes forever.

**Permissions are not duplicated.** A guest that talks to `storage.googleapis.com` declares it
in `[permissions] network`, exactly where a Lua plugin declares it. The *enforcement* differs —
a WASI import versus a Lua API gate — but the *declaration* the user consents to must not,
or the consent dialog has to explain two systems.

**No hashes in the manifest.** See below — they belong to the release, and the manifest has to
stay a hand-written file a reviewer can read.

---

## 4. Releases

### Tag

`<package>-v<semver>` — `cloud-v1.4.0`, `studio-builtin-v2.0.0`. Prefixed because Shape B has
several packages in one repo on independent version lines, and a bare `v2.0.0` cannot say
which.

For Shape A the prefix is redundant but kept: one rule beats a rule with an exception.

### Assets

```
cloud-v1.4.0
├── cloud_gcs.wasm
├── cloud_s3.wasm
├── cloud_azblob.wasm
└── cloud-1.4.0.zip        ← the Lua part + doc.html + icon.svg
```

The zip mirrors what the current installer already unpacks, so the Lua half of a composite
package installs by the path that exists today.

### Two channels, and the manifest decides which

Forcing a release pipeline on a forty-line Lua plugin would be a bad trade. So:

| package has | installs from |
|---|---|
| no `[[provides]]` | the source archive of a ref — **exactly as today** |
| any `[[provides]]` | the release for the tag |

The presence of a binary part is what forces the stricter channel. Nothing else changes for
the twenty plugins that exist.

---

## 5. Where the hash lives — and why it is the registry

A `.wasm` cannot be reviewed, and the registry's vetting model is PR review. That looks fatal
and is not, if the hash is recorded **where the review happens**.

```jsonc
// index.json in arbor-extensions — reviewed by PR
{
  "plugins": [
    {
      "repo": "https://github.com/nightprint-studio/arbor-cloud",
      "subpath": "packages/cloud",
      "tag": "cloud-v1.4.0",
      "pinned_sha": "9f2c1ab",
      "artifacts": {
        "cloud_gcs.wasm":    "sha256:1b7d…",
        "cloud_s3.wasm":     "sha256:44a0…",
        "cloud_azblob.wasm": "sha256:c39e…",
        "cloud-1.4.0.zip":   "sha256:7e51…"
      }
    }
  ]
}
```

The chain becomes: **a human approves a PR → that PR names exact bytes → any later
substitution fails to install.** A reviewer still cannot read the wasm. What they *can* do is
pin which wasm was approved, and that is the whole of what is achievable — every package
manager in existence stops at the same place.

Two consequences worth stating plainly:

- **A new version is a new PR.** That is a cost, and it is the same cost npm and crates.io pay
  for immutability. It also means the registry entry finally carries a version, which closes
  the `REGISTRY_REF = "main"` gap for Lua plugins in the same change.
- **Hashes in the manifest would defeat this.** A manifest is written by the author; a hash the
  author supplies verifies only that the author is consistent with themselves.

---

## 6. Install, end to end

```
1. registry entry            → repo, tag, pinned_sha, artifacts{}
2. verify_pinned_sha(tag)    → the tag still points where it was reviewed   [exists today]
3. GET plugin.toml @ tag     → identity, permissions, [[provides]]          [exists today]
4. consent dialog            → from [permissions], unchanged
5. GET release assets        → the .wasm files and the .zip                 [NEW]
6. verify each sha256        → against artifacts{}                          [NEW]
7. unpack zip                → plugins_dir/{name}/                          [exists today]
8. place .wasm beside it     → plugins_dir/{name}/                          [NEW]
9. host loads                → Lua from [lua].entry, guests from [[provides]]
```

Four of the nine steps exist. The new work is the release-asset fetch, the hash check, and the
loader knowing what to do with a `[[provides]]` entry.

---

## 6b. What the first copy actually looks like

`arbor-extensions` now carries the shape, as `plugins/cloud` (the Lua panel) plus
`plugins/cloud-gcs` (the provider, one `[[provides]]`, its own `[[credentials]]` and its own
`network` allowlist), a cargo workspace at the repo root with the provider as its only member,
and a `wit/` directory.

Two things came out differently from the sketch above, both for the same reason — the repo
already exists and its conventions win over a greenfield diagram:

* **`plugins/`, not `packages/`.** Twenty packages already live under `plugins/`, and the
  release workflow finds a package by the `name` in its manifest rather than by directory, so
  a second tree would have bought nothing but a second spelling.
* **The WIT is vendored.** A guest compiles against the interface, and the interface lives in
  Arbor's repo — a different one. A copy with its origin written down is the honest state until
  there is an SDK crate to fetch it from; a stale copy fails at load with a version mismatch
  rather than subtly at runtime, which is the failure mode worth having meanwhile.

`plugins/cloud-gcs` is deliberately **not listed in `index.json`**: it has no release, so it
has no artifact digests, and an entry without them would install a package whose module is
missing. The marketplace now refuses such an entry outright rather than letting it land.

## 6c. What was verified, not just written

The chain is proven end to end rather than argued for:

* `wit/` parses, and `wasmtime::component::bindgen!` generates host bindings from it.
* Two guests build from the **vendored** copy in the other repo — `cloud_gcs.wasm` (148 KB)
  and `studio_json.wasm` (146 KB), both real components, not core modules.
* Arbor's host **instantiates both**: their imports resolve against what it links and their
  exports match the worlds they claim. Twice each, from one compilation.
  (`cargo test -p arbor-plugin-wasm --features runtime --test instantiate -- --ignored`, with
  `ARBOR_WASM_FIXTURE` pointing at the built module.)

Three things the compiler corrected that no amount of reading would have:

* **`list` and `from` are WIT keywords.** The first is escaped (`%list` — it is the word every
  object store uses); the second was renamed to `source`/`destination`, which reads better
  anyway.
* **A directory is one package.** Three `package` declarations needed a `deps/` tree; merging
  them into `arbor:extensions@1.0.0` was simpler and is what these interfaces are — versioned
  together, by the host that defines all of them.
* **WASI cannot be omitted.** `wasm32-wasip2` links the WASI standard library into every guest
  whether it uses it or not, so refusing to link it produces a guest that will not instantiate
  rather than one without it. It is linked with an empty context: no preopens, no sockets, no
  environment, no stdio. Same guarantee, honest description.

## 7. What I would want decided before writing any of it

1. **Does `entry` really move into `[lua]`?** It breaks every existing `plugin.toml`. Allowed,
   and cheap while the count is twenty — much less cheap later.
2. **Five packages in one repo, or five repos?** Above assumes one repo: `studio-sdk` stays a
   path dependency, a core change is atomic across all five, and one workflow cuts five tags.
   Five repos gives each format a fully independent life at the cost of every one of them
   resolving `studio-sdk` from crates.io on a semver range — which is real work the moment the
   SDK has a breaking change. One repo does not preclude splitting later; splitting first
   cannot be undone cheaply. **This is the one I would settle before writing the workflow.**
3. **Does a package's version pin its interface version, or does the host negotiate?** Above I
   assumed pinning (`version = 1` and the host refuses anything else). Negotiation — a guest
   declaring a range — is friendlier and much more machinery.
4. **Do extensions get their own registry file, or share `index.json`?** Sharing is simpler and
   the entries already differ only by their `artifacts` block. Splitting is honest about them
   being a different kind of thing. I lean shared, and separate them in the **Plugin Manager**
   instead, where the difference is one the user can actually see.
