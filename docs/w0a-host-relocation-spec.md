# W0a Execution Spec — Plugin-Host Relocation (in-process-first)

> **Status: IMPLEMENTED** (build-green + unit tests pass). See
> [§7 Implementation status](#7-implementation-status). Turns Wave 0 of
> [`plugin-relocation-inventory.md`](plugin-relocation-inventory.md) into a
> concrete, sequenced, build-greenable plan grounded in the current working
> tree (post the `start plugin relocation` commit). Every file:line cite was
> read directly; where the inventory's planning text is now stale this spec
> corrects it.
>
> **Scope:** move `PluginHost` ownership out of the shell's `AppState` into a
> corvus-owned module (still compiled into the shell binary, invoked in-process)
> so the inline hook fires the corvus git handlers already perform reach the host
> whether the domain runs in-process or out-of-process; make the `on_pre_commit`
> veto work with no cross-process round-trip; keep the (already deleted)
> `post_hooks` seam dead. **Out of scope:** `ns_shell` migration (Wave 1),
> launcher broadcast channel (Wave 2), OOP transport / backend→shell request
> channel (Wave 3/6).

---

## 1. Current-state anchor (verified)

- **`ipc/corvus/post_hooks.rs` is deleted.** Every corvus git handler fires its
  hooks **inline** via `state.fire_hook(...)` / `state.hook_dispatcher.fire_vetoable_blocking(...)`
  on the shell's `&AppState`. Confirmed fire sites: `branch.rs` (12), `gitflow.rs`
  (8), `linked_worktree.rs` (2), `missing.rs` (2), `notes.rs` (2), `rebase.rs`
  (2), `reset.rs` (2), `remote.rs` (3), `repo.rs` (3), `security.rs` (1),
  `mr.rs` (`fire_mr_hook` helper), `stage.rs` (veto + `on_commit`).
- **`crates/corvus/` has zero references to** `fire_hook` / `PluginHost` /
  `hook_dispatcher`. The plugin host is entirely shell-side: `AppState.plugin_host:
  Arc<Mutex<PluginHost>>`, fired via `AppState::fire_hook` (wraps `hook_dispatcher`).
- **`rpc_commands.rs:71`** calls only `platform::post_hooks::fire`; lines 66-67
  note "the corvus handlers fire their own hooks inline." The corvus seam is dead;
  the platform seam (theme/workspace) survives and is a Wave-2 concern.
- **`corvus-be`** (`crates/corvus/be/src/`) has only `main.rs`, `bisect.rs`,
  `stash.rs`, `issues.rs`, `repo_registry.rs`. Its handlers take `&CorvusState`
  (`crates/corvus/core/src/state.rs`), which holds events/repos/git_program/host
  (reverse-channel) and **no hook-firing capability, no PluginHost**.
- **Two handler bodies per OOP domain.** stash exists both as
  `ipc/corvus/stash.rs` (`&AppState`, fires hooks) and `be/src/stash.rs`
  (`&CorvusState`, **no hooks**). The git-only domains exist solely as in-process
  `&AppState` handlers.

**Stash hook-drop bug is LIVE.** `be/src/stash.rs` has no hook fire and its
doc-comment still says hooks fire later "in `crate::ipc::corvus::post_hooks`" —
a module that no longer exists. When `stash_save`/`stash_apply`/`stash_pop` are
served by `corvus-be` (advertised OOP; the `SplitBroker` routes advertised
methods to the child), the hook is silently dropped. This is the concrete live
failure W0a closes.

**Correction to the inventory (stale text):** §1/§8 still describe `commit` as
"the one stage-domain op not migrated / blocked-inline (veto seam missing)." In
the current tree `commit` is already a `#[corvus::handler]` firing
`fire_vetoable_blocking("on_pre_commit", …)` inline at `stage.rs:38` **before**
the mutation, then `on_commit` at `stage.rs:87` **after** the repos lock drops.
W0a's job for the veto is **preservation under the ownership move**, not a fresh
migration.

---

## 2. Hook-firing seam on `CorvusState` — the core of W0a

**Recommendation: give `CorvusState` an `Arc<HookDispatcher>`** — shared with
`AppState` in-process, owned locally in `corvus-be` OOP. Reject the alternative
("keep `AppState.fire_hook` in-process, build the host into corvus-be only for
OOP"): it keeps two fire surfaces that must be hand-kept byte-identical and fires
in-process vs OOP through different objects — exactly the divergence W0a removes.

The seam already exists and is Tauri-free, so this is cheap:

- `HookDispatcher` + `HookListener` live in `arbor-plugin-api`
  (`crates/plugin/api/src/dispatcher.rs`) — no Tauri, no mlua.
- `PluginHost` + `LuaHookListener` live in `arbor-plugin-core` — **zero Tauri**.
- `corvus-core` / `corvus-be` are already Tauri-free; they can depend on
  `arbor-plugin-api` (for the dispatcher type), and `corvus-be` additionally on
  `arbor-plugin-core` (to construct the host).

### Design

**Add to `CorvusState`** (one field + three methods, mirroring `with_host_caller`
and the shell's `AppState::fire_hook`):

```rust
hooks: Arc<HookDispatcher>,   // arbor_plugin_api::prelude

pub fn with_hooks(mut self, hooks: Arc<HookDispatcher>) -> Self { self.hooks = hooks; self }
pub fn fire_hook(&self, hook: &str, ctx: Value) {
    self.hooks.fire_blocking(hook, /* ctx */);
}
pub fn fire_pre_commit_veto(&self, ctx: Value) -> Option<String> {
    self.hooks.fire_vetoable_blocking("on_pre_commit", /* ctx */)
}
```

`CorvusState::new` builds an **empty** dispatcher (catalog registered, no
listener → fires are clean no-ops). This keeps `corvus-core` depending only on
`arbor-plugin-api` (Tauri-free, mlua-free); the mlua host is wired in by whoever
owns the process.

**Why `Arc<HookDispatcher>` not a new `dyn HookSink` trait:** `HookDispatcher`
is *already* the indirection (`Vec<Arc<dyn HookListener>>`); the listener is the
polymorphism seam (mlua today, wasm later). A second `dyn` wrapper has no caller
that needs it. Introduce the trait only when a future product wants to fire
without linking `arbor-plugin-api`.

**In-process: share the shell's dispatcher.** The shell builds exactly one
`Arc<HookDispatcher>` (`lib.rs:451` via `build_hook_dispatcher`) stored as
`AppState.hook_dispatcher`. Where it seeds `CorvusState` in `setup()`:

```rust
let cs = CorvusState::new(sink).with_hooks(state.hook_dispatcher.clone());
```

Both states now point at the **same** dispatcher → same `LuaHookListener` → same
host. A fire from `&CorvusState` and from `&AppState` are byte-identical because
they hit the same object. Zero serialization, zero divergence.

**OOP: corvus-be owns its own.** `corvus-be` builds the host + dispatcher
locally (the `build_hook_dispatcher` body lifts verbatim — it touches only
`HOOK_CATALOG` + `LuaHookListener` + the host Arc, no Tauri) and `.with_hooks(...)`
it onto its `CorvusState`. OOP handlers firing `state.fire_hook(...)` then reach
corvus-be's local host.

### Lock-then-fire + veto preserved

The deadlock discipline (inventory §4.7) lives in the handler bodies, not the
seam — `CorvusState::fire_hook` is a thin pass-through. OOP handler bodies
replicate the in-process scoping: mutate, **drop the local repo handle**, *then*
fire. `CorvusState` holds no `Mutex<RepoManager>` (only `repos: Mutex<tab→path>`),
so the in-process deadlock class can't recur OOP the same way; the rule still
binds the in-process path (shared host, `LuaHookListener::fire` takes the host
mutex then runs Lua that may call git). `on_pre_commit` is corvus-domain and
runs **in-process only** today (`commit` is not in `corvus-be`), so the veto is
not at risk in W0a — the `fire_pre_commit_veto` seam is provided so that *when*
`commit` later moves OOP it fires against corvus-be's local host with no
round-trip.

---

## 3. Host ownership during in-process-first

**Module:** `crates/corvus/be/src/plugin/` — the landing zone the inventory
§2.4 names. The minimal-churn move: lift `build_hook_dispatcher` (`lib.rs:83-102`)
into a free function in `corvus-be/src/plugin/mod.rs` (depends only on
`arbor-plugin-{types,api,core}`) and have the shell call **that** instead of its
private copy. One definition, two call sites (shell in-process, corvus-be OOP) —
"host co-located, no call-site rewrite" with the least churn. (If pulling
`arbor-plugin-core`'s mlua into the shell's view of `corvus-be` hurts build time,
put the function in a tiny `corvus-plugin` leaf crate both link.)

**Owns:** `Arc<Mutex<PluginHost>>` + the `HookDispatcher` + the `CorvusBeAppCtx`,
plus reload / `start_all_schedulers`. In W0a the shell still drives reload on its
in-process host; corvus-be drives its own when it owns the host OOP.

**`CorvusBeAppCtx` implements `arbor_core::AppCtx` over the BE `EventSink`** (vs
today's `TauriAppCtx` over `AppHandle`+`AppState`). `AppCtx` has 3 required + 5
defaulted methods:

| Method | W0a behavior |
|---|---|
| `as_any` | `self` (downcast smell stays until `ns_shell` migrates) |
| `emit` | forward to the BE `EventSink` (`CorvusState` already holds `Arc<dyn EventSink>`) — no Tauri |
| `spawn` | corvus-be's tokio runtime (OOP); in-process host keeps using `TauriAppCtx` |
| `arbor_dir` | `arbor_core::prelude::arbor_config_dir()` — satisfiable now |
| `is_focused` | **defer** → `false` (global focus is a launcher signal, Wave 2) |
| `record_plugin_log` | **defer** → no-op (log ring buffer is shell UI; OOP push later) |
| `active_repo_path` | `None` for W0a (the hooks carry `tab_id` explicitly) |
| `open_path` | **defer** → error (OS shell-open is UI/global) |
| `invoke_host_command` | **defer** → warn-noop (needs the backend→shell request channel, §6.5; later wave) |

**Satisfied now:** `emit`, `spawn`, `arbor_dir`, and the hook-firing path itself
(via the dispatcher, not `AppCtx`). **Deferred (UI/global):** `is_focused`,
`record_plugin_log`, `open_path`, `invoke_host_command`, full `active_repo_path`.
In W0a the **in-process** host keeps `TauriAppCtx` (all deferred capabilities work
in-process); `CorvusBeAppCtx` is the impl corvus-be uses when it owns the host
OOP. The two are selected at host-construction time (like `Router::register`
picks loopback vs `SplitBroker`); hook *firing* is identical either way because
it flows through the dispatcher, not `AppCtx`.

---

## 4. Sequenced edit plan (each step compiles)

Serial steps touch shared files (`state.rs`, `lib.rs` boot, `corvus-be/main.rs`);
the per-handler re-inlining in `corvus-be` is parallelizable.

1. **[S] Add the hook seam to `CorvusState`.** `corvus-core/Cargo.toml`
   (+`arbor-plugin-api`), `state.rs` (`hooks` field + `with_hooks`/`fire_hook`/
   `fire_pre_commit_veto`), prelude re-export. Default empty dispatcher → fires
   are no-ops. Purely additive; compiles standalone.
2. **[S] Share the shell's dispatcher into the in-process `CorvusState`.**
   `lib.rs` setup: `.with_hooks(state.hook_dispatcher.clone())`. No behavior
   change yet. Compiles.
3. **[S] Lift `build_hook_dispatcher` to a shared corvus-owned definition.** New
   `corvus-be/src/plugin/mod.rs` (body from `lib.rs:83-102`, verbatim;
   +`arbor-plugin-core` dep); shell calls the lifted fn. The ownership relabel.
   Identical body, one definition. Compiles.
4. **[S] Build the host into `corvus-be` and attach it.** `corvus-be/src/main.rs`
   (after the `CorvusState` build): construct `Arc<Mutex<PluginHost>>`, dispatcher
   via step 3, `CorvusBeAppCtx` over `sink`, `set_app_ctx`, `reload`,
   `start_all_schedulers`, then `.with_hooks(dispatcher)`. New
   `corvus-be/src/plugin/app_ctx.rs`. Separate binary; doesn't touch the shell.
   No-plugins reload is a clean no-op. Compiles.
5. **[P] Make `corvus-be` domain handlers fire inline.** `be/src/stash.rs`: add
   `on_stash_push`/`on_stash_pop` mirroring `ipc/corvus/stash.rs:50,72,90`
   field-for-field, **after dropping the local `Repository`**; delete the stale
   "Hooks are NOT fired here" doc-comment. Closes the live stash bug. Additive;
   no-op if no subscriber. Parallelizable across domains.
6. **[verify-only] Confirm the dead post_hooks seam + in-process inline fires.**
   Already in the target state — no edit. The inventory's Wave-0 "delete
   post_hooks / re-inline on_pre_commit / fix reset+notes drift" is **already
   done** in this tree; W0a's residual is the ownership move + the corvus-be fires.

**Boundary (out of W0a):** selecting `CorvusBeAppCtx` vs `TauriAppCtx` per
process is in-scope; *removing* the shell's in-process host (always-OOP) is Wave
6. The `ns_shell` `ctx.app_handle()` downcast backlog is Wave 1. The launcher
broadcast channel + real `is_focused`/`record_plugin_log`/`invoke_host_command`
are Wave 2+. The backend→shell request channel (§6.5) is untouched.

---

## 5. W0a-specific blocking decisions

**None of D1–D8 strictly blocks starting W0a.** Summary:

| Decision | Blocks W0a? | Default to proceed |
|---|---|---|
| D1 repo-registry/workspace home | No | `ns_shell`/Wave 1; W0a hooks carry `tab_id`. Defer. |
| D2 per-product plugin roots/ledger | No | Keep global `plugin_dir()` for now; corvus-be scans the same root. Defer to Wave 5. |
| D3 `arbor.job.*`/`terminal.*` home | No | `ns_shell`/Wave 1. Defer. |
| D4 `arbor.cloud.*` fate | No | Cloud keeps pointing at the shell's in-process host (it runs in the shell). Defer. |
| D5 `host_command_required` git allowlist in core | No | Exercised by `invoke_host_command`, which W0a defers. Defer. |
| D6 split the `AppCtx` god-trait | No (informs §3) | Satisfy `CorvusBeAppCtx` with safe defaults; don't split yet. |
| D7 expose deep-links to plugins | No | Net-new hook, Wave 2. Irrelevant. |
| **D8 one host per product vs shared** | **Deferrable** | Keep the singleton, relabeled corvus-owned (see below). |

**D8 — deferrable for the single-corvus case.** There is exactly one
`PluginHost` today. For W0a: in-process the singleton stays the singleton, merely
relabeled as corvus-owned (step 3) with its dispatcher shared into `CorvusState`
(step 2); OOP, corvus-be builds *its* host and the shell's in-process host is the
no-corvus-be fallback. "One per product vs shared" only becomes forcing when a
**second** product host (merula-be/sitta-be) lands and a plugin wants to hook both
+ `arbor.service.call` across them. The minimal call to proceed: "the corvus host
is the git product host; the shell keeps an in-process instance as the no-spawn
fallback" — already implied by the SplitBroker/loopback structure.

**Backend→shell request channel (§6.5) is NOT a W0a blocker.** It's needed only
for backend-**originated** request/response (`arbor.ui.*`/`confirm`/`notify`,
`invoke_host_command`'s FE-executed verbs). W0a fires **fire-and-forget** hooks
and the **in-process** vetoable `on_pre_commit` — neither originates a request
the shell must answer. The veto runs entirely inside the host process. The §6.5
channel is a Wave-3/Wave-6 prerequisite; `CorvusBeAppCtx` deliberately defers
every capability that would need it.

---

## 6. Verification (without running the app)

**Must compile:** `cargo build -p corvus-core` (step 1 — Tauri-free seam);
`cargo build -p corvus-be` (steps 3-5 — host + `CorvusBeAppCtx` + inline fires,
no Tauri); `cargo build` (shell, steps 2-3 — shared `build_hook_dispatcher` lift,
in-process host unchanged); full workspace (no other crate broke on the new
`CorvusState` field).

**Unit test — in-process fire reaches the host.** In `corvus-core` (extend
`state.rs`'s `RecordingSink` pattern) register a recording `HookListener` (not the
real mlua one), build `CorvusState::new(sink).with_hooks(that)`, call
`fire_hook("on_stash_push", …)`, assert exactly one recorded fire with the right
payload. A second test: `fire_pre_commit_veto` with a listener returning
`Some("nope")` asserts the `Option<String>` propagates. For the shared-Arc
property (step 2): assert `Arc::ptr_eq(AppState.hook_dispatcher, the one handed to
CorvusState)`.

**Manual check — stash no longer drops its hook (static):** grep
`be/src/stash.rs` for `fire_hook` → `on_stash_push`/`on_stash_pop` present,
matching `ipc/corvus/stash.rs:50,72,90` field-for-field, each **after** the local
`Repository` is dropped; the stale post_hooks doc-comment removed; `corvus-be/main.rs`
constructs the host and `.with_hooks(...)` the dispatcher (else step-5 fires are
no-ops). Optional integration assertion: build a `CorvusState` with a recording
listener, run the `stash_save` handler against a temp repo, assert `on_stash_push`
recorded.

---

## 7. Implementation status

Implemented 2026-06-19; full workspace builds green and `corvus-core`'s 5 unit
tests pass (3 new: `fire_hook_reaches_the_listener`,
`fire_hook_is_a_noop_without_a_listener`, `pre_commit_veto_propagates`).

**What landed vs the plan:** the spec's "lift `build_hook_dispatcher` into
`corvus-be/src/plugin/`" was adjusted because `corvus-be` is a **binary-only**
crate — nothing can depend on it as a library, so the shell couldn't share its
copy. Instead a small **lib crate `corvus-plugin`** (`crates/corvus/plugin/`)
holds the shared wiring (`build_hook_dispatcher` + `CorvusBeApiInstaller` +
`CorvusBeAppCtx`); both the shell and `corvus-be` link it. The shell's private
`build_hook_dispatcher` was deleted and re-pointed at `corvus_plugin`. Steps 1
(CorvusState seam), 2 (in-process share), 4 (corvus-be host + reload), 5 (stash
fires) landed as written; the headless installer is `CorvusBeApiInstaller`
(host-pure `arbor.*` via `register_lua_api(.., &[])`), not `NoopApiInstaller`
(which publishes nothing → plugins can't load).

**Deliberately deferred (declared limitations, not bugs):**
- **ns_shell namespaces in corvus-be** — Wave 1. The OOP host publishes host-pure
  `arbor.*` only; a hook that calls `arbor.repo`/`arbor.job`/… gets a clear
  nil-field error (logged), never a silent drop.
- **Plugin schedulers in corvus-be** — `start_all_schedulers` is not called there
  (no `Scheduler` installed). Scheduled plugins don't tick OOP yet.
- **Per-product plugin filtering / `plugin_dir` in the headless process** — D2 /
  Wave 5. `corvus-be`'s `reload()` scans the global plugin root, so (until
  filtering lands) every plugin loads in both the shell host and the corvus-be
  host. Architecturally the per-product-host direction; messy mid-transition.
