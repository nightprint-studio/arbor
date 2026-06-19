# corvus-be Out-of-Process Migration — Grounded Analysis

> Synthesis of the infrastructure scan + per-domain audit, verified against the working tree
> (branch `feature/launcher`). This is a status-and-plan document, not a changelog: it describes
> what exists today and the concrete remaining work, with file/line citations.

## 1. Executive summary — honest current state

**corvus-be is a real second OS process, but it serves almost nothing yet.** The transport,
the advertise-and-route mechanism, the reverse credential channel, and the leaf-domain template
are all genuinely built and working. What is *not* built is the bulk of the migration: 4 of ~30
domains are served out-of-process, the credential broker is half-built (function pointers, not
data), config is never pushed to the backend, and the hook-firing model is mid-refactor and
currently **silently drops hooks on the OOP path**.

Verified facts (working tree, branch `feature/launcher`):

- **corvus-be source surface is exactly 5 files**: `crates/corvus/be/src/{main,repo_registry,bisect,stash,issues}.rs`. Only `bisect`, `stash`, `issues` (Linear/Jira), and the `repo_registry` sync targets are served OOP. (verified via Glob)
- **corvus-git (Tauri-free crate) holds**: `bisect, bisect_sessions, cli, encoding, stash, recovery, reset, error, search, merge, rebase, worktree` — and **nothing else**. No `branch, status, diff, graph, stats, gitflow, init, repo, submodule, notes, reflog, remote, stage`. (verified via Glob: `crates/corvus/git/src/*.rs`)
- **`src-tauri/src/ipc/corvus/post_hooks.rs` is DELETED** (Glob: "No files found"; git status shows `D`). The only post_hooks table is `platform/post_hooks.rs`, which `return`s early for `program != "platform"` (verified, line 21–23). So **for corvus there is no routing-independent hook seam at all** — every corvus hook fires inline from the handler body.
- **`SplitBroker::call` routes by `oop_methods.contains(method)`** (verified, `split_broker.rs:32–38`). When corvus-be advertises a method, the child runs it and the in-process loopback body (which contains the inline `state.fire_hook(...)`) **never executes**. Combined with the deleted corvus post_hooks, this means: **the moment a hook-bearing domain is advertised OOP, its hooks are silently dropped.** `stash` is the live example — corvus-be advertises `stash_save`/`stash_apply`/`stash_pop`, but the inline `on_stash_push`/`on_stash_pop` only fire on the in-process fallback path.

The realistic shape of the remaining work is three structural unlocks (transport hardening is
*optional*; config-push and the hook seam are *mandatory*; the credential broker is the *gate*
for the largest credential-coupled wave), followed by a long tail of mechanical git-crate
extractions + corvus-be handler authoring that can run highly parallel.

**The single most urgent correctness item is the hook seam**, not because anything crashes today
(only the fallback path runs, so hooks still fire in-process), but because `stash` is *already
advertised OOP* and will drop hooks as soon as corvus-be is present — and every future
hook-bearing domain (reset, branch, gitflow, notes, remote, stage) inherits that trap.

## 2. What's missing — concrete, actionable items

These are the pieces NOT yet built for a real OOP corvus-be. Each is independently actionable.

### 2.1 Hook delivery seam for OOP corvus (MANDATORY, blocks every hook-bearing domain)
Re-introduce a **corvus-scoped `post_hooks` table** (mirror of `platform/post_hooks.rs`), called
from `rpc_commands.rs` for `program == "corvus"` after a successful dispatch, reconstructing each
hook ctx from `(params, result)` so it fires exactly once on both loopback and OOP routes. The
hook payloads were already engineered to be `(params,result)`-reconstructable. **Caveat:** some
hooks are conditionally fired — `on_stash_pop` only when `!result.has_conflicts`
(`stash.rs:71,89`), `on_checkout` only on a clean `CheckoutResult` — so the post-hook arm must
read the result, not just params. Alternative (per the per-program-plugins memory): co-locate the
Lua host *inside* corvus-be so it fires directly — heavier, defers to a later phase.
Either way, this MUST be decided and built before `reset/branch/gitflow/notes/remote/stage` go OOP.

### 2.2 Config push into CorvusState (MANDATORY for several domains)
`CorvusState` holds only `events / repos(tab→path) / git_program / host` (`crates/corvus/core/src/state.rs:17–35`). No app config. Push a typed config snapshot (mirroring `git_program`) carrying at minimum:
- recovery `SnapshotPolicy` (retention_days / max_file_size / deny_extensions) — the documented forcing case (`be/src/stash.rs:16–21`; OOP force-apply/abort use `SnapshotPolicy::default()`).
- diff `context_lines` default (8 diff handlers read `config.diff.context_lines`).
- `pipelines.max_concurrent_runs`.
- gitflow global config + per-repo `.arbor/config.toml` access.
- ticket-link config, stats `StatsExcludeConfig`, encoding defaults.

Decision: snapshot-and-re-push-on-change (event-driven; mirrors `git_program`, but `git_program`
is itself only re-pushed on repo-open — a latent staleness bug, `ipc/mod.rs:320`) vs. pull live
over a `__config` reverse-channel round-trip. The recovery policy gap forces this choice first.

### 2.3 Generic credential broker / descriptors-as-data (GATE for the credential wave)
The descriptors-as-data type **does not exist**. `VaultSessionProvider`'s descriptors carry
divergent logic as **function pointers into shell code** (`auth/vault.rs:50–63,68–103`), so the
launcher is not yet provider-code-free. Missing:
- `ProviderCredentialDescriptor { keyring_account, header_scheme(Bearer|Basic), base_url(Fixed|FromAccount), refresh(OAuth{…}|Static) }` as serializable data (distinct name from the existing connect-UI `ProviderDescriptor` in `crates/corvus/provider-descriptor`).
- the `__credential_descriptors` per-backend collection IPC (forward channel) — does not exist; `for_account` hardcodes github/linear/jira/else-gitlab routing (`vault.rs:190–198`).
- semaphore unification: the single-use-token `REFRESH_LOCK` is shared between OAuth refresh and `ci_impl`'s 401-retry; a generic in-process refresh engine would reintroduce the race. Only forced/testable at the real OOP split.

The **established reverse-channel `ChildSessionProvider` pattern works today** (issues domain) and
covers REST credentials (`AuthSession{base_url,auth_header}`). It does NOT cover **git-protocol
push/fetch/clone auth**, which uses a *separate* keyring path (`credential_store::resolve_credentials`
via git2 `RemoteCallbacks` + `git_cli::http_auth_args`) and needs a `(username, secret)` /
HTTP-Basic-with-host-sentinels shape (x-access-token / oauth2), not `AuthSession`. Two resolution
surfaces must be reconciled or accepted as permanent before remote/notes-push/submodule-network
move OOP.

### 2.4 Git-crate extractions (per-domain prerequisite, mechanical)
The following still live in `src-tauri/src/git/` (Tauri-shell crate, on `crate::error::AppError`)
with **no `corvus_git` counterpart** and must be extracted to the Tauri-free crate (+ added to its
prelude, + `mod tests` per the working agreement) before their domain can be served OOP:
- `branch.rs` + `status.rs` (largest, most-used; drags `CheckoutResult`/`safe_checkout_with_stash` which stay shell-side, AppState/worktree-link coupled)
- `diff.rs` + `blame_incremental.rs` (+ DTOs `DiffFile`/`BlameLine`/`BlameProgress`)
- `graph.rs` + `svg_export.rs` (+ DTOs; repoint its `collect_stash_refs` to the crate copy, not the src-tauri one; lift the inline fingerprint ref-walk + file-meta scan)
- `stats.rs` + `stats_export.rs` (+ `RepoStats`; pure, only needs git2 + `StatsExcludeConfig`)
- `gitflow.rs` (+ `GitFlowConfig/GitFlowStatus/FlowStartResult/FlowFinishResult`; repoint `AppError`/`NoWindowExt`/`git_cli`)
- `notes.rs` (+ `CommitNote`/`NoteRemoteStatus`)
- `ticket_links.rs` (+ `TicketLink/LinkSource/TicketLinkConfig/TicketLinkCache`; already Tauri-agnostic in content)
- `init.rs` (+ `is_git_repo` — trivial `Repository::discover` wrapper, unblocks `missing` + `repo` leaf reads)
- `repo.rs` (`GitRepo`/`RepoManager`/`RepoInfo` — foundational; **prerequisite for `reflog`**, which is built on `GitRepo`)
- `reflog.rs` (blocked on `repo.rs`)
- `submodule.rs` (+ `SubmoduleInfo`; CLI-shell + keyring, not pure git2 — audit/inventory mislabels it)
- `remote.rs` (credential-coupled; lift credential resolution OUT of the git layer into an injected resolver so the crate stays keyring-free, like stash)
- `git_cli/mod.rs` (Tauri-free in content but in src-tauri; carries `GIT_CLI` RwLock + `DOWNLOAD_CANCEL` AtomicBool statics that must move with it)
- **stage/index/commit logic is inline in the handler** (`ipc/corvus/stage.rs`) — no module anywhere; must be lifted into `corvus_git::stage`.

### 2.5 State-ownership cuts (CorvusState must grow, or reach back over reverse channel)
CorvusState is a near-empty transport seed. Domains need:
- `RepoManager` (today only a `tab→path` map OOP; each handler re-opens by path).
- `GitProviderRegistry` (mr/ci/security/repo_browser/avatar) — must build in corvus-be with reverse-channel `SessionProvider` per host, OR marshal provider calls back.
- `JobRegistry` + stream registry (graph svg-export, diff-stream, stats-export, ide-detection, mr-conflict-prep). Decide: corvus-be owns its own JobRegistry vs. proxies job lifecycle to the shell's.
- `BrpRegistry` (brp) — must move from AppState (`lib.rs:207`) onto CorvusState.
- `PipelineEngine` Arc + runtime inputs (sink/hooks/plugin_host/plugin_logs/config cap).
- caches: `stats_cache`/`stats_computing`, `ticket_caches`, the fs_git module-static status cache, `SCAN_TOKENS`.
- **Contested registries** (`repo_registry`/`workspaces`/`marketplace`): currently platform-namespaced but `corvus/missing.rs` (a corvus handler) mutates the SAME `repo_registry` + `workspaces` that platform/workspace.rs owns — a cross-process shared-state problem with no current arbitration design.

### 2.6 Vetoable-hook reverse-channel (only for `commit`)
`on_pre_commit` is the sole vetoable hook (`fire_vetoable_blocking`, `stage.rs:38`). The current
plan deliberately keeps `commit` + the Lua host **product-local in-process** to avoid a synchronous
cross-process veto round-trip. If `commit` must ever be served OOP, a blocking veto handshake over
the reverse channel must be designed (the reentrancy machinery exists — used for credentials — but
is intentionally not wired for veto). **Decision required before migrating the stage/commit domain;
recommended: split the domain, move the 9–10 pure stage handlers, leave `commit` in-process.**

### 2.7 Transport hardening (OPTIONAL — does not block stdio-based OOP)
The wire is **framed JSON over child stdin/stdout, unauthenticated**. `tarpc` is declared in
`Cargo.toml:87` workspace deps but **used by no crate** (dead/aspirational). The nonce/ACL
named-pipe/unix-socket (SO_PEERCRED/0600) handshake in `arbor-ipc/lib.rs` doc is unimplemented.
A binary codec (bincode/postcard) is intended but unbuilt (JSON used on the real ChildClient wire,
not just dev loopback). **Decision: either delete the tarpc dep + aspirational doc text, or schedule
the pipe/socket + codec flip.** Also missing: **corvus-be supervision** — if the child dies,
SplitBroker hard-errors (`IpcError::Transport`) with no respawn / fall-back-to-loopback; `CORVUS_OOP`
is a write-once OnceLock. And `is_oop_method` is hard-coded to `program == "corvus"` (`mod.rs:102`),
needing generalization before platform-be/merula-be.

## 3. Per-domain readiness table

Legend — **Served OOP**: handler module exists in corvus-be. **Git extracted**: pure logic in `corvus_git`.
Readiness verdict carried from the audit.

| Domain | Served OOP | Git extracted | Credentials | Hooks (inline) | Events/Jobs | Verdict / gating blocker |
|---|---|---|---|---|---|---|
| **bisect** | ✅ yes | ✅ yes | — | none | — | **DONE / ready_now** (live OOP) |
| **issues** (linear/jira) | ✅ 15/18 | n/a (REST) | reverse-channel ✅ | none | — | **ready_now**; `jira_get_auth_status` + 2 metadata stay shell-side by design |
| **stash** | ✅ yes (mirror) | ✅ yes | — | `on_stash_push/pop` (inline) | — | **needs_config_push + hook seam**; advertised OOP but drops hooks today |
| **repo_registry** (`__*` sync) | ✅ yes | n/a | — | none | — | DONE (internal sync targets) |
| **merge** | ❌ | ✅ yes | — | none | — | **ready_now**; only mechanical `be/src/merge.rs` authoring left |
| **rebase** | ❌ | ✅ yes | — | `on_rebase_start/abort` (inline) | — | **ready_now**; needs hook seam + move `get_rebase_state` git2 read OOP |
| **recovery** | ❌ | ✅ yes | — | none | — | **needs_config_push** (SnapshotPolicy); shares the stash config fix |
| **search** | ❌ | ✅ yes | — | none | — | **ready_now**; mechanical `be/src/search.rs` (pure git2, no GitCli) |
| **worktree** | ❌ | ✅ (git parts) | — | none | ide-detection stream/jobs | **needs_config_push** (IdeConfig/repo_config); defer `start_ide_detection` |
| **brp** | ❌ | n/a (no git) | — | none | — | **ready_now**; move `BrpRegistry` field onto CorvusState |
| **fs_git** | ❌ | n/a (inline git2 OK) | — | none | — | **needs_config_push** (one `try_snapshot` → corvus_git::recovery + module cache) |
| **auth** | ❌ | needs extract (credential_store) | self (keyring) | none | — | **needs_git_extraction**; self-contained, parallel-safe, NOT broker-gated |
| **reset** | ❌ | ✅ yes | — | `on_tag_create/delete` (inline) | — | **needs hook seam** (+ snapshot-policy-as-data for hard reset) |
| **notes** | ❌ | ❌ (git/notes.rs) | push only (`push_note_namespace`) | `on_note_saved/deleted` (inline) | — | **needs_git_extraction** + hook seam; push handler broker-gated |
| **reflog** | ❌ | ❌ (blocked on repo.rs) | — | none | — | **needs_git_extraction** (GitRepo first) |
| **submodule** | ❌ | ❌ (git/submodule.rs) | 5/8 keyring (CLI auth) | none | — | **needs_credential_broker** for 5; 3 leaf can split early |
| **stats** | ❌ | ❌ (git/stats.rs) | — | none | job + branding egress | **needs_git_extraction**; already AppHandle-free |
| **diff** | ❌ | ❌ (git/diff.rs) | — | none | diff-stream jobs | **needs_git_extraction**; 8 config_coupled (context_lines) |
| **graph** | ❌ | ❌ (git/graph.rs) | — | none | file-meta + svg-export jobs | **needs_git_extraction**; repoint stash call, lift inline walks |
| **gitflow** | ❌ | ❌ (git/gitflow.rs) | ambient git push only | `on_flow_*` ×8 (inline) | — | **needs_git_extraction** + config + hook seam; NOT broker-gated |
| **branch** | ❌ | ❌ (git/branch.rs + status.rs) | 2 remote handlers | `on_branch_*`/`on_checkout` (inline) | worktree-links-changed | **needs_git_extraction** (heaviest); waves: reads / local / registry / credential |
| **stage** | ❌ | ❌ (inline in handler) | — | `on_pre_commit` (VETOABLE) + `on_commit` | — | **needs_git_extraction**; split — `commit` stays in-process |
| **missing** | ❌ | ❌ (is_git_repo) | — | `on_project_missing/relocated` (inline) | registry-changed | **needs_config_push** (registry+config+hooks); 2 leaf validators split early |
| **linked_worktree** | ❌ | n/a (no git) | — | 2 member hooks (inline) | worktree-links-changed | **needs_plugin_host_channel**; registry CRUD only, git sync is branch-triggered |
| **tickets** | ❌ | ❌ (git/ticket_links.rs) | — | none | — | **needs_git_extraction** + config; owns ticket_caches |
| **git_cli** | ❌ | ❌ (git_cli/mod.rs) | — (public GH API) | none | download-progress stream | **needs_config_push** (executable_path write) + move statics |
| **pipeline** | ❌ | n/a (model extracted) | — | `on_pipeline_*` (inline, orchestrator) | pipeline events + jobs | **needs_config_push** + PluginHost/Engine/sink ownership |
| **repo** | ❌ | ❌ (git/repo.rs + init.rs) | clone/init/list-remote | `on_repo_open/close/init` (inline) | registry-changed | **needs_git_extraction**; close_repo straddles platform (orphan-GC) |
| **avatar** | ❌ | mixed | keyring + OAuth refresh | none | — | **needs_credential_broker** + provider registry OOP |
| **image** | ❌ | n/a (HTTP proxy) | keyring + OAuth refresh (4 providers) | none | — | **needs_credential_broker** |
| **repo_browser** | ❌ | n/a (providers extracted) | SessionProvider + maybe_refresh | none | fs write | **needs_credential_broker** |
| **mr** | ❌ | ❌ (list_remotes/delete_remote/mr-prep) | provider + refresh + git push | `on_mr_opened/merged/updated` (inline) | mr-conflict streams + jobs | **needs_credential_broker** + hook seam |
| **ci** | ❌ | n/a (providers extracted) | provider + maybe_refresh + keyring has_token | none | — | **needs_credential_broker**; bundle with mr |
| **security** | ❌ | n/a (providers extracted) | provider + refresh | `on_security_summary_loaded` (inline) | export jobs + branding | **needs_credential_broker** + hook seam + extract `security_export` |
| **provider** (connect/oauth) | ❌ | n/a (no git) | 8/9 keyring | none | oauth-done (via sink) | **needs_credential_broker**; ship with `auth` |

## 4. Migration waves (ordered)

Three infrastructure unlocks come first (W0a/b/c), then domain waves. The hook seam (W0a) and
config push (W0b) are cheap and unblock the largest cohort of *non-credential* domains; the
credential broker (W3 gate) is the heaviest and gates the credential cohort.

### Wave 0 — Infrastructure unlocks (prerequisite to everything below)
- **W0a — Corvus hook seam.** Build `ipc/corvus/post_hooks.rs` (or co-located host) + call from `rpc_commands.rs` for `program=="corvus"`, with result-aware arms for conditional hooks (`on_stash_pop`, `on_checkout`). **Also fixes the live stash drift.** Prereq for: stash, reset, rebase, branch, gitflow, notes, mr, security, missing, linked_worktree, repo, pipeline.
- **W0b — Config push to CorvusState.** Typed config snapshot (recovery SnapshotPolicy, diff context_lines, gitflow config, ticket config, stats exclude, pipeline cap, IdeConfig, git executable_path). Prereq for: recovery, stash (force-apply fidelity), worktree, fs_git, diff, gitflow, tickets, git_cli, pipeline, missing.
- **W0c (optional, parallel) — Transport/lifecycle.** Decide tarpc delete-vs-flip; add corvus-be supervision (respawn / loopback fallback on child death); generalize `is_oop_method` keying. Not blocking, but should land before the surface grows large.

### Wave 1 — Ready-now leaves (no extraction, no broker, no config)
Domains whose git logic is already extracted and which need only mechanical `be/src/<domain>.rs`
authoring (+ W0a for the hook-bearing ones).
- **`merge`, `search`** — zero hooks/config/creds. Pure mechanical authoring.
- **`brp`** — move `BrpRegistry` onto CorvusState, author 4 handlers.
- **`rebase`** — author + W0a (2 hooks) + move `get_rebase_state` git2 read into be.
- **`recovery`** — author + W0b (SnapshotPolicy). Shares the config fix with stash.
- **`stash` (finish)** — already authored; needs W0a (hooks) + W0b (force-apply policy) to flip safely.
- **`reset`** — author + W0a (on_tag_*) + W0b (hard-reset snapshot policy).

### Wave 2 — Extraction leaves (git extraction, no broker)
Each = extract `src-tauri/src/git/<x>.rs` → `corvus_git::<x>` (+ DTOs + `mod tests`), then author `be/src/<x>.rs`.
- **`worktree` finish, `fs_git`, `git_cli`** (W0b) — config-coupled, git already mostly local/extracted.
- **`stats`, `diff`, `graph`** — extract + W0b; graph/diff carry job/stream egress (defer the streaming handlers if JobRegistry OOP ownership isn't settled).
- **`gitflow`** — extract + W0b + W0a (8 `on_flow_*` hooks). NOT broker-gated (push uses ambient git).
- **`notes`** (4 local handlers) — extract + W0a; `push_note_namespace` deferred to W4.
- **`tickets`** — extract + W0b; move `ticket_caches` to CorvusState + rewire tab-close cleanup.
- **`auth`** — extract `credential_store` to a Tauri-free crate; self-contained (keyring is process-global). Parallel-safe, NOT broker-gated. Ship with `provider` connect glue in W3 or earlier.
- **`init.rs` extraction** (one-line `is_git_repo` wrapper) unblocks the leaf validators of `missing` and `repo`.
- **`branch` reads + `status`** — extract `git/branch.rs` + `git/status.rs`; migrate the 6 read-only branch handlers + `get_status` (W0b for detect_renames). Local mutators (`delete_branches`, `checkout_commit*`) follow with W0a.
- **`stage` (9–10 pure handlers)** — extract `corvus_git::stage`; author all EXCEPT `commit`. `commit` stays in-process (vetoable hook).

### Wave 3 — Credential broker GATE + credential cohort
**Prereq: W2.3 generic credential broker** (descriptors-as-data + `__credential_descriptors` + semaphore unification), or at minimum the reverse-channel `SessionProvider` + `GitProviderRegistry` built inside corvus-be. The providers are already trait-clean on `SessionProvider`, so these flip with near-zero logic change once the broker lands.
- **`ci`, `mr`, `security`, `repo_browser`, `avatar`, `image`, `provider`, `issues`(finish).** mr/security also need W0a (inline hooks). Bundle `ci`+`mr` (shared `provider_for_tab`/`helpers.rs`). Extract `mr`'s `list_remotes`/`delete_remote_branches`/`security_export` along the way.

### Wave 4 — Git-protocol credential domains (separate credential shape)
**Prereq: W3 + a git-credential resolution shape** (the broker as built is REST/`AuthSession`-only; the smart-HTTP path needs `(username, secret)` / host-sentinel Basic).
- **`remote`** (fetch/push/pull — extract `git/remote.rs` with injected resolver) + W0a hooks.
- **`notes` push** (`push_note_namespace`), **`submodule`** (5 network handlers), **`branch`** (`delete_remote_branches`/`rename_remote_branch`), **`repo`** (`clone`/`init`/`list_remote_branches_for_url`).

### Wave 5 — Contested state + product-local holdouts (decisions, then code)
- **`stage` `commit`** — design vetoable reverse-channel veto, OR keep product-local permanently (recommended).
- **`pipeline`** — owns Engine/PluginHost/sink/jobs; decide corvus-sub-domain vs. standalone crate.
- **`missing`/`repo`/`linked_worktree` registry tail** — resolve `repo_registry`/`workspaces` ownership (platform vs per-product) since `corvus/missing.rs` cross-touches the platform registry; close_repo's orphan-GC straddles the boundary.
- **`linked_worktree`** — registry CRUD can go with W2 (no git), but needs W0a for the 2 member hooks and a CorvusState home for `WorktreeLinkRegistry`.

## 5. Parallelizable groups vs. forced serialization

**Forced serialization (shared new state / shared edits):**
- **W0a hook seam is a single shared file/mechanism** — build once; every hook-bearing domain in W1–W4 then reuses it. Do NOT let multiple domain agents each invent a hook-firing site.
- **W0b config snapshot is a single shared struct** — recovery+stash+diff+gitflow+tickets+pipeline+git_cli all add fields to the *same* CorvusState config type; serialize the *struct definition*, parallelize the per-domain field additions.
- **W3 credential broker is the gate** — ci/mr/security/repo_browser/avatar/image/provider all serialize behind the single broker landing, then fan out (they share `helpers.rs::provider_for_tab` + the broker contract, read-mostly).
- **Shared-file contention**: `mr` + `ci` both edit `git_provider/helpers.rs`; `branch` + `graph` + `mr` + `repo` all touch `src-tauri/src/git/{branch,remote,stash}.rs` during extraction — coordinate to avoid concurrent edits. `repo`'s `close_repo` + platform `workspace_commands.rs` share `repo_registry`/`workspaces`.

**Freely parallel (disjoint, no shared new state):**
- **W1**: `merge`, `search`, `brp`, `rebase`, `recovery` are mutually disjoint — each is its own `be/src/<x>.rs` + (for hook/config ones) a *consumer* of the W0a/W0b shared pieces. Run concurrently once W0a/W0b exist.
- **W2 extractions** are per-file disjoint: `stats`, `diff`, `graph`, `gitflow`, `notes`, `tickets`, `worktree`, `fs_git`, `git_cli`, `auth` each extract a different `src-tauri/src/git/*.rs` and author a different `be/src/*.rs`. Highly parallel; only `graph`↔`stash`-ref repointing and `diff`/`graph` sharing `git/encoding` need a light touch.
- **`auth`** is fully independent of the broker and every other domain (keyring is process-global) — can run anytime after its extraction.
- **`linked_worktree`** (registry CRUD), `brp`, `tickets` each own a private registry/cache → no contention with other domains.

## 6. Dependency chain (hard prerequisites, ordered)

1. **W0a corvus hook seam** → before stash-flip, reset, rebase, branch-mutators, gitflow, notes, mr, security, missing, linked_worktree, repo, pipeline (any inline-hook domain advertised OOP drops hooks without it).
2. **W0b config push** → before recovery, stash(force-apply fidelity), worktree, fs_git, diff, gitflow, tickets, git_cli, pipeline, missing.
3. **`corvus_git::repo` (GitRepo) extraction** → before `reflog` extraction.
4. **`corvus_git::init` (is_git_repo)** → before `missing`/`repo` leaf validators.
5. **Per-domain git extraction** → before that domain's corvus-be handler (branch/status, diff/blame, graph/svg, stats, gitflow, notes, ticket_links, submodule, stage, remote, git_cli).
6. **W3 generic credential broker (or in-corvus-be GitProviderRegistry + reverse-channel SessionProvider)** → before ci, mr, security, repo_browser, avatar, image, provider.
7. **W4 git-credential resolution shape (smart-HTTP Basic, distinct from AuthSession)** → before remote, notes-push, submodule-network, branch-remote, repo clone/init.
8. **Vetoable reverse-channel veto** → before `commit` OOP (or commit stays product-local — recommended; not on the critical path).
9. **(Optional, independent) transport hardening + supervision** → before scaling the OOP surface large / before platform-be/merula-be (`is_oop_method` generalization).

## 7. Ownership decisions (ADR) — locked 2026-06-19

These are the architecture decisions taken before implementation. They set a consistent stance:
**each product backend is self-contained** (owns its own registry, workspaces, and plugin host);
`platform` is the thin app-agnostic *mechanism* layer (jobs, vault, terminal, feedback) + the
launcher shell is router + window + boot.

### ADR-1 — Each backend owns its own registry + workspaces
`repo_registry` and `workspaces` move **into corvus-be** (the git product owns its own), not
platform-be. A future product (Merula, dbmanager) gets its own registry/workspaces; we move to a
shared trait only *if* a concrete need appears later. **Consequence:** the cross-process shared-state
hazard dissolves — there is no shared *platform* registry for `corvus/missing.rs` to fight over;
corvus owns it outright. **Reclassification:** the `workspace` handlers currently migrated to the
`platform` namespace (`ipc/platform/workspace*.rs`) are product-scoped under this ADR — at the OOP
split they move to **corvus**, not platform-be. (The in-process seam work stands; only the
namespace/home changes when they go OOP.)

### ADR-2 — The Lua plugin host moves into the product backend (corvus-be)
Per the per-program-plugins model, the plugin host (mlua + `HookDispatcher` + the `arbor.*` API
surface) **relocates into corvus-be**; corvus-be fires its own hooks against a co-located host.
**Consequence:** the corvus `post_hooks` table is **NOT built** (it would be throwaway) — W0a changes
from "build a corvus post_hooks seam" to "relocate the host into corvus-be." **Interim (no throwaway
work):** corvus-be must **not advertise hook-bearing domains OOP** until the host relocation lands —
this avoids the live stash hook-drop with zero interim cost (hook-bearing domains keep running
in-process, where their inline `fire_hook` works). **Reclassification:** the `plugin` (and plugin-
marketplace) handlers currently in the `platform` namespace are product-scoped — they travel with the
host into corvus-be. (Themes stay platform-level.)

> **OPEN SUB-QUESTION (must be designed before/with the host relocation):** cross-product / platform
> hooks. Not every hook is git-product: `on_theme_changed` (platform), `on_pipeline_*` (CI — corvus
> or a future `sitta`?), marketplace hooks. With workspaces now corvus-owned (ADR-1), `on_workspace_*`
> is corvus too. Need a routing design: which hooks fire from corvus-be's host vs. a platform-level
> host, and how a plugin loaded on corvus receives a platform-originated event. This is the single
> largest piece of remaining design and gates every hook-bearing domain.

### ADR-3 — Corvus jobs proxy to platform's JobRegistry
corvus-be registers its jobs (diff-stream, svg-export, stats-export, ide-detection) in the **shell/
platform JobRegistry via the reverse channel** — single source of truth for the Jobs overlay and for
cancel (which stays shell-side). Matches "job = platform mechanism, scope = product".

### ADR-4 — Credentials: GitProviderRegistry inside corvus-be + reverse-channel SessionProvider
corvus-be builds its own `GitProviderRegistry`; credentials resolve via the **proven reverse-channel
`SessionProvider`** pattern (issues domain). Unblocks the REST credential cohort
(ci/mr/security/repo_browser/avatar/image/provider) in parallel once it lands. Descriptors-as-data
(provider-code-free launcher) is a **follow-up, not a prerequisite**. Git-protocol push/fetch auth
remains a *separate* credential shape (W4). **Before any generic refresh engine:** unify the shared
`REFRESH_LOCK` semaphore (else the single-use-token race returns).

### Low-regret defaults (recorded, override anytime)
- **Other corvus state → CorvusState:** `BrpRegistry`, `ticket_caches`, `stats_cache`,
  `WorktreeLinkRegistry`, the fs_git module status cache.
- **Config delivery:** push a typed snapshot onto CorvusState, event-driven re-push on settings
  change (also fixes the latent `git_program` staleness).
- **Transport:** commit to framed-JSON-over-stdio; **delete the dead `tarpc` dep + aspirational docs**;
  add shared typed request/response structs to catch FE↔BE drift at compile time. Binary codec
  deferred until a measured perf need.
- **Lifecycle:** respawn corvus-be on crash with in-process loopback fallback during the gap; make
  `CORVUS_OOP` refreshable (not write-once); generalize `is_oop_method` keying per-program.
- **`commit` (vetoable `on_pre_commit`):** stays product-local in-process; split `stage` and move the
  ~10 pure handlers OOP, leave `commit`.
- **Cutover:** incremental, prerequisite-first; never big-bang.

### Revised Wave 0 (reflecting ADR-2)
- **W0a — host relocation into corvus-be** (replaces "build corvus post_hooks"). Largest prerequisite;
  carries the cross-product-hook routing sub-question above. Until it lands, hook-bearing domains stay
  in-process (not advertised OOP).
- **W0b — config push to CorvusState** (unchanged).
- **W0c — transport/lifecycle hardening** (unchanged, optional/parallel).
