# Arbor Model-D Migration — Battle Plan

> Generated from a full per-command analysis of the migration surface (~623
> `#[tauri::command]` across ~69 files), classified by 13 parallel scouts +
> synthesis. Drives the multi-agent execution waves. See
> [`ipc-design.md`](ipc-design.md), [`corvus-be-bringup.md`](corvus-be-bringup.md),
> [`migration-roadmap.md`](migration-roadmap.md).

## 1. Surface summary

**Totals by product** (command count, counting expanded clusters):

| Product | ~Commands | Bulk location | Migration intent |
|---|---|---|---|
| **corvus** | ~190 | branch/stage/merge/rebase/diff/graph/gitflow/submodule/worktree/repo/recovery/git-cli + MR/security/provider/pipeline/issues/cloud/auth | → `corvus-be` (OOP, the main event) |
| **platform** | ~135 | config (44), workspace/jobs (33), plugin/marketplace (60), terminal/fs (35) | → stateless `platform` product (most leaf-clean) |
| **studio** | ~74 | ron-studio (25), studio-index (12), studio-format (36) + 12 explorer-window | → `studio-be` (zero cross-cutting deps) |
| **nemus** | ~43 | audio/eval/transport/import/packs/state | **Stays shell-hosted** (no backend planned) |

**By verdict** (across all buckets, approximate):

| Verdict | Count | Meaning |
|---|---|---|
| `leaf-clean` | ~230 | AppState-only or stateless, no emit/hook/cred/real-async → move NOW |
| `needs-emit` | ~50 | Fires `arbor://*` events → needs EventSink seam (already built) |
| `needs-broker` | ~60 | keyring/OAuth via `maybe_refresh_for_*` → blocked on M3 |
| `needs-plugin-host` | ~25 | locks Lua host / vetoable hook → host stays shell-side |
| `needs-async-seam` | ~15 | real reqwest/await → needs backend Tokio runtime |
| `keep-in-shell` | ~30 | irreducible Tauri/Win32 glue (window, opener, global-shortcut, PTY) |

**Prose read.** The bulk is *leaf-clean and ready today* — roughly 230 commands have no seam dependency at all. The single dominant blocker is the **M3 credential broker**: it gates ~60 commands that are otherwise the most valuable cross-product surface (all of MR, security, provider-connect, issues Linear/Jira, cloud secrets, CI/CD fetch in pipeline, avatar/image proxies). Everything that calls `crate::auth::maybe_refresh_for_provider()` / `credential_store::*` is dark until the broker lands. The second blocker is the **vetoable `on_pre_commit` hook**, which strands exactly one high-traffic command (`commit`) and a couple of plugin-host dispatchers. The EventSink seam is **already production-proven** (`CorvusState` holds `Arc<dyn EventSink>`; ~241 emit call-sites already abstracted), so `needs-emit` is not really a blocker — it's a tag meaning "instantiate the sink, don't touch call-sites." Studio is the cleanest standalone product (zero credential/hook/plugin coupling); nemus is explicitly out of scope.

---

## 2. Per-product domain inventory

### corvus

Reuse column references existing crates under `crates/corvus/`.

| Domain group | #cmds | state | git | creds | hooks | verdict mix | reuse (crate) |
|---|---|---|---|---|---|---|---|
| **branch** (`branch_commands.rs`) | 21 | appstate | git2 | no | branch_create/delete/rename, checkout | 9 leaf / 12 needs-emit | `corvus/git` |
| **stage** (`stage_commands.rs`) | 11 | appstate | git2 | no | VETO:on_pre_commit, on_commit | 10 leaf / 1 **needs-plugin-host** (`commit`) | `corvus/git` |
| **merge** (`merge_commands.rs`) | 9 | appstate | git2/cli | no | — | 9 leaf | `corvus/git` |
| **rebase** (`rebase_commands.rs`) | 7 | appstate | cli | no | rebase_start/abort | 5 leaf / 2 needs-emit | `corvus/git` |
| **remote** (`remote_commands.rs`) | 6 | appstate | git2/both | yes (3) | fetch/push/pull | 1 leaf / 2 keep-shell / 3 **needs-broker+async** | `corvus/git` |
| **diff** (`diff_commands.rs`) | 11 | appstate | git2 | no | — | 9 leaf / 2 needs-emit | `corvus/git` |
| **graph** (`graph_commands.rs`) | 10 | appstate | git2 | no | — | 7 leaf / 3 needs-emit | `corvus/git` |
| **gitflow** (`gitflow_commands.rs`) | 15 | appstate | git2 | no | 7 flow hooks (f&f) | 15 leaf | `corvus/git` |
| **submodule** (`submodule_commands.rs`) | 8 | appstate | git2 | no | — | 8 leaf | `corvus/git` |
| **worktree** (`worktree_commands.rs`) | 11 | appstate | git2/none | no | — | 10 leaf / 1 needs-emit | `corvus/git` |
| **linked-worktree** (`linked_worktree_commands.rs`) | 14 | appstate | none | no | member_add/remove | 3 leaf / 11 needs-emit | `corvus/git` |
| **search** (`search_commands.rs`) | 1 | appstate | git2 | no | — | 1 leaf | `corvus/git` |
| **repo** (`repo_commands.rs`) | 9 | mixed | git2/cli | yes (1) | repo_open/close/init | 5 leaf / 3 needs-emit / 1 **needs-broker** (`init_repo`) | `corvus/git` |
| **repo-browser** (`repo_browser_commands.rs`) | 5 | appstate | none | yes | — | 5 **needs-broker** | `corvus/git-provider` |
| **recovery** (`recovery_commands.rs`) | 4 | appstate | git2 | no | — | 4 leaf | `corvus/git` |
| **git-cli** (`git_cli_commands.rs`) | 7 | mixed | cli | no | — | 6 leaf / 1 needs-emit | `corvus/git` |
| **provider-connect** (`provider_commands.rs`) | 9 | none | none | yes | — | 1 leaf / 8 **needs-broker** | `corvus/git-provider` |
| **mr** (`mr_commands.rs`) | 19 | appstate | none/git2 | yes | mr_opened/merged/updated | 1 needs-emit / 18 **needs-broker** | `corvus/git-provider` |
| **security** (`security_commands.rs`) | 4 | appstate | none | yes | security_summary | 4 **needs-broker** | `corvus/git-provider` |
| **brp** (`brp_commands.rs`) | 5 | appstate | none | no | — | 5 leaf | `corvus/brp` |
| **issues** (`issues_commands.rs`) | 19 | none | none | yes | — | 3 leaf / 16 **needs-broker** | `corvus/issue-tracker` |
| **cloud** (`cloud_commands.rs`) | 24 | mixed | none | yes (4) | — | 4 leaf / 8 needs-emit / 8 needs-async / 4 **needs-broker** | `arbor-cloud` |
| **auth** (`auth_commands.rs`) | 6 | none | none | yes | — | 6 **needs-broker** | `arbor-auth` |
| **tickets** (`ticket_commands.rs`) | 9 | appstate | none | no | — | 9 leaf | `corvus/git` |
| **avatars/images** (`avatar_commands.rs`, `image_commands.rs`) | 2 | mixed | none | yes | — | 2 **needs-broker** | `corvus/git-provider` |
| **pipeline-local** (`pipeline_commands.rs`) | 11 | appstate | none | no | pipeline_* (f&f) + on_pipeline_run_request | 5 leaf / 4 needs-emit / 1 needs-plugin-host / 1 needs-emit | `corvus/pipeline`, `corvus/pipeline-api` |
| **pipeline-ci** (`pipeline_commands.rs`) | 6 | appstate | none | yes | — | 6 **needs-broker** | `corvus/pipeline` |

**nemus (separate track, no backend).** All 43 commands run in-process on the shell; the audio thread is spawned *inside* the shell with no IPC seam, there is no `nemus-be` planned, and there is zero credential coupling. Only real-async commands are `nemus_download_model`, `nemus_sync_libraries`, `nemus_pack_download` (reqwest). `open_nemus_window` is irreducible Tauri glue. **Verdict: do not migrate to Model-D — classify-only for wave awareness.** The `crates/nemus/*` crates already hold the pure pattern/lang/audio engine; the shell commands are thin.

**studio (separate later product, clean).** 62 data-plane commands (`ron_studio_commands.rs` 25, `studio_commands.rs` 12, `studio/format/commands.rs` 36) are all `leaf-clean` except `studio_refresh_index` (`needs-emit`, already AppHandle-emit). Zero credentials, zero hooks, zero plugin-host, no async seam (format backends are in-process traits). The 12 `explorer_window.rs` commands are irreducible Tauri glue (window/global-shortcut/DPI drag overlay) and **stay in shell**. Studio can be the first *separate product* extracted after corvus with no unresolved cross-cutting dependency.

---

## 3. Cross-cutting seams (the prerequisites)

### MILESTONE-ZERO — Credential Broker (M3)
**Gates the largest blocked chunk: ~60 commands** (all MR 19, security 4, provider-connect 8, issues 16, auth 6, cloud-secret/gcs 4, pipeline-ci 6, avatar/image 2, repo-browser 5, init_repo, remote fetch/push/pull 3). This is the explicit critical path.

- **Current status (CORRECTED after lead investigation — better than the scouts estimated):** the keyring-free `SessionProvider` contract (`crates/foundation/ipc/src/credential.rs`) AND **all four shell-side adapters already exist and are complete**: `GithubSessionProvider` / `GitlabSessionProvider` (`src-tauri/src/git_provider/session.rs`) and `LinearSessionProvider` / `JiraSessionProvider` (`src-tauri/src/integrations/token_source.rs`) — each composes the keyring read + the provider OAuth refresh into `{ base_url, auth_header, web_base }`. `CredentialBroker` (`crates/foundation/shell-common/src/broker.rs`) is the keyring TTL-cache primitive. The **issue-tracker crates already consume** `SessionProvider` end-to-end (`send_with_refresh`); the **git-provider crate does not yet** — its commands still rely on the shell pre-refreshing via `crate::auth::maybe_refresh_for_provider()` (~30 call-sites in mr/security/repo-browser/pipeline) + `maybe_refresh_for_url` (remote fetch/push/pull).
- **What actually remains (3 pieces, NOT a from-scratch broker):**
  1. **OOP SessionProvider IPC bridge** — the genuinely-new architectural seam: shell-side `__session(account)` / `__refresh(account)` handlers that call the real adapter, + a `ChildSessionProvider` (in `arbor-ipc`/transport) that a headless backend holds as its `Arc<dyn SessionProvider>` and which round-trips to the shell. This keeps the keyring shell-only across the process boundary. (In-process, the shell injects the adapter directly — already possible.)
  2. **git-provider crate consumes `SessionProvider`** end-to-end (like the issue-trackers do): its REST client calls `.session()` and, on 401/403, `.refresh()` — instead of the shell pre-refreshing.
  3. **Remove the `maybe_refresh_for_provider` / `maybe_refresh_for_url` pre-refresh** once (2) self-refreshes (~30+ call-sites, all in the Wave-5 command files — removed as those domains migrate, not a separate sweep).
- **Unblocks:** the entire provider/MR/security/issues/auth/cloud-creds/pipeline-ci cluster — i.e. Waves 5–6. The only NEW infra is piece 1 (the IPC bridge); pieces 2–3 ride along with the Wave-5 domain migrations.

### EventSink (Event egress) — READY
- **Current status:** Production-proven. `EventSink` trait (`crates/foundation/ipc/src/event.rs`), object-safe (`emit(topic, payload)`). `TauriEventSink` (`src-tauri/src/ipc/event_sink.rs`) forwards to `AppHandle::emit`. `CorvusState` already holds `Arc<dyn EventSink>`; ~241 emit call-sites already abstracted (`state.emit(...)`, no `AppHandle`).
- **What must be built:** Nothing structural. When `corvus-be`/`studio-be` launch, instantiate `CorvusState` with an `EventSink` wrapping the IPC event channel. Call-sites don't change.
- **Unblocks:** every `needs-emit` domain — branch/linked-worktree/diff/graph/worktree/rebase/repo/git-cli/workspace/cloud-stream/studio_refresh_index/pipeline-local. These ride along *in the same wave* as their leaf-clean siblings.

### Plugin host + vetoable hooks
- **Current status:** Fire-and-forget is **READY** and routing-independent — the generic `rpc` command calls `post_hooks::fire()` after dispatch, in-process or OOP (stash/bisect/notes/reflog/reset already prove this in `src-tauri/src/ipc/corvus/`). Vetoable (`on_pre_commit`) is **BLOCKED**: no async plugin round-trip seam exists.
- **What must be built (vetoable):** an async veto round-trip from backend → shell plugin host → backend, OR move the plugin host per-product (Phase 6+). The Lua API installers (`crates/plugin/*`, `src-tauri/src/plugin/mod.rs` — a ~30-line shim) are nearly ready to relocate.
- **Unblocks:** `commit` (the only true vetoable command) and the ~18 plugin-marketplace dispatchers that lock the host (`exec_hook`, `fire_command`, enable/disable cascades, contributions/containers). These are **last** (Wave 7) or stay in shell indefinitely.

### Async-real (backend Tokio runtime)
- **Current status:** Most "async" git commands are spawn_blocking (async-by-convention) and need **no seam** — the `rpc` command already wraps dispatch in spawn_blocking. ~15 commands do real reqwest/await (marketplace network, cloud single-ops, nemus downloads).
- **What must be built:** backend holds its own Tokio runtime (or the transport is async-capable — tarpc is). Trivial once the backend process exists.
- **Unblocks:** marketplace network ops, cloud `list/stat/delete/copy/concat`. Independent of broker for the no-creds subset.

---

## 4. Shared-file hotspots

Every migration touches a small set of aggregation files. The rule: **inventory-auto files are conflict-free and any agent may add to them; serialized files must be merged by the lead integrator only.**

| File | Edit type | Conflict rule |
|---|---|---|
| `src-tauri/src/ipc/corvus/mod.rs` | add `pub mod <domain>;` | **Inventory-auto.** `#[corvus::handler]` self-registers via `inventory::submit!`. No central match/enum. Agents add their own `pub mod` line — different lines, merge-free. |
| `crates/foundation/rpc/src/lib.rs` | `#[handler]` macro emits `inventory::submit!(Entry{...})`; `registry()` collects | **Inventory-auto by design.** Never hand-edit a central list. |
| `src-tauri/src/ipc/corvus/post_hooks.rs` | add domain hooks to the `match method { … }` block | **SERIALIZE (lead only).** Hottest spot — two agents editing the match conflict. Lead merges all post-hook wiring after agents finish their handlers. |
| `src-tauri/src/error.rs` | add `AppError` variants | **SERIALIZE (lead only).** Small enum; only conflicts on concurrent same-line edits. Lead owns. |
| `src-tauri/src/ipc/mod.rs` (`build_router`) / `split_broker.rs` | router/broker wiring at the loopback↔child flip | **SERIALIZE (lead only).** Structure is stable; lead handles the flip. |
| `crates/corvus/core/src/state.rs` / `lib.rs` | grow `CorvusState` fields as registries move in | **SERIALIZE (lead only).** Lead adds fields; agents consume them. |
| `crates/corvus/be/src/main.rs` | register handler modules, instantiate `CorvusState` with real `EventSink`/`SessionProvider` | **SERIALIZE (lead only).** Single integration point for OOP flip. |
| `src-tauri/src/commands/mod.rs` + `lib.rs` invoke_handler! | remove old `#[tauri::command]` defs | **SERIALIZE (lead only).** Lead removes shims after the handler is proven. |

**Agent contract:** each agent writes only its `crates/corvus/git/src/<domain>.rs` (pure logic, already mostly extracted) + its `src-tauri/src/ipc/corvus/<domain>.rs` handler file, adds one `pub mod` line to `corvus/mod.rs`, and **stops**. The lead serializes post_hooks/error/router/state/main edits and removes old command shims. This keeps agent file-sets disjoint.

---

## 5. Wave plan

Build-serialization reality: the workspace target is shared (~140 GB, ~5 min full builds). **Each agent builds only its own crate** (`cargo build -p corvus-git` style) — never the workspace, never `src-tauri`. The lead does the one expensive `src-tauri` rebuild per integration step. Agents within a wave touch disjoint files so they parallelize cleanly.

---

### WAVE 0 — Seam landing (lead only, serial) — **PREREQUISITE FOR EVERYTHING WITH EMIT**
- **Theme:** Confirm EventSink instantiation path in a future `corvus-be`/`studio-be` skeleton; wire the post_hooks generic path (already proven by stash). No new seam needed for fire-and-forget or emit — this wave is *validation + skeleton*, not new infra.
- **Domains:** none (infra).
- **Who:** lead.
- **Blocker cleared:** none. **Output:** `corvus-be/src/main.rs` skeleton instantiating `CorvusState` with `TauriEventSink` in-process so OOP and in-process share one path.

### WAVE 1 — Quick wins, pure git read/mutation (FULLY PARALLEL) — fire immediately
- **Theme:** leaf-clean git domains, zero seams beyond what stash already uses.
- **Domains (disjoint files, one agent each):**
  - `merge` (`merge_commands.rs` → `ipc/corvus/merge.rs`) — mechanical
  - `submodule` (`submodule_commands.rs` → `submodule.rs`) — mechanical
  - `recovery` (`recovery_commands.rs` → `recovery.rs`) — mechanical (logic already in `corvus/git`)
  - `search` (`search_commands.rs` → `search.rs`) — mechanical
  - `diff` (leaf-clean 9 of 11 → `diff.rs`) — mechanical; defer the 2 emit ones to Wave 2
  - `gitflow` (`gitflow_commands.rs` → `gitflow.rs`) — mechanical; fire-and-forget flow hooks via post_hooks
  - `tickets` (`ticket_commands.rs` → `tickets.rs`) — mechanical, AppState-only
  - `git-cli` (6 leaf of 7 → `git_cli.rs`) — mechanical
  - `brp` (`brp_commands.rs` → corvus/brp handler) — mechanical
- **All agent-parallelizable** (mechanical extraction). **Blocker:** none.
- **Integration (lead):** add the 9 `pub mod` lines (auto), wire gitflow fire-and-forget hooks into `post_hooks.rs`, remove old command shims, one `src-tauri` rebuild.

### WAVE 2 — Emit-bearing git domains (PARALLEL, rides EventSink) — overlaps Wave 1
- **Theme:** `needs-emit` domains; EventSink is ready so these are still mechanical.
- **Domains:** `branch` (12 emit cmds), `linked-worktree` (11 emit), `worktree` (10 leaf+1 emit), `rebase` (2 emit), `graph` (3 emit incl. `export_graph_svg`/`start_file_meta_scan`), `diff`-stream (the 2 deferred), `repo` (open/close/clone emit, **excluding** `init_repo`), `git-cli` download.
- **All agent-parallelizable.** Each agent's handler calls `state.emit(...)` (already abstracted). **Blocker:** EventSink (ready). The worktree-link registry is *not* in AppState — agent passes it as a seam field on CorvusState (lead adds the field).
- **Integration (lead):** add `pub mod` lines, wire branch/linked-worktree/repo fire-and-forget hooks into `post_hooks.rs` (serialize), add `linked_worktrees` registry field to `CorvusState`.
- **Overlap note:** Wave 1 and Wave 2 can run **simultaneously** — their file-sets (`merge/submodule/...` vs `branch/worktree/...`) are disjoint. The only shared touch is `post_hooks.rs`, which the lead serializes at integration.

### WAVE 3 — Platform leaf cluster (PARALLEL, different product, fully disjoint from corvus)
- **Theme:** stateless/AppState platform config + jobs + workspace + plugin-leaf + terminal/fs-leaf.
- **Domains:**
  - `config` 49 leaf (`config_commands.rs`) — except `set_explorer_config` (keep-shell) — mechanical
  - `theme` 5, `session` 2, `app`-leaf — mechanical
  - `workspace` leaf (~15) + `workspace` emit (~16, rides EventSink) + `jobs` (6) + `scheduler` (1) — mechanical
  - `terminal` leaf (write/resize/close/list/exec/default_shell/builtin) + emit (create/shell-detection) — mechanical
  - `fs_*` leaf (~25, pure `arbor-fs`/`foundation/fs` wrappers) + `fs_copy/move/duplicate` emit — mechanical
  - plugin-leaf (settings, logs, toolchains, templates, marketplace-installed/remove-source, boot_state) — mechanical
- **All agent-parallelizable.** **Blocker:** none (EventSink ready). **keep-in-shell exclusions:** `get_app_info`, `set_explorer_config`, `frontend_ready`, `fs_open_default/reveal/terminal/properties/wallpaper/icon/watch_*`, scheduler-rearm marketplace setters, `dispatch`.
- **Integration (lead):** platform product `Router` extension + `BrokerClient` for platform; this is a *separate product flip* and can proceed in parallel with corvus because no shared corvus files are touched.
- **Overlap:** runs **fully parallel with Waves 1–2** (corvus vs platform, disjoint trees).

### WAVE 4 — Studio product (PARALLEL, standalone, zero cross-cutting deps)
- **Theme:** extract `studio-be` — the cleanest separate product.
- **Domains:** `ron_studio` (25), `studio-index` (11 leaf + `studio_refresh_index` emit), `studio-format` (36). **Exclude** the 12 `explorer_window.rs` commands (keep-shell).
- **All agent-parallelizable.** **Blocker:** none. **Integration (lead):** new `studio-be` handler registry; format backends stay in-process traits (no async seam).
- **Overlap:** parallel with Waves 1–3. Studio lives in `src-tauri/src/studio/`; lead extracts to a `studio-*` crate. Disjoint from everything.

### ⛔ BLOCKER GATE — land Credential Broker (M3) before Waves 5–6
Lead-only, serial. Build the `SessionProvider`-over-`CredentialBroker` adapter + IPC publish. Re-point `maybe_refresh_for_provider()` / `credential_store::*` at the injected provider. **Nothing in Waves 5–6 can start until this merges.** This is milestone-zero for the blocked 60.

### WAVE 5 — Provider & MR & security (PARALLEL, post-broker)
- **Theme:** the credential-coupled `corvus/git-provider` cluster.
- **Domains:** `provider-connect` (8), `mr` (19, incl. `create_mr`/`merge_mr` emit + fire-and-forget hooks), `security` (4), `avatars`/`images` (2), `repo-browser` (5), `remote` fetch/push/pull (3), `init_repo` (1).
- **Agent-parallelizable** once broker lands; each needs the async runtime (Wave 0 backend Tokio) + SessionProvider. **Blocker:** M3 (cleared at gate). **Integration (lead):** wire mr_opened/merged/updated + security_summary fire-and-forget hooks; serialize post_hooks.

### WAVE 6 — Issues, cloud, auth, pipeline-CI (PARALLEL, post-broker)
- **Theme:** remaining credential cluster across `corvus/issue-tracker`, `arbor-cloud`, `arbor-auth`, `corvus/pipeline`.
- **Domains:** `issues` Linear+Jira (16), `auth` (6), `cloud` secrets/gcs-oauth (4) + cloud async-real ops (8, no-creds, need backend Tokio) + cloud-stream emit (8), `pipeline-ci` (6). Pipeline-local leaf is Wave 1-eligible (can pull forward); `request_pipeline_run` (needs-plugin-host → Wave 7).
- **Agent-parallelizable.** **Blocker:** M3 + backend Tokio. **Integration (lead):** cloud cancellation flags must cross the process boundary — lead designs the inter-process cancel protocol (the `cloud_cancellations` AtomicBool registry).

### WAVE 7 — Vetoable hooks & plugin-host dispatchers (LEAD-DELICATE, last)
- **Theme:** the irreducibly host-coupled tail.
- **Domains:** `commit` (VETO `on_pre_commit`), plugin-marketplace dispatchers (~18: `exec_hook`, `fire_command`, enable/disable cascades, contributions/containers, `set_active_tab`, marketplace install/uninstall), `request_pipeline_run`, `dispatch` (plugin→git bridge).
- **Lead-only, delicate.** Requires the async veto round-trip seam OR plugin-host-per-product (Phase 6+). Many of these **may never leave the shell**; `commit` waits for the veto seam. **Blocker:** vetoable-hook seam.

**Overlap summary:** Waves **1, 2, 3, 4 all run concurrently** (corvus-git / platform / studio are disjoint trees; only `post_hooks.rs` is a shared lead-serialized merge point). Waves **5, 6 run concurrently** after the broker gate. Wave 7 is last and mostly lead-driven.

---

## 6. Quick wins (fire today, zero new seams)

These are `leaf-clean`, AppState-only or stateless, no creds/emit/vetoable-hook — exactly the profile stash/bisect/notes/reflog/reset already shipped under `src-tauri/src/ipc/corvus/`. Spin one agent per file:

1. **`merge`** — 9 cmds, `merge_commands.rs`
2. **`submodule`** — 8 cmds
3. **`recovery`** — 4 cmds (logic already in `corvus/git`)
4. **`search`** — 1 cmd
5. **`gitflow`** — 15 cmds (fire-and-forget flow hooks ride the proven post_hooks path)
6. **`tickets`** — 9 cmds, AppState + regex only
7. **`brp`** — 5 cmds, registry-only
8. **`diff`** (9 of 11), **`graph`** (7 of 10), **`git-cli`** (6 of 7) — the non-emit subset
9. **Platform:** `config` (49), `theme` (5), `session` (2), `jobs` (6), `scheduler` (1), `fs_*` pure-`arbor-fs` wrappers (~25), terminal leaf, plugin settings/logs/toolchains/templates
10. **Studio:** the entire `ron_studio` (25) + `studio-format` (36) + `studio-index` leaf (11) — a whole standalone product with no blocker

These alone clear ~180 commands off the shell before the credential broker is even started. Fire them as Wave 1 + Wave 3 + Wave 4 in parallel; the lead serializes only `post_hooks.rs`, `error.rs`, `CorvusState`, and the old-shim removal.

---

## Open notes

- The `post_hooks.rs` `match method { … }` block is the **one true serialization point** for hook wiring — the agent brief must say "do NOT touch post_hooks.rs; the lead wires hooks" so two parallel agents don't both edit it.
- `src-tauri/src/ipc/corvus/` already contains `bisect/notes/reflog/reset/stash/stats` — new domains slot in identically.
- Path-name normalization: keep `docs/migration-roadmap.md` aligned with the real tree (`crates/corvus/{be,core,git,git-provider,...}`, `crates/foundation/{ipc,rpc,shell-common}`) so agents don't guess.
