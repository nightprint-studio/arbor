# Arbor — `arbor-plugin-core` architecture & migration plan (PR #4)

Stato: **in corso**, atterraggio incrementale a sessioni.
Estende [`docs/plugin-api-architecture.md`](plugin-api-architecture.md) — questo
documento è la specifica dettagliata del **PR #4** (creazione di
`arbor-plugin-core`) e funge da tracker dei progressi tra le sessioni.

## Indice

- [Stato del refactor](#stato-del-refactor)
- [Decisioni prese (PR #4)](#decisioni-prese-pr-4)
- [Tabella tracker delle sessioni](#tabella-tracker-delle-sessioni)
- [Step dettagliati](#step-dettagliati)
- [Backward compatibility](#backward-compatibility)
- [Comandi per lanciare la prossima sessione](#comandi-per-lanciare-la-prossima-sessione)

## Stato del refactor

| PR | Crate | Stato |
|----|-------|-------|
| #1 | `arbor-core` + `arbor-scheduler` | ✅ atterrato |
| #2 | `arbor-plugin-types` | ✅ atterrato |
| #3 | `arbor-plugin-api` | ✅ atterrato |
| **#4** | **`arbor-plugin-core`** | **🚧 in corso** |
| #5 | `arbor-plugin-marketplace` | in coda |
| #6+ | `arbor-git-provider-*`, `arbor-issue-tracker-*`, `arbor-pipeline-*`, `arbor-brp` | in coda |
| finale | rinomina `src-tauri` → `arbor` | in coda |

## Decisioni prese (PR #4)

Le scelte qui sotto sono fissate (concordate con l'utente prima di iniziare).
Non vanno rimesse in discussione mid-PR.

| # | Decisione | Motivazione |
|---|-----------|-------------|
| C1 | **Scope namespace**: solo "host-pure" migrano in plugin-core in PR #4 | I ns che dipendono da `crate::git::*` / `crate::jobs::*` / `crate::pipeline::*` / `crate::workspace::*` / `crate::terminal::*` / ecc. restano in `src-tauri/src/plugin/ns_shell/` finché non nasce il loro crate di dominio (PR #6+). |
| C2 | **Pattern API**: mantenere `install(ctx, &lua, &arbor)` per i ns migrati | La conversione a `NamespaceContributor` runtime-agnostic avviene **solo** quando un namespace si sposta nel proprio crate di dominio. Niente doppio standard nello stesso crate, niente rewrite massivo in questo PR. |
| C3 | **Tauri dep**: `arbor-plugin-core` NON dipende da `tauri`, astrazione via `arbor-core::AppCtx` | `AppCtx` estesa con `record_plugin_log`. Sandbox / lifecycle / contribution / tree / event_bus consumano `Arc<dyn AppCtx>` invece di `tauri::AppHandle`. |
| C4 | **Hook bridge**: ogni call site di src-tauri migra a `dispatcher.fire(...)` diretto | Niente più `PluginHost::fire_hook(...)`. `Arc<HookDispatcher>` in `AppState`. `HookDispatcher::fire_blocking` / `fire_vetoable_blocking` aggiunti a `arbor-plugin-api` per i call site sync. |
| C5 | **Backward-compat plugin esistenti**: REQUISITO | Contratto on-the-wire invariato (`arbor.fs.read_text`, `on_commit`, `permissions.git = "write"`, …). Rename interni Rust ammessi, semantica user-facing no. |
| C6 | **Naming directory dei ns che restano in src-tauri**: `src-tauri/src/plugin/ns_shell/` | "shell" = "guscio Tauri" — più chiaro di `ns_coupled`. |

## Tabella tracker delle sessioni

| Sessione | Step | Scope | Stato | Note |
|----------|------|-------|-------|------|
| 1 | 0 + 1 | Scaffold crate + estensione AppCtx | ✅ | `crates/plugin/core` aggiunto al workspace, `src/lib.rs` + `src/prelude.rs` creati. `AppCtx::record_plugin_log` aggiunto con default no-op; `TauriAppCtx` delega a `plugin_logs::record`. |
| 2 | 2 + 3 | `Permissions.ext` + migrazione primitive cross-plugin (`contribution`, `tree`, `toolchain` state, `settings_store`, `event_bus`, `lua_ctx`) | ✅ | `Permissions.ext: HashMap<String, toml::Value>` aggiunto con `#[serde(flatten)]`; `PluginRegistry::validate_manifest` ora itera `ext` e valida contro lo schema registrato. 6 file migrati in `arbor-plugin-core::{contribution, tree, toolchain, settings_store, event_bus, lua_ctx}`; src-tauri tiene solo shim `pub use`. API swap: `ContributionRegistry::notify_changed/notify_containers_changed` non prendono più `&Option<AppHandle>` — l'`Arc<dyn AppCtx>` è installato una volta a boot via `install_app_ctx`. `lua_ctx::install` ora prende `Option<Arc<dyn AppCtx>>`; `event_bus::emit` prende `&dyn AppCtx`. `TauriAppCtx::from_handle` aggiunto per i call site sandbox-side che non hanno `focused`. |
| 3 | 4 | Sandbox + runtime (`consts`, `loaded`, `manifest`, `scheduler`, `host`) | ✅ | Migrati: `sandbox` (con trait `LuaApiInstaller` per ribaltare al guscio l'installazione del namespace `arbor.*` finché i ns non migrano in sessione 5/6), `runtime::{consts, loaded, manifest/*, scheduler/*, host/*}`, `hook_registry`, 8 file Lua in `lua_builtins/`. `host::hooks` spostato as-is per rispettare orphan rules (eliminato in sessione 7). Introdotto `PluginCoreError` con `From` bridge in src-tauri. `PluginHost::set_app_handle` rimosso a favore di `set_app_ctx` / `set_api_installer` / `set_extra_plugin_roots` — il guscio (`src-tauri/src/lib.rs` setup) installa tutti e tre al boot, prima che il thread `arbor-plugin-boot` acquisisca il mutex. Nuovo `crate::plugin::api_installer::TauriApiInstaller` fa da ponte alla `crate::plugin::api::register` esistente. Shim `crate::plugin::{sandbox,hook_registry,runtime}` ridotti a `pub use`. |
| 4 | 5 | api/ctx + api/helpers + register orchestrator | ✅ | `ApiCtx` migrato in `arbor-plugin-core::lua_api::ctx` (campo `app_handle: Option<tauri::AppHandle>` → `app_ctx: Option<Arc<dyn AppCtx>>`, fields `pub`, costruito da `ApiInstallParams::from_install_params`). 10 file helpers migrati in `arbor-plugin-core::lua_api::helpers/*` con visibilità `pub`. Introdotto trait `LuaNamespaceInstaller { fn install(&ApiCtx, &Lua, &Table) -> PluginCoreResult<()> }` + nuovo `arbor_plugin_core::lua_api::register(lua, params, &[Arc<dyn LuaNamespaceInstaller>])`. `AppCtx` esteso con `fn as_any(&self) -> &dyn Any` (+ `TauriAppCtx::handle()` accessor) per il downcast. `src-tauri/src/plugin/api/ctx.rs` diventa shim `pub use arbor_plugin_core::prelude::ApiCtx` + nuovo trait `ApiCtxExt` con metodo `app_handle() -> Option<tauri::AppHandle>` via `as_any` downcast. `helpers/mod.rs` diventa shim di `pub(crate) use` annidati. `api/mod.rs` rimpiazzato: macro `ns_installer!` genera 37 wrapper unitari `XxxInstaller: LuaNamespaceInstaller` (uno per ns del guscio), `shell_installers()` ritorna il `Vec<Arc<dyn LuaNamespaceInstaller>>` nell'ordine legacy. `TauriApiInstaller` ridotto a zero-size, ora chiama `register_lua_api(lua, params, &shell_installers())`. 37 file ns/* aggiornati via PowerShell: `ctx.app_handle.clone()` → `ctx.app_handle()`, import `ApiCtx` → `{ApiCtx, ApiCtxExt}`. |
| 5 | 6 | Migrazione ns host-pure (~22 file) | ⏳ | `log`, `events`, `json`, `text`, `meta`, `notify`, `hooks`, `command`, `keybinding`, `service`, `timer`, `scheduler`, `contribution`, `fs`, `http`, `settings`, `ui/*` (15), studios (5). |
| 6 | 7 | ns "shell" che restano in src-tauri + wiring `LuaNamespaceInstaller` | ⏳ | `repo`, `mr`, `ci`, `issues`, `notes`, `pipeline`, `cloud`, `brp`, `security`, `toolchain` (ns), `terminal`, `tabs`, `workspace`, `linked_worktrees`, `job`, `ui/branding`. |
| 7 | 8 | `HookDispatcher` + `LuaHookListener` + migrazione ~30 call site `fire_hook` | ⏳ | Step ad alto rischio. Rimozione `hook_registry.rs` + metodi `PluginHost::fire_hook` / `collect_veto`. |
| 8 | 9 + 10 + 11 | Cleanup shim + prelude finale + README + docs + sanity check | ⏳ | Rimozione di tutti i re-export `pub use` temporanei. |

Legenda: ⏳ pending · 🚧 in corso · ✅ completato · ❌ bloccato

## Step dettagliati

### Step 0 — Scaffold del crate

- Creare `crates/plugin/core/src/lib.rs` + `src/prelude.rs` (solo doccomment).
- Aggiungere `"crates/plugin/core"` al `[workspace] members` di `Cargo.toml`
  (mantenendo ordine alfabetico).
- Verificare che il `Cargo.toml` esistente di plugin/core sia coerente con
  le deps necessarie (è già scaffoldato).

**Esito atteso**: `cargo check -p arbor-plugin-core` passa standalone (no
codice ancora, solo lib vuota).

### Step 1 — Estendere `arbor-core::AppCtx`

- Aggiungere `fn record_plugin_log(&self, level: &str, plugin: &str, message: &str)`
  con default no-op.
- Implementare in `src-tauri/src/app_ctx.rs::TauriAppCtx` con delega a
  `crate::plugin_logs::record(handle, level, plugin, message)`.

**Esito atteso**: `cargo check` workspace passa.

### Step 2 — `Permissions.ext` in arbor-plugin-types

- Aggiungere `#[serde(flatten, default)] pub ext: HashMap<String, toml::Value>`
  a `Permissions`.
- Verificare backward-compat con i `plugin.toml` esistenti.
- Attivare `arbor_plugin_api::PluginRegistry::validate_manifest`: itera
  `m.permissions.ext`, lookup contro i `PermissionDef` registrati.

### Step 3 — Migrare le primitive cross-plugin in plugin-core

Spostamenti 1:1 con swap `tauri::AppHandle` → `Arc<dyn AppCtx>` e
`crate::plugin_logs::record` → `ctx.record_plugin_log`:

- `plugin/contribution.rs` (774 righe) → `arbor-plugin-core::contribution`
- `plugin/tree.rs` (263 righe) → `arbor-plugin-core::tree`
- `plugin/toolchain.rs` (306 righe, host-side state) → `arbor-plugin-core::toolchain`
- `plugin/settings_store.rs` → `arbor-plugin-core::settings_store`
- `plugin/event_bus.rs` → `arbor-plugin-core::event_bus`
- `plugin/lua_ctx.rs` → `arbor-plugin-core::lua_ctx`

Lasciare `pub use ...` shim nei file originali di src-tauri per non rompere
i ~73 call site nel resto della codebase. Rimossi nella sessione 8.

### Step 4 — Migrare runtime / sandbox / lifecycle / loaded

- `plugin/sandbox.rs` + `plugin/lua_builtins/*.lua` (8 file embedded via
  `include_str!`) → `arbor-plugin-core::sandbox` + `arbor-plugin-core::lua_builtins/`.
- `plugin/runtime/{consts,loaded,manifest/*,scheduler/*}` →
  `arbor-plugin-core::runtime::{...}`.
- `plugin/runtime/host/{mod,lifecycle,service,pipeline_op,introspection,dep_cascade}` →
  `arbor-plugin-core::host::{...}`. `hooks.rs` viene **eliminato** in sessione 7
  (sostituito dal `LuaHookListener`).
- Tutti gli `AppHandle` interni → `Arc<dyn AppCtx>`. Shim in src-tauri.

### Step 5 — Migrare api/ctx + api/helpers + api/mod

- `plugin/api/{ctx.rs, helpers/*, mod.rs}` → `arbor-plugin-core::lua_api::{ctx,helpers,...}`.
- Introdurre il trait:
  ```rust
  pub trait LuaNamespaceInstaller: Send + Sync {
      fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()>;
  }
  ```
- `register(...)` estesa: accetta `extra_installers: &[Arc<dyn LuaNamespaceInstaller>]`
  e li invoca dopo gli host-pure (preservando ordine d'origine).
- `ApiCtx` esposto via `arbor-plugin-core::prelude::ApiCtx`.

### Step 6 — Migrare i ns "host-pure" in plugin-core

Spostare in `arbor-plugin-core::lua_api::ns::{...}` (firma `install` invariata):

- **Trivial / pure-data**: `log`, `events`, `json`, `text`, `meta`, `notify`,
  `hooks`, `command`, `keybinding`, `service`, `timer`, `scheduler`,
  `contribution` (il namespace, non il registry).
- **Filesystem / network host-only**: `fs`, `http`, `settings`.
- **`ui/`** (tutti i 15 file).
- **Studios** (`json_studio`, `yaml_studio`, `toml_studio`, `ron_studio`,
  `properties_studio`).

Update dei `use crate::plugin::api::ctx::ApiCtx` → `crate::lua_api::ctx::ApiCtx`.

### Step 7 — Lasciare i ns "shell" in src-tauri + registrarli al runtime

Restano in `src-tauri/src/plugin/ns_shell/*.rs` (rename del directory):
`repo`, `mr`, `ci`, `issues`, `notes`, `pipeline`, `cloud`, `brp`, `security`,
`toolchain` (ns), `terminal`, `tabs`, `workspace`, `linked_worktrees`, `job`,
`ui/branding`.

- Ognuno espone un wrapper `pub struct FooInstaller; impl LuaNamespaceInstaller for FooInstaller`.
- `src-tauri/src/lib.rs` compone `Vec<Arc<dyn LuaNamespaceInstaller>>` al boot
  e lo passa a `register(...)`.

### Step 8 — Sostituire `hook_registry.rs` con `HookDispatcher` + `LuaHookListener`

- Creare `arbor-plugin-core::hook_router::LuaHookListener` (impl di
  `arbor_plugin_api::HookListener`).
- Spostare `matches_pattern` in `arbor-plugin-core::hook_router`.
- Aggiungere `HookDispatcher::fire_blocking(...)` / `fire_vetoable_blocking(...)`
  in `arbor-plugin-api` (wrap `futures::executor::block_on`).
- Composition root in `src-tauri/src/lib.rs::setup`: costruisce dispatcher,
  registra hook lifecycle del catalog, registra `LuaHookListener`, mette
  `dispatcher` in `AppState`.
- **Migrare i ~30 call site** `plugin_host.fire_hook(...)` →
  `dispatcher.fire(...).await` (async) o `dispatcher.fire_blocking(...)` (sync).
- Rimuovere `crate::plugin::hook_registry` e i metodi `fire_hook` /
  `fire_hook_on` / `collect_veto` da `PluginHost`. Mantenere `plugin_has_handler`
  e `remove_hook` (non sono fire).

### Step 9 — Cleanup degli shim e dei path

- Rimuovere tutti i `pub use` shim lasciati nei Step 3-5.
- Aggiornare i `use crate::plugin::...` rimasti in src-tauri ai path
  `arbor_plugin_core::prelude::...`.
- `src-tauri/src/plugin/mod.rs` ridotto a: solo modulo `ns_shell` + eventuali
  compat re-export.

### Step 10 — Prelude finale + README + docs

- `crates/plugin/core/src/prelude.rs` con re-export completo.
- `crates/plugin/core/README.md` aggiornato dallo stato "planned" → "atterrato".
- Sezione "PR #4" di `docs/plugin-api-architecture.md` aggiornata con
  riferimento a questo doc.
- Questo doc aggiornato con stato finale ✅ atterrato.

### Step 11 — Sanity check finale

- `cargo check -p arbor-plugin-core` standalone OK.
- `cargo check` workspace OK.
- Nessuna entry in CHANGELOG (refactor puramente interno).

## Backward compatibility

I plugin esistenti devono continuare a funzionare senza modifiche al
`plugin.toml` né al `main.lua` a fine PR #4. Validation:

1. Tutti i namespace `arbor.*` esposti devono mantenere stessa signature
   e semantica.
2. Tutti gli hook names del `HOOK_CATALOG` devono restare invariati.
3. Le 11 chiavi typed di `Permissions` restano typed; `ext` cattura solo
   le chiavi sconosciute.

## Comandi per lanciare la prossima sessione

A fine sessione corrente, generato all'utente come comando da incollare
per la prossima.

**Template del prompt per le sessioni 2-8**:
```
Continua PR #4 (arbor-plugin-core). Sessione N (Step X-Y).
Leggi prima `docs/plugin-core-architecture.md` per piano completo + stato.
Quando finisci la sessione: aggiorna la tabella tracker e dammi il
comando per la sessione successiva.
```
