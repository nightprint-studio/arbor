//! The language-server registry: which server serves which file, and its lifecycle.
//!
//! One process-wide registry (like [`crate::index_service`]), keyed by **workspace root +
//! language**. That key is the design decision worth stating: in a Cargo workspace every
//! member crate has its own `Cargo.toml`, and one server per member would mean four
//! rust-analyzers over the same code, each blind to the others' crates — and cross-crate
//! go-to broken in all of them. So the root is the *highest* manifest above the file, and one
//! server covers the whole graph.
//!
//! ## Starting is asynchronous, and the API admits it
//!
//! rust-analyzer takes tens of seconds to index a cold project. [`ensure`](LspRegistry::ensure)
//! therefore never blocks: it starts the server on a background thread and returns `None`
//! until the handshake is done. Every routed handler treats `None` as "the native path, or
//! nothing yet" — which is the same graceful degradation the Java index already has while it
//! builds, so the frontend needs no new state to handle it.
//!
//! Status is what makes that honest rather than mysterious: a slot exists from the moment the
//! start begins, so the UI can say *starting*, *indexing 43%*, or *failed, here is the
//! stderr*.
//!
//! ## Failures are sticky
//!
//! A slot that failed to start stays failed until the user restarts it. Retrying on every
//! keystroke would spawn a process per completion request against a server that is not
//! installed — and the honest answer ("rust-analyzer was not found, install it with …") is
//! one the user has to see once, not have hidden by an automatic retry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::{load_config, CustomLspServer, LspConfig};
use bennu_lsp::prelude::{
    background_init_options, find_root, is_dependency_source, locate, locate_custom, spec_by_id,
    FileEdit, FileOp,
    LspSession, ServerAvailability,
    ServerSpec, SessionConfig, SessionObserver, SessionState, BUILTIN_SERVERS,
};

/// How often the reaper looks for idle background sessions.
///
/// Coarse on purpose: it decides *when* a session that is already going to be stopped is stopped,
/// so a minute of slack costs a minute of memory and saves fifty-nine wake-ups.
const REAP_INTERVAL: Duration = Duration::from_secs(60);

/// Frontend event: a server's state / progress changed.
pub const EVT_STATUS: &str = "arbor://bennu/lsp-status";
/// Frontend event: the server published diagnostics for a file.
pub const EVT_DIAGNOSTICS: &str = "arbor://bennu/lsp-diagnostics";
/// Frontend event: the server wants edits applied (a lazily-computed code action).
pub const EVT_APPLY_EDIT: &str = "arbor://bennu/lsp-apply-edit";
/// Frontend event: the server asked to show a message.
pub const EVT_MESSAGE: &str = "arbor://bennu/lsp-message";

/// A slot key: `(workspace root, language)`.
type SlotKey = (String, String);

/// Why a server is running — which is the same question as whether it may be stopped.
///
/// The distinction is not bookkeeping: a session with a window on it costs a rebuild the moment it
/// is taken away, and one with nothing on screen costs a gigabyte for as long as it is kept. They
/// are opposite trades and the registry cannot make either one without knowing which it is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOrigin {
    /// A window has this project open. Never reclaimed.
    Window,
    /// Started to answer a request with nothing on screen behind it — an AI client, in practice.
    /// Runs on the lean profile and is stopped once it goes quiet.
    Background,
}

/// What the language server for a file can say **right now**.
///
/// Five states because they are five different answers to "nothing came back", and collapsing them
/// is how an empty result quietly becomes a claim. [`ServerReadiness::Warming`] is the one that
/// earns the type: a server whose handshake is done but whose workspace is still loading answers
/// "no references" confidently and wrongly, and for find-usages that is the difference between
/// *unused* and *not yet* — which is the difference between deleting a method and not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReadiness {
    /// No server serves this file. Bennu's own engines are the whole answer.
    Absent,
    /// One serves it, and nothing has started it — nobody has asked it anything yet.
    Idle { name: String },
    /// It cannot answer yet. `detail` is the server's own progress line when it has one
    /// (`Indexing 43%`), empty while the handshake is still going.
    Warming { name: String, detail: String },
    /// It is answering.
    Ready { name: String },
    /// It cannot answer, and will not without something changing — not installed, or it died.
    Failed { name: String, message: String },
}

/// What is known about a slot besides its server: who wanted it, and when it was last any use.
struct Lease {
    origin: SessionOrigin,
    last_used: Instant,
}

/// What is known about a slot regardless of whether its server came up.
#[derive(Clone)]
struct SlotInfo {
    id: String,
    name: String,
    language: String,
    root: String,
    command: String,
}

/// One server slot.
enum Slot {
    /// Spawned; the handshake has not finished.
    Starting(SlotInfo),
    Ready(Arc<LspSession>),
    /// It could not be started. Sticky — see the module docs.
    Failed { info: SlotInfo, message: String, log_tail: Vec<String> },
}

/// A server description resolved against the config — a catalogue entry or a user-defined
/// one, flattened so the rest of the module does not care which it was.
#[derive(Clone)]
struct ResolvedServer {
    id: String,
    name: String,
    language: String,
    command: String,
    args: Vec<String>,
    extensions: Vec<String>,
    root_markers: Vec<String>,
    init_options: Option<serde_json::Value>,
    install_hint: String,
    custom: bool,
}

impl ResolvedServer {
    /// `check_command` is what rust-analyzer runs on save. Threaded in rather than read from a
    /// file inside `bennu-lsp`, which is a leaf that must not know where Bennu keeps its settings.
    fn from_spec(spec: &ServerSpec, check_command: &str) -> Self {
        Self {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            language: spec.language.to_string(),
            command: spec.cmd.to_string(),
            args: spec.args.iter().map(|s| s.to_string()).collect(),
            extensions: spec.extensions.iter().map(|s| s.to_string()).collect(),
            root_markers: spec.root_markers.iter().map(|s| s.to_string()).collect(),
            init_options: spec.init_options(check_command),
            install_hint: spec.install_hint.to_string(),
            custom: false,
        }
    }

    fn from_custom(c: &CustomLspServer) -> Option<Self> {
        // An entry that cannot select a file, or has nothing to run, is not a server. Dropped
        // rather than half-registered: a slot that can never start would show as permanently
        // failed in the settings list for no reason the user could act on.
        if c.id.trim().is_empty() || c.command.trim().is_empty() {
            return None;
        }
        if c.extensions.is_empty() || c.root_markers.is_empty() {
            return None;
        }
        let init_options = parse_init_options(&c.id, &c.initialization_options);
        Some(Self {
            id: c.id.clone(),
            name: if c.name.is_empty() { c.id.clone() } else { c.name.clone() },
            language: if c.language.is_empty() { c.id.clone() } else { c.language.clone() },
            command: c.command.clone(),
            args: c.args.clone(),
            extensions: c.extensions.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect(),
            root_markers: c.root_markers.clone(),
            init_options,
            install_hint: String::new(),
            custom: true,
        })
    }

    /// Whether this server serves `file`, by extension.
    fn serves(&self, file: &str) -> bool {
        let ext = bennu_lsp::prelude::extension_of(file);
        !ext.is_empty() && self.extensions.iter().any(|e| *e == ext)
    }

    fn markers(&self) -> Vec<&str> {
        self.root_markers.iter().map(String::as_str).collect()
    }
}

/// The base options with the background profile laid over them.
///
/// A shallow merge at the top level, which is exact here rather than approximate: every key the
/// profile sets is one the base does not have (see `background_init_options`), so nothing is
/// overwritten and no nested object is half-replaced. A deep merge would be more general and would
/// hide the day that stops being true.
fn lean(
    base: Option<serde_json::Value>,
    profile: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let Some(profile) = profile else { return base };
    let Some(mut base) = base else { return Some(profile) };
    match (base.as_object_mut(), profile.as_object()) {
        (Some(target), Some(extra)) => {
            for (k, v) in extra {
                target.insert(k.clone(), v.clone());
            }
            Some(base)
        }
        // A non-object on either side is a server whose options someone wrote by hand. Left alone:
        // the profile is an optimisation and must never be the reason a session starts wrong.
        _ => Some(base),
    }
}

/// The user's `initialization_options` JSON string, parsed.
///
/// Ignored (with a log line) rather than fatal when it does not parse: a typo in one server's
/// options must not take down the config, and the server still starts — with its own
/// defaults, which is the state the user was in before they typed it.
fn parse_init_options(id: &str, raw: &str) -> Option<serde_json::Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match serde_json::from_str(raw) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[lsp] ignoring invalid initialization_options for `{id}`: {e}");
            None
        }
    }
}

/// The registry.
pub struct LspRegistry {
    slots: Mutex<HashMap<SlotKey, Slot>>,
    /// Why each slot exists and when it was last used.
    ///
    /// Beside `slots` rather than inside it because the two are read at different moments — every
    /// routed request touches a lease, almost none of them look at a slot — and because the three
    /// `Slot` variants would each have to carry it. A lease is created with its slot and dropped
    /// with it; nothing else ever inserts one, which is what keeps the two maps agreeing. They are
    /// never locked at the same time.
    leases: Mutex<HashMap<SlotKey, Lease>>,
    /// Whether the reaper thread is up. Started on first use rather than at boot: a backend serving
    /// a project with no language server should not have a thread waking up forever for nothing.
    reaping: AtomicBool,
    /// The latest event sink, so a background start can report its own outcome. One window per
    /// backend, so the newest is the right one.
    sink: RwLock<Option<Arc<dyn EventSink>>>,
}

static REGISTRY: OnceLock<LspRegistry> = OnceLock::new();

impl LspRegistry {
    /// The global registry.
    pub fn global() -> &'static LspRegistry {
        REGISTRY.get_or_init(|| LspRegistry {
            slots: Mutex::new(HashMap::new()),
            leases: Mutex::new(HashMap::new()),
            reaping: AtomicBool::new(false),
            sink: RwLock::new(None),
        })
    }

    /// Remember the sink a handler brought in.
    pub fn set_sink(&self, sink: Arc<dyn EventSink>) {
        *self.sink.write().unwrap_or_else(|p| p.into_inner()) = Some(sink);
    }

    fn sink(&self) -> Option<Arc<dyn EventSink>> {
        self.sink.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The ready session serving `file`, starting one in the background if none exists.
    ///
    /// `None` while a server is starting, when it failed, when nothing in the catalogue serves
    /// the extension, when there is no workspace root above the file, or when the binary is not
    /// installed. Every one of those is a "the native path or nothing" answer for the caller —
    /// never an error, because a missing language server is not a broken editor.
    pub fn ensure(&'static self, file: &str) -> Option<Arc<LspSession>> {
        let cfg = load_config();
        if !cfg.lsp.enabled {
            return None;
        }
        let server = self.server_for(file, &cfg.lsp)?;
        let root = match self.root_of(file, &server) {
            RootKind::Own(root) => root,
            // A dependency's own source. Borrowed from a live session, never started for: see
            // `root_of`.
            RootKind::Borrowed => return self.sole_session(&server.language),
            RootKind::None => return None,
        };
        let key = slot_key(&root.to_string_lossy(), &server.language);

        match self.peek(&key) {
            Existing::Ready(session) if session.is_alive() => {
                // The one choke point every routed request passes through, which is what makes
                // "idle" mean anything: a session answering questions is never idle, whoever is
                // asking them.
                self.touch(&key);
                return Some(session);
            }
            Existing::Ready(session) => {
                // The process died under us. Recorded as failed rather than silently
                // restarted: a server that crashes on this project would otherwise be
                // respawned forever and the user would only see the CPU.
                self.record_exit(&key, &server, &session);
                return None;
            }
            Existing::Pending => return None,
            Existing::Absent => {}
        }
        // Created as background, deliberately. A project a window opened has already been through
        // `warm_start` and so has a slot; reaching here means a request arrived for a project
        // nothing has open, which is exactly the session that must not outlive the asking. If a
        // window opens it later, `warm_start` claims it — see [`Self::claim`].
        self.begin_start(key, server, root, &cfg.lsp.server_paths, SessionOrigin::Background);
        None
    }

    /// Mark a slot as used now.
    fn touch(&self, key: &SlotKey) {
        if let Some(lease) = self.leases.lock().unwrap_or_else(|p| p.into_inner()).get_mut(key) {
            lease.last_used = Instant::now();
        }
    }

    /// Record who wants a slot, promoting it if a window now does.
    ///
    /// One-way on purpose: a project opened in a window and then closed keeps its session. Closing
    /// a tab is not a statement about memory, and demoting there would make the session most likely
    /// to be wanted again the one most likely to be taken away.
    fn claim(&self, key: &SlotKey, origin: SessionOrigin) {
        let mut leases = self.leases.lock().unwrap_or_else(|p| p.into_inner());
        let lease = leases.entry(key.clone()).or_insert(Lease { origin, last_used: Instant::now() });
        lease.last_used = Instant::now();
        if matches!(origin, SessionOrigin::Window) {
            lease.origin = SessionOrigin::Window;
        }
    }

    /// Whether a slot runs on the lean profile.
    fn is_background(&self, key: &SlotKey) -> bool {
        self.leases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .is_some_and(|l| matches!(l.origin, SessionOrigin::Background))
    }

    /// Which root a file belongs to, and whether a server may be **started** for it.
    ///
    /// The distinction the enum exists for: `find_root` answers "the highest manifest above this
    /// file", and for a file inside an unpacked dependency that is the *dependency's* manifest. Left
    /// alone, a go-to-definition into a library resolved that library's directory as a workspace and
    /// spawned a second language server for it — one per dependency you looked into, each indexing a
    /// crate from scratch, none of them able to say anything about your own code.
    ///
    /// A dependency's source is not a workspace. The session that already has it open is your
    /// project's, whose crate graph includes it by definition — so the file *borrows* that session
    /// instead of getting one of its own.
    fn root_of(&self, file: &str, server: &ResolvedServer) -> RootKind {
        let path = Path::new(file);
        if is_dependency_source(path) {
            return RootKind::Borrowed;
        }
        match find_root(path, &server.markers()) {
            Some(root) => RootKind::Own(root),
            None => RootKind::None,
        }
    }

    /// The single live session for `language`, or `None` when there is not exactly one.
    ///
    /// What a borrowed file gets. With one project open this is exactly right — its server has every
    /// dependency's source in its VFS. With several it declines rather than choosing: picking the
    /// wrong one is harmless in itself (a server asked about a file outside its crate graph answers
    /// nothing, not wrongly) but it would make go-to inside a shared dependency depend on which
    /// project happened to be enumerated first, and an intermittently-working feature is worse than
    /// one that is honestly absent.
    fn sole_session(&self, language: &str) -> Option<Arc<LspSession>> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let mut found: Option<Arc<LspSession>> = None;
        for ((_root, lang), slot) in slots.iter() {
            if lang != language {
                continue;
            }
            let Slot::Ready(session) = slot else { continue };
            if !session.is_alive() {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(Arc::clone(session));
        }
        found
    }

    /// A snapshot of a slot, taken and released so the caller can act without holding the lock.
    fn peek(&self, key: &SlotKey) -> Existing {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        match slots.get(key) {
            Some(Slot::Ready(s)) => Existing::Ready(Arc::clone(s)),
            Some(Slot::Starting(_)) | Some(Slot::Failed { .. }) => Existing::Pending,
            None => Existing::Absent,
        }
    }

    /// Claim a slot and start its server, or record that its binary is missing.
    ///
    /// The one place a slot is created, which is what makes the claim race-free: the
    /// occupancy check and the insert happen under the same lock, so two requests arriving
    /// together cannot both spawn a server for the same root.
    fn begin_start(
        &'static self,
        key: SlotKey,
        server: ResolvedServer,
        root: PathBuf,
        overrides: &std::collections::BTreeMap<String, String>,
        origin: SessionOrigin,
    ) {
        let command = locate_for(&server, overrides);
        let info = SlotInfo {
            id: server.id.clone(),
            name: server.name.clone(),
            language: server.language.clone(),
            root: key.0.clone(),
            command: command.clone().unwrap_or_else(|| server.command.clone()),
        };
        let slot = match &command {
            Some(_) => Slot::Starting(info),
            // Not installed. Recorded rather than ignored, so the panel can say so *with the
            // install hint* — a project that silently has no intelligence explains nothing.
            None => Slot::Failed {
                info,
                message: not_found_message(&server),
                log_tail: Vec::new(),
            },
        };
        {
            let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            if slots.contains_key(&key) {
                return; // another thread claimed it first
            }
            slots.insert(key.clone(), slot);
        }
        // After the slot is claimed and outside its lock: the lease is only meaningful for a slot
        // that exists, and the early return above is the case where it does not.
        self.claim(&key, origin);
        self.start_reaper();
        self.emit_status();
        if let Some(command) = command {
            self.spawn_start(key, server, command, root, origin);
        }
    }

    /// Record that a ready session's process is gone.
    fn record_exit(&self, key: &SlotKey, server: &ResolvedServer, session: &LspSession) {
        let info = SlotInfo {
            id: server.id.clone(),
            name: server.name.clone(),
            language: server.language.clone(),
            root: key.0.clone(),
            command: session.config().command.clone(),
        };
        let log_tail = session.status().log_tail;
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(
            key.clone(),
            Slot::Failed { info, message: "the language server exited".to_string(), log_tail },
        );
        self.emit_status();
    }

    /// Start the server for a claimed slot, off the calling thread.
    fn spawn_start(
        &'static self,
        key: SlotKey,
        server: ResolvedServer,
        command: String,
        root: PathBuf,
        origin: SessionOrigin,
    ) {
        let init_options = match origin {
            SessionOrigin::Window => server.init_options.clone(),
            SessionOrigin::Background => {
                lean(server.init_options.clone(), background_init_options(&server.id))
            }
        };
        let cfg = SessionConfig {
            id: server.id.clone(),
            name: server.name.clone(),
            language: server.language.clone(),
            command,
            args: server.args.clone(),
            root,
            init_options,
            env: Vec::new(),
        };
        let observer: Arc<dyn SessionObserver> =
            Arc::new(RegistryObserver { registry: self, key: key.clone() });
        // Taken by value so the closure does not need `server`, which stays available for the
        // thread name and the spawn-failure message below.
        let install_hint = server.install_hint.clone();

        let spawned = std::thread::Builder::new()
            .name(format!("bennu-lsp-start-{}", server.id))
            .spawn(move || {
                let registry = LspRegistry::global();
                match LspSession::start(cfg.clone(), observer) {
                    Ok(session) => {
                        registry
                            .slots
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(key, Slot::Ready(session));
                    }
                    Err(failure) => {
                        let info = SlotInfo {
                            id: cfg.id,
                            name: cfg.name,
                            language: cfg.language,
                            root: cfg.root.to_string_lossy().replace('\\', "/"),
                            command: cfg.command,
                        };
                        // The install hint rides along even though the binary *was* found. The
                        // case this covers is real and otherwise baffling:
                        // `~/.cargo/bin/rust-analyzer` is a rustup proxy that exists whether or
                        // not the component is installed, so the spawn succeeds and the process
                        // then dies with "Unknown binary". The reason and the remedy belong in
                        // the same sentence.
                        let message = if install_hint.is_empty() {
                            failure.message
                        } else {
                            format!("{} — {}", failure.message, install_hint)
                        };
                        registry.slots.lock().unwrap_or_else(|p| p.into_inner()).insert(
                            key,
                            Slot::Failed { info, message, log_tail: failure.log_tail },
                        );
                    }
                }
                registry.emit_status();
            });
        if spawned.is_err() {
            eprintln!("[lsp] could not spawn the start thread for {}", server.id);
        }
    }

    /// Warm-start every server whose root marker sits **at** `root`.
    ///
    /// Called on project open. Without it the first `.rs` file the user opens starts
    /// rust-analyzer and then answers nothing for half a minute while it indexes — which reads
    /// as "Bennu has no Rust support" rather than as "the server is warming up". Starting at
    /// open moves that wait to where the user is not yet asking a question.
    pub fn warm_start(&'static self, root: &str, origin: SessionOrigin) {
        let cfg = load_config();
        if !cfg.lsp.enabled {
            return;
        }
        let root_path = Path::new(root);
        for server in self.all_servers(&cfg.lsp) {
            // The marker has to be **at** this root, not merely above it: a project opened
            // inside a Cargo workspace member should start the server for the workspace, and
            // that is the root the FE opened, so anything further up is somebody else's
            // project.
            if !server.root_markers.iter().any(|m| root_path.join(m).is_file()) {
                continue;
            }
            let key = slot_key(root, &server.language);
            if !matches!(self.peek(&key), Existing::Absent) {
                // Already running. Still worth saying who wants it now: a project an AI client
                // summarised half an hour ago and the user has just opened in a tab must stop
                // being reclaimable, and this is the moment that becomes true.
                self.claim(&key, origin);
                continue;
            }
            self.begin_start(key, server, root_path.to_path_buf(), &cfg.lsp.server_paths, origin);
        }
        self.prune_dependency_roots();
    }

    /// Stop background sessions that have gone quiet, forever, on their own thread.
    ///
    /// A thread rather than a check folded into `ensure`: the whole point is to reclaim a session
    /// that nothing is asking about, and a session nothing is asking about produces no calls to
    /// hang a check on. Started on the first slot and never stopped — one sleeping thread per
    /// backend, against up to a gigabyte apiece.
    fn start_reaper(&'static self) {
        if self.reaping.swap(true, Ordering::SeqCst) {
            return;
        }
        let spawned = std::thread::Builder::new().name("bennu-lsp-reaper".to_string()).spawn(
            move || loop {
                std::thread::sleep(REAP_INTERVAL);
                LspRegistry::global().reap_idle();
            },
        );
        if spawned.is_err() {
            // Not fatal, and not silent: everything still works, background sessions simply live
            // as long as the backend — which is exactly the state this was added to leave.
            eprintln!("[lsp] could not spawn the reaper thread; background servers will not be stopped");
            self.reaping.store(false, Ordering::SeqCst);
        }
    }

    /// Stop every background session idle beyond the configured timeout.
    ///
    /// Reads the timeout on each pass rather than caching it, so changing the setting takes effect
    /// without a restart. `0` disables reclamation entirely, which is a supported answer: a machine
    /// with the memory to spare should not pay a cold rebuild for tidiness.
    fn reap_idle(&self) {
        let timeout = load_config().lsp.background_idle_timeout_secs;
        if timeout == 0 {
            return;
        }
        let timeout = Duration::from_secs(timeout);
        let expired: Vec<SlotKey> = {
            let leases = self.leases.lock().unwrap_or_else(|p| p.into_inner());
            leases
                .iter()
                .filter(|(_, l)| matches!(l.origin, SessionOrigin::Background))
                .filter(|(_, l)| l.last_used.elapsed() >= timeout)
                .map(|(k, _)| k.clone())
                .collect()
        };
        for (root, language) in expired {
            eprintln!("[lsp] stopping idle background server for {root} ({language})");
            self.stop(&root, &language);
        }
    }

    /// Shut down any session whose root is a dependency's own source.
    ///
    /// Repair, not policy: such a session can no longer be *created* (see [`Self::root_of`]), so the
    /// only way to have one is to have started it before that was fixed — and it is a whole language
    /// server indexing a library, holding memory, answering nothing about your code. Leaving it for
    /// the user to notice in the settings panel and press Stop on is leaving our own mess on screen.
    ///
    /// Deliberately narrow: it stops nothing that a current Bennu would have started, so it can
    /// never take away a session somebody wanted.
    fn prune_dependency_roots(&self) {
        let stale: Vec<SlotKey> = {
            let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            slots
                .keys()
                .filter(|(root, _lang)| is_dependency_source(Path::new(root)))
                .cloned()
                .collect()
        };
        for (root, language) in stale {
            self.stop(&root, &language);
        }
    }

    /// The ready session serving `file`, **without** starting one.
    ///
    /// For the paths that must not have a side effect — a diagnostics poll, a status read.
    pub fn session_for(&self, file: &str) -> Option<Arc<LspSession>> {
        let cfg = load_config();
        let server = self.server_for(file, &cfg.lsp)?;
        let root = match self.root_of(file, &server) {
            RootKind::Own(root) => root,
            RootKind::Borrowed => return self.sole_session(&server.language),
            RootKind::None => return None,
        };
        let key = slot_key(&root.to_string_lossy(), &server.language);
        match self.peek(&key) {
            Existing::Ready(s) if s.is_alive() => Some(s),
            _ => None,
        }
    }

    /// The ready session that covers `path`, whether `path` is a **file it serves** or a
    /// **directory inside its workspace**.
    ///
    /// The extension-keyed lookup is right for a request about a buffer and wrong for a request
    /// about the workspace: a workspace-symbol search or a project-wide problems list is naturally
    /// addressed by the project root, which has no extension and which
    /// [`session_for`](Self::session_for) would therefore refuse. Resolving by containment as well
    /// is what lets those be asked at all.
    ///
    /// The **longest** containing root wins, so a workspace member opened in its own right answers
    /// for itself rather than deferring to the outer workspace.
    pub fn session_covering(&self, path: &str) -> Option<Arc<LspSession>> {
        if let Some(session) = self.session_for(path) {
            return Some(session);
        }
        let needle = path.replace('\\', "/");
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let mut best: Option<(usize, Arc<LspSession>)> = None;
        for ((root, _lang), slot) in slots.iter() {
            let Slot::Ready(session) = slot else { continue };
            if !session.is_alive() {
                continue;
            }
            let contained = needle == *root
                || needle.starts_with(&format!("{}/", root.trim_end_matches('/')));
            if !contained {
                continue;
            }
            if best.as_ref().map(|(len, _)| root.len() > *len).unwrap_or(true) {
                best = Some((root.len(), Arc::clone(session)));
            }
        }
        best.map(|(_, s)| s)
    }

    /// Whether a language server is the engine for `file` — true even while it is starting or
    /// after it failed.
    ///
    /// The routing predicate. It has to be independent of whether the server is *up*, or a
    /// request for a `.rs` file would fall through to the Java engine during startup and get a
    /// confidently wrong answer from a resolver that has never seen Rust.
    pub fn is_lsp_file(&self, file: &str) -> bool {
        let cfg = load_config();
        if !cfg.lsp.enabled {
            return false;
        }
        let Some(server) = self.server_for(file, &cfg.lsp) else {
            return false;
        };
        find_root(Path::new(file), &server.markers()).is_some()
    }

    /// What the server serving `file` can say right now — **without starting one**.
    ///
    /// The no-side-effects part is the point: this is called to explain an empty answer, and a
    /// question about why nothing came back must not itself spawn a language server.
    ///
    /// A file inside an unpacked dependency reports [`ServerReadiness::Absent`] rather than the
    /// state of whichever session happens to have it open: that session is borrowed (see
    /// [`Self::root_of`]) and naming it here would attribute a project's readiness to a file
    /// outside it.
    pub fn readiness_for(&self, file: &str) -> ServerReadiness {
        let cfg = load_config();
        if !cfg.lsp.enabled {
            return ServerReadiness::Absent;
        }
        let Some(server) = self.server_for(file, &cfg.lsp) else {
            return ServerReadiness::Absent;
        };
        let RootKind::Own(root) = self.root_of(file, &server) else {
            return ServerReadiness::Absent;
        };
        let key = slot_key(&root.to_string_lossy(), &server.language);
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        match slots.get(&key) {
            None => ServerReadiness::Idle { name: server.name.clone() },
            Some(Slot::Starting(info)) => {
                ServerReadiness::Warming { name: info.name.clone(), detail: String::new() }
            }
            Some(Slot::Failed { info, message, .. }) => {
                ServerReadiness::Failed { name: info.name.clone(), message: message.clone() }
            }
            Some(Slot::Ready(session)) => {
                let status = session.status();
                if !session.is_alive() {
                    return ServerReadiness::Failed {
                        name: status.name,
                        message: "the language server exited".to_string(),
                    };
                }
                // Handshake done is not the same as ready to answer. `progress` is the server's own
                // "Indexing 43%", and the window it covers is exactly the one in which a confident
                // empty answer is wrong.
                match status.progress.is_empty() {
                    true => ServerReadiness::Ready { name: status.name },
                    false => ServerReadiness::Warming {
                        name: status.name,
                        detail: status.progress,
                    },
                }
            }
        }
    }

    /// Every slot's status, for the status bar and the settings panel.
    pub fn statuses(&self) -> Vec<bennu_proto::prelude::LspStatus> {
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: Vec<bennu_proto::prelude::LspStatus> = slots
            .values()
            .map(|slot| match slot {
                Slot::Ready(session) => {
                    let s = session.status();
                    bennu_proto::prelude::LspStatus {
                        id: s.id,
                        name: s.name,
                        language: s.language,
                        root: s.root,
                        command: s.command,
                        version: s.version,
                        state: s.state.as_str().to_string(),
                        message: s.message,
                        progress: s.progress,
                        features: session.features().iter().map(|f| f.to_string()).collect(),
                        log_tail: s.log_tail,
                    }
                }
                Slot::Starting(info) => status_of(info, SessionState::Starting, String::new(), Vec::new()),
                Slot::Failed { info, message, log_tail } => {
                    status_of(info, SessionState::Failed, message.clone(), log_tail.clone())
                }
            })
            .collect();
        // Deterministic order, or the settings list reshuffles on every poll.
        out.sort_by(|a, b| (&a.language, &a.root).cmp(&(&b.language, &b.root)));
        out
    }

    /// Restart the server for `(root, language)`, or start it if it never ran.
    ///
    /// The only way out of a sticky failure, and the fix for "I just installed it".
    pub fn restart(&'static self, root: &str, language: &str) -> bool {
        let key = slot_key(root, language);
        let previous = self.slots.lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
        if let Some(Slot::Ready(session)) = &previous {
            session.shutdown();
        }
        // The restarted session is the one that was there: a background slot restarted by hand from
        // the settings panel does not thereby acquire a window, and re-reading the lease before it
        // is dropped is what keeps that true.
        let origin = match self.is_background(&key) {
            true => SessionOrigin::Background,
            false => SessionOrigin::Window,
        };
        self.leases.lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
        self.emit_status();
        self.warm_start(&key.0, origin);
        true
    }

    /// Stop the server for `(root, language)` and forget the slot.
    pub fn stop(&self, root: &str, language: &str) -> bool {
        let key = slot_key(root, language);
        let slot = self.slots.lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
        self.leases.lock().unwrap_or_else(|p| p.into_inner()).remove(&key);
        let stopped = matches!(slot, Some(Slot::Ready(_)));
        if let Some(Slot::Ready(session)) = slot {
            session.shutdown();
        }
        self.emit_status();
        stopped
    }

    /// Stop every server — the backend is going away.
    pub fn shutdown_all(&self) {
        let slots: Vec<Slot> =
            self.slots.lock().unwrap_or_else(|p| p.into_inner()).drain().map(|(_, v)| v).collect();
        self.leases.lock().unwrap_or_else(|p| p.into_inner()).clear();
        for slot in slots {
            if let Slot::Ready(session) = slot {
                session.shutdown();
            }
        }
    }

    /// The catalogue + the user's own servers, resolved against this machine — what the
    /// settings panel lists.
    pub fn availability(&self) -> Vec<ServerAvailability> {
        let cfg = load_config();
        // The availability list ignores `disabled`: the settings page has to show a server the user
        // turned off, or there would be no way to turn it back on.
        self.all_servers(&LspConfig { disabled: Vec::new(), ..cfg.lsp.clone() })
            .into_iter()
            .map(|s| ServerAvailability {
                path: locate_for(&s, &cfg.lsp.server_paths),
                enabled: cfg.lsp.enabled && !cfg.lsp.disabled.iter().any(|d| *d == s.id),
                id: s.id,
                name: s.name,
                language: s.language,
                extensions: s.extensions,
                command: s.command,
                install_hint: s.install_hint,
                custom: s.custom,
            })
            .collect()
    }

    /// The server that serves `file`, honouring the user's overrides and disables.
    fn server_for(&self, file: &str, cfg: &LspConfig) -> Option<ResolvedServer> {
        self.all_servers(cfg).into_iter().find(|s| s.serves(file))
    }

    /// Every enabled server, user-defined ones first.
    ///
    /// The order is the override mechanism: a `[[lsp.servers]]` entry whose `id` matches a
    /// built-in shadows it, which is how a server gets *reconfigured* (different args, different
    /// init options) rather than merely re-pointed at another binary.
    /// The whole `LspConfig` and not two slices of it: the list is derived from the custom entries,
    /// the disable list AND the per-server settings, and a signature that grew a parameter per
    /// setting is how six call sites end up each passing a different subset.
    fn all_servers(&self, cfg: &LspConfig) -> Vec<ResolvedServer> {
        let is_disabled = |id: &str| cfg.disabled.iter().any(|d| d == id);
        let mut out: Vec<ResolvedServer> = cfg
            .servers
            .iter()
            .filter_map(ResolvedServer::from_custom)
            .filter(|s| !is_disabled(&s.id))
            .collect();
        let shadowed: Vec<String> = out.iter().map(|s| s.id.clone()).collect();
        for spec in BUILTIN_SERVERS {
            if is_disabled(spec.id) || shadowed.iter().any(|s| s == spec.id) {
                continue;
            }
            out.push(ResolvedServer::from_spec(spec, &cfg.rust_check_command));
        }
        out
    }

    fn emit_status(&self) {
        if let Some(sink) = self.sink() {
            sink.emit(EVT_STATUS, serde_json::json!(self.statuses()));
        }
    }

    fn emit(&self, topic: &str, payload: serde_json::Value) {
        if let Some(sink) = self.sink() {
            sink.emit(topic, payload);
        }
    }
}

/// A slot snapshot, taken under the lock and acted on after it is released.
/// Which root a file belongs to — see [`LspRegistry::root_of`].
enum RootKind {
    /// A workspace root of its own; a server may be started for it.
    Own(PathBuf),
    /// A read-only dependency source. It borrows a live session rather than getting one.
    Borrowed,
    /// Nothing above it that looks like a project.
    None,
}

enum Existing {
    Ready(Arc<LspSession>),
    /// Starting, or failed. Either way there is nothing to serve and nothing to start.
    Pending,
    Absent,
}

/// The slot key for a root + language. Roots are normalised to forward slashes so a path that
/// arrived from the frontend and one that came off the filesystem land in the same slot.
fn slot_key(root: &str, language: &str) -> SlotKey {
    (root.replace('\\', "/"), language.to_string())
}

/// "…was not found", plus what to do about it.
fn not_found_message(server: &ResolvedServer) -> String {
    if server.install_hint.is_empty() {
        format!("`{}` was not found", server.command)
    } else {
        format!("`{}` was not found. {}", server.command, server.install_hint)
    }
}

/// Resolve a server's executable, honouring a config path override.
fn locate_for(
    server: &ResolvedServer,
    overrides: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let override_path = overrides.get(&server.id).map(String::as_str);
    match spec_by_id(&server.id) {
        // A catalogue entry knows its own extra search locations (rustup toolchains, the VS
        // Code extension dir); a user-defined one only has what discovery looks at generally.
        Some(spec) if !server.custom => locate(spec, override_path),
        _ => locate_custom(&server.command, override_path),
    }
}

fn status_of(
    info: &SlotInfo,
    state: SessionState,
    message: String,
    log_tail: Vec<String>,
) -> bennu_proto::prelude::LspStatus {
    bennu_proto::prelude::LspStatus {
        id: info.id.clone(),
        name: info.name.clone(),
        language: info.language.clone(),
        root: info.root.clone(),
        command: info.command.clone(),
        version: None,
        state: state.as_str().to_string(),
        message,
        progress: String::new(),
        features: Vec::new(),
        log_tail,
    }
}

/// Turns session callbacks into frontend events.
struct RegistryObserver {
    registry: &'static LspRegistry,
    key: SlotKey,
}

impl SessionObserver for RegistryObserver {
    fn diagnostics_published(&self, file: &str) {
        // The payload is the file, not the diagnostics: the frontend re-requests them through
        // the same `bennu_diagnostics` pipe every other language uses, which keeps one code
        // path for squiggles instead of a second, LSP-shaped one.
        self.registry.emit(
            EVT_DIAGNOSTICS,
            serde_json::json!({ "file": file, "language": self.key.1, "root": self.key.0 }),
        );
    }

    fn status_changed(&self) {
        self.registry.emit_status();
    }

    fn message(&self, level: &str, text: &str) {
        self.registry
            .emit(EVT_MESSAGE, serde_json::json!({ "level": level, "message": text }));
    }

    fn apply_edit(&self, edits: Vec<FileEdit>, file_ops: Vec<FileOp>) -> bool {
        // Applied by the **frontend**, through CodeMirror, so the edit lands in the undo
        // history like any other. The backend never writes a buffer — the same rule the rename
        // flow follows.
        if edits.is_empty() && file_ops.is_empty() {
            return false;
        }
        let wire: Vec<bennu_proto::prelude::SourceEdit> = edits
            .into_iter()
            .map(|e| bennu_proto::prelude::SourceEdit {
                file: e.file,
                start: e.start,
                end: e.end,
                new_text: e.new_text,
            })
            .collect();
        let ops: Vec<String> = file_ops.iter().map(FileOp::describe).collect();
        let applied = !wire.is_empty();
        self.registry.emit(
            EVT_APPLY_EDIT,
            serde_json::json!({ "edits": wire, "file_ops": ops }),
        );
        // Reported as applied when there were edits to hand over. Strictly this is optimistic
        // — the frontend has not confirmed yet — but the alternative is answering `false` to a
        // server that then reports the refactoring as failed, on an edit that did land.
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `LspConfig` with just the two fields these tests vary.
    fn cfg_with(servers: Vec<CustomLspServer>, disabled: Vec<String>) -> LspConfig {
        LspConfig { servers, disabled, ..LspConfig::default() }
    }

    fn custom(id: &str, ext: &[&str], markers: &[&str]) -> CustomLspServer {
        CustomLspServer {
            id: id.to_string(),
            name: String::new(),
            language: String::new(),
            command: "some-ls".to_string(),
            args: Vec::new(),
            extensions: ext.iter().map(|s| s.to_string()).collect(),
            root_markers: markers.iter().map(|s| s.to_string()).collect(),
            initialization_options: String::new(),
        }
    }

    #[test]
    fn a_custom_entry_needs_a_command_extensions_and_markers() {
        assert!(ResolvedServer::from_custom(&custom("zig", &["zig"], &["build.zig"])).is_some());

        let mut no_ext = custom("zig", &[], &["build.zig"]);
        no_ext.extensions.clear();
        assert!(ResolvedServer::from_custom(&no_ext).is_none(), "it could never be selected");

        let no_markers = custom("zig", &["zig"], &[]);
        assert!(ResolvedServer::from_custom(&no_markers).is_none(), "it could never start");

        let mut no_cmd = custom("zig", &["zig"], &["build.zig"]);
        no_cmd.command = "  ".to_string();
        assert!(ResolvedServer::from_custom(&no_cmd).is_none());

        let mut no_id = custom("", &["zig"], &["build.zig"]);
        no_id.id = String::new();
        assert!(ResolvedServer::from_custom(&no_id).is_none());
    }

    #[test]
    fn a_custom_entry_defaults_its_name_and_language_to_its_id() {
        let r = ResolvedServer::from_custom(&custom("zig", &["zig"], &["build.zig"])).unwrap();
        assert_eq!(r.name, "zig");
        assert_eq!(r.language, "zig");
        assert!(r.custom);
    }

    #[test]
    fn custom_extensions_are_normalised() {
        // A user will write `.zig` as often as `zig`, and `.ZIG` happens too.
        let mut c = custom("zig", &[], &["build.zig"]);
        c.extensions = vec![".ZIG".to_string(), "Zon".to_string()];
        let r = ResolvedServer::from_custom(&c).unwrap();
        assert_eq!(r.extensions, vec!["zig", "zon"]);
        assert!(r.serves("/p/src/main.zig"));
        assert!(r.serves("/p/build.zon"));
    }

    #[test]
    fn a_custom_entry_shadows_a_builtin_with_the_same_id() {
        // The override mechanism: this is how rust-analyzer gets different args or different
        // init options, as opposed to merely a different binary path.
        let reg = LspRegistry::global();
        let mine = custom("rust-analyzer", &["rs"], &["Cargo.toml"]);
        let all = reg.all_servers(&cfg_with(vec![mine], Vec::new()));
        let matches: Vec<&ResolvedServer> =
            all.iter().filter(|s| s.id == "rust-analyzer").collect();
        assert_eq!(matches.len(), 1, "not registered twice");
        assert!(matches[0].custom, "the user's entry wins");
    }

    #[test]
    fn a_disabled_server_disappears_from_the_list() {
        let reg = LspRegistry::global();
        let disabled = vec!["rust-analyzer".to_string()];
        assert!(!reg
            .all_servers(&cfg_with(Vec::new(), disabled))
            .iter()
            .any(|s| s.id == "rust-analyzer"));
        // …and a disabled custom one too.
        let mine = custom("zig", &["zig"], &["build.zig"]);
        let disabled = vec!["zig".to_string()];
        assert!(!reg.all_servers(&cfg_with(vec![mine], disabled)).iter().any(|s| s.id == "zig"));
    }

    #[test]
    fn a_rust_file_selects_rust_analyzer_and_a_java_file_selects_nothing() {
        // The routing predicate's core: Java must never reach a language server, because
        // Bennu's own engine is the better answer for it.
        let reg = LspRegistry::global();
        assert_eq!(
            reg.server_for("/p/src/main.rs", &LspConfig::default()).map(|s| s.id),
            Some("rust-analyzer".to_string())
        );
        let cfg = LspConfig::default();
        assert!(reg.server_for("/p/src/Main.java", &cfg).is_none());
        assert!(reg.server_for("/p/pom.xml", &cfg).is_none());
        assert!(reg.server_for("/p/README", &cfg).is_none());
    }

    #[test]
    fn invalid_initialization_options_are_ignored_not_fatal() {
        assert!(parse_init_options("x", "not json").is_none());
        assert!(parse_init_options("x", "   ").is_none());
        assert_eq!(
            parse_init_options("x", r#"{"a":1}"#),
            Some(serde_json::json!({ "a": 1 }))
        );
    }

    #[test]
    fn a_missing_binary_produces_a_message_that_says_what_to_do() {
        // "not found" alone leaves the user with no next step, which is the whole reason the
        // catalogue carries install hints.
        let spec = spec_by_id("rust-analyzer").unwrap();
        assert!(spec.install_hint.contains("rustup component add"));
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;

    /// The registry is process-wide, so every test uses a key of its own — sharing one would make
    /// these depend on the order they happened to run in.
    fn key(name: &str) -> SlotKey {
        (format!("/lease-test/{name}"), "rust".to_string())
    }

    #[test]
    fn a_window_claims_a_session_an_agent_started() {
        // The case the promotion exists for: an AI client summarised a project half an hour ago and
        // the user has just opened it in a tab. Taking that server away is now a visible stall.
        let reg = LspRegistry::global();
        let k = key("promote");
        reg.claim(&k, SessionOrigin::Background);
        assert!(reg.is_background(&k));
        reg.claim(&k, SessionOrigin::Window);
        assert!(!reg.is_background(&k));
    }

    #[test]
    fn a_later_agent_call_does_not_demote_a_window_session() {
        // One-way. Closing a tab is not a statement about memory, and a session the user has been
        // in is the one most likely to be wanted again.
        let reg = LspRegistry::global();
        let k = key("no-demote");
        reg.claim(&k, SessionOrigin::Window);
        reg.claim(&k, SessionOrigin::Background);
        assert!(!reg.is_background(&k));
    }

    #[test]
    fn touching_a_slot_that_has_no_lease_is_not_an_error() {
        // `stop` drops the lease; a request already in flight may touch the key just after.
        LspRegistry::global().touch(&key("absent"));
    }

    #[test]
    fn the_background_profile_is_added_to_the_base_rather_than_replacing_it() {
        let base = serde_json::json!({ "procMacro": { "enable": true }, "checkOnSave": true });
        let merged = lean(Some(base), background_init_options("rust-analyzer")).unwrap();
        // The settings that make a project resolve survive untouched — the whole safety property.
        assert_eq!(merged["procMacro"]["enable"], serde_json::json!(true));
        assert_eq!(merged["checkOnSave"], serde_json::json!(true));
        assert_eq!(merged["cachePriming"]["enable"], serde_json::json!(false));
        assert!(merged["lru"]["capacity"].is_number());
        assert!(merged["numThreads"].is_number());
    }

    #[test]
    fn a_server_with_no_profile_is_left_exactly_as_it_was() {
        let base = serde_json::json!({ "a": 1 });
        assert_eq!(lean(Some(base.clone()), background_init_options("gopls")), Some(base));
    }

    #[test]
    fn hand_written_options_that_are_not_an_object_are_never_rewritten() {
        // An optimisation must not be the reason a session starts wrong.
        let odd = serde_json::json!(["not", "an", "object"]);
        assert_eq!(lean(Some(odd.clone()), background_init_options("rust-analyzer")), Some(odd));
    }
}
