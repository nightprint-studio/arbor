# Arbor — `arbor-plugin-api` architecture & migration roadmap

Stato: **piano approvato, non ancora implementato**. Estende
[`docs/crate-refactor.md`](crate-refactor.md) — questo documento è la
specifica dettagliata del **PR #3** (creazione di `arbor-plugin-api`) e
imposta lo stile dei PR seguenti che spostano i namespace plugin-facing
nei rispettivi crate di dominio.

## Stato del refactor

| PR | Crate | Stato |
|----|-------|-------|
| #1 | `arbor-core` + `arbor-scheduler` | ✅ atterrato |
| #2 | `arbor-plugin-types` | ✅ atterrato |
| #3 | `arbor-plugin-api` | ✅ atterrato |
| #4 | `arbor-plugin-core` (mlua runtime + namespace migration) | ✅ atterrato |
| #5 | `arbor-plugin-marketplace` | in coda |
| #6+ | `arbor-git-provider-*`, `arbor-issue-tracker-*`, `arbor-pipeline-*`, `arbor-brp` | in coda |
| finale | rinomina `src-tauri` → `arbor` | in coda |

## Perché un altro PR per il plugin system

Il piano originale prevedeva `arbor-plugin-api` come "hook dispatcher
name-agnostic + Registry listener". Mentre si pianifica si è deciso di
allargare la portata del crate per assorbire **quattro obiettivi
architetturali** che altrimenti contaminerebbero ogni PR successivo:

1. **Multi-runtime ready** — l'API dei plugin oggi è completamente
   intrecciata con `mlua`. Vogliamo strutturarla in modo che un secondo
   runtime (WASM, oppure altro embedded language) si possa aggiungere come
   adapter senza riscrivere niente del codice di dominio. *Non* integriamo
   wasm in questo PR — prepariamo solo il terreno.

2. **Function registration distribuita** — oggi `plugin/api/ns/*.rs`
   raccoglie tutti i namespace (`arbor.fs`, `arbor.repo`, `arbor.terminal`,
   …) in src-tauri. Vogliamo che ogni crate di dominio
   (`arbor-git-provider-api`, `arbor-issue-tracker-api`, …) contribuisca i
   propri namespace e le proprie funzioni plugin-facing. src-tauri torna ad
   essere "il guscio Tauri", non più "il mega-core che implementa tutto".

3. **Hook registration distribuita** — `HOOK_CATALOG` in
   `arbor-plugin-types` è static + centralizzato. Lo trasformiamo in
   registry dinamico popolato al boot, alimentato da ogni crate che emette
   gli hook (i `on_mr_*` stanno nel git-provider crate, i `on_issue_*`
   nell'issue-tracker crate, ecc.).

4. **Permission custom per crate** — lo struct `Permissions` ha 11 campi
   tipati fissi. Vogliamo che ogni crate dichiari le sue permission custom
   senza dover toccare `arbor-plugin-types`.

## Decisioni prese

Le scelte qui sotto sono **fissate**: chi implementa il PR può procedere
senza richiamarle in causa. Sono tutte motivate da discussioni in
chat — la motivazione è riassunta una riga sotto ogni decisione.

| # | Decisione | Motivazione |
|---|-----------|-------------|
| D1 | Value bridging = `PluginValue` enum, non `serde_json::Value` né generic | In-process è ~5-10× più economico di JSON; generic-via-trait esplode in monomorfizzazione |
| D2 | Permissions = **core tipati + `ext: HashMap<String, toml::Value>`** | Backward-compat con `plugin.toml` esistenti; hot path (fs, terminal) restano type-safe |
| D3 | Plugin self-extension (plugin che registrano fn/hook/perm) = **post-MVP** | Il registry è dinamico per design, aprire ai plugin si fa in un PR successivo banale |
| D4 | Ordine PR: `arbor-plugin-core` **prima** di `arbor-plugin-marketplace` | Marketplace atterra già col pattern nuovo, niente refactor doppio |
| D5 | Permission-per-funzione = **solo per fn contribuite dai crate Arbor** | I plugin Lua usano i permission del loro `plugin.toml`; cross-crate permission ownership stays clean |
| D6 | `PluginFn::call` = **async via `async_trait`** | Le fn di dominio (HTTP, GraphQL, libgit2) sono naturalmente async; mlua ha feature `async` |
| D7 | Hook vetoable = **flag su `HookDef` + due API separate** | `fire(name, ctx)` vs `fire_vetoable(name, ctx) -> Option<String>` — type-safe al call site |
| D8 | `HookDispatcher` = **metadata router + broker di `HookListener`** | Il dispatcher non conosce mlua; ogni runtime registra un listener al boot |
| D9 | Nomi hook = **`<prodotto>:<evento>`, con prefisso facoltativo alla sottoscrizione** | Estende la regola che `arbor.events.emit` già applica ai plugin (`<plugin>:<evento>`); rende la collisione strutturalmente impossibile invece che evitata a mano |
| D10 | Ogni nome hook = **una costante Rust**, mai una stringa scritta a mano al call site | Oggi un nome sbagliato in `fire_hook` compila ed è un no-op silenzioso per sempre; con le costanti diventa un errore di compilazione |

### D9/D10 — namespacing degli hook

**Il problema.** I *metodi* Lua sono già namespaced (`arbor.repo.*`, `arbor.notes.*`,
`arbor.job.*` — è la ragione per cui esiste `corvus-plugin-ns`). Gli *hook* no: vivono in un
unico spazio piatto, e l'arrivo di Garrulus l'ha dimostrato — `on_note_saved` significava già
"git note salvata" in Corvus e ha significato "nota del vault salvata" per un giro. Gli hook
sono l'unica parte della superficie Lua che non segue la regola già in vigore.

**Il precedente da estendere, non da inventare.** `arbor.events.emit`
(`plugin/core/src/lua_api/ns/events.rs`) risolve già così un evento non qualificato:

```rust
let full_event = match event.find(':') {
    None => format!("{}:{}", pname, event),   // il prefisso è FACOLTATIVO
    Some(_) => /* già qualificato: si usa com'è */,
};
```

Stesso separatore, stessa regola, stessa opzionalità. Per gli hook il prefisso implicito non è
il nome del plugin ma **l'id del prodotto ospite** (`PRODUCT_GARRULUS`, `PRODUCT_CORVUS`, …),
e la risoluzione è priva di ambiguità perché il dispatcher è già per-prodotto
(`App::plugin_host(product_id, …)`): dentro `garrulus-be`, `arbor.events.on("note_saved", fn)` non
può voler dire altro che `garrulus:note_saved`.

Ne cade fuori gratis anche `arbor.events.on("garrulus:*", fn)` — `install_on` gestisce già il
wildcard.

**Le costanti.** Il prodotto compare **una volta sola**, e ogni nome è una costante:

```rust
// crates/products/garrulus/core/src/hooks.rs
pub const NS: &str = PRODUCT_GARRULUS;          // l'unico posto dove si scrive "garrulus"
pub const NOTE_SAVED:   &str = concat_ns!(NS, "note_saved");
pub const SYNC_DONE:    &str = concat_ns!(NS, "sync_done");
```

Le stesse costanti alimentano il catalogo, così catalogo e call-site non possono divergere.
Il motivo vero non è l'estetica: oggi `fire_hook("on_note_savd", …)` compila, non spara niente
e nessuno se ne accorge mai — in entrambe le direzioni, perché anche una voce di catalogo
scritta male non fallisce. Le costanti trasformano metà di quel buco in un errore di
compilazione.

**L'altra metà del buco resta lato Lua**, dove non c'è compilazione: `arbor.events.on("note_savd", fn)`
si risolve in `garrulus:note_savd` e non spara mai. Namespacing e costanti non lo chiudono. Lo
chiude **validare il nome risolto contro il catalogo al momento della sottoscrizione** e
avvisare — il catalogo dinamico per-dominio esiste già (`plugin/api/src/hook.rs`), quindi è
poco lavoro. Vale la pena farlo nella stessa passata: è il difetto più costoso dei tre, perché
si manifesta come "il plugin non fa niente" senza un solo messaggio.

**Portata.** Sweep completa, non solo i prodotti nuovi: ~50 voci di catalogo, ogni `fire_hook`,
l'SDK (`sdk.d.lua`, repo `arbor-extensions`) e le docs. Mezza migrazione su una API costa più
della migrazione intera, e CLAUDE.md ammette esplicitamente i breaking change qui perché
l'unico consumatore dei plugin è l'utente.

## Architettura

### Layout del crate

```
crates/plugin/api/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── prelude.rs
    ├── value.rs        // PluginValue (enum cross-runtime)
    ├── error.rs        // PluginError (thiserror)
    ├── ctx.rs          // PluginCtx (trait async-safe)
    ├── func.rs         // PluginFn (async_trait) + NamespaceFn entry
    ├── perm.rs         // PermissionDef + PermSchema + PermReq
    ├── hook.rs         // HookDef + HookKind (estende plugin-types)
    ├── namespace.rs    // NamespaceContributor trait
    ├── registry.rs     // PluginRegistry (raccoglitore)
    └── dispatcher.rs   // HookDispatcher + HookListener
```

### Cargo.toml

```toml
[package]
name         = "arbor-plugin-api"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
authors.workspace      = true
description  = "Plugin extension API: namespaces, hooks, permissions, dispatcher. Runtime-agnostic — mlua / wasm adapters live elsewhere."

[dependencies]
arbor-plugin-types = { path = "../types" }
serde       = { workspace = true }
serde_json  = { workspace = true }
thiserror   = { workspace = true }
async-trait = { workspace = true }
toml        = "0.8"
```

Niente `mlua`, niente `tauri`, niente `arbor-core`.

### Tipi chiave

```rust
// value.rs
pub enum PluginValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<PluginValue>),
    Map(BTreeMap<String, PluginValue>),
}

impl PluginValue {
    pub fn from_serializable<T: serde::Serialize>(v: &T) -> Result<Self, PluginError>;
    pub fn as_map(&self) -> Option<&BTreeMap<String, PluginValue>>;
    // …helper for typed extraction (get_string, get_int, get_bool, …)
}

// error.rs
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("permission denied: '{0}' requires '{1}'")]
    PermissionDenied(String, String),
    #[error("bad args: {0}")]
    BadArgs(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("domain: {0}")]
    Domain(String),
    #[error("{0}")]
    Other(String),
}

// ctx.rs
pub trait PluginCtx: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn manifest(&self) -> &arbor_plugin_types::prelude::Manifest;
    /// Lookup di un permission (core o ext).
    fn permission(&self, key: &str) -> Option<&toml::Value>;
    /// App-level event broadcast (oggi tauri::Emitter, domani altro).
    fn emit_app(&self, event: &str, payload: PluginValue);
}

// func.rs
#[async_trait]
pub trait PluginFn: Send + Sync {
    async fn call(
        &self,
        ctx:  &(dyn PluginCtx + Sync),
        args: PluginValue,
    ) -> Result<PluginValue, PluginError>;
}

pub struct NamespaceFn {
    pub namespace: &'static str,
    pub name:      &'static str,
    pub requires:  &'static [PermReq],
    pub body:      Arc<dyn PluginFn>,
}

// hook.rs
pub enum HookKind {
    FireAndForget,
    Vetoable,
}

pub struct HookDef {
    pub name:        &'static str,
    pub category:    &'static str,
    pub description: &'static str,
    pub kind:        HookKind,
    pub ctx:         &'static [arbor_plugin_types::prelude::HookField],
}

// perm.rs
pub enum PermSchema {
    Bool,
    String,
    StringList,
    Enum(&'static [&'static str]),   // ordered low → high per AtLeast
}

pub enum PermReq {
    Has(&'static str),
    AtLeast(&'static str, &'static str),
    Equals(&'static str, &'static str),
}

pub struct PermissionDef {
    pub key:         &'static str,
    pub schema:      PermSchema,
    pub default:     toml::Value,
    pub description: &'static str,
    pub requires:    &'static [PermReq],
}

// namespace.rs
pub trait NamespaceContributor {
    fn contribute(&self, reg: &mut PluginRegistry);
}

// registry.rs
pub struct PluginRegistry {
    namespaces:  HashMap<&'static str, BTreeMap<&'static str, NamespaceFn>>,
    permissions: HashMap<&'static str, PermissionDef>,
    hooks:       HashMap<&'static str, HookDef>,
}

impl PluginRegistry {
    pub fn new() -> Self;
    pub fn register_fn(&mut self, f: NamespaceFn);
    pub fn register_hook(&mut self, h: HookDef);
    pub fn register_permission(&mut self, p: PermissionDef);
    pub fn lookup_fn(&self, ns: &str, name: &str) -> Option<&NamespaceFn>;
    pub fn validate_manifest(&self, m: &Manifest) -> Result<(), Vec<ManifestPermError>>;
    pub fn iter_hooks(&self) -> impl Iterator<Item = &HookDef>;
    pub async fn invoke(
        &self,
        ctx:  &(dyn PluginCtx + Sync),
        ns:   &str,
        name: &str,
        args: PluginValue,
    ) -> Result<PluginValue, PluginError>;
}

// dispatcher.rs
#[async_trait]
pub trait HookListener: Send + Sync {
    async fn fire(&self, name: &str, ctx: &PluginValue);
    async fn fire_vetoable(&self, name: &str, ctx: &PluginValue) -> Option<String>;
}

pub struct HookDispatcher {
    hooks:     HashMap<&'static str, HookDef>,
    listeners: Vec<Arc<dyn HookListener>>,
}

impl HookDispatcher {
    pub fn new() -> Self;
    pub fn register_hook(&mut self, h: HookDef);
    pub fn register_listener(&mut self, l: Arc<dyn HookListener>);
    pub fn lookup(&self, name: &str) -> Option<&HookDef>;

    pub async fn fire(&self, name: &str, ctx: PluginValue) {
        for l in &self.listeners { l.fire(name, &ctx).await; }
    }
    pub async fn fire_vetoable(&self, name: &str, ctx: PluginValue) -> Option<String> {
        for l in &self.listeners {
            if let Some(reason) = l.fire_vetoable(name, &ctx).await { return Some(reason); }
        }
        None
    }
}
```

### Pattern: come un crate contribuisce

Esempio canonico — `arbor-git-provider-api` aggiunge l'hook `on_mr_created`,
la permission `gitprovider`, e due funzioni: `mr_detail` (read) e
`create_mr` (write, fire hook al success).

```rust
// crates/git-provider/api/src/plugin_api.rs
use std::sync::Arc;
use async_trait::async_trait;

use arbor_plugin_api::prelude::*;
use crate::registry::GitProviderRegistry;
use crate::types::MergeRequest;

fn permissions() -> Vec<PermissionDef> {
    vec![PermissionDef {
        key: "gitprovider",
        schema: PermSchema::Enum(&["none", "read", "write"]),
        default: toml::Value::String("none".into()),
        description: "GitHub PR / GitLab MR API access",
        requires: &[],
    }]
}

fn hooks() -> Vec<HookDef> {
    vec![HookDef {
        name: "on_mr_created",
        category: "mr",
        description: "Fired after a MR is created via arbor.gitprovider.create_mr.",
        kind: HookKind::FireAndForget,
        ctx: &[
            HookField { name: "number",        ty: FieldType::Number, required: true,
                        description: "Provider-side MR number." },
            HookField { name: "title",         ty: FieldType::String, required: true,
                        description: "MR title." },
            HookField { name: "source_branch", ty: FieldType::String, required: true,
                        description: "Source branch." },
            HookField { name: "target_branch", ty: FieldType::String, required: true,
                        description: "Target branch." },
            HookField { name: "provider",      ty: FieldType::String, required: true,
                        description: "'github' | 'gitlab'." },
            HookField { name: "web_url",       ty: FieldType::String, required: true,
                        description: "Provider URL for the MR." },
        ],
    }]
}

struct MrDetailFn { registry: Arc<dyn GitProviderRegistry> }

#[async_trait]
impl PluginFn for MrDetailFn {
    async fn call(&self, _ctx: &(dyn PluginCtx + Sync), args: PluginValue)
        -> Result<PluginValue, PluginError>
    {
        let args   = args.as_map().ok_or(PluginError::bad_args("expected table"))?;
        let tab_id = args.get_string("tab_id")?;
        let number = args.get_int("number")?;
        let provider = self.registry.provider_for_tab(&tab_id).await
            .ok_or(PluginError::not_found("no provider for tab"))?;
        let mr: MergeRequest = provider.fetch_mr(number as u32).await
            .map_err(PluginError::domain)?;
        PluginValue::from_serializable(&mr)
    }
}

struct CreateMrFn {
    registry:   Arc<dyn GitProviderRegistry>,
    dispatcher: Arc<HookDispatcher>,
}

#[async_trait]
impl PluginFn for CreateMrFn {
    async fn call(&self, _ctx: &(dyn PluginCtx + Sync), args: PluginValue)
        -> Result<PluginValue, PluginError>
    {
        let args = args.as_map().ok_or(PluginError::bad_args("expected table"))?;
        let tab_id = args.get_string("tab_id")?;
        let title  = args.get_string("title")?;
        let source = args.get_string("source_branch")?;
        let target = args.get_string("target_branch")?;

        let provider = self.registry.provider_for_tab(&tab_id).await
            .ok_or(PluginError::not_found("no provider for tab"))?;
        let mr = provider.create_mr(&title, &source, &target).await
            .map_err(PluginError::domain)?;

        let payload = PluginValue::from_serializable(&serde_json::json!({
            "number":        mr.number,
            "title":         mr.title,
            "source_branch": mr.source_branch,
            "target_branch": mr.target_branch,
            "provider":      mr.provider_kind.as_str(),
            "web_url":       mr.web_url,
        }))?;
        self.dispatcher.fire("on_mr_created", payload).await;

        PluginValue::from_serializable(&mr)
    }
}

pub struct GitProviderContributor {
    pub registry:   Arc<dyn GitProviderRegistry>,
    pub dispatcher: Arc<HookDispatcher>,
}

impl NamespaceContributor for GitProviderContributor {
    fn contribute(&self, reg: &mut PluginRegistry) {
        for p in permissions() { reg.register_permission(p); }
        for h in hooks()       { reg.register_hook(h); }

        reg.register_fn(NamespaceFn {
            namespace: "gitprovider",
            name:      "mr_detail",
            requires:  &[PermReq::AtLeast("gitprovider", "read")],
            body:      Arc::new(MrDetailFn { registry: self.registry.clone() }),
        });

        reg.register_fn(NamespaceFn {
            namespace: "gitprovider",
            name:      "create_mr",
            requires:  &[PermReq::AtLeast("gitprovider", "write")],
            body:      Arc::new(CreateMrFn {
                registry:   self.registry.clone(),
                dispatcher: self.dispatcher.clone(),
            }),
        });
    }
}
```

Stessa simmetria per `arbor-issue-tracker-api` con `on_ticket_created` +
`ticket_detail` + `create_ticket`. I due crate non si conoscono.

### Composition root in src-tauri (visione target)

> **Nota (post-PR #4):** lo snippet sotto è la *visione target* (PR #6+, con
> i contributor di dominio). Il PR #4 è atterrato col pattern più
> conservativo descritto nella sezione [PR #4](#pr-4--arbor-plugin-core--atterrato):
> i namespace host-pure restano `install(&ApiCtx, &Lua, &Table)` dentro
> `arbor_plugin_core::lua_api::ns::*`, quelli del guscio sono
> `LuaNamespaceInstaller` in `src-tauri/src/plugin/ns_shell/*` iniettati via
> `register(lua, params, &shell_installers())`. Le free fn
> `contribute_host_namespaces` / `contribute_host_hooks` qui sotto non sono
> mai esistite con quei nomi.

```rust
// src-tauri/src/lib.rs (excerpt, post-PR #4)
fn setup_plugin_system(
    app_handle:        tauri::AppHandle,
    git_provider_reg:  Arc<dyn GitProviderRegistry>,
    issue_tracker_reg: Arc<dyn IssueTrackerRegistry>,
) -> Arc<LuaRuntime> {
    let dispatcher = Arc::new(HookDispatcher::new());
    let mut reg = PluginRegistry::new();

    // ── Host-side namespaces (fs, terminal, ui, settings, jobs, …) ────────
    arbor_plugin_core::contribute_host_namespaces(&mut reg, &app_handle);
    // ── Host-side lifecycle hooks (on_plugin_*, on_repo_*, on_workspace_*) ─
    arbor_plugin_core::contribute_host_hooks(&mut reg);

    // ── Crate-contributed namespaces, hook e permission ───────────────────
    GitProviderContributor   { registry: git_provider_reg.clone(),  dispatcher: dispatcher.clone() }
        .contribute(&mut reg);
    IssueTrackerContributor  { registry: issue_tracker_reg.clone(), dispatcher: dispatcher.clone() }
        .contribute(&mut reg);
    PipelineContributor      { dispatcher: dispatcher.clone(), /* … */ }
        .contribute(&mut reg);

    // Materializza il registry in mlua + registra il LuaRuntime come
    // HookListener del dispatcher.
    let runtime = LuaRuntime::install(app_handle, Arc::new(reg), dispatcher.clone());
    // Domani: anche WasmRuntime::install(..., dispatcher.clone());
    runtime
}
```

### Sequence di un fire end-to-end

```
1. UI Svelte → Tauri command arbor::commands::mr::create_mr(tab_id, …)
2. command  → arbor_git_provider_api impl create_mr (HTTP/GraphQL)
3. impl     → dispatcher.fire("on_mr_created", payload).await
              └── itera Vec<Arc<dyn HookListener>>:
                   a. LuaHookListener.fire:
                      └── per ogni plugin Lua subscribed (hooks.on_mr_created=true):
                          ├── deserializza PluginValue → mlua::Table
                          ├── invoca arbor.hook_registry['on_mr_created'](ctx)
                          └── drop errori (logged), non bloccare la catena
                   b. (futuro) WasmHookListener.fire: stessa cosa
4. command ritorna il MR alla UI
```

Il dispatcher **non sa né cura** quanti plugin Lua ci sono, né se ci sono
plugin wasm. I listener lo sanno. Aggiungere un runtime nuovo = registrare
un listener nuovo al boot, niente altro.

## Cosa NON facciamo in PR #3

Esplicitamente fuori scope (vivono in PR successivi):

- **Migrazione dei namespace concreti** (`fs`, `repo`, `ui`, `terminal`,
  `settings`, `job`, `command`, …). Restano in `src-tauri/src/plugin/api/ns/*`
  invariati. La migrazione al pattern `NamespaceContributor` è il **PR #4**
  (`arbor-plugin-core`).
- **Cambio strutturale a `arbor-plugin-types::Permissions`**. I 11 campi
  fissi restano. L'aggiunta di `#[serde(flatten)] ext: HashMap<String,
  toml::Value>` arriva in PR #4 quando serve davvero.
- **Migrazione di `HOOK_CATALOG`** in `arbor-plugin-types`. Resta static. Il
  nuovo `HookDef` vive accanto e viene riempito dinamicamente dai
  contributor — la convergenza arriva quando spostiamo gli hook nei crate
  di dominio.
- **Sostituzione di `hook_registry.rs`** in src-tauri. Resta invariato. Il
  `HookDispatcher` di plugin-api è dormiente, viene wired-up in PR #4.
- **`arbor.service.call` / `arbor.service.export`**: ridisegnabili ma
  restano in src-tauri/plugin-core. Non in plugin-api.
- **Plugin che dichiarano fn / hook / permission a runtime** (D3 — post-MVP).

## Backward compatibility

I plugin esistenti devono **continuare a funzionare senza modifiche al
`plugin.toml` né al `main.lua`** dopo l'atterraggio di tutti i PR di
questa famiglia. I rename interni (struct names, paths Rust) sono OK; il
contratto on-the-wire con i plugin (`arbor.fs.read_text`, `on_commit`,
`permissions.git = "write"`, …) deve essere preservato.

In PR #3 questo è banale perché niente è ancora cablato. In PR #4 va
verificato eseguendo i plugin reali.

## Roadmap di PR #3

Ordine consigliato di lavoro, ciascuno self-contained:

### Step 1 — Scaffold del crate

- Crea `crates/plugin/api/Cargo.toml` con dependencies sopra elencate.
- Crea `src/lib.rs` + `src/prelude.rs` con la stessa struttura di
  `arbor-core` / `arbor-plugin-types`.
- Aggiungi `"crates/plugin/api"` al `[workspace] members` del root
  `Cargo.toml` (mantenendo l'ordine alfabetico già in uso).
- Aggiungi `arbor-plugin-api = { path = "../crates/plugin/api" }` alle
  `[dependencies]` di `src-tauri/Cargo.toml` (anche se non ancora
  consumato — così `cargo check` del workspace fallisce subito se
  qualcosa si rompe).
- `cargo check -p arbor-plugin-api` deve passare.

### Step 2 — `value.rs` + `error.rs`

- `PluginValue` enum con le 8 varianti.
- Helper `from_serializable<T: serde::Serialize>` che passa per
  `serde_json::Value` interno e converte.
- Helper di estrazione: `as_map`, `as_string`, `as_int`, `as_bool`,
  `as_list`, `as_bytes`, e l'API ergonomica `get_string("key")`,
  `get_int("key")`, `get_string_opt("key")` su `Map`.
- `PluginError` thiserror enum con i variants nello snippet sopra.
- Test unitari per le conversioni round-trip JSON ↔ PluginValue (importante
  per evitare drift cross-runtime).

### Step 3 — `perm.rs`

- `PermSchema` enum (Bool, String, StringList, Enum).
- `PermReq` enum (Has, AtLeast, Equals).
- `PermissionDef` struct con tutti i campi.
- Helper `PermSchema::validate(&self, value: &toml::Value) -> Result<(), String>` —
  controlla che un valore TOML matchi lo schema dichiarato.
- Helper `PermReq::check(&self, perms: &PermissionsView) -> Result<(), PluginError>` —
  controlla un singolo requisito contro la vista delle permission di un
  plugin (`PermissionsView` è un trait/wrapper esposto dal `PluginCtx`).

### Step 4 — `hook.rs`

- `HookKind` enum.
- `HookDef` struct (riusa `HookField` / `FieldType` da
  `arbor-plugin-types::hook_catalog`).
- Niente catalog static qui — il catalog è il `PluginRegistry`.

### Step 5 — `ctx.rs` + `func.rs`

- `PluginCtx` trait (sync methods, no `async fn` per dyn-safety).
- `PluginFn` trait con `#[async_trait]`.
- `NamespaceFn` struct.

### Step 6 — `namespace.rs` + `registry.rs`

- `NamespaceContributor` trait (sync `contribute` method).
- `PluginRegistry` struct + tutti i metodi `register_*` / `lookup_fn` /
  `invoke` / `validate_manifest` / `iter_hooks`.
- `invoke` fa il permission gate (controlla `f.requires` contro
  `ctx.permission(...)`) **prima** di chiamare `f.body.call(...)`.
- `validate_manifest` itera `m.permissions.ext` (quando esisterà — per ora
  no-op che ritorna `Ok(())`) e controlla ogni chiave contro il
  `PermissionDef` registrato.

### Step 7 — `dispatcher.rs`

- `HookListener` trait con `#[async_trait]`.
- `HookDispatcher` struct + metodi `register_*` / `fire` / `fire_vetoable`
  / `lookup`.

### Step 8 — `prelude.rs` finale

Re-export di tutto pubblico:

```rust
pub use crate::ctx::PluginCtx;
pub use crate::dispatcher::{HookDispatcher, HookListener};
pub use crate::error::PluginError;
pub use crate::func::{NamespaceFn, PluginFn};
pub use crate::hook::{HookDef, HookKind};
pub use crate::namespace::NamespaceContributor;
pub use crate::perm::{PermissionDef, PermReq, PermSchema};
pub use crate::registry::PluginRegistry;
pub use crate::value::PluginValue;
// Re-export dei tipi shared da plugin-types per ergonomia delle implementazioni:
pub use arbor_plugin_types::prelude::{FieldType, HookField, Manifest};
```

### Step 9 — README + smoke test

- `crates/plugin/api/README.md` analogo a quello di `arbor-plugin-types`:
  Purpose / Contents / Depends on / Consumed by (planned) / Public API: the
  prelude / Notes.
- `cargo check -p arbor-plugin-api` passa standalone (senza linkare
  src-tauri).
- `cargo check` del workspace passa (src-tauri compila perché nessun
  consumer è ancora cablato — il crate è "library only").
- **Niente CHANGELOG entry** — refactor puramente interno, zero
  user-facing.

## Roadmap dopo PR #3 (preview)

Solo per dare la traiettoria — i dettagli si decideranno PR per PR:

### PR #4 — `arbor-plugin-core` ✅ atterrato

Il PR grosso. Piano dettagliato + tracker delle sessioni in
[`docs/plugin-core-architecture.md`](plugin-core-architecture.md). Cosa è
effettivamente atterrato (alcune scelte divergono da questa preview, presa
dall'ottica del PR #3):

- Runtime mlua, sandbox, lifecycle, hook routing e la fetta "host-pure"
  della superficie `arbor.*` vivono in `arbor-plugin-core`. Il crate
  **non** dipende da `tauri`: l'astrazione passa per `arbor-core::AppCtx`.
- I namespace host-side **mantengono** la firma `install(&ApiCtx, &Lua,
  &Table)` — niente conversione a `NamespaceContributor` in questo PR
  (decisione C2). Gli host-pure atterrano in
  `arbor_plugin_core::lua_api::ns::*`; quelli ancora accoppiati al guscio
  (`git::*`, `pipeline::*`, `jobs::*`, `terminal::*`, `workspace::*`,
  `brp::*`, `cloud::*`, …) restano in `src-tauri/src/plugin/ns_shell/*`
  come impl di `LuaNamespaceInstaller` e vengono iniettati al boot via
  `register(lua, params, &shell_installers())`.
- `src-tauri/src/plugin/runtime/host/*` (lifecycle, service, pipeline_op,
  introspection, dependency cascade) migrato in
  `arbor_plugin_core::runtime::host`.
- `hook_registry.rs` sostituito da `arbor_plugin_core::hook_router` +
  `LuaHookListener`, registrato sul `HookDispatcher` di `arbor-plugin-api`
  (con `fire_blocking` / `fire_vetoable_blocking` per i call site sync).
- `Permissions.ext: HashMap<String, toml::Value>` aggiunto in
  `arbor-plugin-types` (catch-all backward-compatible, validato contro i
  `PermissionDef` registrati).

### PR #5 — `arbor-plugin-marketplace`

Marketplace + installer + scheduler refresh, già col pattern nuovo. Non
contribuisce namespace plugin-facing, quindi il refactor architetturale
non lo tocca direttamente — ma dipende da `arbor-plugin-types` e
`arbor-scheduler` (entrambi pronti).

### PR #6+ — Crate di dominio

Ognuno contribuisce namespace + hook + permission per il suo dominio:

- `arbor-git-provider-api` → `arbor.gitprovider.*` + `on_mr_*` + perm
  `gitprovider`.
- `arbor-issue-tracker-api` → `arbor.issues.*` + `on_issue_*` + perm
  `issues`.
- `arbor-pipeline-api` → `arbor.pipeline.*` + `on_pipeline_*` + perm
  `pipeline`.
- `arbor-brp` → `arbor.brp.*` (eventualmente).

A quel punto `HOOK_CATALOG` in `arbor-plugin-types` si riduce ai soli
hook lifecycle (`on_plugin_load`, `on_plugin_unload`) e gli hook
shell-level (`on_repo_*`, `on_tab_switch`, `on_workspace_*`,
`on_theme_changed`) si spostano in `arbor-plugin-core::contribute_host_hooks`.

## Riferimenti

- [`docs/crate-refactor.md`](crate-refactor.md) — piano generale dei
  crate del workspace.
- [`crates/plugin/types/README.md`](../crates/plugin/types/README.md) —
  crate di tipi (PR #2, già atterrato).
- [`CLAUDE.md`](../CLAUDE.md) — working agreement del repo.
