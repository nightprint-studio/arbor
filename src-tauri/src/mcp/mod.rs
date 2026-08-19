//! The MCP endpoint's lifecycle: bring it up when the user enables it, take it down
//! when they don't, and publish where it is.
//!
//! ## Why the launcher, and not a separate process
//!
//! MCP defines stdio and Streamable HTTP. A stdio server is *spawned by its client*, so
//! serving stdio from a running GUI app would mean a bridge process whose only job is to
//! relay into it. Over HTTP the client dials us instead, and the endpoint can live in
//! the process that already holds the state, the backends, and the user's consent.
//!
//! ## What is deliberately absent
//!
//! No server-initiated *requests* — no sampling, no elicitation. The two things that do
//! reach a client unasked are narrow and both earn their keep: progress on a `tools/call`
//! that asked for it, and `notifications/tools/list_changed` when the exposed set moves.
//! The second is what keeps a client that connected an hour ago from offering tools that
//! no longer exist — see [`reconcile`], which is where the set can move.
//!
//! No `Mcp-Session-Id`. Sessions are optional in the spec, and the state one would hold
//! — which project is open, which backend is up — lives in the launcher and outlives any
//! one client. Two lifetimes to keep in step, for nothing.

pub mod audit;
pub mod catalog;
pub mod consent;
pub mod policy;
pub mod resources;

use std::sync::{Arc, Mutex, OnceLock};

use arbor_http::prelude::Server;
use arbor_mcp::prelude::{ClientRecord, Guards, McpServer, ServerInfo};
use serde::Serialize;
use tauri::AppHandle;

use crate::mcp::catalog::ShellCatalog;
use crate::mcp::resources::ShellResources;

/// What the frontend needs to show the user, and to write their client's config line.
#[derive(Debug, Clone, Serialize, Default)]
pub struct McpStatus {
    pub running: bool,
    pub port: u16,
    /// The bearer token. Shown so the user can paste the `claude mcp add` line; it is a
    /// local loopback credential, not a secret worth hiding from the person who owns
    /// the machine.
    pub token: String,
    /// The endpoint URL.
    pub url: String,
    /// Why it is not running, when it isn't (port in use, disabled, …).
    pub detail: Option<String>,
}

/// The live server, if any.
struct Running {
    port: u16,
    token: String,
    shutdown: tokio::sync::watch::Sender<bool>,
    catalog: Arc<ShellCatalog>,
    /// Kept so the shell can tell connected clients the tool set moved. The same `Arc`
    /// the serve loop holds — a second one would notify a server nobody is talking to.
    server: Arc<McpServer<ShellCatalog>>,
}

static RUNNING: OnceLock<Mutex<Option<Running>>> = OnceLock::new();
static LAST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Running>> {
    RUNNING.get_or_init(|| Mutex::new(None))
}

fn error_slot() -> &'static Mutex<Option<String>> {
    LAST_ERROR.get_or_init(|| Mutex::new(None))
}

/// Bring the endpoint in line with the config: start it, stop it, or restart it on a
/// changed port. Safe to call repeatedly — it is what both boot and every settings save
/// go through.
pub fn reconcile(app: &AppHandle) {
    use tauri::Manager;

    let state = app.state::<crate::AppState>();
    let (enabled, port, mut token) = match state.lock_config() {
        Ok(cfg) => (cfg.mcp.enabled, cfg.mcp.port, cfg.mcp.token.clone()),
        Err(_) => (false, 0, String::new()),
    };

    // First run, or a token the user just regenerated away: mint one and keep it. It
    // has to survive restarts because it is written into the client's own config.
    if enabled && token.is_empty() {
        token = uuid::Uuid::new_v4().to_string();
        if let Ok(mut cfg) = state.lock_config() {
            cfg.mcp.token = token.clone();
            let snapshot = cfg.clone();
            drop(cfg);
            if let Err(e) = crate::config::app_config::save(&snapshot) {
                // The endpoint still comes up on this token; it just will not be the
                // same one after a restart. Better than refusing to start.
                tracing::warn!("mcp: could not persist the endpoint token — {e}");
            }
        }
    }

    // A tightened policy must bite now, not after a restart: a session grant made under
    // the old settings is exactly what a user tightening them wants revoked.
    consent::clear_session_grants();

    let already = slot().lock().ok().and_then(|s| {
        s.as_ref().map(|r| (r.port, r.token.clone(), r.catalog.clone(), r.server.clone()))
    });
    match (enabled, already) {
        (false, Some(_)) => stop(),
        (false, None) => {}
        // Same endpoint AND same credential: only the exposed set can have moved. A
        // changed token has to go through a restart — the running server holds the old
        // one, and leaving it up would mean a regenerated token that still admits the
        // old one until the app is closed.
        (true, Some((live_port, live_token, catalog, server)))
            if live_port == port && live_token == token =>
        {
            catalog.invalidate();
            // The set a client is holding is now wrong, and it has no way to find that out
            // by itself: it listed the tools when it connected. This is the only moment we
            // know it moved, so it is the only moment we can say so.
            server.notify_tools_changed();
        }
        (true, _) => {
            stop();
            start(app.clone(), port, token);
        }
    }
}

/// Throw the current token away. The next reconcile mints and persists a new one.
///
/// Deliberate rotation, which is the only kind worth having: every client configured
/// with the old token stops working and has to be re-registered, so this is the action
/// for "someone saw my screen", not something a restart should do behind your back.
pub fn regenerate_token(app: &AppHandle) {
    use tauri::Manager;

    let state = app.state::<crate::AppState>();
    if let Ok(mut cfg) = state.lock_config() {
        cfg.mcp.token.clear();
        let snapshot = cfg.clone();
        drop(cfg);
        let _ = crate::config::app_config::save(&snapshot);
    }
    reconcile(app);
}

/// Start the endpoint on `port`, admitting `token`.
fn start(app: AppHandle, port: u16, token: String) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let catalog = Arc::new(ShellCatalog::new(app.clone()));

    let server = McpServer::new(
        catalog.clone(),
        ServerInfo {
            name: "arbor".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            instructions: Some(INSTRUCTIONS.to_string()),
        },
        Guards::with_token(token.clone()),
    )
    // Read-only context, offered without a prompt: it reports *that* a project is
    // open, never what is in it. Reading a file stays a tool call, gated like one.
    .with_resources(Arc::new(ShellResources::new(app.clone())));

    let server = Arc::new(server);
    let server_for_slot = server.clone();
    let token_for_slot = token.clone();
    tauri::async_runtime::spawn(async move {
        // Bind before announcing: a port conflict must surface as "not running, here is
        // why" rather than as a server the user believes is up.
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let http = match Server::bind(addr).await {
            Ok(s) => s,
            Err(e) => {
                let detail = format!("could not bind 127.0.0.1:{port} — {e}");
                tracing::warn!("mcp: {detail}");
                if let Ok(mut slot) = error_slot().lock() {
                    *slot = Some(detail);
                }
                if let Ok(mut s) = slot().lock() {
                    *s = None;
                }
                return;
            }
        };
        tracing::info!("mcp: serving on http://127.0.0.1:{port}/mcp");
        server
            .serve(http, async move {
                // Resolves when reconcile/stop flips the flag.
                let _ = shutdown_rx.changed().await;
            })
            .await;
        tracing::info!("mcp: endpoint stopped");
    });

    if let Ok(mut s) = slot().lock() {
        *s = Some(Running {
            port,
            token: token_for_slot,
            shutdown: shutdown_tx,
            catalog,
            server: server_for_slot,
        });
    }
    if let Ok(mut e) = error_slot().lock() {
        *e = None;
    }
}

/// A backend just attached, so what it serves may not be what we cached.
///
/// The case this exists for is mundane and was invisible: a developer rebuilds a backend
/// and Arbor respawns it, and the shell goes on serving the descriptors it read from the
/// previous child — a client then offers tools that are gone and cannot see the ones that
/// arrived, with nothing anywhere saying so. Attaching is the only moment the shell knows
/// the inventory may have moved.
///
/// Silent unless the endpoint is up and the program actually contributes tools: a client
/// told to re-list because Corvus came up would be re-listing for nothing.
///
/// It DOES fire on a first attach, where nothing stale can exist yet, and that costs one
/// wasted `tools/list` per backend start. Deliberate: the alternative — notify only when
/// something was cached — goes quiet in the case that matters most, a client that listed
/// while a backend was down and would otherwise never learn its tools arrived. One extra
/// round trip against a client permanently missing a product is not a close call.
pub fn backend_attached(program: &str) {
    if !catalog::is_exposable(program) {
        return;
    }
    let live = slot().lock().ok().and_then(|s| {
        s.as_ref().map(|r| (r.catalog.clone(), r.server.clone()))
    });
    let Some((catalog, server)) = live else { return };
    catalog.invalidate();
    server.notify_tools_changed();
    tracing::debug!("mcp: {program} re-attached — tool list invalidated and clients told");
}

/// Take the endpoint down.
pub fn stop() {
    if let Ok(mut s) = slot().lock() {
        if let Some(running) = s.take() {
            let _ = running.shutdown.send(true);
        }
    }
}

/// Who has connected, and whether anything is listening now.
#[derive(Debug, Clone, Serialize, Default)]
pub struct McpClients {
    /// Everyone who has introduced themselves this run, first contact first.
    pub clients: Vec<ClientRecord>,
    /// Notification streams open right now. The only live presence a stateless transport
    /// can honestly report — see `ClientRecord`.
    pub open_streams: usize,
    /// Authenticated requests this run, and when the last one arrived (0 = never).
    ///
    /// Answers "is anything talking to this" without needing anyone to have said who they
    /// are — which, after a restart, is the only form the answer can take.
    pub requests: usize,
    pub last_request_ms: u64,
    /// False when the endpoint is down, so an empty list reads as "nothing is listening"
    /// rather than as "nobody has connected".
    pub running: bool,
}

/// Who is on the other end.
pub fn clients() -> McpClients {
    let live = slot().lock().ok().and_then(|s| s.as_ref().map(|r| r.server.clone()));
    match live {
        Some(server) => {
            let (requests, last_request_ms) = server.traffic();
            McpClients {
                clients: server.clients(),
                open_streams: server.open_streams(),
                requests,
                last_request_ms,
                running: true,
            }
        }
        None => McpClients::default(),
    }
}

/// What the settings panel shows.
pub fn status() -> McpStatus {
    let detail = error_slot().lock().ok().and_then(|e| e.clone());
    match slot().lock().ok().and_then(|s| s.as_ref().map(|r| (r.port, r.token.clone()))) {
        Some((port, token)) => McpStatus {
            running: true,
            port,
            url: format!("http://127.0.0.1:{port}/mcp"),
            token,
            detail: None,
        },
        None => McpStatus { running: false, detail, ..Default::default() },
    }
}

/// The `instructions` string handed to the model once, at connection time.
///
/// Worth its length: it says what this server *is*, which no individual tool
/// description can, and it front-loads the two facts that otherwise cost a wasted call
/// each — that a project must be opened first, and that the index warms asynchronously.
const INSTRUCTIONS: &str = "\
Arbor is a desktop developer workspace. These tools reach its running backends, so they \
see the same projects the user has open, with the same configuration.

Bennu is a code intelligence engine for Java and Rust projects, aimed at legacy \
enterprise stacks (Struts, JSP, Spring XML, MyBatis) where behaviour is spread across \
XML configuration rather than expressed in the code. Call bennu_project_summary first: \
it opens a project and reports its build model, frameworks and index state. Its semantic \
index builds in the background — when a lookup comes back empty and the index is not \
ready, that means 'not yet', not 'nothing there'.

Tyto captures the screen. Use it to see what the user sees when no other interface \
exposes it.

Calls may be refused: the user controls which products are reachable, which project \
paths are in scope, and whether actions that modify anything need their approval. A \
refusal explains itself — read it rather than retrying the same call.";
