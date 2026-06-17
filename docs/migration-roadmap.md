# Roadmap di migrazione — Arbor platform + Corvus / Merula / Sitta

Roadmap operativa e dettagliata. Materializza l'analisi di
[`docs/crate-refactor-round2.md`](crate-refactor-round2.md) (analisi + naming +
struttura target) e continua [`docs/crate-refactor.md`](crate-refactor.md)
(round 1). Questo doc è il **come**; quei due sono il **cosa** e il **perché**.

## Da → A

- **Da**: tutto `arbor-*` (ombrello + git client confusi); round 1 ~60% (PR #1-4 atterrati); nemus già scorporato in crate; esplora e git GUI dentro `src-tauri/`.
- **A**: `Arbor` = piattaforma + launcher (l'albero). Prodotti = uccelli: **Corvus** (git), **Merula** (musica, ex nemus), **Sitta** (file). Plugin engine riusabile multi-app, runtime Lua + WASM, capability host generiche. Un solo binario `arbor` che monta le finestre dei prodotti (RAM condivisa).

## Principi guida (valgono per ogni milestone)

1. **Ogni step compila e i plugin esistenti girano.** Niente stati intermedi rotti (come round 1). Backward-compat del contratto `arbor.*` / hook / `plugin.toml`.
2. **Nomi finali alla creazione.** I crate nuovi nascono già col prefisso giusto (`corvus-*`, `merula-*`, `sitta-*`, piattaforma `arbor-*`) → niente doppio rename. La piattaforma `arbor-*` **non si rinomina** (è legittimamente il nome della piattaforma).
3. **Shell = libreria, binario = uno.** Ogni prodotto espone un `*-shell` (lib); il bin `arbor` li monta come finestre. Bin separati per-prodotto solo per distribuzione indipendente.
4. **Capability generiche, non one-off.** `net`/`secrets`/handle/stream nascono generiche, non per un singolo consumer.
5. **Decision gate prima del codice.** Ogni milestone con un ⚠️ richiede una decisione (vedi tabella gate in fondo) prima di partire.
6. **Architettura = Modello D (1 FE + N BE)** — DECISO (vedi round2 §"Modello di processo"). Conseguenze che rimodellano M3-M5: ogni prodotto = **backend headless** (`corvus-be`/`merula-be`/`sitta-be`, exe separato, no webview) + **modulo FE** (`fe-<prodotto>` + `*-ipc`); lo **shell** possiede l'unica WebView e fa da **router/broker** (comandi + credenziali). IPC shell↔BE con spawn parent→child + nonce + ACL. FE disaccoppiata a pacchetti (`fe-shared` + per-prodotto, regola no-cross-import) per estrazione standalone futura. Credenziali: solo lo shell tocca il keyring, con caching in memoria (access token short-lived, refresh nel keyring) — vedi round2 §D.5.

## Mappa delle dipendenze

```
M0 decisioni/naming ──┬─► M1 fs + ipc + shell-common ──┬─► M5 Sitta (sitta-be+fe) ─┐
                      │                                 │                            ├─► M7 D.4
                      ├─► M2 domini Corvus ──► M3 split FE/BE Corvus ──► M4 Merula ──┘
                      │                          (arbor shell + corvus-be + fe)
                      ├─► M6 engine multi-app ──────────────────────────────────────┘
                      └─► M8 runtime WASM ──► M9 studi WASM
                                    └──► M10 capability ──► M11 cloud / (DB futuro)
                      (M12 launcher+icone: dopo che esistono ≥2 finestre-prodotto)
```

Architettura = **Modello D** (1 FE + N BE): shell process possiede l'unica WebView
+ router + credential broker; ogni prodotto = **backend headless** (`*-be`) + **modulo FE**
(`fe-*` + `*-ipc`); IPC via `arbor-ipc` (in-process oggi, spawn+nonce domani).

Tracce parallelizzabili: **M1**, **M2**, **M6**, **M8** sono in gran parte
indipendenti dopo M0 → si possono alternare. M3→M4→M5 sono la spina del prodotto.

---

## M0 — Decisioni, naming lock, scaffolding

**Stato**: ✅ **fatto** — layout `foundation/` confermato (i crate esistenti restano flat, niente move); `tarpc` + `zeroize` aggiunti a `[workspace.dependencies]` (parcheggiati, unused → smoke build pulito); scheletri `crates/foundation/{fs,ipc,shell-common}` creati (compilano vuoti, prelude per ognuno, in `members`).

**Obiettivo**: sbloccare tutto fissando le scelte e la forma del workspace.
**Dipende da**: niente.
**Decisioni** ⚠️ (le 9 di round2 §Decisioni aperte): `wasmtime` come dep · formato ABI · cloud WASM-vs-subprocess · DB manager framing · sede SDK WASM · launcher sì/no · identità taskbar · timing engine multi-app · capability generiche. **Naming già deciso**: Arbor/Corvus/Merula/Sitta.
**Step**:
1. Registra le decisioni nel doc (chiudi le 9 aperte o marca "deferred").
2. Concorda il layout directory target (`foundation/ plugin/ studio/ corvus/ merula/ sitta/ bins/`) — solo come convenzione, niente move ancora.
3. (Se OK) aggiungi `wasmtime` a `workspace.dependencies` **senza usarlo** (smoke build).
4. Definisci il **linguaggio visivo della famiglia** (stile uccelli coerente, palette, griglia icona) — i loghi per-prodotto si creano all'estrazione (M3/M4/M5).
**Deliverable**: decisioni scritte; convenzione directory; linguaggio visivo; eventuale dep wasmtime parcheggiata.
**Gate**: decisioni bloccanti chiuse (almeno: wasmtime, naming, ABI).
**Rischio/Size**: nullo / **S**.
**Sblocca**: tutto.

## M1 — Foundation Modello D: `arbor-fs` + `arbor-ipc` + `arbor-shell-common` + debito prelude

**Stato**: 🚧 **in corso** — **M1a `arbor-fs` fatto** (FS puro estratto da `fs_commands.rs`: read/mutate/copy-move-dup con `ProgressSink`+`CancelToken` iniettati, trash, zip, roots/WSL/overview, path-expand; i comandi sono thin wrapper, `From<FsError> for AppError` preserva la wire string; watcher + glue OS restano nello shell). **M1b `arbor-ipc`** = design ([`docs/ipc-design.md`](ipc-design.md)) + scheletro `BrokerClient`/`Event` + ping loopback in-process (tarpc parcheggiato, flip a M3). **M1c `arbor-shell-common`** = scheletro router + credential broker (cache+zeroize). **M1d** = prelude per auth/cloud/process-ext.

**Obiettivo**: i fondamentali condivisi del Modello D (1 FE + N BE): FS puro, il transport shell↔BE, e il runtime dello shell (host WebView + router + credential broker).
**Dipende da**: M0.
**Step**:
1. `crates/foundation/fs` (`arbor-fs`): operazioni FS pure (read/write/copy/move/delete/list/glob/zip/watch), **niente Tauri**. Sposta la logica da `commands/fs_commands.rs`; i comandi diventano thin wrapper.
2. **`crates/foundation/ipc` (`arbor-ipc`, nuovo)** — il cuore del Modello D: **due canali** shell↔BE (stile LSP: requests + notifications).
   - **Comandi (req/resp) via `tarpc`** (✓ approvato): `#[tarpc::service] trait` → client + server **tipati generati** = **un comando ≈ una definizione**, niente boilerplate (copre la parte grossa dell'API). Codec serde binario (es. bincode). È la codegen che neutralizza la "tassa per-feature".
   - **Eventi (push BE→shell) su canale one-way dedicato**: messaggi serde length-prefixed, un `enum Event`. `tarpc` **non fa streaming by design** — e gli eventi **non vanno** sul canale RPC. Qui throttling/coalescing/backpressure (come già per i meter Merula).
   - **FE-facing push = eventi Tauri** (`emit`/`listen`): lo shell riceve dal BE sul canale eventi e ri-emette alla FE (meccanismo già esistente).
   - **Transport-agnostico**: in-process (canale in-memory) ↔ IPC (named pipe Win / unix socket `0600`+`SO_PEERCRED`), **spawn parent→child con nonce** per il bootstrap sicuro. `tarpc` gira su entrambi cambiando solo il transport → mappa diretta sul flip `BrokerClient` in-process→IPC (principio #6).
3. **`crates/foundation/shell-common` (`arbor-shell-common`)** — runtime dello shell: host WebView2 + window mgmt + single-instance + deep-link bus + icone + **router** (FE `invoke` → `arbor-ipc` → BE) + **credential broker** (unico detentore del keyring; caching in memoria: **access token short-lived in cache, refresh nel keyring**, TTL + invalida-su-401, **`zeroize`-on-drop** [approvato], mai a FE/BE/plugin).
4. Debito + deps: `prelude` a `arbor-auth`/`-cloud`/`-process-ext`; aggiungi `zeroize` (✓ approvato) e `tarpc` (✓ approvato) a `workspace.dependencies`.
**Deliverable**: `arbor-fs`; `arbor-ipc` con echo request/response/event end-to-end; `shell-common` con router+broker funzionanti; prelude ovunque.
**Gate**: `cargo check` standalone dei crate; un ping FE → shell → `BrokerClient` (loopback) → FE; broker legge/scrive una entry keyring con caching.
**Rischio/Size**: medio / **L**.
**Sblocca**: M3/M4/M5 (tutti usano `arbor-ipc` + `shell-common`).

## M2 — Completare round 1: domini → crate `corvus-*`

**Stato**: ✅ **essenzialmente completo** (resta solo lo step 6, rimandato a M6). **Ordine adottato per rischio** (no-compile in sessione): si parte dai pezzi leaf/isolati per stabilire il pattern, poi i grossi. Fatti:
- **`corvus-brp`** (655 righe, leaf): crate `crates/corvus/brp` con prelude + **unit test** (capability ingestion, SSE frame parsing, status); `src-tauri/src/brp/` rimosso, i 4 consumer usano `corvus_brp::prelude::`; stub round1 `crates/brp` cancellato.
- **`corvus-issue-tracker-api`** (leaf `*-api`): i DTO provider-agnostici (Issue/comments/filters/…) + il helper puro `branch_name_for_issue` estratti da `integrations/` in `crates/corvus/issue-tracker/api` (solo `serde`), con **unit test** sul slugify (incl. cap char-safe per titoli non-ASCII). `integrations/mod.rs` ora re-exporta dal crate (`pub use …prelude::*`) → tutti i call-site invariati; restano lì solo `tracker_for_repo`+`lookup_by_identifier` (host-coupled) e le impl jira/linear. **Trait `IssueTracker` rimandato** finché le impl non diventano crate (un solo set di free-function oggi → niente trait premature). Stub `crates/issue-tracker/api` cancellato.
- **`corvus-pipeline-api`** (leaf): il **motore espressioni puro** di pipeline — `vars` (RunContext/VarValue/interpolazione/transform chain) + `condition` (Condition/CompareOp/`evaluate`) + `condition_parser` — estratto in `crates/corvus/pipeline/api` (serde/serde_json/regex/tracing), con i **unit test già presenti** (parser, evaluator, vars/transform). Ora ospita anche il **data-model** (`model`: StepDef/StageDef/PipelineDef/LuaOpSpec + RunStatus/StepRun/StageRun/PipelineRun/LogEvent/ResumeCursor + parse_log_level/parse_stage_mode), i **builtin** (`builtin`) e l'**if-block** (`if_block`: IfBlock/IfBranch/BranchSelection, bodies `StepDef`). Stub `crates/pipeline/api` cancellato.
- **`corvus-pipeline-core`** (FATTO): il **core run-tracking host-free**, estratto da `pipeline/mod.rs` — `registry` (PipelineRegistry: defs/runs/lock di concorrenza/cancel-token/running-count), `persist` (JSON per-run + `registry_from_disk` recovery, `now_ms`, `RUN_LOG_CAP`) e `run_tree` (helper puri: `find_step_mut`, `compute_resume_cursor`/`resumable_step_indices`, `split_chunk_lines`/`drain_partial_line`, `infer_step_log_level`, `step_preview`). Dipende da `corvus-pipeline-api` + `arbor-core`; 12 unit test. Lo **shell `pipeline/mod.rs` resta l'orchestratore Tauri** (thread per-run, emit, spawn processi, dispatch lua_op) e re-exporta da api+core → call-site `crate::pipeline::*` invariati; `builtin.rs`/`condition.rs` shell cancellati.

**Stato M2 — essenzialmente completo**:
- ✅ **git_provider** → `corvus-git-provider-{api,github,gitlab}` (keyring-free via `SessionProvider`; tutto il REST trait-path: MR/CI/security/repo-browser). Fatto durante M3 Asse A.
- ✅ **issue-tracker** → `corvus-issue-tracker-{api,linear,jira}` (keyring-free via `SessionProvider`; registry `Arc<dyn IssueTracker>` self-describing). Fatto durante M3 Asse A.
- ✅ **pipeline** → `corvus-pipeline-{api,core}` (model + expression engine + run registry/persistence/helper puri; orchestratore Tauri resta nello shell).
- ✅ **brp** → `corvus-brp`.
- ✅ **marketplace** → `arbor-plugin-marketplace` (catalogo + installer + cache, Tauri-agnostic via trait `MarketplaceHost`; lo shell tiene solo ~150 righe di glue: host impl + scheduler auto-refresh). Atterrato nel round 1 (PR #5).
- ⏳ **Solo lo step 6** resta: contribuire namespace/hook/permission in modo runtime-agnostic (pattern `NamespaceContributor`), spostandoli da `ns_shell/`. **Rimandato a M6** (engine plugin multi-app) — oggi i namespace `arbor.*` vivono ancora in `ns_shell/` e importano i crate.

Le impl credential-coupled (jira/linear, git_provider) sono già state estratte **keyring-free** col seam `SessionProvider` (lo shell resta unico detentore del keyring) — il "nodo credenziali" che la versione precedente di questo doc segnalava è risolto.

**Obiettivo**: tirare fuori da `src-tauri` i domini git-client come crate, **già col nome finale**.
**Dipende da**: M0 (naming). Segue il piano round 1 PR #5-#9.
**Step** (un dominio per volta, ognuno compila):
1. ✅ `corvus-git-provider-{api,github,gitlab}` ← `src-tauri/src/git_provider/` (FATTO).
2. ✅ `corvus-issue-tracker-{api,jira,linear}` ← `src-tauri/src/integrations/` (FATTO; github/gitlab issue NON serviti via tracker separato).
3. ✅ `corvus-pipeline-{api,core}` ← `src-tauri/src/pipeline/` (FATTO).
4. ✅ `corvus-brp` ← `src-tauri/src/brp/` (FATTO).
5. ✅ `arbor-plugin-marketplace` (PR #5, FATTO) — atterrato col pattern nuovo (multi-host arriva in M6).
6. Ogni dominio **contribuisce i suoi namespace/hook/permission** in modo runtime-agnostic (pattern `NamespaceContributor` di `arbor-plugin-api`), spostandoli da `ns_shell/`.
**Deliverable**: `src-tauri` più sottile; domini come crate; `HOOK_CATALOG` si svuota verso i domini.
**Gate**: ogni crate `cargo check` standalone; plugin reali girano (mr/ci/issues/security invariati).
**Rischio/Size**: medio / **L** (è un piano già scritto, ma tanto codice).
**Sblocca**: M6 (host import per WASM crescono), M3.

## M3 — Split FE/BE su Corvus: `corvus-be` + shell `arbor` + pacchetti FE

**Obiettivo**: stabilire il **pattern Modello D** sul primo prodotto (Corvus) e **dissolvere `src-tauri`**. È il milestone-fondamento: ciò che funziona qui si replica su Merula/Sitta.
**Dipende da**: M1 (`arbor-ipc` + `shell-common`), M2 (domini `corvus-*`).
**Step**:
1. **`bins/corvus-be`** (binario Rust **headless**, no webview): assorbe la logica git-client da `src-tauri` (i 547 comandi → handler `arbor-ipc`), usa i crate `corvus-*` (M2) + `git2`; le credenziali le chiede al **broker** dello shell (mai keyring diretto). Espone la sua API su `arbor-ipc`.
2. **`bins/arbor`** (shell): host WebView2, monta **`fe-shell`**, fa da **router** (FE `invoke` → `arbor-ipc` → `corvus-be`) + **credential broker**, e **spawna `corvus-be`** (parent→child + nonce). `src-tauri` si dissolve qui.
3. **Pacchetti FE**: `fe-shared` (agnostico) · **`fe-corvus`** (UI git, importa **solo** `fe-shared` + `corvus-ipc`) · `fe-shell` (host + routing). Regola **no-cross-import**.
4. **`corvus-ipc`** (client FE→BE via shell-router): l'SDK FE di Corvus.
5. **Rebranding user-facing** ⚠️: nome app/finestra git → **Corvus**. `arbor` resta il nome di shell/launcher.
6. **🎨 Creazione logo**: **Arbor** (l'albero, shell) + **Corvus** (il corvo) — app icon multi-size + SVG; `tauri.conf.json` + `set_icon` sulla finestra Corvus.
**Deliverable**: `arbor` avvia, spawna `corvus-be`, la git GUI gira come **Corvus via IPC**; `src-tauri` rimosso; loghi Arbor + Corvus.
**Gate**: feature git invariate; **un crash di `corvus-be` non porta giù lo shell** (crash-isolation visibile); credenziali via broker con caching.
**Rischio/Size**: alto / **XL**. **Incrementale**: prima `BrokerClient` **in-process** (loopback, un solo processo) per spostare i 547 comandi dietro `arbor-ipc` senza rompere nulla; *poi* sposta `corvus-be` in processo separato (flip del backend del `BrokerClient`).
**Asse B — pilota in-process FATTO (forma generica)**: il primo slice verticale è atterrato — il **dominio `stash` (11 comandi)** instrada FE→**un solo comando Tauri generico `rpc(program, method, params)`**→`AppState.router` (`arbor-shell-common::Router`)→`LoopbackBroker`(`"corvus"`)→registry→`ipc/corvus/stash.rs` (handler su `&AppState`)→JSON, tutto in-process. **La shell non ridichiara firme per-comando**: è un router puro (`generate_handler!` passa da 11 voci stash a 1). `dispatch_rpc` mappa `IpcError::Backend(s)`→`AppError::Other(s)` preservando la wire-string. BE: **niente match, niente arg-struct** — ogni handler è una funzione annotata `#[arbor_rpc::handler("stash.…")]` (crate platform `arbor-rpc` + proc-macro `arbor-rpc-macros`) che legge la firma, genera decode-args + serializza + `inventory::submit!` (auto-registrazione); il contesto passa type-erased `&dyn Any`. FE: helper generico `corvus(method, params)` (`ipc/rpc.ts`); i wrapper tipati in `ipc/branch.ts` restano (DX) → firme/comportamento invariati. **Inter-programma** (Corvus→Sitta) userà lo stesso `rpc` con `ensure_running` (spawn+wait single-flight, no-op in-process). **Sweep in corso (🔄)**: dopo stash, migrati **bisect (11), notes (5), reset/tags (3), reflog (1)** → **5 domini / 31 comandi** instradano via `rpc` (ogni dominio = un `ipc/corvus/<dom>.rs` di funzioni `#[corvus::handler]`, auto-registrate via inventory, zero righe in `lib.rs`). Migrare un comando solo-`State<AppState>` è meccanico (sposta corpo→handler, cancella registrazione, wrapper FE su chiavi snake_case); i comandi che prendono `AppHandle`/`Window` per emettere usano ora `state.emit`/`state.event_sink` (seam `EventSink`, vedi sotto). **Processo separato avviato (Stage 1 FATTO)**: c'è un **`corvus-be` reale** (`crates/corvus/be`, binario headless) che la shell spawna e con cui parla via `arbor-ipc::transport` (frame JSON su stdin/stdout del figlio); un `SplitBroker` instrada i metodi annunciati da corvus-be al processo, il resto al loopback (auto-route allo spostamento). `ping`/`echo`/`emit` provati out-of-process. **Prossimo**: estrarre le deps git di stash/reset/bisect e spostare quei domini in corvus-be. Dettaglio: [`docs/ipc-design.md`](ipc-design.md), [`docs/corvus-be-bringup.md`](corvus-be-bringup.md).
**Pre-flip (pulizia, FATTO)**: il **seam credenziali keyring-free** è in piedi — `arbor_ipc::prelude::SessionProvider` (contratto async `session`/`refresh` → `AuthSession { base_url, auth_header }`, niente `keyring`/HTTP). È ciò che sblocca l'estrazione **keyring-free** dei domini coupled (linear/jira, git_provider): tengono `Arc<dyn SessionProvider>` invece di chiamare `credential_store`+keyring. Una sola session-shape copre Bearer a endpoint fisso (Linear), base per-tenant Bearer/Basic (Jira), e basi self-hosted. Impl da adapter shell per-provider (keyring read + OAuth refresh). Linear estratto in `corvus-issue-tracker-linear` + registry `IssueTracker` con descriptor self-describing.
**Sblocca**: pattern Modello D per M4/M5.

## M4 — Merula: `merula-be` + `fe-merula` (+ rename nemus→merula)

**Obiettivo**: portare il prodotto musica sul Modello D.
**Dipende da**: M3 (pattern Modello D). Indipendente da M5.
**Step**:
1. Rinomina i 7 crate `arbor-nemus-*` → `merula-*` (+ facade). Aggiorna `use`, Cargo.toml, prelude. **DSL `.nemus` resta** come sotto-brand (file/skill non si toccano).
2. **`bins/merula-be`** (headless): la logica nemus (oggi `src-tauri/src/nemus/`) come backend su `arbor-ipc`. **L'audio `cpal` resta DENTRO `merula-be`** (possiede lo stream RT); solo gli **eventi** (meter, active-haps — già throttlati) attraversano l'IPC, **mai campioni audio**. `JobSink`/feedback via eventi del broker.
3. **`fe-merula`** + **`merula-ipc`** (regola no-cross-import: importa solo `fe-shared` + `merula-ipc`).
4. `bins/arbor` **spawna `merula-be`** e monta `fe-merula` come finestra `merula`.
5. **🎨 Creazione logo**: **Merula** (il merlo) — app/window icon multi-size + SVG; `set_icon` sulla finestra `merula`.
**Deliverable**: Merula gira come BE separato + finestra montata dallo shell; crate `merula-*`; logo Merula.
**Gate**: feature nemus invariate; **audio fluido** (nessun campione sull'IPC, latenza RT preservata); `.nemus` apre.
**Rischio/Size**: medio-alto / **L** (rename meccanico + il taglio FE/BE su un dominio real-time è il caso più delicato per l'IPC).
**Sblocca**: bin `merula` distribuibile a sé (FE+BE proprio); plugin in Merula (via M6).

## M5 — Sitta: `sitta-be` + `fe-sitta`

**Obiettivo**: l'esplora sul Modello D, su `arbor-fs`, parametrizzata.
**Dipende da**: M1 (`arbor-fs`, `arbor-ipc`), M3 (pattern Modello D).
**Step**:
1. **`bins/sitta-be`** (headless): operazioni FS su `arbor-fs` + git-awareness **path-based** (capability generica, non "Arbor"); espone l'API su `arbor-ipc`. Lo `ExplorerClipboard`/`DragOverlay`/`PendingReveals` cross-window diventano stato del BE (o dello shell se è cross-finestra).
2. **`fe-sitta`** + **`sitta-ipc`** (regola no-cross-import). Parametrizza le **7 dip FE hard** (round2 §D.2): config/theme via config-object, `showToast` via callback, Projects/registry opzionali, deep-link opzionale, "Open in Corvus" opzionale.
3. La git-awareness è una capability **generica** (utile a qualsiasi file manager), non legata a Corvus.
4. `bins/arbor` **spawna `sitta-be`** e monta `fe-sitta` come finestra `explorer`.
5. **🎨 Creazione logo**: **Sitta** (la sitta) — app/window icon multi-size + SVG; `set_icon` sulla finestra `explorer`.
**Deliverable**: esplora come `sitta-be` + `fe-sitta`, self-sufficient sui FS; arbor-ness disattivabile; logo Sitta.
**Gate**: esplora invariata; gira anche senza le feature Corvus attive.
**Rischio/Size**: medio / **L**.
**Sblocca**: M7 (contribution + plugin Corvus); bin `sitta` distribuibile a sé.

## M6 — Engine plugin multi-app (C.6)

**Stato**: ⏸️ **rimandato** (decisione C3: standalone-first). Si fa **dopo** che i 3 prodotti girano (M3-M5). Fino ad allora ogni prodotto può avere plugin via il proprio embed dell'engine, ma il marketplace multi-host + `targets` arriva qui.
**Obiettivo**: rendere `arbor-plugin-core` un engine che anche Merula/Sitta incorporano.
**Dipende da**: M0. Beneficia di M2 (domini contribuiti). Parallelo a M3-M5.
**Step**:
1. **Lista host-pure selezionabile**: `register()` non hardcoda i 22 ns — l'host sceglie il set (feature flags / lista installer).
2. **Marketplace multi-host**: plugin roots per-app + campo manifest `targets = ["corvus"|"merula"|"sitta"]`; il fetcher filtra per host.
3. **Catalogo hook per-app**: ogni shell contribuisce i suoi hook al `HookDispatcher` (Corvus: git; Merula: `on_clip_launch`/`on_render_done`; Sitta: `on_file_open`).
4. **Storage plugin namespaced** per-app; `AppCtx` esteso con le capability di ciascun host.
**Deliverable**: Merula/Sitta possono caricare plugin propri riusando l'engine.
**Gate**: un plugin di prova gira in Merula e in Sitta; i plugin Corvus invariati.
**Rischio/Size**: medio / **L**.
**Sblocca**: M7, e i plugin WASM multi-app.

## M7 — Contribution point esplora + plugin Corvus ad-hoc (D.4)

**Obiettivo**: re-integrare le feature Corvus in Sitta **come plugin**, zero coupling.
**Dipende da**: M5 (Sitta), M6 (engine multi-app).
**Step**:
1. Sitta espone i propri contribution point: `explorer:file-badge`/`column`, `explorer:context-menu:file|folder`, `explorer:sidebar-section`, `explorer:address-bar-action`.
2. Scrivi il **plugin "Corvus for Sitta"** (`targets=["sitta"]`, vive in `arbor-extensions`): legge il registry Corvus (file su disco), contribuisce sezione Projects + badge, azione "Open in Corvus" via **deep-link**.
3. git-awareness fornita dalla capability generica (M5.3), non dal plugin.
**Deliverable**: Sitta "pulito" che si accende con git+Projects se il plugin è installato.
**Gate**: Sitta standalone = file manager; con plugin = + Corvus features; nessun IPC necessario (file + deep-link).
**Rischio/Size**: basso-medio / **M**.
**Sblocca**: pattern "feature di un prodotto dentro un altro" (es. Merula→Sitta).

## M8 — Runtime WASM: `arbor-plugin-wasm`

**Obiettivo**: secondo runtime plugin, accanto a Lua.
**Dipende da**: M0 (wasmtime ⚠️ + formato ABI ⚠️). Parallelo a M3-M7. `arbor-plugin-api` è già multi-runtime.
**Step**:
1. `crates/plugin/wasm` (`arbor-plugin-wasm`): `WasmRuntime` (carica `.wasm`, istanzia, linka import via `wasmtime::Linker`).
2. ABI = **encoding binario custom di `PluginValue`** (C2): marshalling ⇄ bytes in/out linear memory; export `alloc`/`dealloc`/entry-points; host import gated da permission (`host_fs_read`, `host_http`, `host_net`, `host_log`, `host_emit`, …).
3. `WasmHookListener: HookListener`, registrato sul `HookDispatcher` (come `LuaHookListener`).
4. `crates/plugin/wasm-sdk` (`arbor-plugin-wasm-sdk`, C4): SDK Rust per autori (nasconde ptr/len; API `arbor::fs::read(...)`). **Dipende dal core di contratto condiviso col Lua** (`arbor-plugin-api`/`-types`) — un contratto, due runtime.
5. Manifest: `runtime = "lua" | "wasm"`, `entry = "plugin.wasm"`.
**Deliverable**: un plugin `.wasm` di prova gira in-process e fira/riceve hook.
**Gate**: plugin Lua invariati; plugin WASM hello-world funziona; permission enforced.
**Rischio/Size**: alto / **XL** (tecnologia nuova).
**Sblocca**: M9, plugin pesanti self-contained.

## M9 — Prima migrazione WASM: gli studi

**Obiettivo**: gli studi escono dal binario come plugin/crate WASM.
**Dipende da**: M8.
**Step**:
1. Estrai `arbor-studio-core` (trait `StudioFormatBackend` + `StudioRegistry`) e `arbor-studio-{json,ron,yaml,toml,properties}` (puro parsing, compilano a wasm32).
2. `WasmStudioBackend: StudioFormatBackend` che serializza ogni metodo via ABI verso il `.wasm`. Il `StudioRegistry` non sa se è nativo o wasm.
3. Spedisci i formati come plugin WASM; nuovo formato = nuovo plugin, niente patch al binario.
**Deliverable**: studi come backend WASM dietro il registry esistente.
**Gate**: tutti i formati invariati lato UI; footprint CodeMirror+schema fuori dal processo host.
**Rischio/Size**: medio / **L**.
**Sblocca**: prova reale del modello WASM su un dominio serio.

## M10 — Capability host generiche (C.7)

**Obiettivo**: aggiungere le capability che mancano a plugin stateful.
**Dipende da**: M8. ⚠️ decisione su quali e come.
**Step**: progetta come **estensioni generali** (non per un singolo consumer):
1. **`arbor.net` (TCP raw)** gated da permission `net = [host:port]` — **priorità (D1)**, l'unica capability rete richiesta per ora.
2. `arbor.secrets` (keyring) — ⏸️ **rimandato (D1)**: i plugin non ne hanno bisogno ora. Se mai aggiunto: **host tiene il segreto, il plugin non vede mai il valore raw** (stesso boundary dei token git).
3. **Handle di risorse** long-lived (connessioni/pool) gestiti dall'host, handle opaco al plugin.
4. **Streaming/cursor** per result-set grossi (critico per WASM: niente copia di 100k righe per call).
5. **Custom view**: superficie UI a tutto schermo controllata dal plugin (oltre form/contribution).
**Deliverable**: capability disponibili a Lua e WASM, permission-gated.
**Gate**: un plugin di prova apre una connessione TCP/legge un secret senza vedere credenziali.
**Rischio/Size**: alto / **L-XL**.
**Sblocca**: DB manager, client API stateful, watcher.

## M11 — Migrazioni opportunistiche: cloud, DB manager

**Obiettivo**: portare a casa i candidati rimasti.
**Dipende da**: M8 (+ M10 per DB).
**Step**:
1. **Cloud** → **plugin WASM** (E1): l'HTTP passa per `host_http` import (opendal non ha socket in sandbox; il transport va su capability host).
2. **DB manager** → ⏸️ **fuori scope (E2)**: forse futuro. Se mai → riusa `net` (M10) e, se la UI esplode, diventa un **4° prodotto** standalone (nome-uccello + logo propri).
**Deliverable**: cloud fuori dal binario come plugin WASM.
**Gate**: per ognuno, feature equivalente a oggi (cloud) o MVP funzionante (DB).
**Rischio/Size**: variabile / **L**.
**Sblocca**: footprint opendal/aws-lc fuori dal binario.

## M12 — Launcher polish + icone (B.3 / B.4)

**Obiettivo**: rifinire identità e routing dei prodotti.
**Dipende da**: ≥2 finestre-prodotto (dopo M5).
**Step**:
1. Routing nel callback single-instance: `--window=corvus|merula|sitta` (o deep-link `arbor://open/<prodotto>`) + auto-route al cold start.
2. `set_icon` per-finestra (icona propria a Corvus/Merula/Sitta) — già API disponibile.
3. ⚠️ Se serve **identità taskbar separata**: AUMID per-finestra (Windows `windows_sys`); cross-platform solo con bundle separati (macOS non ha icone Dock per-finestra).
**Deliverable**: collegamenti che aprono il prodotto giusto nel processo condiviso, con icona propria.
**Gate**: lancio di un collegamento apre/focalizza la finestra giusta; icone corrette.
**Rischio/Size**: basso / **M**.

> ⚠️ **REVISIONE (decisione utente): il launcher è un PRODOTTO con UI dedicata, stile JetBrains Toolbox** — non solo routing+icone.
> M12 come scritto sopra copre solo il funnel (single-instance + deep-link + icona per-finestra). L'aspettativa reale è una **finestra-launcher dedicata** (la "home" di Arbor) che:
> - elenca i **prodotti** installati (Corvus/Merula/Sitta) e i **progetti/workspace** recenti, con apertura diretta;
> - gestisce **install / update / versioni** dei prodotti (come Toolbox fa con gli IDE) — rilevante quando i prodotti diventano `*-be` separati / bundle distinti;
> - è la finestra che si apre al lancio "nudo" di `arbor` (senza `--window=`), da cui si saltano i prodotti.
> Implica: un `fe-launcher` (modulo FE shared-only) + una finestra `launcher` nello shell + storage proprio (lista prodotti/progetti/recenti). Va promosso da "polish M12" a **milestone-prodotto a sé** (sequenziabile dopo che esistono ≥2 finestre-prodotto, ma con UI/UX dedicata da progettare). Vedi round2 §B.3 (revisione). **Scope UI da concordare** prima del codice.

---

## Ordine consigliato & tracce parallele

**Spina dorsale (sequenziale)**: M0 → M1 → M2 → M3 → (M4 ∥ M5) → M7.
**Traccia engine/WASM (parallela dopo M0)**: M6, poi M8 → M9, poi M10 → M11.
**Polish**: M12 dopo M5.

Solo dev: fai prima M0-M5 (fondamenta + i tre prodotti come shell, un binario) — è il grosso del valore e non introduce tecnologie nuove. M6-M12 introducono WASM/capability e si possono diluire.

## Gate decisionali (le 9 di round2 → quando servono)

| Decisione | Serve entro |
|-----------|-------------|
| Naming Arbor/Corvus/Merula/Sitta | **✅ deciso** — usato da M2 in poi |
| `wasmtime` come dep | M0 (parcheggio) / M8 (uso) |
| Formato serializzazione ABI | M8 |
| Launcher sì/no + routing argv vs deep-link | M3 (bin) / M12 (routing) |
| Identità taskbar separata (AUMID) | M12 |
| Engine multi-app: ora o dopo | M6 |
| Capability generiche (net/secrets/handle/stream/custom-view) | M10 |
| Cloud: WASM vs subprocess | M11 |
| DB manager: namespace host / plugin / standalone | M11 |
| SDK WASM: arbor-extensions vs crate | M8 |

## Invarianti da non rompere mai

- Contratto plugin (`arbor.*`, hook, `plugin.toml`) backward-compatible a ogni step.
- Token/segreti mai esposti al plugin (host-import boundary) — vale anche per WASM.
- Un solo binario di default (RAM condivisa); bin separati solo per distribuzione.
- `git2` resta **nativo** nell'host; i plugin lo **importano**, non lo contengono.

## Riferimenti
- [`docs/crate-refactor-round2.md`](crate-refactor-round2.md) — analisi, naming, struttura target.
- [`docs/crate-refactor.md`](crate-refactor.md) — round 1 (PR #1-9).
- [`docs/plugin-api-architecture.md`](plugin-api-architecture.md) — API plugin multi-runtime.
