# `arbor-ipc` — IPC design for Model D (1 FE + N BE)

Stato: **M3 Asse B — pilota in-process atterrato, in forma GENERICA, sweep in
corso**. Lo scheletro `arbor-ipc` (transport-agnostic + loopback) è ora *usato
sul serio*: **6 domini** (`stash` 11, `bisect` 11, `notes` 5, `reset`/tags 3,
`stats` 2, `reflog` 1 = **33 comandi**) instradano FE→shell→`Router`→`LoopbackBroker`→
registry→handler→JSON, tutto in-process, con la wire-string d'errore preservata.
La shell **non ridichiara nessuna firma per-comando**: un solo comando generico
`rpc(program, method, params)` inoltra; le firme vivono una volta sola sul
backend. Il transport reale (named pipe / unix socket) e la codegen `tarpc`
restano da fare al flip a BE separato. Riferimenti:
[`docs/migration-roadmap.md`](migration-roadmap.md) (M1/M3),
[`docs/crate-refactor-round2.md`](crate-refactor-round2.md) (§"Modello di processo", §D.5).

## Pilota in-process — forma generica (5 domini migrati)

La shell è un **router puro** (redirect, non ridefinizione). Decisione di design
(discussa con l'utente): meglio un seam stringly-typed generico che N firme
replicate — tanto il confine vero (FE TypeScript → BE) è validato a runtime
comunque, `tarpc` darebbe check a compile-time solo su un hop Rust↔Rust che il
design generico non ha.

- **UN comando Tauri generico** `rpc(program, method, params)`
  (`commands/rpc_commands.rs`) — `generate_handler!` passa da 11 voci stash a 1.
  La FE: `invoke("rpc", { program:"corvus", method:"stash.save", params })`.
- **`arbor-shell-common::Router`** in `AppState.router` (`OnceLock`, riempito in
  `setup()` perché cattura l'`AppHandle`); backend `"corvus"` = `LoopbackBroker`.
  `ipc/mod.rs::dispatch_rpc(state, program, method, json)` serializza→
  `Router::call`→deserializza, mappa `IpcError::Backend(s)`→`AppError::Other(s)`.
- **Registry, niente match, niente arg-struct**: il dispatch BE è il crate
  **`arbor-rpc`** (platform, `crates/foundation/rpc` + proc-macro
  `crates/foundation/rpc-macros`). Ogni dominio è un modulo `ipc/corvus/<dom>.rs`
  = **funzioni annotate `#[corvus::handler]`** e basta (il modulo `ipc/corvus`
  ri-esporta la macro generica sotto il suo nome → `#[corvus::handler]`,
  `#[merula::handler]`, …). Migrare un comando = spostare il corpo del vecchio
  `#[tauri::command]` in un handler e cancellare la registrazione in `lib.rs`.
  **Nome metodo opzionale**: di default = **il nome della funzione**
  (`stash_save`, `list_stashes`, … = i vecchi nomi comando); `#[corvus::handler("x.y")]`
  per override. La macro legge la firma, genera decode-args + serializza +
  `inventory::submit!` → auto-registrazione, zero liste centrali.
  `ipc/corvus/mod.rs::dispatch` prende `arbor_rpc::registry()` (cache `OnceLock`),
  passa il contesto **type-erased** `&dyn Any` (downcast a `&AppState` dentro
  l'handler) → `arbor-rpc` resta product-agnostic. Errori handler attraversano
  come stringa `Display` (la wire-string). FE: `corvus("stash_save", {…})` (la
  stringa = nome funzione = vecchio nome comando).
- **Contesto handler + threading (l'unblock dei domini grossi)**: l'handler riceve
  **solo `&AppState`** (passato type-erased come `&dyn Any`). `AppState` espone le
  due capability che servono ai comandi non-triviali:
  - **`state.emit(event, payload)`** — egress eventi Model-D-safe. Instrada attraverso
    **`CorvusState`** (crate `corvus-core`, vedi sotto) → un `arbor_ipc::EventSink`:
    in-process inoltra ad `AppHandle::emit`; al flip a `corvus-be` il sink wrappa il
    canale `arbor-ipc` e ogni emit diventa un `Event::Notify { topic, payload }` che lo
    shell ri-emette — **il call-site non cambia, cambia solo il backing**. È così che un
    handler emette senza prendere un `AppHandle` (gli ~84 comandi che lo facevano).
  - **`state.event_sink() -> Option<Arc<dyn EventSink>>`** — handle d'egress **clonabile**
    (`Send + 'static`) per i **thread di background** che sopravvivono al comando ed
    emettono da dentro: catturano questo invece di un `AppHandle` (più gli `Arc` dei
    registry che gli servono, es. `jobs`). È esattamente la forma di `corvus-be` (sink→
    canale, registry→stato del backend) — **niente `AppHandle` nei thread**. Nessuna
    escape-hatch `AppHandle` esposta: un handler riceve `{ &AppState, event_sink }` e
    basta; un eventuale bisogno Tauri-only futuro avrà una capability dedicata, non un
    handle catch-all.
  - **Threading**: il comando `rpc` è **`async` + un solo `spawn_blocking` centrale**.
    Ogni handler sync gira così sul **blocking pool** — off main thread (no freeze UI,
    hard-rule #9) e off runtime-worker — ereditando l'offload che ogni comando git
    pesante faceva da sé (diff/graph usavano `spawn_blocking` + reopen-by-path). Un
    handler che vuole restare concorrente fa **brief-lock `repos` → clona il path →
    rilascia → lavoro pesante su repo riaperto** (stessa forma del vecchio comando,
    meno il wrapper). I comandi **`async fn` senza `.await` reale** (offload
    fire-and-forget, es. `stats`) sono handler sync identici. Restano fuori dallo sweep
    finché non si estende il contesto: i comandi che **awaitano I/O vero** (HTTP
    provider/cloud) — servirebbe un dispatch async o un `block_on` nel blocking pool.
- **Scaffold `corvus-be` avviato — crate `corvus-core`**: il primo paletto del
  futuro backend headless. `corvus-core` (`crates/corvus/core`, Tauri-free, dipende
  solo da `arbor-ipc`+serde_json) ospita **`CorvusState`** = il seme dello stato che
  `corvus-be` possiederà. Oggi gira **in-process**: lo shell ne costruisce uno in
  `setup()` con un `EventSink` Tauri-backed (`ipc/event_sink.rs::TauriEventSink`) e
  `AppState` vi delega `emit`/`event_sink` (campo `AppState.corvus: OnceLock<CorvusState>`).
  Per ora tiene **solo** l'egress eventi; cresce campo-per-campo man mano che i domini
  git diventano transport-ready (prossimi: `JobRegistry` già Arc-shared, poi
  `RepoManager`+`crate::git`). Quando terrà abbastanza, gli handler passano da
  `&AppState` a `&CorvusState` e il crate + handler si spostano in `bins/corvus-be`,
  parlando con lo shell via `arbor-ipc`. **Il trait `EventSink` vive in `arbor-ipc`**
  (egress product-agnostic, accanto a `Event`): merula/sitta lo riuseranno.
- **FE**: helper generico `corvus(method, params)` (`src/lib/ipc/rpc.ts`); i
  wrapper tipati in `ipc/branch.ts` restano (DX) ma instradano via `corvus(…)` con
  chiavi **snake_case** nei `params`. Firme dei wrapper / comportamento invariati.
- **Inter-programma** (Corvus→Sitta): stesso `rpc`, caller un BE invece della FE;
  lo shell farà da tramite con `ensure_running` (spawn-if-absent + attesa ready
  single-flight + timeout) — no-op in-process, reale al flip a processi separati.
- **Boilerplate BE azzerata**: aggiungere un comando = scrivere **una funzione
  annotata**. Niente arg-struct, niente match, niente lista centrale. (`arbor-rpc`
  + `arbor-rpc-macros`, deps `inventory`/`syn`/`quote` — già nel lock.)

## Topologia

```
            ┌─────────────────────── shell process (bin `arbor`) ───────────────────────┐
            │  unica WebView2   │   Router   │   Credential broker (keyring + cache)      │
            └────────┬──────────┴─────┬──────┴────────────────────────────────────────────┘
   FE invoke/listen  │                │  arbor-ipc (2 canali)
        (Tauri)      │                │
                     ▼                ▼
         ┌────────────────┐   commands (req/resp, tarpc)   ┌──────────────────────────┐
         │  fe-shell +    │ ─────────────────────────────▶ │  corvus-be / merula-be / │
         │  fe-<prodotto> │ ◀───────────────────────────── │  sitta-be  (headless exe)│
         └────────────────┘   events (push, one-way)        └──────────────────────────┘
```

Lo **shell** possiede l'unica WebView e fa da **router** (FE `invoke` → BE) e da
**credential broker** (unico detentore del keyring). Ogni **prodotto** è un
backend **headless** (`*-be`, exe separato, no webview). Due canali fra shell e BE.

## I due canali (stile LSP)

### 1. Comandi — request/response, via `tarpc`

Ogni prodotto definisce il **proprio** service tipato. `tarpc` genera client +
server: **un comando ≈ una definizione**, niente boilerplate per-feature.

```rust
// corvus-ipc (esempio della shape — NON ancora nel codice; tarpc è parcheggiato)
#[tarpc::service]
pub trait CorvusRpc {
    async fn status(tab_id: String) -> Result<RepoStatus, RpcError>;
    async fn commit(tab_id: String, message: String, amend: bool) -> Result<Oid, RpcError>;
    // … un metodo per comando git
}
```

- **Codec**: serde binario (bincode/postcard) sul transport; JSON solo nel loopback di sviluppo.
- **`tarpc` NON fa streaming** by design: i risultati grossi usano cursori/handle (capability M10), gli eventi vanno sull'altro canale.
- Il client generato (`CorvusRpcClient`) gira **identico** su transport in-process e su pipe/socket — è il flip del [`BrokerClient`](#brokerclient-il-livello-transport-agnostic).

### 2. Eventi — push BE→shell, canale one-way dedicato

`tarpc` non streamma e gli eventi **non** vanno sul canale RPC. Canale separato,
messaggi serde **length-prefixed**, un solo `enum Event`:

```rust
pub enum Event {
    /// Evento generico instradabile: lo shell lo ri-emette alla FE come
    /// evento Tauri `topic` con `payload` (meccanismo emit/listen già esistente).
    Notify { topic: String, payload: serde_json::Value },
    /// Heartbeat di liveness (vedi auto-reconnect, CLAUDE.md).
    Ping,
}
```

- **Throttling/coalescing/backpressure** qui (come già per i meter Merula): il BE non deve poter inondare lo shell.
- **FE-facing**: lo shell riceve `Event::Notify` e fa `emit(topic, payload)` → la FE `listen`a. Nessun nuovo meccanismo lato FE.

## `BrokerClient`: il livello transport-agnostic

Il **router** dello shell non conosce il transport: parla a un `BrokerClient`.

```rust
pub trait BrokerClient: Send + Sync {
    /// Invoca `method` sul backend con un blob di parametri, restituendo il
    /// blob risultato. Il loopback usa JSON; il transport di produzione usa un
    /// codec binario — la shape del trait non cambia (è il "flip" del principio #6).
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError>;
}
```

- **In-process**: `LoopbackBroker` — handler in-memory, zero serializzazione di rete. Sblocca lo spostamento dei comandi dietro `arbor-ipc` **senza** un secondo processo (M3 incrementale).
- **Out-of-process (atterrato, Stage 1)**: `ChildClient` (`arbor-ipc::transport`) — un **processo `corvus-be` reale**, frame JSON length-prefixed sullo **stdin/stdout** del figlio (zero dep nuove, inerentemente parent-private). Implementa lo stesso `BrokerClient` → il router non cambia. La shell registra **un** backend `"corvus"` = `SplitBroker` che instrada i metodi annunciati da corvus-be (nel suo `Hello`) al processo, il resto al loopback; spostare un handler in corvus-be ne **flippa l'instradamento da solo**. Vedi [`corvus-be-bringup.md`](corvus-be-bringup.md).
- **Hardening (dopo)**: swap del byte-stream sotto `ChildClient` a named pipe (Windows) / unix socket (`0600` + `SO_PEERCRED`) + nonce/ACL; opzionale codec binario / `tarpc`. **Protocollo, router e handler invariati** — cambia solo il listener/connector.

Il `BrokerClient` è il livello "stringly" che il router usa per mappare un FE
`invoke` (nome comando + JSON) sul backend giusto; il **service tipato `tarpc`**
vive *sotto*, per-prodotto, e traduce method+blob ⇄ chiamate tipate.

## Handshake (bootstrap sicuro spawn parent→child)

Quando il transport diventa inter-processo (M3+):

1. Lo **shell** crea l'endpoint (pipe/socket con permessi `0600` su Unix).
2. Lo shell **spawna il BE** passando endpoint + un **nonce** monouso (via env/arg).
3. Il BE si connette e invia il **nonce** come primo messaggio; lo shell lo verifica e poi accetta RPC.
4. **ACL**: su Unix `SO_PEERCRED` verifica che il peer sia lo stesso utente; su Windows il named pipe è ristretto al SID dell'utente. Nonce + ACL = nessun terzo processo può agganciarsi.

Finché il transport è in-process (loopback) l'handshake è un no-op (stesso processo, nessun confine).

## Mappa sul flip in-process → IPC

| Fase | Transport | `BrokerClient` | Quando |
|------|-----------|----------------|--------|
| M1   | — (solo loopback + ping, scheletro) | `LoopbackBroker` (ping) | ✅ fatto |
| M3 (a) pilota | in-process | `LoopbackBroker` reale — **dominio `stash` (11 cmd)** via comando generico `rpc(program,method,params)` + registry, un processo | ✅ **fatto** |
| M3 (a) sweep | in-process | `LoopbackBroker` reale — **stash + bisect + notes + reset/tags + stats + reflog (33 cmd, 6 domini)**; restano gli altri domini | 🔄 in corso |
| M3 (b) seam | **processo `corvus-be` reale**, frame JSON su stdio | `ChildClient` + `SplitBroker` — ping/echo/emit out-of-process provati end-to-end | ✅ **fatto (Stage 1)** |
| M3 (b) bisect | stdio | **bisect (11 cmd) servito da corvus-be**: logica in `corvus-git`, repo path via registry `tab_id→path` su `CorvusState` (push shell on open/close), fallback in-process | ✅ **fatto (Stage 2a)** |
| M3 (b) stash | — | **logica stash estratta in `corvus-git`** (in-process via wrapper; recovery via callback, encoding via shim) | ✅ estratto (2b); OOP dopo `recovery` |
| M3 (b) reset / recovery | stdio | estrarre `recovery`, poi stash+reset serviti da corvus-be (+ hook shell-side) | ⏭️ prossimo |
| M3 (b) hardening | named pipe / unix socket + nonce/ACL (+ tarpc/bincode opz.) | swap del byte-stream sotto `ChildClient` | dopo |

## Cosa c'è nello scheletro M1

- `error::IpcError`, `event::Event`, `client::BrokerClient` + `client::LoopbackBroker` (ping in-process), `Bytes` alias.
- **NON** ancora: dep `tarpc`/`tokio`, transport di rete, handshake reale, service per-prodotto. Sono il lavoro di M3.
