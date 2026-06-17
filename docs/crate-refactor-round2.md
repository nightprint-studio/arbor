# Arbor — Crate split round 2: scorporo app-standalone + plugin WASM

Stato: **piano** (analisi approvata, niente codice). Estende
[`docs/crate-refactor.md`](crate-refactor.md) (round 1) e
[`docs/plugin-api-architecture.md`](plugin-api-architecture.md). Round 1
ha disaccoppiato il backend in crate di dominio (PR #1-4 atterrati, #5+ in
coda). Questo doc copre i tre obiettivi successivi:

1. **Completare** il round 1 (domini ancora shell) — riassunto, dettagli in round 1.
2. **Scorporare in app-standalone** sotto-sistemi coesi: **nemus** e l'**esplora risorse**.
3. **Layer plugin WebAssembly** — far girare plugin `.wasm` autocontenuti accanto a quelli Lua; candidati: gli **studio** (RON/JSON/TOML/YAML/.properties), **cloud-storage**, e in futuro **db-query**.

> Il pattern WASM-app browser (compilare *tutta* Arbor a `wasm32`) **non** è
> oggetto di questo doc: è gated da `git2`/libgit2, `portable-pty`, `cpal`,
> `windows_sys`. Resta un "round 3" ipotetico dietro una strategia su gix.
> Qui l'host resta **nativo Tauri**; WASM è solo il runtime *guest* dei plugin.

---

## Parte A — Completare round 1 (contesto)

Stato dei crate (vedi [`crate-refactor.md`](crate-refactor.md) per il dettaglio):

- ✅ **Attivi** (17): `arbor-core`, `arbor-scheduler`, `arbor-plugin-{types,api,core,marketplace}`, `arbor-auth`, `arbor-cloud`, `arbor-process-ext`, `arbor-feedback`, i 7 `arbor-nemus-*`.
- 🔲 **Shell** (Cargo.toml senza `src/lib.rs`, fuori dai members): `arbor-brp`, `arbor-git-provider-{api,github,gitlab}`, `arbor-issue-tracker-{api,github,gitlab,jira,linear}`, `arbor-pipeline-{api,core}`.

Debito noto da saldare lungo la strada:
- `arbor-auth` / `arbor-cloud` / `arbor-process-ext` **non espongono `prelude`** (predatano la convenzione). Allinearli quando si tocca il loro confine.
- Il livello *commands* (547 `#[tauri::command]` in `src-tauri/src/lib.rs`) non è astratto. Non blocca lo scorporo dei domini, ma è perché `src-tauri` resta grosso.

Questa parte è **prerequisito morbido** delle Parti B/C: ogni dominio che esce
da `src-tauri` rende l'host più sottile e (per i domini plugin-facing)
contribuisce namespace/hook in modo runtime-agnostic — la pre-condizione del
layer WASM.

---

## Parte B — Scorporo in app-standalone

### Principio: "app che condividono il binario → app che condividono solo il guscio"

Sia nemus sia l'esplora risorse oggi sono **finestre Tauri separate** che vivono
nello stesso processo, già abbastanza isolate (route branch su `window.label`,
storage proprio per nemus). L'obiettivo round 2 è promuoverle a **sotto-sistemi
estraibili**: crate-core puri + un guscio Tauri sottile, così che domani siano
`cargo new` + move, non un refactor.

Le finestre già esistono:
- `src-tauri/src/nemus_window.rs` → label `"nemus"`
- `src-tauri/src/explorer_window.rs` → label `"explorer"` / `"explorer-N"` + overlay `"drag-overlay"`

### ⚠️ Costo memoria: scorporo crate ≠ processo separato

**Distinzione da non perdere mai.** Sono due decisioni indipendenti:

1. **Scorporo in crate** (`arbor-nemus-*`, `arbor-fs`, …) = organizzazione del
   **codice**. Zero impatto RAM: i crate si caricano nello stesso binario,
   stessa WebView2 Environment, stesse finestre. È l'obiettivo di questo refactor.
2. **App separata** (eseguibile/processo a sé) = scelta di **deployment**. *Questa*
   moltiplica la memoria, perché ogni processo istanzia il proprio stack WebView2
   completo.

La RAM di WebView2 si divide in **overhead fisso per Environment** (browser + GPU
+ network + crashpad process, decine di MB, pagato **una volta per Environment**)
e **marginale per webview** (renderer + heap pagina, ~30-70MB **per finestra**).

| Scenario | Costo |
|---|---|
| 1 binario, N finestre, Environment condiviso (**= oggi**) | fisso ×1 + marginale ×N |
| App separate (1 processo ciascuna) | fisso ×N + marginale ×N |

Oggi siamo già nel caso ottimale: tutte le finestre condividono **un** user-data-folder
+ Environment (vedi `explorer_window.rs::WEBVIEW_BROWSER_ARGS` — devono combaciare,
altrimenti `HRESULT 0x8007139F`), quindi browser/GPU/network sono condivisi.

**Regola del piano**: scorporare i crate liberamente, ma **mantenere un solo
binario + N finestre** (Environment condiviso). Estrarre un binario separato
**solo** se l'obiettivo è la *distribuzione indipendente* (es. spedire nemus come
prodotto a sé). Per un utente che usa tutte le finestre, un binario è strettamente
meglio sulla RAM. Lo scorporo in crate preserva comunque la possibilità di
estrarre un binario domani senza averlo già fatto ("free crate split").

> I plugin **WASM** (Parte C) sono all'opposto: non aprono webview, girano in
> `wasmtime` (sandbox CPU/memoria) → zero costo browser. Quella direzione è
> "gratis" sul fronte RAM-webview.

### Modello di processo (DECISO): **D — 1 FE + N BE** (thin-client / LSP)

Quattro modelli erano sul tavolo; **scelto il D**:

| | Unità separata | WebView/RAM | Disaccoppiamento | Artefatto separato |
|---|---|---|---|---|
| A — crate in 1 binario | crate | ✅ 1 WebView | ✅ crate | ❌ 1 exe |
| B — processi pieni | exe (con webview) | ❌ N WebView | ✅ | ✅ |
| C — dylib/wasm | `.dll`/`.wasm` | ✅ 1 WebView | ✅ | ✅ (ma ABI Rust / no nativo in wasm) |
| **D — 1 FE + N BE** ⭐ | **BE headless exe** | ✅ **1 WebView** | ✅ **+ crash-isolation** | ✅ |

**Topologia**: lo **shell process** possiede l'unica WebView2 e fa da **router/broker**; i backend prodotto (`corvus-be`, `merula-be`, `sitta-be`) sono **Rust headless** (no webview) come `.exe` separati. IPC shell↔BE con bootstrap sicuro (lo shell spawna i figli e passa endpoint + nonce; canale ACL'd). Ottiene **tutti e tre**: WebView condivisa (RAM) + N eseguibili (i BE) + disaccoppiamento con **crash-isolation / deploy indipendente** del BE.

**Vincolo FE (DECISO): la FE deve essere disaccoppiata** per estrarre standalone domani. Struttura a pacchetti:
- `fe-shared` (componenti agnostici) · `fe-corvus`/`fe-merula`/`fe-sitta` (UI per-prodotto) · `fe-shell` (host sottile: lazy-load + routing) · `*-ipc` (client IPC per-prodotto).
- **Regola**: un `fe-<prodotto>` importa solo `fe-shared` + il suo `*-ipc`, **mai** un altro prodotto né lo shell. → estrazione standalone = `fe-<prodotto>` + suo BE + mini-shell + webview propria, modulo FE invariato.

**Costo reale (valutazione onesta).** Lo strato IPC è **boilerplate codegen-abile**:
con **`tarpc`** (`#[tarpc::service] trait`) un comando = **~1 definizione** (client +
server tipati generati) → **niente "tassa per-feature"** (l'errore di una prima
analisi). Gli **eventi push** stanno su un **canale one-way dedicato** (stile LSP
notifications), non sul canale RPC; FE-facing restano eventi Tauri. Il
**decoupling/refactor lo pagheresti anche con un binario unico** — non è un costo
specifico di D. I costi *propri* di D sono **due, bounded e una-tantum**:
1. **Transport robusto** (M1): lifecycle processi, race avvio/shutdown, deadlock, backpressure stream, ordini emit/listen — **debugging di runtime**, non codice da scrivere.
2. **Tuning latenza audio** (M4): Merula real-time su IPC — si misura/regola, non si codegen-a.

**Mitigazioni**: (a) codegen dell'IPC da subito; (b) **in-process-first** via
`BrokerClient` per isolare i bug di refactor da quelli di transport, poi flip a IPC.
**Beneficio**: pulizia + crash-isolation + espandibilità. La crash-isolation ha
valore *modesto* per un desktop solo-utente — trade accettato a fronte di costi
circoscritti, non perenni. Altri costi minori: overhead per-call (stream throttlati,
audio dentro `merula-be`), FE unificata a runtime (mitigata dai pacchetti).

### B.1 — Nemus standalone

**Readiness: alta.** I 7 crate `arbor-nemus-*` sono già **100% disaccoppiati**:
zero deps da `arbor-core`, `tauri`, `AppCtx`. Storage già separato
(`%APPDATA%\nemus\`, sibling di `arbor\`). UI già isolata
(`src/lib/components/nemus/`, 115 file, zero store globali Arbor). IPC congelato
(`src/lib/ipc/nemus.ts`).

Tutta la colla vive nello **shell** `src-tauri/src/nemus/` (28 file). Blocker da
sciogliere, in ordine di costo:

| # | Coupling | Dove | Costo | Azione |
|---|----------|------|-------|--------|
| 1 | Path helper | `nemus/config.rs`, `libraries.rs`, `state.rs`, `models.rs`, `packs/`, `mod.rs` chiamano `arbor_core::prelude::{nemus_config_path, nemus_data_dir, arbor_data_dir, client}` | Banale (~30 LOC) | Spostare i path helper nemus in un crate proprio (es. `arbor-nemus-host` o dentro la facade), inline del `client()` HTTP |
| 2 | JobRegistry globale | `nemus/render.rs`, `nemus/packs/download.rs` registrano job in `crate::jobs::JobRegistry` | Basso | Astrarre dietro un trait `JobSink` (render/download accumulano via trait, il guscio lo implementa col registry globale) |
| 3 | Feedback system | UI nemus usa `FeedbackHost`/`FeedbackStatusButtons` da `$lib/feedback/`; job emessi con `target="nemus"` | Medio | `arbor-feedback` è già un crate: renderlo dipendenza opzionale/condivisa, o context-provider lato FE |
| 4 | Window lifecycle | `lib.rs` fa `manage`/`migrate_storage`/`shutdown` di nemus | Basso | Spostare in un setup-module nemus invocato dal guscio |

**Target**: lo shell `src-tauri/src/nemus/` diventa il guscio Tauri di un'app
nemus che dipende solo dai crate `arbor-nemus-*` + `arbor-feedback`. Da lì,
estrarre in un repo/binario separato è meccanico. Le dipendenze native (cpal,
ort/ONNX, symphonia, fundsp) non sono un problema per uno standalone desktop.

### B.2 — Esplora risorse standalone

**Readiness: media.** Più accoppiata di nemus perché è **window-heavy** (cross-window
clipboard, drag-overlay multi-finestra, global shortcut OS) e si appoggia ai
comandi `fs` generici.

Stato attuale:
- BE: `src-tauri/src/explorer_window.rs` (window mgmt, `PendingReveals`, `ExplorerClipboard`, `DragOverlayText`, hit-test drop cross-window, global shortcut `Ctrl+Shift+E`). I/O file via `src-tauri/src/commands/fs_commands.rs` (40KB, condiviso col resto dell'app).
- FE: `src/lib/components/shared/docs/FileExplorer.svelte`, `shared/FileExplorerModal.svelte`, `ExplorerWindow.svelte` (route branch su label `explorer`).
- Memory nota deferred: global-shortcut+new-window (fatti), OS clipboard, system icons, native properties.

Coupling da sciogliere:

| # | Coupling | Costo | Azione |
|---|----------|-------|--------|
| 1 | `fs_commands.rs` condiviso (read/write/copy/move/delete/list/glob) | Medio | Estrarre un crate `arbor-fs` (operazioni FS pure, no Tauri) consumato sia dall'esplora sia dai comandi generici. Candidato naturale di round 1/2 a prescindere |
| 2 | Window/overlay/clipboard tutto su `AppHandle` | Alto | È intrinsecamente legato a Tauri (multi-window, WebView2 env, global shortcut). Resta nel guscio; isolarlo in `explorer/` come modulo coeso |
| 3 | System icons / native properties / OS clipboard | Medio | Capability OS-specific (`windows_sys` ecc.) — vanno dietro un trait platform, non WASM-able |

**Target realistico**: l'esplora *non* diventa un crate-core puro come nemus —
la sua natura è UI+OS. L'estrazione utile è (a) tirare fuori `arbor-fs` (utile a
tutti), (b) raccogliere l'intero modulo finestra in `src-tauri/src/explorer/`
coeso e documentato come "guscio dell'esplora", pronto a diventare un binario
separato che importa `arbor-fs` + il guscio window. Flaggare: il drag-overlay e
il global-shortcut sono `cfg(desktop)` e non avranno mai una storia WASM.

### B.3 — Arbor come launcher (processo condiviso, identità multiple)

> ⚠️ **REVISIONE (decisione utente, 2026-06): il launcher è un prodotto con UI dedicata, stile JetBrains Toolbox.**
> Questa sezione era stata scritta assumendo un launcher *senza UI* — solo il funnel
> single-instance + deep-link + `set_icon` per-finestra (descritto sotto e in M12).
> L'aspettativa reale è più ambiziosa: una **finestra-launcher dedicata** (la "home"/hub
> di Arbor), analoga a **JetBrains Toolbox**, punto d'ingresso e di gestione. Attese:
> - **Hub dei prodotti**: elenca Corvus / Merula / Sitta installati, apertura diretta della finestra-prodotto.
> - **Hub progetti/workspace recenti**: lista navigabile (stile welcome-screen IntelliJ / "Projects" di Toolbox), apre il prodotto giusto sul progetto scelto.
> - **Gestione install/update/versioni** dei prodotti — rilevante quando diventano `*-be` separati / bundle distinti (Modello D).
> - È ciò che appare al lancio "nudo" di `arbor` (nessun `--window=`); da lì si saltano i prodotti.
> **Conseguenze**: serve un `fe-launcher` (modulo FE *shared-only*, niente import di prodotto), una finestra `launcher` nello shell, storage proprio (prodotti/progetti/recenti, sotto `~/.config/arbor`). Il funnel sotto (single-instance + deep-link) resta valido come *plumbing* di routing, ma la UI dedicata è il vero deliverable. **In roadmap**: promosso da "polish M12" a milestone-prodotto a sé; **scope UI/UX da concordare prima del codice.**

Variante al "binario separato": tenere **un solo processo host** ma esporre più
**punti di lancio** (collegamenti/icone distinti) che convergono nello stesso
processo via single-instance + routing degli argomenti. Dà identità desktop
separate **mantenendo la RAM condivisa** (un solo WebView2 Environment).

**Infra già in piedi.** `tauri-plugin-single-instance` è **già dipendenza e già
cablato** in [lib.rs:455](../src-tauri/src/lib.rs). Oggi il callback ignora
`argv` e si limita a rifocalizzare `main`:

```rust
.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
    if let Some(w) = app.get_webview_window("main") { /* unminimize/show/focus */ }
}))
```

C'è anche un **router di deep-link `arbor://`** completo (`DeepLinkBuffer`,
`deep_link_commands`, `on_open_url`) — una seconda strada per il routing senza
parsing CLI.

**Costo d'integrazione: basso.** Le funzioni di apertura finestra esistono già
(`explorer_window::open_or_focus`, l'equivalente nemus). Serve solo:
1. Routing di `argv` nel callback single-instance (~20-30 righe): `--window=nemus|explorer` → `open_or_focus` della finestra giusta.
2. Stesso routing al **cold start** in `setup()` (leggi `std::env::args()`).
3. Tre collegamenti a `arbor.exe --window=…`, ognuno con la sua icona (vedi B.4).

In alternativa, riusare il deep-link: collegamenti che invocano `arbor://open/nemus`.

**Caveat** (già documentati nel codice):
- Single-instance è **OFF in `cargo tauri dev`** (litiga col relaunch del dev runner): si testa in release o con la feature `deep-link-dev` ([lib.rs:452](../src-tauri/src/lib.rs)).
- Resta **un processo** → **nessun isolamento** (shared fate). È funzionalmente "un binario, N finestre" con in più le icone separate. Non confondere con i 3 processi separati.
- 3 eseguibili *distinti* che convergono in 1 host **non** sono questo: single-instance deduplica per identità del singolo exe, servirebbe un named-pipe host/client a mano. Molto più lavoro, nessun vantaggio rispetto a "1 exe + 3 collegamenti".

**"Se lancio l'eseguibile di arbor, chiede l'esecuzione al launcher?"** — Non
esattamente: **non esiste un launcher-daemon separato**. Il funnel è il
single-instance plugin, e funziona così a ogni lancio (doppio-click, collegamento
con args, o URL `arbor://` aperto dall'OS):

- **Nessuna istanza attiva** → il processo appena lanciato **diventa lui** l'istanza primaria e parte normalmente (cold start). Deve auto-instradarsi leggendo i propri `std::env::args()` nel `setup()` (per aprire la finestra giusta).
- **Istanza già attiva** → il nuovo processo parte, rileva il primario (lock di sistema del plugin), **gli inoltra `argv` + `cwd`**, poi **termina subito** senza aprire UI. Il primario gestisce la richiesta nel callback (oggi: focalizza `main`; con B.3: apre/focalizza la finestra indicata).

Quindi il processo appena lanciato **è** il messaggero, non un client che "chiede"
a un launcher: o diventa l'host, o consegna all'host e muore. Lancio diretto
dell'exe e URL `arbor://` passano per lo **stesso** imbuto. È esattamente il
comportamento "launcher" che vuoi: ogni collegamento converge nell'unico processo.

### B.4 — Icone diverse per tipo di finestra/app

**Per-finestra: già possibile, già in uso.** `tauri::WebviewWindow::set_icon`
imposta l'icona di una singola finestra ed è già usata in tre punti:
[taskbar_icon_refresh.rs:77](../src-tauri/src/taskbar_icon_refresh.rs) (refresh
post-sleep), [lib.rs:799](../src-tauri/src/lib.rs) (tray) e perfino dai plugin via
[ns_shell/ui/branding.rs:30](../src-tauri/src/plugin/ns_shell/ui/branding.rs)
(`arbor.ui.set_branding{ window_icon_path }`). Quindi dare a `nemus`/`explorer`
un'icona propria nella titlebar/taskbar è **una `set_icon` alla creazione della
finestra** — zero infrastruttura nuova.

**Identità taskbar separata (raggruppamento): richiede AppUserModelID.** Su
Windows i pulsanti in taskbar si raggruppano per **AppUserModelID (AUMID)**.
Tutte le finestre di un processo condividono l'AUMID del processo → di default
si raggruppano sotto **un solo pulsante** (quello di Arbor), anche con `set_icon`
diversi. Per far comparire nemus/explorer come **voci taskbar separate** (icona +
jump-list proprie, come app distinte) serve assegnare un **AUMID per-finestra**
sull'HWND (`IPropertyStore` / `System.AppUserModel.ID`, via `windows_sys` — già
dipendenza, vedi `platform.rs`). Fattibile, non ancora fatto.

Riepilogo del comportamento icone per scenario:

| Scenario | Icona per-finestra | Voce taskbar separata |
|---|---|---|
| 1 binario, N finestre (oggi) | ✅ `set_icon` (già usata) | ❌ raggruppate sotto Arbor — serve AUMID per-finestra (Win32, da fare) |
| Launcher (B.3, 1 processo) | ✅ `set_icon` | ⚠️ come sopra: separabili solo con AUMID per-finestra |
| 3 eseguibili separati | ✅ icona di bundle propria | ✅ gratis (AUMID per-processo distinto) |

Cioè: l'icona propria la ottieni sempre e subito; la *identità taskbar da app
separata* è gratis solo coi processi separati, altrimenti è un pezzetto di Win32
(`windows_sys`) da scrivere una volta.

**Fattibilità su OS non-Windows.** Il quadro cambia, e in peggio per il modello
"un processo, identità multiple":

- **macOS** — il Dock mostra **una sola icona per applicazione** (per bundle
  `.app`/processo), presa dal `Info.plist` (`CFBundleIconFile`). **Non esiste**
  un'icona-Dock per-finestra: `set_icon` su macOS è di fatto un no-op sul Dock
  (Tauri lo documenta). `NSApplication.setApplicationIconImage` cambia l'unica
  icona app-wide a runtime, non per-finestra. Conclusione: identità Dock separata
  per finestra nello stesso processo = **impossibile**. L'unico modo per avere
  nemus/explorer con icone Dock proprie è **bundle `.app` separati**.
- **Linux** — il raggruppamento + icona in taskbar dipende da `WM_CLASS` (X11) o
  `app_id` (Wayland xdg-shell), con un file `.desktop` corrispondente che fornisce
  l'icona. Per-finestra: su **X11** puoi impostare `_NET_WM_ICON` per finestra
  (icona per-finestra possibile). Ma il *raggruppamento/identità* è per `WM_CLASS`,
  e Tauri/WebKitGTK assegnano una sola class al processo; class diverse per finestra
  sono awkward su X11 e non realmente supportate allo stesso modo su **Wayland**.
  Inoltre l'icona si risolve bene solo se la class matcha un `.desktop` installato.

Riassunto cross-platform:

| | Icona per-finestra | Identità taskbar/Dock separata (1 processo) |
|---|---|---|
| Windows | ✅ `set_icon` | ⚠️ AUMID per-finestra (Win32, da fare) |
| Linux/X11 | ✅ `_NET_WM_ICON` | ⚠️ WM_CLASS per-finestra, fragile; serve `.desktop` |
| Linux/Wayland | ⚠️ limitato | ❌ non per-finestra |
| macOS | ❌ (no per-window Dock) | ❌ impossibile |

**Implicazione architetturale**: se l'identità desktop separata (icona +
raggruppamento) deve funzionare **cross-platform**, l'unica strada robusta sono
**bundle separati** (3 `.exe` / 3 `.app` / 3 `.desktop`) — che è il modello a
processi separati, con il suo costo RAM. Il modello "1 processo, identità
multiple" è un trucco solo-Windows (e parzialmente X11). Da pesare nella scelta.

### B.5 — Come funzionano i deep link `arbor://`

Meccanismo (il routing vero è frontend; il BE fa registrazione + buffering):

1. **Registrazione schema OS**: in `setup()` ([lib.rs:523](../src-tauri/src/lib.rs)) `app.deep_link().register("arbor")` registra `arbor://` presso l'OS (su Windows va in registro; in dev `--no-bundle` la registrazione runtime supplisce all'assenza del bundle).
2. **Arrivo URL**: l'OS, quando si clicca un `arbor://…`, lancia l'eseguibile registrato. Il callback `on_open_url` ([lib.rs:533](../src-tauri/src/lib.rs)) riceve l'URL.
3. **Funnel single-instance**: col plugin single-instance (feature `deep-link`), un secondo lancio non apre una seconda app — inoltra l'URL al primario (vedi B.3).
4. **Buffer cold-start**: l'URL passa per [`DeepLinkBuffer`](../src-tauri/src/deep_link.rs) (`push_or_emit`): se il frontend è pronto emette subito `arbor://deep-link`, altrimenti **bufferizza** finché `deep_link_ready` (chiamato da `AppShell.onMount`) fa il flush. Risolve la race "URL arrivato prima che la webview ascolti". Al cold start gli URL iniziali si leggono via `app.deep_link().get_current()` ([lib.rs:549](../src-tauri/src/lib.rs)).
5. **Dispatch**: il frontend instrada l'URL (repo_open, commit_jump, branch_checkout, mr_open, …), gated da `DeepLinkConfig` (opt-in master + per-azione + conferme; default tutto **off** — CYA: un link condiviso non muta mai un workspace in silenzio).

**Rilevanza per lo scorporo**: il deep-link è già il **bus di routing
inter-finestra/inter-app** di Arbor. Per il modello launcher (B.3) è la strada
più pulita: un collegamento "Nemus" che invoca `arbor://open/nemus` riusa
registrazione + funnel + buffer **già scritti e testati**, invece di aggiungere
parsing CLI. Per app scorporate (nemus/esplora con schema proprio, es.
`nemus://`) lo stesso pattern si replica con `DeepLinkBuffer` per-app. Caveat:
oggi `DeepLinkConfig` è arbor-specifica (azioni git) — un'app diversa porta il
suo set di azioni.

### Ordine Parte B

1. **`arbor-fs`** (FS puro, no Tauri) — sblocca sia esplora sia comandi generici, basso rischio.
2. **Nemus standalone** — il candidato facile; ottimo banco di prova del pattern "app fuori da arbor".
3. **Esplora coesione** — raccogliere il modulo finestra; estrazione binaria solo se/quando serve.
4. **Launcher + icone** (B.3/B.4) — incrementale e a basso costo; indipendente dai punti 1-3. Da fare solo se serve l'identità desktop separata senza pagare RAM.

---

## Parte C — Layer plugin WebAssembly

### C.0 — Inquadramento

Obiettivo: far girare **plugin compilati a `.wasm` autocontenuti** (portano le
proprie deps Rust) in una sandbox, **accanto** ai plugin Lua, scegliendo il
runtime per-plugin. L'host resta nativo.

Perché ora ha senso: il sistema plugin è **già stato progettato multi-runtime**
(vedi [`plugin-api-architecture.md`](plugin-api-architecture.md)). I pezzi
abilitanti esistono già:

- `arbor-plugin-api::PluginValue` — enum bridge cross-runtime (8 varianti: Null/Bool/Int/Float/String/**Bytes**/List/Map). Scelto (decisione D1) **proprio** per attraversare un confine non-in-process. Round-trip JSON↔PluginValue già testato.
- `arbor-plugin-api::HookDispatcher` — name-agnostic, itera `Vec<Arc<dyn HookListener>>`. Oggi un solo listener (`LuaHookListener`); il doc dice già letteralmente *"Domani: anche `WasmRuntime::install(..., dispatcher.clone())`"*.
- `Permissions.ext: HashMap<String, toml::Value>` + permission tipate (`fs_scope`, `network`, `terminal_scope`, …) — il contratto che un guest WASM rispetterebbe, **identico** a quello Lua.
- Trait di dominio già a confine `Arc<dyn>`: `StudioFormatBackend` + `StudioRegistry`, futuro `GitProvider`, `IssueTracker`, `PipelineStep`.

### C.1 — Scelta del runtime: wasmtime + ABI manuale su `PluginValue`

Decisione: **wasmtime** come engine, **ABI manuale** con marshalling
`PluginValue → bytes` sulla linear memory (NON Component Model/WIT in prima
battuta).

Motivazione:
- Riusa il tipo `PluginValue` che già esiste e già fa round-trip → niente nuovo schema da mantenere in parallelo.
- Meno boilerplate/infrastruttura iniziale del Component Model; si parte prima.
- wasmtime ha sandbox solida, fuel metering (anti-loop infiniti), epoch interruption (timeout), e capability-based imports — mappano 1:1 sul permission model.

⚠️ **`wasmtime` non è nel workspace** e va **approvato** prima (hard rule 7:
niente librerie senza OK). Non aggiungere altre deps (`wit-bindgen`, ecc.) senza
richiesta esplicita. Il Component Model resta una possibile migrazione futura se
l'ABI manuale diventa scomodo — `PluginValue` non preclude la strada.

### C.2 — ABI host↔guest

Modello: ogni chiamata passa **un blob `PluginValue` serializzato** in/out della
linear memory del modulo.

```
Host (nativo)                         Guest (.wasm)
─────────────                         ─────────────
  PluginValue  ──serialize──> bytes
                              [write linear mem @ ptr,len]
  call export  ───────────────────────> fn(ptr,len) -> (ptr,len)
                                          deserialize -> PluginValue
                                          ...logica plugin...
                                          serialize result
                              [read linear mem] <─── ret(ptr,len)
  deserialize <── bytes
  PluginValue
```

Convenzioni:
- **Guest exports**: `alloc(len)->ptr` / `dealloc(ptr,len)` (gestione memoria), + un export per ogni entry-point (es. `studio_parse`, `studio_apply_mutation`, `on_hook`). Ognuno `(ptr,len) -> u64` (ptr<<32|len del risultato).
- **Host imports** (capability, gated da permission del manifest): `host_fs_read`, `host_fs_write`, `host_http`, `host_log`, `host_emit`, `host_settings_*`, … Ogni import controlla `fs_scope`/`network` **prima** di eseguire. Questo è il punto chiave: un plugin WASM è sandboxed → rete e FS **non** ci sono di default, devono essere concesse dall'host esattamente come per Lua. La sandbox è *più forte* di quella mlua.
- **Serializzazione del blob**: scegliere un formato compatto (es. un encoding binario di `PluginValue`, NON JSON, per il hot path). Da decidere; `PluginValue` ha già `Bytes` per i payload grossi (file content, audio, ecc.).
- **Async**: i trait di dominio (`StudioFormatBackend`, `PluginFn`) sono async-friendly. wasm guest è sync; l'host wrappa la chiamata wasm in `spawn_blocking`/thread, gli import async (http) si risolvono host-side e tornano sincroni al guest (come fa già `arbor.http` per Lua via current-thread runtime).

### C.3 — Componenti da costruire

| Componente | Dove | Analogo Lua esistente |
|------------|------|------------------------|
| `arbor-plugin-wasm` (crate nuovo) | `crates/plugin/wasm/` | `arbor-plugin-core` (mlua) |
| `WasmRuntime` (carica `.wasm`, istanzia, linka import) | `crates/plugin/wasm/src/runtime.rs` | `PluginHost` + `sandbox` |
| `WasmHookListener: HookListener` | `crates/plugin/wasm/src/hook_listener.rs` | `LuaHookListener` (hook_router.rs) |
| Host capability imports (fs/http/log/emit/settings) gated da permission | `crates/plugin/wasm/src/host_fns.rs` | i ns `arbor.*` in `lua_api/ns/` |
| Adapter di dominio: `WasmStudioBackend: StudioFormatBackend` | accanto al `StudioRegistry` | — (nuovo) |
| SDK Rust per autori (tipi + macro `#[arbor_plugin]`) | `crates/plugin/wasm-sdk/` o repo `arbor-extensions` | `sdk.d.lua` |
| Manifest: `runtime = "lua" | "wasm"` + `entry = "plugin.wasm"` | `arbor-plugin-types::Manifest` | `entry = "main.lua"` |

Registrazione al boot (visione target, vedi snippet in `plugin-api-architecture.md`):
```rust
let dispatcher = Arc::new(HookDispatcher::new());
LuaRuntime::install(app_ctx.clone(),  registry.clone(), dispatcher.clone());
WasmRuntime::install(app_ctx.clone(), registry.clone(), dispatcher.clone()); // ← nuovo listener
```

### C.4 — Candidati alla migrazione WASM (ordine consigliato)

#### 1. Studi (RON/JSON/TOML/YAML/.properties) — **candidato #1**

Perché ideale: il confine **esiste già**. `StudioFormatBackend`
(`src-tauri/src/studio/format/backend.rs`) è un trait con metodi tutti
serializzabili (`parse`, `apply_mutation`, `query`, `diff`, `save`, schema, bulk
edit, rename refactor), registrato in `StudioRegistry` via `Arc<dyn>` al boot.
Le impl per-formato (`json_studio/`, `ron_studio/`, `toml_studio/`,
`yaml_studio/`, `properties_studio/`) sono **puro parsing** (serde, ron,
serde_yaml_ng, toml, jsonc-parser) → compilano a wasm32 senza blocker nativi.

Migrazione: un `WasmStudioBackend` che implementa `StudioFormatBackend`
serializzando ogni metodo via ABI verso il `.wasm`. Il `StudioRegistry` non sa
se il backend è nativo o wasm. Beneficio diretto della roadmap "Migrate the
Studio plugins" (vedi `roadmap.md`): la footprint CodeMirror+schema esce dal
processo host. **Ogni nuovo formato = un plugin WASM**, niente patch al binario.

#### 2. Cloud-storage — **candidato #2**

`arbor-cloud` (opendal GCS/S3/Azure + `aws-lc-sys`) è già destinato a uscire dal
binario (commenti in `Cargo.toml` root + roadmap "Migrate cloud-storage"). 

⚠️ Tensione da risolvere: opendal fa HTTP nativo; un guest WASM **non ha socket**.
Due strade:
- (a) Il plugin WASM delega tutto l'HTTP all'host via `host_http` import (gated da `network` permission). Pulito ma l'opendal interno andrebbe configurato su un transport custom.
- (b) Cloud-storage resta un plugin **subprocess** nativo (la roadmap originale), non WASM. Più semplice se opendal non si piega al transport host.

**Decisione aperta** — vedi sotto.

#### 3. DB-query (futuro) — **candidato #3, possibile app-standalone**

L'utente lo vede potenzialmente "come nemus" (progetto a sé). Due inquadramenti:
- **Plugin WASM**: un explorer/query di DB come plugin che usa `host_*` per I/O. Adatto se è una *feature dentro Arbor*.
- **App-standalone** (pattern Parte B): se cresce in un IDE-da-DB con finestra propria, storage proprio, UI grande, allora è un sotto-sistema come nemus, non un plugin.

**Decisione aperta** — dipende dall'ambizione della feature. Default suggerito:
partire come plugin WASM (riusa tutta l'infra C), promuovere ad app-standalone
solo se la UI/stato esplode come è successo a nemus.

### C.6 — Plugin host come engine condiviso (arbor / nemus / esplora)

Idea: **astrarre il plugin host** così che non solo Arbor, ma anche nemus e
l'esplora (e domani un DB manager) possano avere i propri plugin, riusando lo
stesso motore.

**Quanto è già astratto (buona notizia).** Il design del refactor round 1 ha già
fatto l'80% del lavoro:

- `arbor-plugin-core` **non dipende da `tauri`**: l'astrazione passa per `arbor-core::AppCtx` (decisione C3). Qualsiasi app che implementa `AppCtx` può ospitarlo.
- I namespace sono **iniettabili**: `register(lua, params, extra_installers: &[Arc<dyn LuaNamespaceInstaller>])` ([lua_api/mod.rs:58](../crates/plugin/core/src/lua_api/mod.rs)) lascia all'host fornire i propri namespace di dominio. Arbor inietta i suoi 16 `ns_shell/*`; nemus inietterebbe `nemus.*` (transport, clip, render…), l'esplora `explorer.*`.
- Infra condivisa già crate-level e Tauri-free: scheduler (`arbor-scheduler`), contribution registry, settings store, hook dispatcher (`arbor-plugin-api`), marketplace (`arbor-plugin-marketplace`).
- `HookDispatcher` name-agnostic: ogni app registra i **propri** hook + il proprio `HookListener`.

**Cosa manca per renderlo davvero multi-app:**

| Gap | Oggi | Serve |
|-----|------|-------|
| Set di namespace host-pure | `register()` **hardcoda** 22 ns (inclusi gli studi e `ui` form) | Renderlo **selezionabile per host** (feature flags / lista installer), così nemus prende solo ciò che serve |
| Marketplace mono-host | plugin roots + registry URL arbor-specifici | **Per-host plugin roots** + campo manifest `host`/`targets` (es. `targets = ["nemus"]`) per filtrare quali plugin valgono per quale app |
| Hook catalog | `HOOK_CATALOG` arbor-centrico | Ogni app **contribuisce il suo catalogo** (nemus: `on_clip_launch`, `on_render_done`; esplora: `on_file_open`…) via il dispatcher |
| Settings/storage plugin | sotto `~/.config/arbor` | Namespacing per-host (nemus ha già `%APPDATA%\nemus`) |
| `AppCtx` capability | tagliate sui bisogni di Arbor | Ogni host estende `AppCtx` con le sue (nemus: `active_project`, …) |

**Verdetto**: non è un riscrittura, è **promuovere `arbor-plugin-core` da "host
di Arbor" a "engine di plugin riusabile"**. Le mosse: (a) parametrizzare la lista
host-pure, (b) marketplace multi-host + `targets` nel manifest, (c) catalogo hook
per-app. Con questo, nemus/esplora ottengono un sistema plugin "gratis"
condividendo motore, sandbox, scheduler, UI-contribution e — quando atterra la
Parte C — anche il runtime WASM. È la stessa logica del "free crate split":
il confine c'è già, va solo reso esplicito.

### C.7 — Caso studio: plugin DB manager (query DB) → eventuale standalone

Un DB manager è il banco di prova più severo: stressa proprio i limiti attuali
del sistema plugin. Analisi dei gap, prima **come plugin**, poi **promozione a
standalone** (pattern nemus).

**Cosa manca lato plugin (in ordine di gravità):**

1. **Capability di rete grezza / DB — BLOCCANTE.** Oggi l'unica rete plugin è `arbor.http` (GET/POST), permission `network = [hostnames]`. Postgres/MySQL parlano un **wire protocol su TCP**, non HTTP → oggi **impossibile**. Confermato: `tcp`/`socket` compaiono solo come stringhe in `permissions.rs`, non come capability viva. Servono due opzioni:
   - (a) capability `arbor.net` (TCP raw) gated da permission `net = [host:port]` — il plugin porta il driver (pure-Rust) e parla il protocollo;
   - (b) namespace host `arbor.db` — l'**host** possiede il driver (sqlx/tokio-postgres, native), il plugin manda SQL e riceve righe. Più sicuro, meno potente.
2. **Secret store — BLOCCANTE.** Le connessioni DB hanno credenziali. I plugin hanno `arbor.settings` (config in chiaro), **nessun** secret store. Serve `arbor.secrets` su keyring, gated da permission. (Arbor usa già `keyring` lato host per OAuth — la capability esiste, va esposta.)
3. **Handle di risorse long-lived.** Il modello plugin è request/response + hook + timer. Una connessione DB è persistente (pool, transazioni, cursori). Manca il concetto di **handle opaco gestito dall'host** (`open()` → handle → riuso/chiusura).
4. **Streaming result-set.** I result possono essere enormi. `PluginValue` ha `Bytes`/`List`/`Map` ma **nessuna paginazione/streaming**. Per WASM è critico: copiare 100k righe nella linear memory a ogni call è proibitivo. Serve un'API cursor/stream.
5. **Superficie UI ricca — IL gap più grosso.** Un DB manager vuole data-grid, editor SQL con highlight, schema-tree, tab di risultati. La UI plugin oggi = `arbor.ui.form` + contribution points + bottom panel. **Non esiste** "il plugin possiede una view/editor custom a tutto schermo". Non a caso gli **studi sono feature host** (CodeMirror nel FE host), **non** plugin. Un DB manager-plugin sarebbe limitato a form finché non aggiungiamo una **"plugin custom view"** (una region webview/iframe che il plugin controlla, o un set di componenti dichiarativi molto più ricco).

**Perché WASM da solo non basta qui.** Un guest WASM è sandboxed → niente socket:
il punto 1 va comunque risolto host-side (capability `net`/`db`). E i punti 4-5
(streaming, view ricca) sono ortogonali al runtime. Quindi il DB manager **come
plugin puro** richiede estensioni host significative, non solo "compilarlo a wasm".

**Promozione a standalone (come nemus).** Quando la UI/stato esplodono
(multi-connessione, history, schema explorer, grid avanzata) vale la stessa
traiettoria di nemus: **finestra propria + storage proprio + crate propri**, con
i driver DB **native** (come nemus usa cpal/ort senza problemi in desktop), e —
se vuole estensioni proprie (driver per-dialetto, formatter SQL) — **incorpora il
plugin host astratto** della C.6. In quel mondo i gap 1-2 spariscono (l'app native
ha socket e keyring), e il gap 5 si risolve costruendo la UI nativamente, non come
contribution.

**Raccomandazione**: il DB manager è un candidato **più da host-feature/standalone
che da plugin WASM puro**. Inquadramento a tre livelli:
- *MVP dentro Arbor*: namespace host `arbor.db` (opzione 1b) + una view custom minimale → utile subito, gap 5 ridotto.
- *Plugin serio*: richiede capability `net`+`secrets`+handle+stream+custom-view (estensioni host pesanti).
- *Standalone*: quando cresce, esce come nemus, riusando il plugin engine C.6.

Le capability `net`, `secrets`, handle e stream sono **generiche** (non DB-specifiche):
servirebbero anche ad altri plugin (client API stateful, watcher, ecc.) → vale la
pena progettarle come estensioni generali dell'host, non come un namespace `db`
una-tantum.

### C.5 — Cosa NON cambia (backward-compat)

- I plugin **Lua esistenti continuano a girare** senza modifiche. WASM è additivo: un secondo `HookListener` + un secondo runtime. Il contratto on-the-wire (`arbor.fs.read_text`, `on_commit`, permission del `plugin.toml`) resta invariato.
- Il marketplace (`arbor-plugin-marketplace`) deve imparare a distribuire artefatti `.wasm` oltre ai sorgenti Lua — ma è un'estensione del fetcher, non un redesign.

---

## Parte D — Quanto dipendono da Arbor (analisi di dipendenza)

Misura, per ognuno dei tre, **dove** vive l'accoppiamento (codice vs contratto a
runtime) e **quanto** è estraibile. Sintesi comparativa in fondo.

### D.1 — Plugin

**Accoppiamento di codice: ZERO.** I plugin vivono in un repo separato
(`arbor-extensions`), sono distribuiti via marketplace, e non linkano codice
Arbor. Dipendono da Arbor **solo attraverso il contratto a runtime**:

1. La superficie `arbor.*` (~33 namespace, ~150 funzioni).
2. Il `HOOK_CATALOG` (gli hook a cui si sottoscrivono).
3. Il modello di permessi del `plugin.toml`.

Il punto chiave per WASM/scorporo è che **questa superficie è già stratificata**
in due tier per grado di accoppiamento ad Arbor:

| Tier | Dove | Namespace | Dipendenza |
|------|------|-----------|------------|
| **Host-pure** | `arbor-plugin-core::lua_api::ns/*` (~22) | `log`, `events`, `json`, `text`, `meta`, `notify`, `hooks`, `command`, `keybinding`, `service`, `timer`, `scheduler`, `contribution`, `fs`, `http`, `settings`, `ui/*`, studi | Solo `arbor-plugin-core` + `AppCtx`. **Nessuna** conoscenza di git/repo/workspace. Portabili così come sono |
| **Shell** | `src-tauri/src/plugin/ns_shell/*` (16) | `repo`, `mr`, `ci`, `issues`, `notes`, `pipeline`, `cloud`, `brp`, `security`, `toolchain`, `terminal`, `tabs`, `workspace`, `linked_worktrees`, `job`, `ui/branding` | Legati ai domini Arbor (git, provider, pipeline…). Migrano ai crate di dominio in PR #6+ |

**Implicazione**: la dipendenza *effettiva* di un singolo plugin da Arbor =
quali namespace chiama. Un plugin che usa solo `fs`/`http`/`json`/`ui`/`settings`
(es. un formatter, un linter, uno studio) dipende **solo dal tier host-pure** →
candidato perfetto a girare come `.wasm` (vedi Parte C) anche prima che i domini
shell siano migrati. Un plugin che usa `repo`/`mr`/`workspace` dipende dai domini
Arbor → potrà andare WASM man mano che i crate di dominio atterrano (round 1 PR
#6+) e contribuiscono i loro namespace in modo runtime-agnostic.

Già pronto per il multi-runtime: `PluginValue` (bridge), `HookDispatcher`
(N listener), `Permissions.ext`. Vedi Parte C.

### D.2 — Esplora risorse

**Estraibilità ~70%** (richiede refactor moderato). L'accoppiamento è quasi tutto
**frontend**; il backend è già pulito.

**Backend — accoppiamento basso.** [explorer_window.rs](../src-tauri/src/explorer_window.rs)
tocca solo i **propri** 3 campi di `AppState` (`PendingReveals`,
`ExplorerClipboard`, `DragOverlayText`) + `crate::config::app_config` (per lo
shortcut globale). I comandi FS ([fs_commands.rs](../src-tauri/src/commands/fs_commands.rs))
sono **generici** (read/write/copy/move/delete/list/glob/zip/watch/icon). I
comandi git ([fs_git_commands.rs](../src-tauri/src/commands/fs_git_commands.rs))
sono **path-based e opzionali** (scoprono il repo dal path, non dipendono dallo
stato repo/tab). **Zero** coupling a workspace/repo-store/graph.

**Frontend — 7 dipendenze hard** (in `ExplorerWindow.svelte` + `FileExplorerModal.svelte`):

| # | Dipendenza | Uso | Mitigazione |
|---|-----------|-----|-------------|
| 1 | `explorerStore` + theme/appearance/animations store | config + theming | parametrizzare con un config object + loader standalone |
| 2 | `uiStore.showToast()` | notifiche | callback iniettata / `arbor-feedback` |
| 3 | `tabsStore.activeTab?.path` | "repo attivo" per la sezione Projects | reso opzionale |
| 4 | `listWorkspaces` / `listRegistryRepos` | sidebar Projects | feature flag opzionale |
| 5 | `dispatchDeepLink` | address-bar `arbor://` | disabilitabile in standalone |
| 6 | `fsOpenInArbor` | delega git pesante alla finestra main | opzionale |
| 7 | persistenza config in `~/.config/arbor/config.toml` | settings esplora | equivalente standalone |

**Già disaccoppiato**: tutte le operazioni FS, la git-awareness (opzionale,
path-based), drag/drop e clipboard cross-window (stato self-contained), tutti i
widget `shared/ui/*` usati (Button, Card, Tree, ContextMenu, Dropdown, Tabs,
WindowControls…). Natura del modulo: **UI + OS**, non un core puro. L'estrazione
utile è (a) tirare fuori `arbor-fs`, (b) parametrizzare le 7 dipendenze FE.

### D.3 — Nemus

**Estraibilità ~95%** (la più alta). Già progettato per uscire.

- **Crate (`arbor-nemus-*`, 7): accoppiamento ZERO.** Nessuna dipendenza da `arbor-core`/`tauri`/`AppCtx`. Sotto-workspace autosufficiente.
- **UI (115 file in `components/nemus/` + 33 store): ZERO store globali Arbor.** Importa solo da `components/nemus/` e `shared/ui/`. IPC congelato (`ipc/nemus.ts`).
- **Storage: già separato** (`%APPDATA%\nemus\`, sibling di `arbor\`).
- **Shell (`src-tauri/src/nemus/`, 28 file): unico punto di colla**, e minimale → `arbor_core` solo per **path helper** + `client()` HTTP, `crate::jobs::JobRegistry` per render/download, `arbor-feedback` per i toast (target `"nemus"`). Dettaglio e mitigazioni in [B.1](#b1--nemus-standalone).

### Sintesi comparativa

| Sottosistema | Coupling di codice | Coupling a runtime | Estraibilità | Lavoro residuo |
|---|---|---|---|---|
| **Plugin** | Zero (repo separato) | Solo contratto API `arbor.*` (host-pure vs shell) | n/a (già fuori) | Adapter WASM (Parte C) |
| **Nemus** | Zero nei crate; shell → `arbor_core` paths + jobs + feedback | UI zero store globali | **~95%** | Path helper, JobSink trait, feedback opzionale |
| **Esplora** | Backend basso (3 struct proprie); FE 7 dep hard | FS generico + git path-based opzionale | **~70%** | `arbor-fs`, parametrizzare 7 dep FE, theming standalone |

Ordine di facilità decrescente: **plugin** (già fuori) → **nemus** → **esplora**.

### D.4 — Funzionalità Arbor dentro l'esplora: come integrarle

Oggi l'esplora **hardcoda** la "arbor-ness": git-awareness (`fs_git_*`),
"Open in Arbor" (`fsOpenInArbor`), sezione Projects che legge il registry repo
(`listWorkspaces`/`listRegistryRepos`), deep-link. Se l'esplora diventa app a sé,
queste vanno re-integrate **senza** ricreare l'accoppiamento. Tre modelli:

| Modello | Come | Costo | Coupling |
|---------|------|-------|----------|
| A — hardcode (oggi) | tutto nell'esplora | nessuno | alto |
| B — **plugin via engine (C.6)** | l'esplora ospita il plugin engine; Arbor contribuisce una feature | medio (serve C.6 + contribution points esplora) | **nullo** (contratto) |
| C — IPC tra app | l'esplora chiama Arbor via socket | alto | medio |

**Raccomandato: B + deep-link.** È la chiusura del cerchio che giustifica
l'investimento C.6. Si scompone così:

1. **Git-awareness → capability *generica*, non "Arbor".** Lo status/stage/discard
   path-based è utile a **qualsiasi** file manager, non solo ad Arbor. Diventa una
   capability host (`arbor.git` su `arbor-git`/`arbor-fs-git`, gated da permission
   `git`), o un plugin git generico. Niente di arbor-specifico.
2. **"Open in Arbor" → deep-link.** Un'azione di context-menu che spara
   `arbor://repo/open?path=…`. È il **bridge inter-app pulito** (B.5): funziona che
   Arbor sia aperto o chiuso (il funnel single-instance lo lancia/focalizza al
   repo). **Zero coupling di codice** — solo una stringa URL.
3. **Sezione "Projects" + badge "questa cartella = repo X / workspace Y" → plugin
   ad-hoc di Arbor.** *Questo* è il tuo "plugin di Arbor che legge i suoi
   repository". Un plugin (che Arbor spedisce, `targets = ["explorer"]`) che:
   - **legge il registry repo di Arbor** (file di config sotto `~/.config/arbor`, con `fs` read-scope a quel path) → niente bisogno che Arbor giri;
   - contribuisce una **sezione sidebar** "Projects" e **badge/colonne** sui file via i contribution point dell'esplora;
   - aggiunge l'azione context-menu "Open in Arbor" (deep-link del punto 2).

**Cosa serve perché funzioni** (oltre a C.6):
- l'esplora deve **esporre i propri contribution point**: `explorer:file-badge`/`column`, `explorer:context-menu:file|folder`, `explorer:sidebar-section`, `explorer:address-bar-action`. Oggi i contribution point sono arbor-specifici (`arbor:sidebar`, `arbor:context-menu:<target>`) → l'esplora ne definisce di propri, stesso meccanismo `ContributionRegistry`.
- una **read capability** per file di config arbitrari (il registry) — già coperta da `arbor.fs` con scope.

**Payoff**: l'esplora si spedisce come **file manager pulito** (zero git, zero
Arbor). Chi ha solo l'esplora ha un file manager; chi ha Arbor installato vede
"accendersi" git + Projects perché il **plugin Arbor** li contribuisce. Esattamente
il valore di C.6: le feature Arbor entrano nell'esplora **come plugin**, non come
codice incollato. E lo stesso pattern vale al contrario (nemus che contribuisce
una sezione "Tracks" all'esplora, ecc.).

### D.5 — Credenziali: il launcher come broker (+ caching)

Coerente col modello D: lo **shell/launcher è l'unico detentore del keyring**
(broker). FE e backend prodotto **non toccano mai il keyring** — lo chiedono via
lo shell-router (IPC, lo stesso canale dei comandi). Vantaggi: su macOS una sola
app registrata col Keychain (niente prompt per-prodotto); policy in un punto solo;
blast radius minimo.

**Token boundary** (F4/D2): il broker espone **capability/risultati o credenziali
scoped**, non vende il token raw a FE/backend quando può evitarlo. Plugin: **mai**
(D1).

**Caching** (le credenziali si usano spesso → leggere il keyring ad ogni op è
lento, su macOS può promptare). Cache **in memoria nel broker** — dove l'accesso
keyring già vive, quindi nessuna nuova superficie. Regole:

1. **Solo memoria, mai su disco** (il disco è il keyring; la cache muore alla chiusura).
2. **Cache l'access token short-lived; il refresh token resta nel keyring** (il segreto pesante non lascia il keyring).
3. **TTL allineato all'expiry** + **invalida su 401/403** (revoca/rotazione).
4. **Mai a FE né ai backend** — interna al broker (token boundary).
5. **Opzionale**: `zeroize`-on-drop per ripulire la memoria (dep nuova, da approvare).

**Sicurezza**: accettabile sotto il threat model desktop — il segreto è in memoria
mentre lo usi comunque, e same-user può leggere keyring/memoria a prescindere (il
keyring protegge *a riposo*). Il caching allarga di poco la finestra in-memory →
mitigata da TTL corto + zeroize. Pratica standard (`ssh-agent`,
`git-credential-cache`, client OAuth).

## Parte E — Resoconto finale: fattibilità, gap, struttura crate

### E.1 — Verdetto di fattibilità

| Obiettivo | Fattibilità | Stato | Sforzo residuo |
|-----------|-------------|-------|----------------|
| Crate split domini (round 1 #5+) | ✅ alta | ~60% | esecuzione meccanica di un piano scritto |
| Scorporo **nemus** | ✅ alta | ~95% | basso (path helper, JobSink, feedback opzionale) |
| Scorporo **esplora** | ✅ media | ~70% | medio (`arbor-fs`, parametrizzare 7 dep FE, contribution point propri) |
| **Launcher** (1 processo, N identità) | ✅ alta | infra già cablata | basso (routing argv/deep-link) |
| **Icone separate** cross-platform | ⚠️ parziale | per-finestra OK; identità solo Win/X11 | bundle separati per il caso macOS |
| **Plugin WASM** | ✅ media-alta | API già progettata | medio-alto (runtime + ABI + adapter) |
| **Engine plugin multi-app** (C.6) | ✅ alta | ~80% | medio (lista host-pure selez., marketplace multi-host) |
| **DB manager** plugin/standalone | ⚠️ media | gap reali | alto (capability net/secrets/handle/stream + custom-view) |
| Arbor stesso in WASM (browser) | ❌ bassa | bloccato da git2/pty/cpal | fuori scope |

**Sintesi**: tutto ciò che hai chiesto è **fattibile e in gran parte già
imbastito**. Non ci sono muri architetturali; il lavoro è migrazione di codice
(crate) + adozione di 2 tecnologie nuove (wasmtime, capability host) + rendere
espliciti confini già esistenti (engine multi-app).

### E.2 — Quanto manca, per fasi

> Roadmap operativa dettagliata (milestone M0-M12, step, gate, dipendenze, size):
> [`docs/migration-roadmap.md`](migration-roadmap.md). Qui sotto la sintesi.

1. **Finire round 1** (PR #5→finale): domini ancora shell + rinomina `src-tauri`→`arbor`. Sblocca tutto. *Nessuna nuova tecnologia.*
2. **`arbor-fs`** + parametrizzare le 7 dep FE dell'esplora. Sblocca esplora e ripulisce i comandi FS.
3. **Nemus standalone**: shell proprio, `JobSink` trait, feedback opzionale. Il banco di prova del pattern app-fuori-da-arbor.
4. **Engine plugin multi-app (C.6)**: lista host-pure selezionabile, marketplace multi-host + `targets`, catalogo hook per-app. Abilita plugin in nemus/esplora.
5. **Contribution point dell'esplora + plugin Arbor ad-hoc (D.4)**: chiude l'integrazione esplora↔arbor senza coupling.
6. **Runtime WASM (Parte C)**: `arbor-plugin-wasm` (wasmtime + ABI su `PluginValue`) + `WasmHookListener` + adapter `WasmStudioBackend`. Primo candidato: gli studi.
7. **Capability host generiche (C.7)**: `net`/`secrets`/handle/stream/custom-view — sbloccano DB manager e plugin stateful.
8. **Migrazioni opportunistiche**: studi → crate + WASM; cloud → WASM o subprocess; DB manager (plugin o standalone).

Fasi 1-3 sono indipendenti e a basso rischio: si possono fare subito. 4-5 sono il
cuore "engine riusabile". 6-7 introducono le tecnologie nuove. 8 è raccolto.

### E.3 — Naming & struttura crate (per-prodotto)

**Decisione di branding.** `Arbor` smette di essere "git client + ombrello" e
diventa **solo la piattaforma / launcher** (l'albero che ospita le creature). I
prodotti sono uccelli del bosco che vivono nell'albero:

```
Arbor                       ← l'albero: piattaforma + launcher + plugin engine
 ├── Corvus   (git)         ← il corvo: memoria e storia (version control)
 ├── Merula   (musica)      ← il merlo: canto              (ex nemus)
 └── Sitta    (file)        ← la sitta: esplora l'albero   (esplora risorse)
```

Vantaggio pratico **enorme**: dato che Arbor resta legittimamente il nome della
**piattaforma**, i crate foundation/plugin/studio **tengono il prefisso `arbor-`
senza rename**. Solo i crate di *prodotto* cambiano prefisso. La semantica delle
dipendenze finalmente fila: *"`merula-shell` dipende da `arbor-core`"* = "l'app
musica dipende dalla piattaforma Arbor".

| Prefisso | Cosa | Rename? |
|----------|------|---------|
| **`arbor-`** | piattaforma: core, process-ext, auth, feedback, scheduler, fs, shell-common, cloud · plugin-{types,api,core,wasm,marketplace,wasm-sdk} · studio-* | ✅ **restano** |
| **`corvus-`** | git client: git, git-provider-*, issue-tracker-*, pipeline-*, brp, workspaces, linked-worktrees, terminal, shell | 🔁 rinomina da `arbor-*` |
| **`merula-`** | musica: pattern, lang, audio, engine, import, transcribe, facade, shell | 🔁 rinomina da `arbor-nemus-*` |
| **`sitta-`** | file explorer: shell (usa `arbor-fs`) | ➕ nuovo |

> Il DSL/estensione `.nemus` può restare come **sotto-brand** di Merula (prodotto
> ≠ formato): rinomini crate/app, i tuoi file `.nemus` non si toccano.
> Brand: oggi "Arbor" = il git client user-facing → rebranding del prodotto a
> **Corvus** (decisione consapevole, vedi nota in fondo a E.3).

**Struttura directory (per-prodotto, vertical slice).** Path liberi, nomi crate
col prefisso giusto. Legenda: ✅ fatto · 🔲 shell round 1 · ➕ nuovo · 🔁 rinomina.

```
crates/
  foundation/                    arbor-*  (piattaforma)
    core ✅  process-ext ✅  auth ✅  feedback ✅  scheduler ✅
    fs ➕   shell-common ➕ (launcher/single-instance/deep-link/window/icone)
    cloud ✅ (o → plugin quando diventa plugin)
  plugin/                        arbor-plugin-*  (engine riusabile)
    types ✅  api ✅  core ✅🔁  marketplace ✅🔁  wasm ➕  wasm-sdk ➕
  studio/                        arbor-studio-*  (format backend, wasm-able)
    core ➕  json ➕ ron ➕ yaml ➕ toml ➕ properties ➕
  corvus/                        corvus-*  (git client)
    git ➕  git-provider/{api,github,gitlab} 🔲  issue-tracker/{api,github,gitlab,jira,linear} 🔲
    pipeline/{api,core} 🔲  brp 🔲  workspaces ➕  linked-worktrees ➕  terminal ➕
    shell ➕ (window/UI + comandi + impl AppCtx)
  merula/                        merula-*  (musica, ex nemus)
    pattern ✅ lang ✅ audio ✅ engine ✅ import ✅ transcribe ✅ facade ✅
    shell ➕
  sitta/                         sitta-*  (file explorer)
    shell ➕ (dep arbor-fs; arbor git-awareness opzionale via plugin)

bins/
  arbor          ⭐ l'UNICO binario: launcher/single-instance + routing finestre,
                    dep corvus-shell + merula-shell + sitta-shell + arbor-shell-common
  [corvus] [merula] [sitta]   opzionali: bin thin, SOLO per distribuzione separata
```

**Principio di assemblaggio** (decisione RAM): di **default un solo binario
`arbor`** che monta le finestre dei prodotti condividendo una WebView2 Environment
— RAM minima. I crate sono splittati per-prodotto, quindi un bin separato è
`cargo new` + guscio sottile + `arbor-shell-common`, **solo** per distribuzione
indipendente. Split crate = gratis in RAM; bin separato = no (Parte B).

**Dipendenze ancora a layer** anche se le cartelle sono per-prodotto: `foundation`
← tutti; `plugin` ← incorporato dagli shell; `studio` ← plugin; ogni `*-shell`
← i suoi domini + foundation; `arbor` (bin) ← gli shell. Niente cicli, i `*-api`
restano leaf.

**Nota rebranding**: spostare "Arbor" da git-client a piattaforma e rinominare il
client in **Corvus** è la decisione user-facing da prendere consapevolmente; il
resto è rename meccanico, da accorpare al pass di rinomina già pianificato in
round 1 (PR finale). Conviene fissare i nomi **presto**: il costo del rename
cresce con ogni nuovo crate.

### E.4 — Cosa NON fare

- **Non** trasformare l'esplora in un crate-core puro come nemus: è UI+OS. Il deliverable è `arbor-fs` + contribution point, non un binario a sé (a meno di distribuzione).
- **Non** compilare Arbor intero a WASM-browser: gated da git2/pty/cpal, fuori scope.
- **Non** fare un namespace `db` una-tantum: le capability (net/secrets/handle/stream) vanno generiche.
- **Non** introdurre processi separati per "risparmiare" o "isolare" senza un motivo di distribuzione: paghi RAM senza guadagno.

## Decisioni — checklist completa

`[DECISO]` = già scelto · le altre sono da confermare. La colonna "serve entro"
rimanda ai milestone di [`docs/migration-roadmap.md`](migration-roadmap.md).

**Risolte (sessione naming/roadmap):**
- **C1** ✅ `wasmtime` **approvato** come dipendenza.
- **C2** ✅ ABI = **encoding binario custom di `PluginValue`** (no nuova dep; postcard/bincode resta fallback se diventa scomodo).
- **C3** ✅ **standalone-first**: l'engine multi-app (C.6 / M6) si fa **dopo** che i 3 prodotti girano.
- **C4** ✅ SDK WASM = **crate dedicato** (`arbor-plugin-wasm-sdk`) che dipende da un **core di contratto condiviso** col runtime Lua (`arbor-plugin-api`/`-types`).
- **C5** ✅ manifest `runtime` + `targets` confermati.
- **D1** ✅ aggiungi solo **`net`** (TCP); **`secrets` NON serve ai plugin** per ora.
- **D2** ✅ principio confermato (host tiene il segreto, il plugin non lo vede).
- **E1** ✅ cloud → **plugin WASM**.
- **E2** ✅ DB manager **fuori scope** (forse futuro, non integrato ora).
- **E3** ✅ studi → **crate + WASM**.

Restano aperte: A2-A8 (branding/loghi), B1-B4 (launcher/icone), C4-sede SDK già decisa, D-ordine `net`.

### A. Branding & loghi
- **A1** `[DECISO]` Famiglia: **Arbor** (piattaforma/launcher) · **Corvus** (git) · **Merula** (musica) · **Sitta** (file).
- **A2** Conferma rebranding user-facing git client Arbor → **Corvus**. *(rec: sì — M3)*
- **A3** Logo **launcher Arbor** (l'albero): nuovo o raffina l'esistente? *(rec: albero esistente raffinato — M0/M3)*
- **A4** Logo **Corvus**: eredita il vecchio logo Arbor o nuovo (corvo)? *(rec: nuovo corvo, coerente con la famiglia uccelli — M3)*
- **A5** Loghi **Merula** (merlo, M4) e **Sitta** (sitta, M5): nuovi. *(step in roadmap)*
- **A6** Linguaggio visivo della famiglia (stile uccelli, palette, griglia icona). *(rec: definire in M0)*
- **A7** Estensione DSL musica `.nemus`: resta o → `.merula`? *(rec: resta come sotto-brand)*
- **A8** Disponibilità su crates.io + trademark dei 4 nomi: **verificare** prima di committare.

### B. Architettura processo / distribuzione
- **B1** Launcher a processo condiviso (B.3): sì/no. *(rec: sì — M3/M12)*
- **B2** Routing launcher: `argv` (`--window=…`) vs deep-link (`arbor://open/…`). *(rec: deep-link, riusa infra — M12)*
- **B3** Bin separati per-prodotto (distribuzione indipendente): quali, se mai. *(rec: no default)*
- **B4** Identità taskbar separata (AUMID per-finestra): sì/no. Cross-platform solo con bundle separati (macOS no per-finestra). *(rec: no, basta `set_icon` — M12)*

### C. Plugin engine & WASM
- **C1** ⚠️ `wasmtime` come dipendenza: OK? versione/feature. *(bloccante — M0/M8)*
- **C2** ⚠️ Formato ABI: encoding custom di `PluginValue` vs postcard/bincode (nuova dep). *(bloccante — M8)*
- **C3** Engine multi-app (C.6): ora o dopo? *(rec: dopo M3-M5 — M6)*
- **C4** SDK autori WASM: `arbor-extensions` vs crate `arbor-plugin-wasm-sdk`. *(M8)*
- **C5** Manifest: campi `runtime` + `targets` — conferma lo shape (il manifest è contratto: non aggiungere campi senza ok). *(M6/M8)*

### D. Capability host (C.7)
- **D1** Quali capability generiche e in che ordine: `net` (TCP), `secrets` (keyring), resource-handle, stream, custom-view. *(M10)*
- **D2** Modello secrets: host tiene il segreto, il plugin non vede mai il valore raw (come token git). *(rec: sì)*

### E. Candidati di migrazione
- **E1** Cloud: plugin WASM con `host_http` vs subprocess nativo. *(M11)*
- **E2** DB manager: namespace host `arbor.db` (MVP) / plugin pieno / **standalone**. Se standalone → **4° prodotto** (nome-uccello + logo propri!). *(M11)*
- **E3** Studi: confermare crate + WASM (vs restare host-feature). *(rec: crate+WASM — M9)*

### F. Già decise (per chiarezza)
- **F1** `[DECISO]` git2→gix: **parcheggiato** (non in questa roadmap; serve solo per il browser, fuori scope).
- **F2** `[DECISO]` Nomi finali alla creazione (no doppio rename); piattaforma `arbor-*` non si rinomina.
- **F3** `[DECISO]` Un solo binario di default (RAM condivisa); bin separati solo per distribuzione.
- **F4** `[DECISO]` Token/segreti mai esposti ai plugin (host-import boundary), anche WASM.

---

## Riferimenti

- [`docs/crate-refactor.md`](crate-refactor.md) — piano round 1.
- [`docs/plugin-api-architecture.md`](plugin-api-architecture.md) — API plugin runtime-agnostic (la base del layer WASM).
- [`docs/plugin-core-architecture.md`](plugin-core-architecture.md) — runtime mlua (il modello da replicare per WASM).
- [`docs/roadmap.md`](roadmap.md) — blocchi "subprocess runtime" e "Studio plugins" (cugini della Parte C).
- [`CLAUDE.md`](../CLAUDE.md) — working agreement.
