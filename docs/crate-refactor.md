# Arbor — Workspace crate split (round 1)

Stato: **piano**, niente codice ancora migrato. I `Cargo.toml` dei nuovi
crate sono già in `crates/`, ma non sono cablati nel `[workspace] members`
e non hanno `src/lib.rs` — sono shell. Si attivano un crate per volta nei PR
successivi.

## Perché

Oggi quasi tutto vive in `src-tauri/`. Il backend è cresciuto al punto che:

- esistono pattern duplicati in 18+ posti (es. `dirs::config_dir().…join("arbor")`),
- più moduli costruiscono `reqwest::Client` con default leggermente diversi,
- l'API GitHub per risolvere uno SHA è scritta due volte con `struct Resp { sha: String }` anonima,
- lo scheduler del marketplace e quello dei plugin Lua sono due implementazioni separate dello stesso loop fixed_rate / fixed_delay / cron,
- l'errore comune (`AppError::Other(format!(...))`) annega ogni informazione tipata prima che arrivi al frontend.

Lo split serve a:

1. **Eliminare le duplicazioni** una volta sola, in un crate dedicato (`arbor-core`).
2. **Aprire la strada** ai trait con impl multiple già presenti nel codice (`GitProvider`, e a venire `IssueTracker`, `PipelineStep`) come crate di API + crate di impl indipendenti.
3. **Disaccoppiare** da Tauri i moduli che non hanno bisogno di `AppHandle` direttamente — testing più facile, build incrementale più veloce, e preparazione futura a host non-Tauri (CLI tool, daemon headless).
4. **Preparare il terreno** alla migrazione WASM dei plugin (gli Studio, cloud-storage) — più boundary chiari oggi = cut più netto domani.

## Principi

- **Prefisso `arbor-`** per ogni crate (consistenza con `arbor-auth` / `arbor-cloud` / `arbor-process-ext` già esistenti; consente pubblicazione su crates.io senza collisioni).
- **Path sul filesystem libero**: `crates/plugin/types/Cargo.toml` con `name = "arbor-plugin-types"` è OK — il path serve solo a leggere meglio il workspace.
- **`*-api` invece di `*-registry`**: meno overloaded in Rust, dice chiaramente "questo è ciò che `arbor` consuma".
- **Niente `tauri::*` nei crate di dominio**: `arbor-core` definisce un trait `AppCtx` minimale; `arbor` (il guscio Tauri) lo implementa con `AppHandle` dentro.
- **Errore per dominio** (`MarketplaceError`, `IssueTrackerError`, …) con `From<...> for AppError` al boundary `arbor`.
- **`async_trait`** sui trait di dominio per coerenza con `GitProvider` esistente.
- **Niente `dirs::config_dir()` ad-hoc**: `arbor-core::paths` centralizza tutto.

## Blueprint

```
arbor-core                          paths, http builder, AppError base, AppCtx trait
arbor-scheduler                     engine standalone (FixedRate/FixedDelay/Cron + cancel + focus gate)
arbor-brp                           JSON-RPC + SSE client (oggi solo Bevy, futuro: plugin Lua, poi sparisce)

arbor-plugin-types                  manifest, permissions, hook catalog (solo nomi), deps, schedule, PluginConfig
arbor-plugin-api                    hook dispatcher (name-agnostic) + Registry listener
arbor-plugin-marketplace            community + custom + installer + MarketplaceConfig
arbor-plugin-core                   PluginHost mlua runtime, lifecycle, sandbox, api/ns/*

arbor-git-provider-api              DTOs (Pr, Mr, Release, …) + trait GitProvider + Registry
arbor-git-provider-github           impl + GithubClient pubblico (HTTP + auth + parser base)
arbor-git-provider-gitlab           impl + GitlabClient pubblico

arbor-issue-tracker-api             DTOs (Issue, Comment, IssueState) + trait IssueTracker + Registry
arbor-issue-tracker-github          impl GitHub Issues, dep → arbor-git-provider-github (riusa GithubClient)
arbor-issue-tracker-gitlab          impl GitLab Issues, dep → arbor-git-provider-gitlab
arbor-issue-tracker-jira            standalone
arbor-issue-tracker-linear          standalone

arbor-pipeline-api                  DTOs + trait step esterni + Registry + PipelineConfig
arbor-pipeline-core                 orchestratore, run state machine

arbor-cloud                         (esistente)
arbor-auth                          (esistente)
arbor-process-ext                   (esistente)

arbor                               (era src-tauri/) — tauri shell, commands, setup, impl AppCtx
                                    contiene ancora: git/, git_cli/, studio/, *_studio/, workspace/,
                                                     linked_worktrees/, terminal/, config aggregator
```

### Round 2 (futuro, non in questo refactor)

Roadmap operativa: [`docs/migration-roadmap.md`](migration-roadmap.md).
Piano/analisi: [`docs/crate-refactor-round2.md`](crate-refactor-round2.md):
scorporo app-standalone (nemus, esplora risorse), crate `arbor-fs`, e il layer
plugin WebAssembly (wasmtime + ABI su `PluginValue`; candidati: studi,
cloud-storage, db-query). Crate ancora previsti qui:

- `arbor-git` (libgit2 + cli, due moduli interni)
- `arbor-fs` (operazioni FS pure, consumate da esplora + comandi generici)
- `arbor-workspaces` (la feature multi-repo)
- `arbor-linked-worktrees`
- `arbor-terminal`
- `arbor-studio-core` + `arbor-studio-{ron,yaml,json,toml,properties}` post-WASM

## Dipendenze cross-crate

```
arbor-core                              ← (everyone)
arbor-scheduler                         ← arbor-core
arbor-brp                               ← arbor-core

arbor-plugin-types                      ← arbor-core
arbor-plugin-api                        ← arbor-plugin-types
arbor-plugin-marketplace                ← arbor-plugin-types, arbor-scheduler
arbor-plugin-core                       ← arbor-plugin-types, arbor-plugin-api, arbor-scheduler

arbor-git-provider-api                  ← arbor-core
arbor-git-provider-github               ← arbor-git-provider-api, arbor-auth
arbor-git-provider-gitlab               ← arbor-git-provider-api, arbor-auth

arbor-issue-tracker-api                 ← arbor-core
arbor-issue-tracker-github              ← arbor-issue-tracker-api, arbor-git-provider-github
arbor-issue-tracker-gitlab              ← arbor-issue-tracker-api, arbor-git-provider-gitlab
arbor-issue-tracker-jira                ← arbor-issue-tracker-api, arbor-auth
arbor-issue-tracker-linear              ← arbor-issue-tracker-api, arbor-auth

arbor-pipeline-api                      ← arbor-core
arbor-pipeline-core                     ← arbor-pipeline-api, arbor-plugin-api, arbor-scheduler

arbor (il guscio Tauri)                 ← tutti
```

Nessun ciclo. Convenzione: i `*-api` sono leaf di dominio (DTOs + trait + Registry), i `*-core` o impl-specifici li consumano. Niente impl conosce un altro impl.

## AppCtx — l'astrazione di Tauri

In `arbor-core`:

```rust
pub trait AppCtx: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
    fn arbor_dir(&self) -> &std::path::Path;
    fn is_focused(&self) -> bool;
    // aggiunte solo quando un dominio le chiede davvero
}
```

In `arbor` (guscio Tauri):

```rust
pub struct TauriAppCtx { handle: tauri::AppHandle }
impl AppCtx for TauriAppCtx { /* … */ }
```

Regole:

- Niente `tauri::*` nel trait. Solo `std` + `serde_json`.
- I crate di dominio prendono `&dyn AppCtx` (o `Arc<dyn AppCtx>`) nei metodi del loro trait di dominio o nel costruttore del provider.
- Per testing, `MockAppCtx` da poche righe rende ogni crate testabile senza spin-up Tauri.

## Config per dominio

Strada **(2)**: ogni crate definisce la propria struct di config (es. `MarketplaceConfig` in `arbor-plugin-marketplace`, `PluginConfig` in `arbor-plugin-types`, …). `arbor-core` espone un piccolo helper di composizione; `arbor` (guscio) le aggrega in una `AppConfig` finale che è il root del TOML su disco.

Il file su disco resta `~/.config/arbor/config.toml` (e l'equivalente per-repo) — solo che la sua struttura ora è la somma delle config esposte dai crate, non una struct monolitica scritta a mano.

## Errore per dominio

```rust
// es. arbor-issue-tracker-api
#[derive(thiserror::Error, Debug)]
pub enum IssueTrackerError {
    #[error("authentication failed: {0}")]      Auth(String),
    #[error("not found: {0}")]                  NotFound(String),
    #[error("transport: {0}")]                  Transport(String),
    #[error("rate limited: retry after {0}s")]  RateLimited(u64),
    #[error("other: {0}")]                      Other(String),
}
pub type Result<T> = std::result::Result<T, IssueTrackerError>;
```

In `arbor`:

```rust
impl From<IssueTrackerError> for AppError {
    fn from(e: IssueTrackerError) -> Self { /* mappatura ricca */ }
}
```

Vantaggio: il frontend può distinguere "rete giù" da "auth fallita" da "rate limited" — oggi sono tutti `AppError::Other` indistinguibili.

## Hook dispatcher in `arbor-plugin-api`

Per evitare cicli (`plugin-api` non deve conoscere `issue-tracker-api`):

- `arbor-plugin-api` espone `fire_hook(name: &str, ctx: serde_json::Value)`, **name-agnostic**.
- Ogni dominio dichiara le sue costanti hook nel suo crate:
  ```rust
  // arbor-issue-tracker-api
  pub const HOOK_ON_ISSUE_LINKED:       &str = "on_issue_linked";
  pub const HOOK_ON_ISSUE_TRANSITIONED: &str = "on_issue_transitioned";
  ```
- Il **catalogo globale degli hook** (consumato dalla DocsPanel) si costruisce via `HookContributor` trait: ogni dominio implementa il trait per descrivere i propri hook + lo schema del ctx; `arbor` aggrega a startup.

## Shared GitHub / GitLab client

Decisione: `arbor-issue-tracker-github` dipende direttamente da `arbor-git-provider-github`, che espone un `pub mod gh` con `GithubClient`, auth keyring, parser di `User`/`Repository`/`Label`. Niente crate "client" condiviso a parte. Stesso pattern per GitLab.

Trade-off accettato: `arbor-git-provider-github` non è più solo "impl del trait GitProvider", è anche la **home della shared infra GitHub**. Va dichiarato nel `README` del crate.

## Cose flaggate ma fuori scope di questo refactor

- **`src-tauri/src/studio/mod.rs` è 1225 righe** (sopra la soglia morbida CLAUDE.md). Va splittato internamente in `walker`, `kind`, `walk_options`, ecc. **Indipendente** da questo refactor — si può fare in qualsiasi momento.
- **`commands/marketplace_commands.rs` è 384 righe** — vicino al limite. Si splitterà naturalmente in `marketplace_catalog_commands.rs` + `marketplace_install_commands.rs` + `marketplace_custom_commands.rs` quando atterra `arbor-plugin-marketplace`.
- **`fetcher.rs` è 581 righe**: quando atterra `arbor-plugin-marketplace`, lo splittiamo in `github_api.rs` + `index.rs` + `fetch.rs` + `custom.rs`.
- **Deps comuni** (`cron`, `chrono`, `semver`, `toml`, …) sono inline nei nuovi crate per ora. Quando saranno consumate da 2+ crate, promuoverle a `workspace.dependencies` nel `Cargo.toml` root.

## Ordine dei PR

Vincoli di dipendenza:

1. **PR #1 — `arbor-core` + `arbor-scheduler`**.
   Sbocca tutto il resto. È la prova del 9 del setup workspace (feature unification OpenSSL, design `AppCtx`, mapping errori cross-crate). Piccolo e contenuto.

2. **PR #2 — `arbor-plugin-types`**.
   Estrae manifest, permissions, hook catalog, dependencies, schedule, PluginConfig.

3. **PR #3 — `arbor-plugin-api`**.
   Hook dispatcher + Registry listener. Sblocca chi fa partire hook.

4. **PR #4 — `arbor-plugin-marketplace`**.
   Catalog + installer + scheduler refresh. Risolve quasi tutti i bug discussi (path dup, http client dup, `verify_pinned_sha` dup, SHA struct dup).

5. **PR #5 — `arbor-git-provider-api` + `arbor-git-provider-github` + `arbor-git-provider-gitlab`** (uniti o split — da decidere a quel punto).

6. **PR #6 — `arbor-issue-tracker-*`**. Sposta `integrations/jira` + `integrations/linear` + scorpora GitHub/GitLab Issues dal git-provider.

7. **PR #7 — `arbor-plugin-core`**. Il PluginHost mlua. Grosso, va fatto da solo.

8. **PR #8 — `arbor-pipeline-{api,core}`**.

9. **PR #9 — `arbor-brp`**.

10. **PR finale — rinomina `src-tauri` → `arbor`**, pulizia `[workspace]`, mass-find-and-replace dei vecchi `crate::*` interni.

Ogni PR deve compilare a sé e passare i test. Niente stati intermedi rotti.

## Cose decise ESPLICITAMENTE *non* fare ora

- `arbor-git`, `arbor-workspaces`, `arbor-linked-worktrees`, `arbor-terminal`: round 2.
- `arbor-studio-*`: dopo WASM migration.
- Rinominare `crates/arbor-auth` / `crates/arbor-cloud` / `crates/arbor-process-ext`: già in posto, non si tocca finora.
- `crates/types` globale: deciso di non farlo — `arbor-core` ha un submodule `types` se serve.

## Per chi prende in mano il refactor

- Niente sviluppo finché il PR #1 non è atterrato — gli altri Cargo.toml sono shell senza `src/lib.rs` e non sono nel workspace.
- Quando attivi un crate: aggiungi `src/lib.rs` con almeno `//! <descrizione>`, aggiungi il path al `[workspace] members` del root `Cargo.toml`, sposta il primo modulo di responsabilità del crate.
- Test del fumo: `cargo check -p arbor-<nome>` da workspace root deve passare senza warning.
- Mai aggiungere dipendenze nuove senza l'OK dell'utente — vale anche qui (vedi CLAUDE.md).
