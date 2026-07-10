# The reverse channel — backend→shell request/response (Model D)

How an out-of-process backend (`corvus-be`, …) calls **back** into the shell
mid-request and blocks on a reply — the missing transport direction that gates
the OOP split of the credential-coupled domains and the `arbor.ui.*` plugin
round-trips.

This document is the design that precedes the build. It is grounded in the
current transport (`crates/foundation/ipc/src/transport.rs`), the existing
`corvus-be` process (`crates/corvus/be/`), and the two design docs it serves
(`docs/credential-architecture.md`, `docs/plugin-relocation-inventory.md`).

## Why it exists, and why now

Today's transport is **asymmetric** (`transport.rs`):

| Direction | Shape | Frame |
|---|---|---|
| shell → backend | request/response (blocking `BrokerClient::call`) | `Request` / `Response` |
| backend → shell | one-way push only | `Event` |

A handler that runs **in the backend process** and needs something only the
shell can provide — a credential (the keyring lives shell-side), or a user's
answer to a plugin form — must originate a **request to the shell and block on
the reply**. That is request/response in the backend→shell direction, and it
**does not exist**.

It has exactly two consumers, both reentrant RPC in the same missing direction —
the channel is built once and serves both:

1. **Credential resolution — the driver.** `SessionProvider::session/refresh`
   from an OOP backend marshals to the shell's `VaultSessionProvider`. This gates
   the OOP split of MR/PR, security, issues, the git-provider layer, and CI (~60
   commands). See `docs/credential-architecture.md` §"reverse-channel requirement".
2. **`arbor.ui.*` plugin round-trips.** A plugin behind the seam that pops a
   form / confirm / settings panel and waits for the user's submission is the
   same shape. See `docs/plugin-relocation-inventory.md` §"critical missing channel".

In-process is **unaffected**: an in-process handler holds `&AppState` and reaches
the keyring / plugin host directly. The reverse channel matters **only** once a
domain actually moves into `corvus-be`.

## The reentrancy problem (the crux)

The shell side is already reentrancy-safe. `ChildClient` (`transport.rs:206-225`)
reads the child's stdout on a **dedicated reader thread**, demuxing `Response`
(wake the blocked `call`) and `Event` (re-emit). A caller blocked in
`ChildClient::call` waits on an `mpsc` `rx.recv()`; the reader thread runs
independently, so it can handle an inbound backend request **while** a forward
call is in flight. No change to the shell's threading is forced for reentrancy —
only the addition of "handle a backend-originated request" to that reader thread.

The **backend side is the problem.** `serve_stdio` (`transport.rs:114-134`) is a
**single-threaded** loop:

```rust
while let Some(frame) = read_frame(&mut reader)? {   // ← the ONLY reader
    if let Frame::Request { id, method, params } = frame {
        let result = dispatch(&method, params);       // ← blocks here
        write_frame(&mut *w, &Frame::Response { id, result })?;
    }
}
```

If `dispatch` tries to call the shell and block on the reply, the reply arrives
on stdin — but the only thing that reads stdin is this same loop, which is parked
**inside** `dispatch`. The reply is never read. **Deadlock.**

So the reverse channel forces a backend-side restructure: the stdin reader must
be **separate** from request dispatch, exactly as the shell's `ChildClient`
already is. The backend grows its own reader thread + pending map (mirroring
`ChildClient`), and dispatch runs off the reader thread so the reader stays free
to receive backend→shell responses.

## Frame additions

Two additive variants on the `Frame` enum (`transport.rs:36-53`). They are
symmetric to the existing forward pair, in the opposite direction:

```rust
enum Frame {
    Hello { methods: Vec<String> },
    Request  { id: u64, method: String, params: Value },   // shell → backend
    Response { id: u64, result: Result<Value, String> },   // backend → shell
    Event    { topic: String, payload: Value },            // backend → shell (push)

    // NEW — the reverse channel:
    HostRequest  { id: u64, method: String, params: Value }, // backend → shell
    HostResponse { id: u64, result: Result<Value, String> }, // shell → backend
}
```

`id` is a **separate id space** from `Request`/`Response` (each side mints its own
ids; correlation is per-direction). Wire-error mapping mirrors `Response`:
domain errors cross as the `Err(String)` of `result`; structured errors (if a
host method needs them) ride the `Ok` channel as data, exactly as elsewhere in
Model D.

## Backend side

### `HostCaller` — what handlers hold

A backend handler reaches the shell through an object-safe trait, the
request/response twin of `EventSink`:

```rust
/// Reentrant backend→shell request/response. The twin of `EventSink`: where
/// `EventSink::emit` is one-way fire-and-forget, `HostCaller::call` blocks on
/// the shell's reply. Object-safe so backend state holds an `Arc<dyn HostCaller>`.
pub trait HostCaller: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}
```

`CorvusState` (`crates/corvus/core/src/state.rs`) gains an
`Arc<dyn HostCaller>` field beside its `Arc<dyn EventSink>`, so any handler that
takes `&CorvusState` can call the shell — the call site is transport-agnostic
exactly like `emit`.

### The serve-loop restructure

`serve_stdio` splits its single loop into a reader + dispatch + a host-call
client, mirroring `ChildClient`:

- **A `FrameHostCaller`** (the `HostCaller` impl): owns the shared stdout writer
  + a `pending: HashMap<u64, mpsc::Sender<…>>` + a `next_id`. `call` writes a
  `HostRequest`, registers the pending sender, and blocks on `rx.recv()` —
  byte-for-byte the shape of `ChildClient::call`.
- **The reader thread** reads stdin and demuxes:
  - `Request { id, method, params }` → run `dispatch` **on a spawned worker
    thread**, which writes the `Response` when done (so the reader never blocks
    on a handler);
  - `HostResponse { id, result }` → wake the matching pending host-call;
  - other frames ignored.
- **Egress writer is shared + mutex-serialized** across `Response`, `Event`, and
  `HostRequest` (the existing `SharedWriter` already serializes `Event` +
  `Response`).

This is a chicken-and-egg at construction: `dispatch` captures `state`, `state`
holds the `HostCaller`, the `HostCaller` is created inside the transport. The
bring-up API therefore changes shape from "pass `dispatch`" to "transport hands
you a `HostCaller`, you build `state` + `dispatch` with it, then run the loop":

```rust
// today:
serve_stdio(out, methods, dispatch)?;

// proposed:
let host = FrameHostCaller::new(out.clone());          // exposes Arc<dyn HostCaller>
let state = CorvusState::new(sink, host.clone());      // state can now call the shell
let dispatch = move |m, p| registry.get(m)…(&state, p);
serve_stdio(out, methods, host, dispatch)?;            // reader + worker pool + host demux
```

### Concurrency note (intentional, consistent)

Dispatching each `Request` on its own worker thread makes backend handlers run
**concurrently**, where the old loop ran them **sequentially**. This is not a
regression — it matches the in-process model (the `LoopbackBroker` is already
called concurrently from Tauri's `spawn_blocking` pool), and the corvus handlers
are already thread-safe (state is `Mutex`-guarded). Sequential-only was an
artifact of the single-thread loop, not a guarantee. **Flag for confirmation:**
if any backend handler relies on serialization, it must guard it explicitly
(none do today).

## Shell side

The `ChildClient` reader thread (already on its own thread → reentrancy-safe)
gains one arm:

```rust
Ok(Some(Frame::HostRequest { id, method, params })) => {
    // dispatch to a shell-side host-handler, then reply
    let result = host_dispatch(&method, params);              // may itself block (keyring/OAuth)
    let mut w = writer.lock()…;
    write_frame(&mut *w, &Frame::HostResponse { id, result })?;
}
```

`host_dispatch` is a **shell-side host-handler registry** keyed by method name —
the mirror of the backend's method registry. It is where `__session` / `__refresh`
(credentials) and later the `arbor.ui.*` verbs live. Registration is shell-local;
the backend never sees the implementations, only the method names it may call.

> Threading caveat: a `HostRequest` that runs slow shell work (an OAuth refresh)
> blocks the `ChildClient` reader thread for its duration, stalling demux of
> `Response`/`Event` from that backend. If that becomes a problem, dispatch the
> host-request on a worker thread too (the reply is correlated by `id`, so
> out-of-order completion is fine). Start simple (inline); promote to a worker
> only if a slow host-call measurably stalls the channel.

### First host-handlers: `__session` / `__refresh`

```rust
// shell-side, registered in the host-handler registry
"__session" => {
    let account: String = parse(params)?;
    let s = vault.session(&account).await…;     // VaultSessionProvider (already exists)
    Ok(to_value(s))
}
"__refresh" => { … vault.refresh(&account).await … }
```

The shell already owns `VaultSessionProvider` (increment 1, `auth/vault.rs`).
The host-handlers are a thin async→blocking adapter over it.

## `ChildSessionProvider` — the first consumer

Backend-side, the `SessionProvider` the OOP credential domains hold:

```rust
pub struct ChildSessionProvider { host: Arc<dyn HostCaller> }

#[async_trait]
impl SessionProvider for ChildSessionProvider {
    async fn session(&self, account: &str) -> Result<AuthSession, CredentialError> {
        let v = self.host.call("__session", json!(account))
            .map_err(CredentialError::Store)?;
        Ok(from_value(v)?)
    }
    async fn refresh(&self, account: &str) -> Result<AuthSession, CredentialError> {
        let v = self.host.call("__refresh", json!(account))
            .map_err(CredentialError::Refresh)?;
        Ok(from_value(v)?)
    }
}
```

The backend holds an `Arc<dyn SessionProvider>` and **cannot tell** whether it's
`VaultSessionProvider` (in-process) or `ChildSessionProvider` (OOP) — the call
site never changes. That is the payoff of the trait being async + keyring-free.

`host.call` is sync (blocks on the reply); `SessionProvider` is async. Bridge
with a `spawn_blocking`/`block_in_place` shim, or make `HostCaller::call` return
a future backed by the pending `mpsc` (a oneshot the reader thread completes).
**Decision below.**

## Open decisions (confirm before build)

1. **Sync vs async `HostCaller`.** Sync `call` (blocking, mirrors `ChildClient`)
   is simplest and matches the existing transport. `ChildSessionProvider` then
   bridges sync→async via `spawn_blocking`. Alternative: an async `HostCaller`
   whose `call` awaits a oneshot the reader completes — cleaner for async
   callers, but the backend's runtime story (corvus-be has no tokio runtime in
   the serve loop today) needs settling. **Recommendation: sync `HostCaller`,
   bridge at the `ChildSessionProvider` boundary.**
2. **Host-request dispatch threading (shell).** Inline on the reader thread vs a
   worker. **Recommendation: inline first**, promote to worker only if a slow
   host-call stalls the channel (caveat above).
3. **Backend worker model.** Thread-per-request (simple, unbounded) vs a bounded
   pool. **Recommendation: thread-per-request to start** (corvus-be load is low;
   matches "spawn a thread per inbound call"), revisit if it matters.

## Phased implementation plan

Each phase builds + is verifiable on its own; the OOP path can't be
runtime-exercised until a credential domain is actually served by `corvus-be`,
so the mechanism is proven by an **in-memory-duplex unit test** first.

1. **Transport mechanism.** Add the `HostRequest`/`HostResponse` frames; the
   backend reader/worker/pending restructure; `FrameHostCaller`; the shell
   `HostRequest` arm + a host-handler registry hook. **Unit test over an
   in-memory duplex** (a `Read+Write` pair backed by `std::sync::mpsc`, no new
   deps): a backend handler calls `host.call("echo", x)`, the shell host-handler
   answers, the handler returns it through the normal `Response` — proving the
   reentrant round-trip end-to-end without real processes.
2. **`HostCaller` on `CorvusState`** + the bring-up API change in
   `crates/corvus/be/src/main.rs`. Existing bisect/stash handlers untouched
   (they don't call back). Build green; `corvus-be` still serves the same set.
3. **`__session`/`__refresh` host-handlers** (shell) over `VaultSessionProvider`
   + `ChildSessionProvider` (backend). Still inert until a credential domain
   moves OOP — but now the bridge exists.
4. **Move one credential domain into `corvus-be`** (smallest first — likely
   `issues`, already trait-clean via `SessionProvider`) and let `SplitBroker`
   route it. This is the first **runtime** exercise of the reverse channel; the
   user validates with a real Linear/Jira token. Then the rest follow.
5. **`arbor.ui.*` as the second consumer** — the plugin host's form/confirm/
   settings round-trips over the same channel, once the plugin host relocates
   (`docs/plugin-relocation-inventory.md`).

## Testing

- **In-memory duplex unit test** (phase 1) — the load-bearing test; proves
  reentrancy without processes.
- **`corvus-be` smoke** (phase 4) — the existing bring-up harness
  (`docs/corvus-be-bringup.md`) plus a credential round-trip against a live token.
- The shell→backend forward path and the `EventSink` egress already have their
  proofs (bisect/stash run OOP today); the reverse channel reuses the same
  framing/demux shapes, so the risk is concentrated in the backend restructure —
  hence the dedicated unit test.
