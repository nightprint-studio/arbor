# WASM Plugin Integration — Analysis & Design

How to add **WASM plugins** (Rust → wasm) alongside the existing **Lua** plugins,
which runtime to embed, and how the three earmarked first-party crates
(`arbor-studio*`, `arbor-brp`, `arbor-cloud`) become wasm guests.

Companion to [studio-extraction-plan.md](studio-extraction-plan.md) and the
round-2 roadmap (`migration-roadmap.md`, M8–M11). Grounded in a read-only audit
of the plugin host (June 2026).

---

## 1. The host already has the right seam

The hook system is **runtime-agnostic by construction** — adding wasm does **not**
require touching the dispatcher:

- `crates/platform/plugin/api/src/dispatcher.rs` — `HookListener` trait
  (`fire`, `fire_vetoable`) + `HookDispatcher` holding `Vec<Arc<dyn HookListener>>`.
  Domains fire `dispatcher.fire("on_X", ctx)`; the dispatcher fans out to every
  listener sequentially.
- `crates/platform/plugin/core/src/hook_router.rs` — `LuaHookListener` implements
  `HookListener` over a `Weak<Mutex<PluginHost>>`; walks the Lua plugins that
  subscribe (manifest `[hooks]` booleans) and invokes their handlers. Vetoable only
  for `on_pre_commit`.
- `build_hook_dispatcher` (`arbor-plugin-core`) wires the listener(s).

**A `WasmHookListener` is a second `impl HookListener`** holding a `Weak` to a
`WasmRuntime`, registered next to `LuaHookListener` in `build_hook_dispatcher`.
The dispatcher itself is unchanged.

Host APIs are exposed via installers (`crates/platform/plugin/core/src/lua_api/`):
`LuaApiInstaller` (one per shell) + per-namespace `LuaNamespaceInstaller`
(`ns::{log,events,service,json,fs,text,settings,ui,timer,scheduler,hooks,…}`), all
reading a snapshot `ApiCtx` (permissions + shared registries + `host_weak` for
callbacks). Product namespaces (git) are injected as `extra_installers` by
`CorvusBeApiInstaller`.

The canonical hook list lives in `hook_catalog.rs` — **one source of truth** both
runtimes must bind against.

---

## 2. Runtime choice — **wasmtime** (Component Model + WASI 0.2 / `wasm32-wasip2`)

Used **directly**. Not extism, not wasmer.

| | wasmtime | extism | wasmer |
|---|---|---|---|
| Async **host** calls (yield host thread on SSE/network) | ✅ `add_to_linker_async` | ❌ **sync only** | ⚠️ possible, less-standard |
| Rust guest build | `rustup target add wasm32-wasip2`, stable | `extism-pdk`, simplest | ok |
| Capability model | WASI preopens + host imports, default-deny | manifest `allowed_hosts/paths` (closest to ours) | WASIX legacy-compat |
| Maturity / adoption | highest (Zed, Fastly, MS, Shopify) | stable, wasmtime under the hood | mature, different bet |
| Binary size | heaviest (bundles Cranelift) | = wasmtime | medium |

**Why wasmtime wins on Arbor's actual constraints:**

1. **It's the only one that satisfies the hard rule.** Arbor must call host APIs
   sync *and* async, with long-lived SSE/network ops that **must not freeze the UI**
   (CLAUDE.md: "tutto ciò che potrebbe stallare >50ms va async"). wasmtime's
   `add_to_linker_sync` / `add_to_linker_async` split delivers exactly that.
   **extism is disqualified** — its host/HTTP calls are synchronous-only by design,
   colliding head-on with that rule. wasmer can do async-host but on a less-maintained path.
2. **It maps 1:1 onto the existing host.** Define the `arbor.*` surface as a WIT
   `import` world, generate host traits with wit-bindgen, and reuse the same
   `host()` dispatch already powering Lua (`CorvusRpcCtx` / `host_handle::host()`).
   Capability scoping (Lua `fs_scope`/manifest) → WASI preopens + which host imports
   you add to the linker. Same default-deny mental model.
3. **Future-proof without re-platforming:** true in-guest streaming
   (`stream<T>`/`future<T>`, WASI 0.3 / Preview 3) is the *same engine* (Wasmtime
   43+, RC in 2026, stable ~late 2026) — a world bump, not an engine swap.

> If we want extism's manifest ergonomics, replicate that thin layer (a few hundred
> lines) on top of raw wasmtime and **keep async** — don't adopt extism and lose it.

**Recommended initial config:** Component Model + async; `add_to_linker_async` for
`arbor.*`; `epoch_interruption` with a background epoch-ticker (Zed's pattern) for
CPU-loop protection; per-instance memory caps; host-future timeouts for
SSE/network cancellation. Cranelift initially; revisit Winch/AOT-min only if binary
size becomes a *measured* problem.

---

## 3. The async model (the crux)

Two **separate** axes — don't conflate:

- **Host-side async** (host import yields while it works): solved today. A host
  import implemented as Rust `async fn` suspends the guest and returns control to
  Tokio while an SSE/network op is in flight — no UI freeze. Maps onto the existing
  `AppCtx::spawn` / `tauri::async_runtime::spawn` discipline.
- **Guest-side concurrency** (guest awaits multiple in-flight ops / streams):
  under `wasm32-wasip2` a guest is **sequential per export** — calls one async host
  import, blocks *that instance* until it returns, proceeds. **This is exactly the
  Lua hook shape** (synchronous from the guest's POV, async-capable on the host), so
  the migration is behaviorally faithful. Genuine in-guest streams are WASI 0.3
  (adopt later, same engine).

**SSE / long-lived:** model the stream as **host-owned**; the host pushes events to
the guest via hook/callback invocations — identical to how Lua does it today
(`arbor-brp`'s `run_watch_stream` lifecycle moves into a host import; the `on_event`
callback stays in the guest). Works fully on stable p2 now.

**Cancellation:**
- Guest blocked in a host import (SSE read) → cancel the **host future** (drop/timeout). Native.
- Guest stuck in a CPU loop → `epoch_interruption` cuts it off (Zed's approach).
  (Note: epoch/fuel do **not** interrupt a guest blocked in a host call — cancel that
  at the host-future level, which is the correct lever anyway.)

---

## 4. Insertion points (where wasm attaches)

No change to `HookDispatcher` or the RPC API; everything additive:

1. **`WasmHookListener`** `impl HookListener` over `Weak<Mutex<WasmRuntime>>`;
   registered in `build_hook_dispatcher` next to `LuaHookListener`.
2. **`WasmRuntime`** (new, e.g. `crates/platform/plugin/wasm/`) mirroring `PluginHost`:
   holds `LoadedWasmPlugin { manifest, instance, memory, enabled: Arc<AtomicBool> }`,
   `host_weak` for callbacks into shared state (`TreeStore`, `ContributionRegistry`…).
3. **Manifest runtime field** (`crates/platform/plugin/types/src/manifest.rs`):
   `runtime: "lua" | "wasm"` (default `lua`), or infer from `entry` extension
   (`.lua`/`.wasm`); discovery filters by runtime + `targets` (e.g. `wasm32-wasip2`).
4. **`WasmApiInstaller` + `WasmNamespaceInstaller`** mirroring the Lua installers:
   each `ns::*` registers `wasmtime::Func`s into the linker, marshalling args via
   serde_json, **re-checking the plugin's capability grant on every call** (the
   shared-nothing boundary protects memory, *not* policy).
5. **Loader routing** (`runtime/host/lifecycle.rs`): `load_plugin` branches to
   `create_wasm_sandbox` when `manifest.runtime == "wasm"`.
6. **Dual-runtime `PluginHost`** (`runtime/host/mod.rs`): add `plugins_wasm`;
   `unload_all` / `invoke_service` / hook fan-out walk both slices.
7. **Product host** (`crates/products/corvus/plugin*`): `CorvusBeApiInstaller`
   unchanged; add a `CorvusBeWasmInstaller` exposing the same git namespaces to wasm
   guests (wrapping the Rust-facing `NsHost` trait, marshalling JSON).

Manifest/permission model is **runtime-agnostic** and stays: `network` allowlist,
`fs`/`fs_scope`, `git`, `terminal`, `issues`, `provider`, `service_*`, the `ext`
HashMap for domain keys. Add only `sandbox.wasm_memory_pages` (initial/max).

---

## 5. Migration targets & order

All three are feasible; sequence easiest-first so each proves a pattern for the next:

### 1) Studio — **first** (pure CPU, 100% feasible, immediate ROI)
- **Zero** runtime deps, no network/threads. Every parser is pure-Rust and compiles
  to wasm32: `simd-json` (SIMD degrades to scalar — slower, still correct), `syn`,
  `prettyplease`, `jsonc-parser`, `toml_edit`, `yaml-edit`, `serde_yaml_ng`,
  `serde_json_path`. **No FFI, no blockers.**
- Prereq: the [studio extraction](studio-extraction-plan.md) (Tauri-free
  `arbor-studio-{core,api,ron,json,toml,yaml,properties}`). The extraction is step 1
  of wasm-ification — do it in-process first (behavior-preserving), then target wasm32.
- Payoff: proves the ABI/serialization layer; removes the 5 formats + their heavy
  deps from the host binary.

### 2) BRP — **second** (~95%; needs an http/streaming host import)
- Only blocker: `reqwest` needs sockets → host provides `http_open` + streaming frame
  delivery. The seam is clean: `sse.rs::run_watch_stream` signature is unchanged, the
  https lifecycle moves *inside* the host import; the `on_event` callback stays in the
  guest. `AbortHandle`/cancellation map to dropping the host future. **Validates the
  async/streaming/cancellation model** for cloud.

### 3) Cloud — **last** (~90%; highest complexity)
- Blocker: **`aws-lc-sys` (NASM crypto) cannot link for wasm32.** Mitigations:
  (a) host-side TLS — `opendal`'s `reqwest-rustls-tls` already abstracts transport;
  (b) swap to pure-Rust `rustls`/`sha2`; (c) keep crypto host-side behind the network
  broker. Also: `keyring` stays host-only (the M3 credential-broker seam already
  exists). Becomes "apply the brp http/credential patterns to opendal" once brp lands.

**Time-to-value (discovery estimate):** studio 1–2 wk · brp 2–3 wk · cloud 3–4 wk.
Roadmap alignment: studio = M9, brp = M8–M9 foundation, cloud = M11.

---

## 6. Risks

1. **Binary bloat (biggest concrete cost).** wasmtime+Cranelift adds several MB on top
   of vendored libgit2/lua54/mlua. Mitigate (in effort order): drop CLI/clap → Winch
   baseline compiler → AOT-precompile trusted plugins to `.cwasm` + ship an
   interpreter "min" build (no in-process JIT; also removes the JIT-RWX attack
   surface, good on hardened Windows/macOS). **Measure first.**
2. **mlua coexistence.** Keep one canonical `arbor.*` dispatch (`host()`); don't let the
   Lua `sdk.d.lua` and the WIT world drift — generate/check both against
   `hook_catalog.rs`. **Threading:** the wasm async executor runs in `Future::poll`;
   per [[feedback_be_spawn_blocking_pool]] never drive it from a worker that also does
   sync framed-IPC `rx.recv()` (credential reverse-channel deadlock risk).
3. **Capability leaks.** The async host import is the *entire* attack surface. Every
   `arbor.*` import must re-check the plugin's grant (don't trust the caller); keep
   WASI preopens per-plugin and default-deny; SSE/network imports honour the
   `network` allowlist. Shared-nothing protects memory, **not** policy.
4. **Debugging.** Weakest area — wasm source-level debugging on desktop is immature.
   Invest in structured logging across the boundary (a host `log` import) from day one.
5. **WASI 0.3 timing bet.** True in-guest streaming is RC in 2026 (stable ~late 2026).
   Mitigated architecturally: host-owned SSE streams pushing to guests via hooks work
   on stable p2 today, so p3 is an enhancement, not a dependency.
6. **WIT/component-model learning curve.** One-time cost; pays back in typed, versioned,
   multi-language-ready boundaries (a plugin bug can't corrupt host memory).

---

## 7. Suggested first milestone

1. New crate `crates/platform/plugin/wasm/` (`arbor-plugin-wasm`): `WasmRuntime` +
   `WasmHookListener` + `create_wasm_sandbox`, wired into `build_hook_dispatcher`.
2. Define the `arbor.*` WIT world from `hook_catalog.rs` + the existing namespace list;
   implement host imports over the same `host()` dispatch as Lua, `add_to_linker_async`.
3. Manifest `runtime` field + discovery/loader routing.
4. Port **Studio** as the first guest (after its in-process extraction) — pure CPU,
   no broker needed → proves the whole pipeline end-to-end before tackling brp's
   network broker.

See [[project_per_program_plugins]] (WasmHookListener born on the BE), [[project_round2_impl_progress]], [[project_crate_reorg]].
