# Plugin Relocation Inventory — moving the Lua host into the product backends

> **Status:** planning doc, drives a multi-agent fan-out (same role `docs/migration-inventory.md` played for the command migration).
> **Decided direction:** the mlua plugin host moves OUT of the shell/launcher and INTO each running product backend (IntelliJ model: plugins load on the product, not the launcher). The launcher becomes a thin router that owns only *global* signals.
> **Source:** synthesis of six parallel scout reports (runtime-core, API-surface, hooks, manager-UI, global-events, landing-zone). File:line cites are from those scouts; the tree layout (16 `ns_shell` namespaces, `ipc/{corvus,platform,studio}` broker split, dual `post_hooks.rs`) was re-verified against the working tree.

---

## 1. Executive summary

### The goal
Today `PluginHost` (mlua, one `Lua` VM per loaded plugin) lives in the shell binary, owned by `AppState.plugin_host: Arc<Mutex<PluginHost>>` (`src-tauri/src/lib.rs:109`) and reached via `AppState::lock_plugin_host()`. As the command surface migrates out-of-process into `corvus-be` (and later `merula-be`/`sitta-be`), the host is being stranded: the in-process handler that *used* to fire a hook is bypassed when the method runs OOP. Relocating the host into the product backend that owns the domain (git→corvus) **co-locates the host with the handlers and the hooks**, which is what the per-program-plugins direction requires anyway.

### Why this dissolves two architectural seams (not "requires new ones")

1. **The `post_hooks` seam is an artifact of the stranded host.** `src-tauri/src/ipc/corvus/post_hooks.rs` (+ `platform/post_hooks.rs`), fired generically from `rpc_commands.rs:51-52` after `dispatch_rpc`, exists *only* to re-derive ~25 hook payloads from `(params, result)` and fire them shell-side because the shell owns the host (`corvus/post_hooks.rs:1-19`). It **cannot** capture pre-mutation state and **cannot** veto (header comment `corvus/post_hooks.rs:12-14`). Once the host is inside `corvus-be`, the handler fires its own hook **inline, in-process, with first-hand data** — `post_hooks.rs` collapses back into the handlers and is **deleted**, not ported. Porting it OOP would entrench the smell.

2. **The vetoable `on_pre_commit` problem evaporates.** It's the only vetoable hook (`lib.rs:89-92`, `HookKind::Vetoable`), fired by `fire_vetoable_blocking` in the still-inline `commit` command (`stage_commands.rs:30-39`). `commit` is the one stage-domain op *not* migrated precisely because the broker has no vetoable round-trip seam (`stage_commands.rs:8-14`, `ipc/corvus/stage.rs:13-16`). With the host inside `corvus-be`, the commit handler calls `fire_vetoable_blocking` in-process before the mutation — **no cross-process round-trip, no new infrastructure**. The hardest blocker becomes trivial.

### Headline scope numbers
- **4 plugin crates already Tauri-free** and move/stay as-is: `arbor-plugin-{types,api,core,marketplace}`. `arbor-plugin-core` has **zero** Tauri deps (`crates/plugin/core/Cargo.toml:10-43`).
- **2 decoupling seams**, both already dependency-inverted: `AppCtx` (capability trait, `crates/core/src/app_ctx.rs:22-93`) and `LuaApiInstaller` (`sandbox.rs:71-109`).
- **1 Tauri-coupled rind** to rewrite: `TauriAppCtx` + 16 `ns_shell/*` namespaces + boot wiring + `post_hooks.rs`.
- **~23 host-pure `arbor.*` namespaces** relocate for free (speak only `AppCtx`); **16 `ns_shell` namespaces** are the relocation backlog (each does `ctx.app_handle()` downcast — the downcast count IS the backlog).
- **49 catalog hooks**: ~40 are **product-local** (trivial inline once co-located), the rest split global-broadcast / launcher-ui-roundtrip / cross-product. **2 net-new hooks** proposed (`on_window_focus`, `on_deep_link`).
- **1 master kill-switch**, **3 settings stores**, **1 marketplace ledger** — storage that may need per-product scoping.

---

## 2. What physically moves

### 2.1 Stays put — already clean shared libraries (no new crate, no move)

| Crate | Path | Role | Tauri dep |
|---|---|---|---|
| `arbor-plugin-types` | `crates/plugin/types` | Manifest, permissions, `hook_catalog.rs`, schedule shapes | No |
| `arbor-plugin-api` | `crates/plugin/api` | `PluginCtx`, `PluginValue`, `HookDispatcher`/`HookListener`, `HookKind` | No |
| `arbor-plugin-core` | `crates/plugin/core` | The mlua host: `PluginHost`, sandbox, lifecycle, hook router, host-pure `arbor.*` | No |
| `arbor-plugin-marketplace` | `crates/plugin/marketplace` | Catalog/install/ledger | No |

**`arbor-plugin-core` stays product-agnostic and is linked by every `*-be`.** This resolves the "is it corvus-specific?" tension: keep core generic; push product-specific bits (e.g. the `host_command_required` `arbor:git.*` allowlist, `command.rs:180-200`) out of core into the product (see Open Decision D5).

### 2.2 The host runtime (lives in core, relocates by re-pointing `AppCtx`)

The entire `crates/plugin/core/src/` runtime moves with the product host **unchanged**: `PluginHost` (`runtime/host/mod.rs:29-68`), `LoadedPlugin` (one `mlua::Lua` per plugin, `runtime/loaded.rs:81-99`), `create_sandbox` (`sandbox.rs:111-222`), the full lifecycle state machine (`runtime/host/lifecycle.rs`), `hook_router.rs`, service/command/pipeline-op dispatch (`runtime/host/{service,command,pipeline_op}.rs`), dep cascade (`dep_cascade.rs`), manifest discovery + topo-sort + state persistence (`runtime/manifest/mod.rs`), scheduler bridge (`runtime/scheduler/`). None reference Tauri.

### 2.3 The Tauri-coupled rind (must be rewritten per-product / relocated)

| Artifact | Path | Coupling | Disposition |
|---|---|---|---|
| `TauriAppCtx` | `src-tauri/src/app_ctx.rs:18-106` | `AppHandle` + `AppState` (the **only** thing touching both) | Replace per-product with `CorvusBeAppCtx` over the BE's `EventSink`; UI/global capabilities become RPC-to-shell |
| `ApiCtxExt::app_handle()` downcast | `src-tauri/src/plugin/ns_shell/ctx_ext.rs:23-30` | `as_any()` → `TauriAppCtx` | Dies as each `ns_shell/*` migrates |
| 16 `ns_shell/*` namespaces | `src-tauri/src/plugin/ns_shell/*` | `crate::git*`, `crate::pipeline`, `crate::cloud`, `crate::git_provider`, `tauri::Emitter` | Move into product backends next to their domain logic |
| `api_installer.rs` | `src-tauri/src/plugin/api_installer.rs:34-39` | bridges `shell_installers()` | Becomes per-product `installers()` |
| `plugin_host_commands.rs` | `src-tauri/src/plugin_host_commands.rs` | shell-side `arbor:*` built-in dispatch | Splits: git verbs → product-local; UI verbs → launcher-ui-roundtrip |
| Boot/composition wiring | `src-tauri/src/lib.rs:82-101,412,644-857` | `app.state`, async_runtime, `build_hook_dispatcher` | Moves into product backend setup |
| Hook-firing seam | `ipc/corvus/post_hooks.rs` + `ipc/platform/post_hooks.rs` + `rpc_commands.rs:51-52` | shell `AppState.fire_hook` | **Deleted** once host co-located; hooks fire inline |

### 2.4 Target landing zone

```
crates/corvus/be/src/plugin/
  mod.rs        ← owns Arc<Mutex<PluginHost>> + HookDispatcher + LuaHookListener
                  (the build_hook_dispatcher body lib.rs:82-101 lifted verbatim);
                  owns reload / start_all_schedulers
  app_ctx.rs    ← CorvusBeAppCtx: impl arbor_core::AppCtx over the BE's EventSink
                  (CorvusState already holds Arc<dyn EventSink>, crates/corvus/core/src/state.rs:17-30)
  ns/           ← the git-coupled ns_shell namespaces, re-expressed against CorvusState
```

`corvus-be` already crosses the process boundary via `FrameEventSink` (`crates/corvus/be/src/main.rs:69`), which `AppCtx::emit` reuses with no Tauri dependency. Use `ipc/studio/mod.rs` as the *shape* template for a clean product backend module (zero coupling), not for the host itself.

---

## 3. `arbor.*` API surface map — the classification table

**Categories:** `PL` product-local · `UI` launcher-ui-roundtrip · `GB` global-broadcast · `XP` cross-product.
**Coupling:** `AppCtx` = via trait, relocates with host · `AppHandle` = downcast backlog · `VM` = pure VM globals.

### 3.1 Host-pure namespaces (`crates/plugin/core/src/lua_api/ns/`) — relocate for free with the host

| Namespace.fn | Touches | Cat | Coupling | Relocation note |
|---|---|---|---|---|
| `log.debug/info/warn/error` | logs | UI/PL | `AppCtx.record_plugin_log` | log buffer travels with host; entries stream FE via push channel |
| `events.on` | runtime | PL | VM | pure |
| `events.emit` | runtime | PL / XP | `host_weak` | XP only if cross-plugin across products |
| `service.export/unexport/list_own` | runtime | PL / XP | VM globals | |
| `service.call / list` | runtime | XP | `host_weak.invoke_service` | XP if target plugin on another product |
| `json.encode/decode` | — | PL | none | |
| `text.replace/contains/find_all/escape` | — | PL | none | |
| `fs.*` (read/write/structured-edit/glob/join) | fs | PL | perm snapshot via `__arbor_current_repo__` | scope is product-local repo ctx |
| `meta.plugin_name/api_version/app_version/plugin_dir/os/plugin_loaded` | state | PL | `host_weak` | |
| `meta.is_app_focused` | — | **GB** | `AppCtx.is_focused` | focus broadcast from launcher |
| `settings.global/project` (r/w) | fs/config | PL | `AppCtx.active_repo_path` (project) | |
| `settings.read/read_project` | fs/config | PL / XP | `AppCtx.active_repo_path` | XP = reading another product's plugin |
| `http.get` | network | PL | `host_weak` | |
| `timer.after/every/cancel` | runtime | PL | `host_weak` | |
| `scheduler.register/list` | runtime/config | PL | `ScheduleRegistry` | `only_when_focused` → **GB** dep |
| `keybinding.register` | ui/registry | UI | `ContributionRegistry` | |
| `command.register/unregister` | ui/registry | UI | `ContributionRegistry` | |
| `command.fire` | runtime | PL / XP | `host_weak.invoke_command` | |
| `hooks.list/describe` | — | PL | none (static catalog) | |
| `contribution.list/list_points` | registry | PL | `ContributionRegistry` | |
| `notify(cfg)` | ui | UI | `AppCtx.emit` (window-routed via `target`) | |

### 3.2 `arbor.ui.*` cluster (`ns/ui/`) + `*_studio` — **all UI** (backend→shell requests)

Every function emits a `plugin:*`/`arbor://*` event to the shell webview via `AppCtx.emit`, or writes the `ContributionRegistry` (fires `arbor://contributions-changed`). None can be served by a headless backend. Post-split the product backend `emit`s these *requests* to the launcher.

`pick_file`, `form` (+ all `.set_*`/`.patch`/`.replace`/`.close`), `confirm`, `operation.{start,set_current,update_step,finish}`, `add_context_menu_item`/`add_menu_item`/`add_toolbar_action`/`add_sidebar`/`add_view`/`set_panel_content`, `add_graph_combo`/`set_combo_options`/`add_separator`, `set_autocomplete_options`, `open_path` (OS), `copy_to_clipboard`/`open_job_output`/`open_panel`/`show_pipeline_run`, `tree.set/get`, `contribute*`/`unregister_contribution`/`contribution_point`/`list_contributions`, `settings.{panel,open,close}`, `icon.register`, `container.{register,open,close}`. **`json_studio`/`ron_studio`/`toml_studio`/`yaml_studio`/`properties_studio`.open** → all **UI** (`AppCtx.emit`).

> **`AppCtx.emit` already routes by window `target`** (`notify`, `job.spawn`, `operation.start` all accept `target`). The window-routing plumbing for the launcher-roundtrip pattern **already exists**.

### 3.3 Shell-side `ns_shell/*` namespaces — the relocation backlog (16 modules)

All `ctx.app_handle()`-downcast to reach `AppState`. They move physically into the product backends.

| Namespace.fn | Touches | Cat | Lands in | Note |
|---|---|---|---|---|
| `repo.current` | state | PL | corvus-be | reads `__arbor_current_repo__` VM global, no AppHandle |
| `repo.branch/is_dirty/remote/branches/tags/commits/untracked/staged_files` | git | PL | corvus-be | git2 direct |
| `repo.fetch_active_tab` | git/state | PL (+UI emit) | corvus-be | `arbor://graph-refresh` |
| `repo.release_handles` | state | PL | corvus-be | |
| `repo.clone` | git/jobs | PL | corvus-be | bg clone job + `on_done` |
| `job.spawn/list/cancel/dismiss/clear_finished` | process/jobs | PL | corvus-be (or platform — see D3) | window-routed via `target` |
| `workspace.list/active/get/list_repos/repo` | state | PL? / launcher | **D1** | repo-registry may be launcher-owned |
| `workspace.switch` | state | **GB** | launcher | `fire_broadcast(on_workspace_switched)` |
| `tabs.open_repo` | state/ui | UI | launcher | tab state is launcher chrome |
| `terminal.exec` | process | PL | corvus-be | perm-gated subprocess |
| `toolchain.list/active/env/detect/add/remove/set_active` | state/process | PL | platform-be | app-global JDK/node/rust registry |
| `notes.list/get/set/delete` | git | PL | corvus-be | git notes + `on_note_*` broadcast |
| `issues.search/get/lookup/transition/comment` | network/state | **XP** | corvus-be | credential-coupled → **M3 gate** |
| `issues.branch_name` | — | PL | corvus-be | pure compute |
| `pipeline.*` (define/run/resume/discard/locks/runs/ops) | state/process | PL | corvus-be | `corvus-pipeline-api` already a crate |
| `mr.list/current_user` | network/state | **XP** | corvus-be | credential-coupled → **M3 gate**; `resolve_repo_path` (`mr.rs:53-79`) shared by ci/security |
| `ci.runs` | network/state | **XP** | corvus-be | credential-coupled → **M3 gate** |
| `security.supports/summary/findings/refresh_active_tab` | network/state | **XP** | corvus-be | credential-coupled → **M3 gate**; `arbor://security-refresh` |
| `cloud.*` (secret/connection/list/search/sync/oauth/transfer/report) | network/fs/jobs/keyring | PL | platform/own product | self-contained `arbor-cloud`; **WASM-earmarked, may not relocate — D4** |
| `brp.connect/disconnect/status/call/watch/unwatch` | network/state | PL | corvus-be / own product | `corvus_brp` crate exists |
| `linked_worktrees.list/get/set_sync_enabled` | state | PL | corvus-be | `arbor://worktree-links-changed` |
| `ui.branding.set_branding/clear/set_theme_tokens/clear_theme_tokens` | ui/window/state | **GB** | launcher | theme overlay broadcast from launcher |

### 3.4 Key seams

- **`AppCtx` is a god-trait-in-waiting** — it mixes PL (`active_repo_path`, `spawn`), UI (`emit`, `record_plugin_log`, `open_path`), and GB (`is_focused`). Post-split it should arguably be **split** so a product impl doesn't fake/RPC capabilities it doesn't own (Open Decision D6). The `as_any()` downcast is documented as a smell (`app_ctx.rs:24-28`) and disappears only when all `ns_shell/*` migrate.
- **`__arbor_current_repo__`** (per-VM global, injected `hook_router.rs:180` + `lifecycle.rs:697`) is the product-context seam: product-local namespaces resolve the active repo through it, **not** via AppHandle. Maps cleanly to the product backend's own active context.
- **`invoke_host_command` must remain non-blocking** (`command.rs:116-119`, `app_ctx.rs:84-92`): caller holds the host lock; handler may re-fire hooks that re-lock. Over a process boundary this becomes async RPC — the non-blocking contract must carry over.
- **Async hazard:** the credential cluster (`issues`/`mr`/`ci`/`security`) uses ad-hoc `block_on`/throwaway `Runtime::new()` (e.g. `issues.rs:30-43`). Off the Tauri shell, replace with the product runtime / `AppCtx::spawn`.

---

## 4. Hook relocation map

Status legend: **broker** = `post_hooks.rs` · **inline** = inline `#[tauri::command]` → `state.fire_hook` · **inline-in-broker** ⚠ = fires inline inside an already-migrated `ipc/corvus/*` module (drift) · **bg** = background orchestrator/pipeline thread · **FE/Lua** = `exec_hook`/`fire_plugin_action` · **dormant** = catalog-declared, no live fire.

### 4.1 Product-local — become trivial inline fires once co-located (~40 hooks)

| Hook | Fired by | Status | Veto / pre-mut |
|---|---|---|---|
| `on_plugin_load` / `on_plugin_unload` | host lifecycle | host-internal | — |
| `on_repo_open` | corvus `post_hooks.rs:199`; inline `repo_commands.rs:240`; reload re-fire; FE | broker+inline (dual ⚠) | — |
| `on_repo_close` | `repo_commands.rs:27` | inline | — |
| `on_repo_init` | `repo_commands.rs:94` | inline | — |
| `on_project_missing` / `on_project_relocated` | corvus `post_hooks.rs:177,188` | broker | `old_path` from result |
| `on_branch_create` | corvus `post_hooks.rs:121` | broker | — |
| `on_branch_delete` (single) | `branch_commands.rs:214` | inline (+ alias cleanup) | — |
| `on_branch_delete` (bulk) | corvus `post_hooks.rs:129` | broker | deleted = params − result |
| `on_branch_rename` (local) | `branch_commands.rs:247` | inline | — |
| `on_branch_rename` (remote) | corvus `post_hooks.rs:150` | broker | — |
| `on_checkout` | `branch_commands.rs:280,313,344,377`; corvus `post_hooks.rs:158` | inline+broker (+ `maybe_trigger_checkout_sync`) | — |
| `on_tag_create` / `on_tag_delete` | `ipc/corvus/reset.rs:75,94` | **inline-in-broker** ⚠ | — |
| **`on_pre_commit`** | `stage_commands.rs:30` | **blocked-inline** (veto seam missing) | **VETOABLE + pre-mutation** → trivial in-process once co-located |
| `on_commit` | `stage_commands.rs:79` | inline (rides pre_commit) | — |
| `on_stash_push` / `on_stash_pop` | corvus `post_hooks.rs:34,46` | broker | msg/idx from result |
| `on_rebase_start` / `on_rebase_abort` | corvus `post_hooks.rs:106,114` | broker | — |
| `on_fetch` / `on_push` / `on_pull` | `remote_commands.rs:111,148,245` | inline | — |
| `on_note_saved` / `on_note_deleted` | `ipc/corvus/notes.rs:60,93` | **inline-in-broker** ⚠ | — |
| `on_flow_*` (init/feature/release/hotfix start+finish) | corvus `post_hooks.rs:64-102` | broker | base from result |
| `on_pipeline_started/step_done/done` | `pipeline/mod.rs:812,1196,979` | bg inline | — |
| `on_repo_deregistered` | `workspace_commands.rs:110,341` | inline (platform) | — |
| `on_security_finding_state_changed` | none in host | FE/Lua | **pre-mut `from_state`** |

### 4.2 Credential-gated (blocked behind M3 broker, independent of the host move)

| Hook | Fired by | Note |
|---|---|---|
| `on_mr_opened/merged/updated` | `mr_commands.rs:95,240,305` → `fire_mr_hook*` | still-inline credential-coupled cmd; product-local once corvus-be owns providers |
| `on_security_summary_loaded` | `security_commands.rs:78` | same |
| `on_issue_linked` | **dormant** (docs only) | fire inline when issue-link lands |
| `on_issue_transitioned` | **dormant** (docs only) | carries pre-mut `from_state`; capture at transition call site. ⚠ catalog/docs drift: `hook_catalog.rs:479` says `from_state/to_state`, docs say `from_status/to_status`+`identifier` |

### 4.3 Global-broadcast (launcher → all product hosts — see §5)

| Hook | Fired by | Exists? | Note |
|---|---|---|---|
| `on_theme_changed` | platform `post_hooks.rs:22` | ✅ `hook_catalog.rs:576` | user-confirmed launcher-broadcast; **level-triggered** (replay on host join) |
| `on_workspace_switched` | `workspace_commands.rs:241` | ✅ | active workspace is launcher state; edge-triggered |
| `on_workspace_created/updated/deleted/repo_added/repo_removed` | `workspace_commands.rs` + platform `post_hooks.rs:38-73` | ✅ | launcher-level; broadcast candidates |
| `on_tab_switch` | `plugin_commands.rs:335,109` | ✅ | tab identity launcher; refreshes `__arbor_current_repo__` per host; **level-triggered** |
| **`on_window_focus`** | `lib.rs:948` (`WindowEvent::Focused`) | ❌ **net-new** | focus only flips `AtomicBool` today; **level-triggered**; multi-window → needs `window_label` |
| **`on_deep_link`** | `lib.rs:620`/`deep_link_commands.rs:43` | ❌ **net-new** | today terminates in FE; exposing to plugins is a policy/permission question — **D7**; edge-triggered |

### 4.4 Launcher-ui-roundtrip (targeted, shell webview lifecycle)

| Hook | Fired by | Note |
|---|---|---|
| `on_view_open` / `on_view_close` | `PluginViewPanel.svelte:83,90` → `firePluginAction` → `fire_on` | shell must round-trip a **targeted** fire into the product host |

### 4.5 Cross-product (linked-worktree orchestrator, launcher-level bg thread)

| Hook | Fired by | Note |
|---|---|---|
| `on_worktree_link_sync_started/done` | `linked_worktrees/orchestrator.rs:151,269` | sync spans multiple repos/products |
| `on_worktree_link_member_added/removed` | corvus `post_hooks.rs:208,215` | currently corvus methods; conceptually cross-product |

### 4.6 Pre-mutation finding (important)
The corvus/platform domain hooks were deliberately engineered so the pre-state is recoverable from params or result (each `post_hooks` P/R split is documented). The only hooks intrinsically needing a *captured* prior snapshot — `on_issue_transitioned`, `on_security_finding_state_changed` — have **no Rust fire site today**. ⇒ **No production hook is currently blocked by a pre-mutation need; the sole true seam-blocker is veto (`on_pre_commit`)**, which the host move dissolves.

### 4.7 Lock-then-fire deadlock guard is load-bearing
`stage_commands.rs:41-77` scopes the `repos` lock and releases it *before* `fire_hook("on_commit")` because Lua hooks call git ops and would deadlock. **Relocation risk:** whoever re-inlines these fires in `corvus-be` must preserve "fire after releasing the repo lock." Same discipline in `fire_vetoable_blocking` — keep it on a worker thread, never the UI thread.

---

## 5. Launcher broadcast channel (global events: theme / focus / deep-link / workspace / tab)

The launcher keeps the **sources** of global signals (FE theme picker, native `WindowEvent::Focused`, OS deep-link, workspace/registry state) and fans them to each product host. This is the **mirror of the existing egress** `EventSink`/`Event::Notify` (backend→shell→FE, `ipc/event_sink.rs`, `docs/ipc-design.md:141`).

### 5.1 Design: `ProductHost` trait + `Broadcaster` (in `arbor-ipc`, via prelude)

```rust
// arbor-ipc (product-agnostic; merula/sitta reuse it)
pub trait ProductHost: Send + Sync {
    /// Fire-and-forget global event to this product's plugin host. Launcher does NOT await handlers.
    fn broadcast(&self, hook: &str, payload: serde_json::Value);
}
pub struct Broadcaster { targets: Vec<Arc<dyn ProductHost>> }
impl Broadcaster {
    pub fn fire(&self, hook: &str, payload: serde_json::Value) {
        for t in &self.targets { t.broadcast(hook, payload.clone()); }
    }
}
```

Two impls chosen at product-registration time (exactly like `Router::register` picks loopback vs `SplitBroker`):
- **`InProcessHost`** — wraps `Weak<Mutex<PluginHost>>`, calls `hook_router::fire_broadcast` directly (identical to today's `LuaHookListener::fire`, `hook_router.rs:282`). Zero serialization.
- **`ChildHost`** — wraps the product's `ChildClient`, writes a **new additive frame** `Broadcast { hook, payload }` (shell→BE) down the duplex pipe. The product's frame reader demuxes it and calls `fire_broadcast` on its local host. **No response frame** (fire-and-forget). Strictly additive to `Hello`/`Request`/`Response`/`Event` (`docs/corvus-be-bringup.md`).

### 5.2 Fire-site rewiring
Replace `state.fire_hook(...)` with `state.broadcast(...)` at the launcher-level sites:
- `ipc/platform/post_hooks.rs:22` (`on_theme_changed`)
- `commands/plugin_commands.rs:335` (`on_tab_switch`)
- `commands/workspace_commands.rs:241` + siblings (`on_workspace_*`)
- **new:** `lib.rs:948` (`on_window_focus`), `lib.rs:620` + `deep_link_commands.rs:43` (`on_deep_link`)

Product-local git hooks do **not** change — they fire inside the product.

### 5.3 Correctness rules
- **Fire-and-forget, never blocks the launcher** (hard rule #9). `ChildHost::broadcast` writes a frame (cheap) and returns; heavy Lua runs on the product thread.
- **Veto-free channel.** `on_pre_commit` (and any future vetoable) is product-local and never travels this channel — avoids a cross-process blocking dependency.
- **Subscription filtering stays product-side** — `fire_broadcast` already filters by `manifest.hooks.subscribes_to(hook)` (`hook_router.rs:188`). No launcher-side registry needed. (Optional later: products advertise their aggregate subscribed-hook set in `Hello`; launcher skips unwanted hooks.)
- **Replay level-triggered state on host join.** Theme + focus + active-tab are **level-triggered** (a late-loading host needs current value) — the `Broadcaster` snapshots the last value and replays on product registration. The existing `source:"init"` theme fire (`theme.svelte.ts`) is exactly this mechanism. **Do not replay edge events** (deep-link, workspace-switch). Use a `Ready`/`Hello`-time signal so the launcher only fans out to ready targets.

---

## 6. Plugin Manager UI + lifecycle + settings rewiring

### 6.1 The two transports today (`src/lib/ipc/plugin.ts`)
- `platform<R>('method', params)` (broker `rpc`) for migrated reads: `list_plugins`, `get_plugins_enabled`, `plugin_enable_preview`, `plugin_disable_preview`, `list_plugin_info`, `plugin_dep_graph`, `plugin_dependents`, `plugin_settings_get/set_all`, `get_installed_plugin_path`, templates. `contribution.ts`/`container.ts`/`plugin-logs.ts` are **all** already broker.
- raw `invoke('command')` for runtime mutation: `reload_plugins`, `setPluginsEnabled`, `execHook`, `firePluginAction`, `fireCommand`, `enablePlugin`, `disablePlugin`, `deletePlugin`, `start/stopPluginScheduler`, `setAppFocus`, `setActiveTab`.

### 6.2 Re-tagging: most `program="platform"` plugin handlers are actually product-local
`ipc/platform/plugin.rs` handlers that **lock the live host** are tagged `platform` only because the host shares the binary. After the move they must re-tag to the product program and physically relocate:

| Handler (`platform/plugin.rs`) | Locks host? | Program after move |
|---|---|---|
| `plugin_enable_preview` / `plugin_disable_preview` | yes | **product** |
| `list_plugin_info` | yes | **product** |
| `plugin_dep_graph` / `plugin_dependents` | yes | **product** |
| `list_plugin_contributions` / `list_contribution_points` | yes | **product** |
| `list_containers` / `get_container` | yes | **product** |
| `list_plugins` / `get_plugin_directory` / `get_installed_plugin_path` | no (FS scan of `plugin_dir()`) | platform OR product (depends D2) |
| `get_plugins_enabled` | no (`cfg.plugins_enabled`) | **platform** (kill-switch config) |
| `plugin_settings_get` / `plugin_settings_set_all` | no (file I/O) | **platform** unless settings go per-product (D2) |
| `list_toolchains`/`add`/`remove`/`set_active`/`detect` | no (`toolchain_registry`) | **platform** |

The `#[platform::handler(program=…)]` tag is the single routing decision point (`ipc/platform/mod.rs:58-61`) — mechanical but must not be missed.

### 6.3 Runtime-mutating lifecycle commands (`commands/plugin_commands.rs`) → product-local
`set_plugins_enabled` (config write is platform; reload is product), `reload_plugins` (re-fires `on_repo_open`/`on_tab_switch` for open tabs — needs the open-tab list from the launcher), `exec_hook`, `fire_plugin_action`, `fire_command`, `enable_plugin`, `disable_plugin`, `delete_plugin` (needs repo-path list from product), `start/stop_plugin_scheduler`. These currently hold an `AppHandle` and emit `arbor://*`; once in the product backend, emits become **push events over the broker reverse channel** (`ChildClient` topic/payload, `ipc/mod.rs:135-138`). So `arbor://plugins-reloaded`, `arbor://host-ui-command`, form/container-open events flow product→shell via that existing path.

The FE wrappers change from `platform('list_plugin_info')` / `invoke('reload_plugins')` to `<product>('list_plugin_info')` / `<product>('reload_plugins')`.

### 6.4 Host built-in dispatch (`plugin_host_commands.rs`) splits
- `arbor:git.commit/push/fetch/pull/branch_create/checkout/branch_delete/stage_all/unstage_all` (`host_commands.rs:47-88`) → already route via `corvus_rpc`/`branch_commands`/`stage_commands` → **product-local in-process**.
- `arbor:repo.refresh`, `arbor:app.open_settings` (`host_commands.rs:36-40`) → emit `arbor://host-ui-command`, executed by FE (`AppShell.svelte:1001`) → **launcher-ui-roundtrip**.

### 6.5 The critical missing channel (⚠ gap)
A **backend→shell request/response** channel does NOT exist — only event push (`ChildClient` topic/payload). The launcher-ui-roundtrip verbs (`arbor:app.open_settings`, `arbor:repo.refresh`, and **every** `arbor.ui.form`/`confirm`/`notify`, plus the vetoable-with-modal case) need the backend to *originate a request the shell answers*. Today `ipc-design.md`/`corvus-be-bringup.md` define only shell→BE request + BE→shell event. **This is the load-bearing prerequisite for moving the host (and for the OOP phase).**

### 6.6 Marketplace coupling
Catalog/fetch/cache/ledger = **platform** (app-agnostic; `crates/plugin/marketplace`, all under `~/.config/arbor/`). But install/uninstall/enable must call into the host to reload + cascade:
- `marketplace_install_plugin` → `reload_plugin_host` → `host.reload()` + emit (`marketplace_commands.rs:152,336`)
- `marketplace_uninstall_plugin` → `disable_required_dependents` + ledger sync + `reload_plugin_host` (`:175`)
- `marketplace_set_plugin_enabled` → `host.enable/disable` cascade + emit (`:216`)
- `host.set_extra_plugin_roots([marketplace::plugins_dir()])` (`lib.rs:688`) — the product host must learn the install root from the launcher.

**Clean cut:** marketplace stays platform for catalog/download/ledger; the "tell the host to re-scan + cascade" step becomes an **rpc into the product program**. Theme install/uninstall (`marketplace_install_theme/uninstall_theme`) is pure platform (no host coupling).

### 6.7 FE store note
`pluginStore` (`src/lib/stores/plugin.svelte.ts`) caches `disabledPlugins`/`comboSelections` in `localStorage` (`arbor:disabled-plugins`, `arbor:combo:*`) reconciled from backend via `syncFromInfos`. Treated as ephemeral session mirror (tolerated exception to the no-localStorage rule). After the move, `syncFromInfos` is fed by the product's `list_plugin_info`.

---

## 7. Crate / process landing zone — the in-process-first path

### 7.1 In-process-first is already half-built
`PluginHost` is constructed once as `Arc<Mutex<PluginHost>>` (`lib.rs:412`), reached via `state.lock_plugin_host()`. `build_router` (`ipc/mod.rs:57-87`) already runs `corvus` **in-process via loopback** when `corvus-be` isn't spawned. The host rides the same seam: move ownership into a `corvus-be`-named module **still compiled into the shell binary**, invoke in-process, then "ride the OOP split" when the binary separates (`SplitBroker` flips routing per-method via the `Hello` advertisement, `split_broker.rs:31-39`) — **no call-site rewrite**.

### 7.2 The value-delivering order
Wave 0 (in-process, host owned by the corvus module, hooks fired inline) **immediately unblocks**:
- the deferred hard commands (commit, the inline-in-broker tag/note hooks),
- the vetoable `on_pre_commit` (in-process `fire_vetoable_blocking`, no round-trip),
- deletion of `post_hooks.rs` for the corvus domain,

…all **before** any process actually separates. OOP becomes a later, transport-only concern.

### 7.3 Risk: per-product host vs shared host (the biggest open question)
`PluginHost` is a singleton today. Model-D implies **one host per product** (clean isolation; cross-product via `service.export/call`). But a plugin that wants to hook both git (corvus) and future merula events, plus `arbor.service.call` across products, needs a cross-product story. **D8.**

---

## 8. Wave plan (MVP-first)

Mark: **[P]** parallel-agent-able · **[S]** lead-serial · deps · blast-radius hotspots.

### Wave 0 — Commit to "host = product backend", in-process **[S, lead]**
**Goal:** move `PluginHost` ownership into a `corvus`-named in-process module and re-enable inline hook fires. No process separation.
- Lift `build_hook_dispatcher` (`lib.rs:82-101`) + host construction (`lib.rs:412`) into `crates/corvus/be/src/plugin/mod.rs` (or an in-shell `corvus` module if `corvus-be` isn't yet a separate crate). Implement `CorvusBeAppCtx` over the BE `EventSink`.
- Re-inline the corvus-domain hooks at their handlers; **delete `ipc/corvus/post_hooks.rs`** and its call in `rpc_commands.rs:51`. Preserve the lock-then-fire discipline (§4.7).
- Re-inline `on_pre_commit` vetoable in the commit handler in-process; migrate `commit` off the inline-`#[tauri::command]` exception.
- Fix the inline-in-broker drift (`reset.rs` tags, `notes.rs` notes) consistently as inline-in-product.
- **Blast radius:** `rpc_commands.rs`, `stage_commands.rs`, `ipc/corvus/{post_hooks,reset,notes}.rs`, `lib.rs` boot. **Deps:** none. **This is the MVP — it delivers the dissolution of both seams.**

### Wave 1 — API-surface split: migrate `ns_shell/*` into the product **[P]**
**Goal:** kill the `ctx.app_handle()` downcast namespace-by-namespace.
- Per-namespace agents move `repo`/`notes`/`pipeline`/`terminal`/`linked_worktrees`/`job`/`brp`/`tabs` (non-credential, non-global) into `corvus-be/src/plugin/ns/`, re-expressed against `CorvusState`; each capability they needed via downcast migrates onto `AppCtx` (or the product ctx).
- **Blast radius hotspots (shared files):** `ns_shell/mod.rs` (installer list), `ctx_ext.rs` (shrinks to nothing), `app_ctx.rs` (trait grows). Serialize edits to these three; the per-namespace bodies are independent **[P]**.
- **Deps:** Wave 0. **Excluded here:** credential cluster (Wave 4), global namespaces (Wave 2).

### Wave 2 — Launcher broadcast channel **[S then P]**
**Goal:** global events reach product hosts.
- **[S]** Add `ProductHost`/`Broadcaster` to `arbor-ipc` (+ prelude); `InProcessHost` impl; register at product spin-up; level-triggered replay + `Ready` signal.
- **[P]** Rewire the four global fire sites + add the **2 net-new hooks** (`on_window_focus`, `on_deep_link`) to `hook_catalog.rs`, `sdk.d.lua` (arbor-extensions), `PluginDevHooks.svelte`, `Shortcuts`/docs.
- **Blast radius:** `arbor-ipc`, `lib.rs` (focus/deep-link sites), `platform/post_hooks.rs`, `hook_catalog.rs`. **Deps:** Wave 0.

### Wave 3 — Plugin Manager UI + lifecycle rewiring **[P]**
**Goal:** FE talks to the product host; re-tag handlers.
- Re-tag host-locking `platform/plugin.rs` handlers to the product program; move them physically; convert raw-`invoke` lifecycle commands to `<product>(...)` rpc; route `arbor://*` emits through the reverse push channel.
- Marketplace: keep catalog/ledger platform; make "reload host" an rpc into the product.
- **Blast radius:** `ipc/platform/plugin.rs`, `commands/plugin_commands.rs`, `commands/marketplace_commands.rs`, `src/lib/ipc/plugin.ts`. **Deps:** Wave 0; the **backend→shell request channel** (§6.5) is a hard prerequisite for the `arbor.ui.*`/`notify` UI-roundtrip path → coordinate with the transport scout.

### Wave 4 — Credential cluster (behind M3 gate) **[S, gated]**
**Goal:** relocate `issues`/`mr`/`ci`/`security`.
- Move with their shared helper `resolve_repo_path` (`mr.rs:53-79`); replace throwaway-runtime `block_on` with product runtime. **Deps:** the M3 credential broker (independent of the host move). Do **not** start before M3.

### Wave 5 — Crate extraction / per-product roots **[P]**
**Goal:** finalize physical homes + per-product plugin storage.
- Per-product `plugin_dir`/`plugin_states.json`/`discover_plugins` if D2 lands per-product; fix the `plugin_dir()` dev-mode `ancestors().nth(3)` arithmetic (`manifest/mod.rs:107-116`) for the new location. **Deps:** D2, Waves 1+3.

### Wave 6 — Out-of-process (OOP) **[S]**
**Goal:** separate the product binary.
- Add the `Broadcast` frame variant + `ChildHost` impl; the launcher-ui-roundtrip request frame (the new backend-originated request/response); the `Ready` handshake + replay. The host move itself was already done in-process (Wave 0); this is transport-only. **Deps:** Waves 0,2,3 + the transport request-channel.

---

## 9. Open decisions (need the user's call)

- **D1 — Where does the repo-registry / workspace state live?** `arbor.workspace.*` reads + `arbor.tabs.*` read like a **launcher** concept (cross-product repo organization), but `arbor.repo.clone` writes the same `AppState.jobs`/registry. Launcher-owned or corvus-be-owned? Straddles PL/launcher. *(Scouts 2 & 4 flag this.)*
- **D2 — Per-product plugin roots + ledger?** Everything is **global** today (`plugin_dir()`, `plugin_states.json`, `plugin_data/<name>/global.json`). The decided IntelliJ direction implies per-product (`~/.config/arbor/<product>/plugins/`). Decide before relocating `list_plugins`/`get_plugin_directory`, which otherwise scan the wrong root. *(Scout 4.)*
- **D3 — `arbor.job.*` / `arbor.terminal.*` home:** product-local (corvus) or a platform job runner? Jobs are window-routed and product-scoped today.
- **D4 — `arbor.cloud.*`:** slated for deletion when WASM lands (`cloud.rs:5-6`). Relocate at all, or let it die? Also `arbor.brp.*` + `arbor.cloud.*` may each want their **own** headless backend rather than living in corvus-be (both are already standalone crates).
- **D5 — `host_command_required` git allowlist in core** (`command.rs:180-200`) hard-codes `arbor:git.*` in product-agnostic `arbor-plugin-core`. Push it out to the product so core stays generic?
- **D6 — Split the `AppCtx` god-trait?** It mixes PL/UI/GB capabilities; a headless product impl shouldn't fake/RPC capabilities it doesn't own.
- **D7 — Expose deep-links to plugins?** `on_deep_link` is net-new; a plugin seeing every deep-link URL is a mild info-leak. Gate behind a manifest permission?
- **D8 — One host per product vs one shared host?** The biggest design question. Per-product = clean isolation + cross-product via `service.export/call`; shared = simpler cross-plugin but contradicts the IntelliJ model. Code today assumes **one** host. Also resolves how `arbor-cloud`'s current direct hook-firing (it holds a `Arc<Mutex<PluginHost>>` clone, `lib.rs:105-109`, `cloud/mod.rs:46`) gets re-routed.

### Scout conflicts / gaps flagged
- **Net-new vs relocation:** `on_window_focus` and `on_deep_link` are **not relocations** — they have no hook today (Scout 5). Treat as new feature work in Wave 2.
- **Inline-in-broker drift** (Scout 3): four hooks (`on_tag_create/delete`, `on_note_saved/deleted`) fire inline inside already-migrated `ipc/corvus/*` modules, contradicting the `post_hooks` invariant. Make consistent in Wave 0.
- **Dual-fire `on_repo_open`** (Scout 3): fires from corvus `post_hooks`, inline `repo_commands.rs:240`, reload re-fire, AND FE — confirm no double-delivery to plugins.
- **Backend→shell request channel does not exist** (Scouts 4 & 6): only event push. This is the single hardest transport prerequisite for the UI-roundtrip surface and is the gating dependency for Wave 3's `arbor.ui.*`/`notify` path and Wave 6.
