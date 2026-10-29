//! Connecting to a game, and staying connected to it.
//!
//! `bennu_brp_connect` probes the endpoint (`rpc.discover`, then `registry.schema`), resolves the
//! method names that game really speaks, and starts the **monitor**. The monitor is what turns "the
//! game was restarted" from a stack of error banners into nothing at all, following the auto-reconnect
//! rules of the project:
//!
//! * retries are **silent** — the state event carries the attempt number for an inline banner;
//! * the *lost* toast fires **once per episode**, and only once the first retry has also failed
//!   (a game that comes back in a second never bothers anyone);
//! * the *regained* toast fires only if a *lost* one did;
//! * the *gave up* toast fires once, at the end of the attempts.
//!
//! One `disconnect_notified` flag, local to the episode, gates all three.

use std::time::Duration;

use arbor_brp::prelude::{methods, BrpCapabilities, BrpClient, MethodMap, Op, TypeRegistry, DEFAULT_ENDPOINT};
use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::{lock, reset_all, reset_world, touch, Conn, Link, Live, TOPIC_STATE};

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
/// How often a quiet connection is checked.
const PING_EVERY: Duration = Duration::from_secs(3);
/// ~8 minutes of trying (the backoff tops out at 8 s) before the monitor gives up.
const MAX_ATTEMPTS: u32 = 60;

// ── The wire ─────────────────────────────────────────────────────────────────

/// Args for [`bennu_brp_connect`].
#[derive(Deserialize)]
pub struct ConnectArgs {
    /// The game's BRP HTTP endpoint. Defaults to `http://127.0.0.1:15702`.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Per-request timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// What the connected game lets the inspector do — read from `rpc.discover`, never assumed from a
/// version number, so the UI offers exactly the verbs this game answers to.
#[derive(Debug, Default, Serialize)]
pub struct Can {
    pub watch: bool,
    pub spawn: bool,
    pub despawn: bool,
    pub insert: bool,
    pub remove: bool,
    pub mutate: bool,
    pub reparent: bool,
    pub resources: bool,
    pub mutate_resource: bool,
    pub insert_resource: bool,
    pub remove_resource: bool,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub connected: bool,
    pub state: Link,
    pub endpoint: Option<String>,
    /// `world` (Bevy 0.17+), `bevy` (0.15 / 0.16) or `unknown` — informational.
    pub family: &'static str,
    /// Method names came from the game's own `rpc.discover`.
    pub discovered: bool,
    pub method_count: usize,
    pub type_count: usize,
    pub component_types: usize,
    pub resource_types: usize,
    pub can: Can,
    /// Changes on every (re)connect. Events carry it so a stale frame can be told from a fresh one.
    pub generation: u64,
    /// Not loopback: mutate/spawn/despawn on a remote machine deserves a visible warning.
    pub non_loopback: bool,
}


fn status_of(conn: &Conn) -> Status {
    let Some(live) = conn.live.as_ref() else {
        return Status {
            connected: false,
            state: conn.link,
            endpoint: None,
            family: "unknown",
            discovered: false,
            method_count: 0,
            type_count: 0,
            component_types: 0,
            resource_types: 0,
            can: Can::default(),
            generation: conn.generation,
            non_loopback: false,
        };
    };
    let m = &live.methods;
    let (components, resources) = live.registry.count_by_kind();
    Status {
        connected: conn.link == Link::Connected,
        state: conn.link,
        endpoint: Some(live.endpoint.clone()),
        family: m.family(),
        discovered: m.was_discovered(),
        method_count: live.caps.methods.len(),
        type_count: live.registry.len(),
        component_types: components,
        resource_types: resources,
        can: Can {
            watch: m.supports(Op::GetComponentsWatch),
            spawn: m.supports(Op::Spawn),
            despawn: m.supports(Op::Despawn),
            insert: m.supports(Op::Insert),
            remove: m.supports(Op::Remove),
            mutate: m.supports(Op::Mutate),
            reparent: m.supports(Op::Reparent),
            resources: m.supports(Op::ListResources) && m.supports(Op::GetResource),
            mutate_resource: m.supports(Op::MutateResource),
            insert_resource: m.supports(Op::InsertResource),
            remove_resource: m.supports(Op::RemoveResource),
        },
        generation: conn.generation,
        non_loopback: !is_loopback(&live.endpoint),
    }
}

fn is_loopback(endpoint: &str) -> bool {
    let host = endpoint
        .split("://")
        .last()
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h);
    matches!(host.trim_matches(['[', ']']), "127.0.0.1" | "localhost" | "::1")
}

/// `localhost:15702` → `http://localhost:15702`. A bare host is what people paste.
fn normalise_endpoint(raw: Option<String>) -> String {
    match raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        None => DEFAULT_ENDPOINT.to_string(),
        Some(s) if s.contains("://") => s,
        Some(s) => format!("http://{s}"),
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Connect to a game, replacing any previous connection.
///
/// Fails with the transport's or the game's own message when the endpoint does not answer
/// `rpc.discover` — a game without `RemoteHttpPlugin` looks exactly like a closed port, which is the
/// first thing the UI's hint says.
#[arbor_rpc::handler]
async fn bennu_brp_connect(ctx: &BennuState, args: ConnectArgs) -> Result<Status, String> {
    let endpoint = normalise_endpoint(args.endpoint);
    let timeout = Duration::from_millis(args.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let live = establish(&endpoint, timeout).await?;

    let sink = ctx.event_sink();
    let status = {
        let mut conn = lock();
        reset_all(&mut conn);
        install(&mut conn, live);
        let generation = conn.generation;
        let handle = tokio::spawn(monitor(generation, Arc::clone(&sink)));
        conn.monitor = Some(handle.abort_handle());
        status_of(&conn)
    };
    emit_state(&sink, &status, None, 0, None);
    Ok(status)
}

/// Close the connection and stop everything running for it.
#[arbor_rpc::handler]
async fn bennu_brp_disconnect(ctx: &BennuState) -> Result<Status, String> {
    let status = {
        let mut conn = lock();
        reset_all(&mut conn);
        conn.live = None;
        conn.link = Link::Disconnected;
        status_of(&conn)
    };
    emit_state(&ctx.event_sink(), &status, None, 0, None);
    Ok(status)
}

/// Where the connection stands. Cheap — no round-trip to the game.
#[arbor_rpc::handler]
async fn bennu_brp_status(_ctx: &BennuState) -> Result<Status, String> {
    Ok(status_of(&lock()))
}

// ── Connecting ───────────────────────────────────────────────────────────────

/// Probe `endpoint` and build everything the inspector needs to talk to it.
async fn establish(endpoint: &str, timeout: Duration) -> Result<Live, String> {
    let client = BrpClient::new(endpoint.to_string(), timeout).map_err(|e| e.to_string())?;
    // Hard: a game that cannot even describe itself is not a BRP server.
    let discover = client
        .call(methods::RPC_DISCOVER, None)
        .await
        .map_err(super::err_string)?;
    let mut caps = BrpCapabilities::default();
    caps.ingest_discover(&discover);
    let method_map = MethodMap::resolve(&caps.methods);

    // Soft: without the registry the inspector still browses and edits raw JSON; it just has no
    // type list and no typed editors.
    let schema = match method_map.get(Op::Schema) {
        Some(m) => client.call(m, None).await.ok(),
        None => None,
    };
    // A registry is megabytes of JSON; indexing it must not run on a runtime worker.
    let (caps, registry) = tokio::task::spawn_blocking(move || {
        let registry = match schema {
            Some(s) => {
                caps.ingest_schema(&s);
                TypeRegistry::from_schema(&s)
            }
            None => TypeRegistry::default(),
        };
        (caps, registry)
    })
    .await
    .map_err(|e| format!("registry indexing failed: {e}"))?;

    use arbor_brp::prelude::preferred_type_paths;
    Ok(Live {
        endpoint: endpoint.to_string(),
        timeout,
        name_types: preferred_type_paths(&caps.name_types),
        parent_types: preferred_type_paths(&caps.parent_types),
        client: Arc::new(client),
        caps,
        methods: method_map,
        registry: Arc::new(registry),
    })
}

/// Make `live` the current connection. The caller holds the lock.
fn install(conn: &mut Conn, live: Live) {
    reset_world(conn);
    conn.live = Some(Arc::new(live));
    conn.link = Link::Connected;
    conn.last_ok = std::time::Instant::now();
}

// ── The monitor ──────────────────────────────────────────────────────────────

/// Why a monitor tick ended: still the same connection, or time to stop.
fn is_current(generation: u64) -> bool {
    let c = lock();
    c.generation == generation && c.live.is_some()
}

fn emit_state(sink: &Arc<dyn EventSink>, status: &Status, toast: Option<&str>, attempt: u32, message: Option<&str>) {
    sink.emit(
        TOPIC_STATE,
        json!({
            "status": status,
            "toast": toast,
            "attempt": attempt,
            "max_attempts": MAX_ATTEMPTS,
            "message": message,
        }),
    );
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.saturating_sub(1).min(3))
}

/// Watches one connection for the whole of its life. Exits when the generation moves on (a new
/// connect, a disconnect) or when it gives up.
async fn monitor(generation: u64, sink: Arc<dyn EventSink>) {
    loop {
        tokio::time::sleep(PING_EVERY).await;
        if !is_current(generation) {
            return;
        }
        let (live, quiet_for) = {
            let c = lock();
            (c.live.clone(), c.last_ok.elapsed())
        };
        let Some(live) = live else { return };
        // Real traffic already proves liveness; only a quiet connection needs a probe.
        if quiet_for < PING_EVERY || ping(&live).await {
            continue;
        }
        if !recover(generation, &live, &sink).await {
            return;
        }
    }
}

async fn ping(live: &Live) -> bool {
    let ok = live.client.call(methods::RPC_DISCOVER, None).await.is_ok();
    if ok {
        touch();
    }
    ok
}

/// The reconnect episode. `true` = back on a (new) connection, `false` = gave up or superseded.
async fn recover(generation: u64, lost: &Live, sink: &Arc<dyn EventSink>) -> bool {
    let mut disconnect_notified = false;
    set_link(generation, Link::Reconnecting);
    emit_current(sink, None, 0, None);

    for attempt in 1..=MAX_ATTEMPTS {
        tokio::time::sleep(backoff(attempt)).await;
        if !is_current(generation) {
            return false;
        }
        match establish(&lost.endpoint, lost.timeout).await {
            Ok(fresh) => {
                let status = {
                    let mut c = lock();
                    if c.generation != generation {
                        return false;
                    }
                    install(&mut c, fresh);
                    // `install` bumped the generation; this monitor now belongs to the new one.
                    status_of(&c)
                };
                let toast = disconnect_notified.then_some("regained");
                emit_state(sink, &status, toast, 0, None);
                // The loop in `monitor` keeps the OLD generation number; hand over by restarting.
                respawn(status.generation, sink);
                return false;
            }
            Err(e) => {
                // First retry failed: only now is it worth saying anything.
                let toast = (!disconnect_notified).then_some("lost");
                disconnect_notified = true;
                emit_current(sink, toast, attempt, Some(&e));
            }
        }
    }

    let status = {
        let mut c = lock();
        if c.generation != generation {
            return false;
        }
        reset_world(&mut c);
        c.live = None;
        c.link = Link::Disconnected;
        c.monitor = None;
        status_of(&c)
    };
    emit_state(sink, &status, Some("gave_up"), MAX_ATTEMPTS, None);
    false
}

/// After a successful reconnect `install` moved the connection to a new generation, so the monitor
/// that was watching the old one is done; start a fresh one for the new.
fn respawn(generation: u64, sink: &Arc<dyn EventSink>) {
    let handle = tokio::spawn(monitor(generation, Arc::clone(sink)));
    lock().monitor = Some(handle.abort_handle());
}

fn set_link(generation: u64, link: Link) {
    let mut c = lock();
    if c.generation == generation {
        c.link = link;
    }
}

fn emit_current(sink: &Arc<dyn EventSink>, toast: Option<&str>, attempt: u32, message: Option<&str>) {
    let status = status_of(&lock());
    emit_state(sink, &status, toast, attempt, message);
}
