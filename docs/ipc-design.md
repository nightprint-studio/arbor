# `arbor-ipc` — IPC design for Model D (1 FE + N BE)

Stato: **design (M1b)**. Lo scheletro compilabile (`crates/foundation/ipc`) implementa
oggi solo il livello transport-agnostic + un loopback in-process con ping; il
transport reale (named pipe / unix socket) e la codegen `tarpc` atterrano al
flip di M3 (in-process-first). Riferimenti: [`docs/migration-roadmap.md`](migration-roadmap.md)
(M1), [`docs/crate-refactor-round2.md`](crate-refactor-round2.md) (§"Modello di processo", §D.5).

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

- **In-process (oggi)**: `LoopbackBroker` — handler in-memory, zero serializzazione di rete. Sblocca lo spostamento dei 547 comandi dietro `arbor-ipc` **senza** un secondo processo (M3 incrementale).
- **IPC (domani)**: `PipeBroker` — named pipe (Windows) / unix socket (`0600` + `SO_PEERCRED`), `tarpc` sopra. Stessa interfaccia → il router non cambia.

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
| M1   | — (solo loopback + ping, scheletro) | `LoopbackBroker` (ping) | **ora** |
| M3 (a) | in-process | `LoopbackBroker` reale (547 comandi dietro arbor-ipc, un processo) | M3 incrementale |
| M3 (b) | named pipe / unix socket + tarpc | `PipeBroker` | flip a BE separato |

## Cosa c'è nello scheletro M1

- `error::IpcError`, `event::Event`, `client::BrokerClient` + `client::LoopbackBroker` (ping in-process), `Bytes` alias.
- **NON** ancora: dep `tarpc`/`tokio`, transport di rete, handshake reale, service per-prodotto. Sono il lavoro di M3.
