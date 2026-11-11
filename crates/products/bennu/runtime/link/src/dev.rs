//! The link to a fulcrum game over the **`dev` channel**.
//!
//! The game (any fulcrum game started with its dev channel on — Voriz: `--dev-channel`, cargo feature
//! `dev-channel`) listens on loopback and writes a discovery file in `<tmp>/fulcrum-debug`; this finds it,
//! says hello over the shared transport (`fulcrum-debug-channel`) and speaks the typed protocol of
//! `fulcrum-dev-channel`: a console command, state / HUD / chat / mods / world queries, a screenshot, and
//! injected input. **There is no ECS on this channel and none is invented here**: entities, components and
//! resources are the Bevy Remote Protocol's, and this link says so when asked.
//!
//! ## Why every call runs on `spawn_blocking`
//!
//! `DevClient` is synchronous — it was made for command-line tools and tests — and a request polls the
//! socket until its answer arrives. Run on an async worker it would hold the worker for as long as the game
//! takes (a screenshot finishes a few frames after it is asked for). So each call locks the one client,
//! asks, and gives the answer back from a blocking thread. The lock is also what makes the session single:
//! two tools asking at once are served one after the other, never interleaved on the socket.
//!
//! ## A restarted game
//!
//! The channel client reconnects by itself (a wait that doubles up to two seconds) and repeats the hello,
//! so a game restarted by `cargo run` is found again on the same port **if it gets the same one**. A call
//! made in the gap fails with a sentence that says to try again; nothing here caches a reply.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use fulcrum_debug_channel::discovery::{self, DiscoverConfig};
use fulcrum_debug_channel::error::ChannelError;
use fulcrum_debug_channel::topic;
use fulcrum_dev_channel::client::{DevClient, DevError};
use fulcrum_dev_channel::message::{
    Caller, HudEntry, HudValue, ImageSource, Input, KeyAction, KeyPress, MouseButton, MouseOp, Query, Request, Response,
};
use fulcrum_dev_channel::TOPIC;
use serde::Serialize;
use serde_json::{json, Value};

use crate::args::{ActionArgs, AnalyzeArgs, CommandArgs, KeyArgs, MouseArgs, PoseArgs, PropsArgs, RegionArgs, ScreenshotArgs, TextArgs, UiArgs};
use crate::game::{GameLink, Shot};
use crate::target::{Capability, LiveTarget, TargetKind};

/// How long one request may take before the call gives up. A screenshot or a command that loads a world is
/// the slow case; the game still answers a frame at a time.
const DEFAULT_WAIT: Duration = Duration::from_secs(20);
/// The name Arbor says in its hello (the game's log shows who attached).
const CLIENT_NAME: &str = "arbor";
/// `bevy_game_type` takes 1–500 characters, like the BRP companion.
const MAX_TEXT_CHARS: usize = 500;
/// A drag is `steps` moves over `duration_ms`; these bound what a caller can ask for.
const DEFAULT_DRAG_STEPS: u32 = 12;
const MAX_DRAG_STEPS: u32 = 100;
const DEFAULT_DRAG_MS: u64 = 300;
const MAX_DRAG_MS: u64 = 10_000;

/// Which game to attach to.
#[derive(Debug, Clone, Default)]
pub struct DevTarget {
    /// The game's id in its discovery file (`voriz`). Needed only when more than one game is listening.
    pub game: Option<String>,
    /// Skip the discovery folder and connect to this loopback port.
    pub port: Option<u16>,
    /// A discovery folder other than `<tmp>/fulcrum-debug` (tests; a game started with another one).
    pub dir: Option<PathBuf>,
}

/// A game found in the discovery folder and verified alive with a hello.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundDevGame {
    pub game: String,
    pub name: String,
    pub port: u16,
    pub pid: u32,
    /// `false`: it answers, but speaks another version of the `dev` topic.
    pub compatible: bool,
    /// What it hosts, as `ai=13,dev=1`.
    pub topics: String,
}

fn discover_config(dir: Option<&Path>) -> DiscoverConfig {
    match dir {
        Some(d) => DiscoverConfig::in_dir(d),
        None => DiscoverConfig::default(),
    }
}

/// The games that host the `dev` topic and answer a hello right now. A game stopped at a breakpoint or
/// loading for a long time does not answer in time and is not listed — it comes back when it runs again.
pub fn list_games(dir: Option<&Path>) -> Vec<FoundDevGame> {
    discovery::discover(&discover_config(dir), &[TOPIC])
        .into_iter()
        .map(|f| FoundDevGame {
            game: f.record.game,
            name: f.record.name,
            port: f.record.port,
            pid: f.record.pid,
            compatible: f.probe.compatible,
            topics: topic::describe(&f.probe.topics),
        })
        .collect()
}

/// The link. Cheap to share: the client lives behind one lock.
pub struct DevChannelLink {
    client: Arc<Mutex<DevClient>>,
    game: String,
    name: String,
    port: u16,
    wait: Duration,
}

impl DevChannelLink {
    /// Find the game and say hello. Blocking: call it from `spawn_blocking` or use [`Self::connect`].
    pub fn connect_blocking(target: &DevTarget) -> Result<DevChannelLink, String> {
        let (addr, game, name, port) = match target.port {
            Some(port) => (
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                target.game.clone().unwrap_or_else(|| format!("port {port}")),
                String::new(),
                port,
            ),
            None => {
                let found = discovery::find(&discover_config(target.dir.as_deref()), target.game.as_deref(), &[TOPIC]).map_err(explain_connect)?;
                (found.addr(), found.record.game.clone(), found.record.name.clone(), found.record.port)
            }
        };
        let client = DevClient::connect(addr, CLIENT_NAME).map_err(|e| explain_connect_error(&e))?;
        let name = client.server_name().map(str::to_string).filter(|n| !n.is_empty()).unwrap_or(name);
        Ok(DevChannelLink { client: Arc::new(Mutex::new(client)), game, name, port, wait: DEFAULT_WAIT })
    }

    /// [`Self::connect_blocking`] off the async workers.
    pub async fn connect(target: DevTarget) -> Result<DevChannelLink, String> {
        tokio::task::spawn_blocking(move || Self::connect_blocking(&target))
            .await
            .map_err(|e| format!("the dev channel connection task failed: {e}"))?
    }

    /// The same link with a different patience per request (tests use a short one).
    pub fn with_wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    pub fn game(&self) -> &str {
        &self.game
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether the socket is up right now. `None` when a request is in flight (the lock is taken): a
    /// request in flight is the best sign of life there is.
    pub fn is_connected(&self) -> Option<bool> {
        self.client.try_lock().ok().map(|c| c.is_connected())
    }

    /// One request, one answer. A game-side `Error` becomes the `Err` with the game's own sentence.
    async fn ask(&self, request: Request) -> Result<Response, String> {
        let client = Arc::clone(&self.client);
        let wait = self.wait;
        tokio::task::spawn_blocking(move || {
            let mut c = client.lock().unwrap_or_else(|e| e.into_inner());
            ask_blocking(&mut c, request, wait)
        })
        .await
        .map_err(|e| format!("the dev channel task failed: {e}"))?
    }

    /// Like [`Self::ask`] with a longer patience: a pose series answers only after its last frame.
    async fn ask_for(&self, request: Request, wait: Duration) -> Result<Response, String> {
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            let mut c = client.lock().unwrap_or_else(|e| e.into_inner());
            ask_blocking(&mut c, request, wait)
        })
        .await
        .map_err(|e| format!("the dev channel task failed: {e}"))?
    }

    /// Queue an input batch and say what the game counted.
    async fn input(&self, input: Input) -> Result<Value, String> {
        match self.ask(Request::Input(input)).await? {
            Response::InputQueued { keys, chars, mouse } => Ok(json!({ "protocol": "dev", "queued": { "keys": keys, "chars": chars, "mouse": mouse } })),
            other => Err(unexpected("input", &other)),
        }
    }
}

fn ask_blocking(client: &mut DevClient, request: Request, wait: Duration) -> Result<Response, String> {
    match client.request(request, wait).map_err(|e| explain_request_error(&e))? {
        Response::Error(message) => Err(message),
        other => Ok(other),
    }
}

// ── Sentences ────────────────────────────────────────────────────────────────────────

const START_HINT: &str = "Start the game with `--dev-channel` (the cargo feature `dev-channel` must be compiled in) and make sure it is running its frame loop: a paused or still-loading game cannot answer.";

fn explain_connect(e: ChannelError) -> String {
    match e {
        ChannelError::NotFound(what) => format!("No game answers on the `dev` channel ({what}). {START_HINT}"),
        ChannelError::Ambiguous(list) => format!("More than one game is listening on the `dev` channel ({list}): pass `game` to say which."),
        other => explain_connect_error(&DevError::Channel(other)),
    }
}

fn explain_connect_error(e: &DevError) -> String {
    match e {
        DevError::Channel(ChannelError::Incompatible { reason, .. }) => {
            format!("The game and Arbor speak different versions of the `dev` channel ({reason}). Rebuild the older of the two.")
        }
        DevError::Channel(ChannelError::Rejected(reason)) => format!("The game refused the connection ({reason}). It may not host the `dev` topic."),
        DevError::Channel(ChannelError::Timeout) | DevError::Timeout => format!("The game did not answer the hello in time. {START_HINT}"),
        DevError::Channel(ChannelError::Io(io)) => format!("Cannot reach the game: {io}. {START_HINT}"),
        other => other.to_string(),
    }
}

fn explain_request_error(e: &DevError) -> String {
    match e {
        DevError::Timeout => "The game did not answer in time (is it paused, minimised, loading or not rendering?).".into(),
        DevError::Disconnected(reason) => format!("The game closed the connection ({reason}). If it restarted, Arbor reconnects by itself: call again in a moment."),
        DevError::Decode(what) => format!("The game sent an answer Arbor cannot read ({what}): the `dev` protocol versions differ. Rebuild the older of the two."),
        DevError::Channel(ChannelError::NotConnected) => "The game is not connected right now. If it restarted, Arbor reconnects by itself: call again in a moment.".into(),
        DevError::Channel(other) => format!("The `dev` channel failed: {other}."),
    }
}

fn response_name(r: &Response) -> &'static str {
    match r {
        Response::Cmd(_) => "a command result",
        Response::State(_) => "a mod state",
        Response::Hud(_) => "HUD values",
        Response::Chat(_) => "chat lines",
        Response::Mods(_) => "a mod list",
        Response::World(_) => "a world description",
        Response::Screenshot(_) => "a screenshot",
        Response::InputQueued { .. } => "an input receipt",
        Response::Error(_) => "an error",
        Response::Ui(_) => "a UI tree",
        Response::Image(_) => "image analysis results",
        // Answers of later protocol versions that this link never asks for (the pose queries are read by their own tools).
        #[allow(unreachable_patterns)]
        _ => "an answer of a newer protocol",
    }
}

fn unexpected(asked: &str, got: &Response) -> String {
    format!("Asked the game for {asked} and it answered with {}: the `dev` protocol versions differ.", response_name(got))
}

// ── Shapes ───────────────────────────────────────────────────────────────────────────

fn hud_value(v: &HudValue) -> Value {
    match v {
        HudValue::Unset => Value::Null,
        HudValue::Int(i) => json!(i),
        HudValue::Fx(f) => json!(f),
        HudValue::Bool(b) => json!(b),
        HudValue::Text(t) => json!(t),
    }
}

fn hud_entry(e: &HudEntry) -> Value {
    json!({ "mod": e.mod_id, "key": e.key, "value": hud_value(&e.value), "writes": e.writes })
}

/// `name arg`, the console line an action becomes: a string argument is the text as it is, anything else its JSON.
fn action_line(name: &str, arg: Option<&Value>) -> String {
    match arg {
        None | Some(Value::Null) => name.trim().to_string(),
        Some(Value::String(s)) => format!("{} {}", name.trim(), s.trim()).trim().to_string(),
        Some(other) => format!("{} {other}", name.trim()),
    }
}

fn key_action(word: Option<&str>) -> Result<KeyAction, String> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("press") | Some("tap") => Ok(KeyAction::Tap),
        Some("down") => Ok(KeyAction::Down),
        Some("up") => Ok(KeyAction::Up),
        Some(other) => Err(format!("unknown key action `{other}`: use `press`, `down` or `up`")),
    }
}

fn mouse_button(word: Option<&str>) -> Result<MouseButton, String> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("left") => Ok(MouseButton::Left),
        Some("right") => Ok(MouseButton::Right),
        Some("middle") => Ok(MouseButton::Middle),
        Some(other) => Err(format!("the `dev` channel has the buttons left, right and middle, not `{other}`")),
    }
}

/// The key batch of a press: modifiers down, the key, modifiers up (kept down for a `down`, so a held
/// combination can be built with several calls).
fn key_batch(args: &KeyArgs) -> Result<Input, String> {
    let action = key_action(args.action.as_deref())?;
    let mut keys: Vec<KeyPress> = args.modifiers.iter().map(|m| KeyPress { key: m.clone(), action: KeyAction::Down }).collect();
    keys.push(KeyPress { key: args.key.clone(), action });
    if action != KeyAction::Down {
        keys.extend(args.modifiers.iter().rev().map(|m| KeyPress { key: m.clone(), action: KeyAction::Up }));
    }
    Ok(Input { keys, ..Input::default() })
}

fn point(args: &MouseArgs) -> Option<MouseOp> {
    match (args.x, args.y) {
        (Some(x), Some(y)) => Some(MouseOp::MoveTo { x, y }),
        _ => None,
    }
}

fn require_point(args: &MouseArgs) -> Result<MouseOp, String> {
    point(args).ok_or_else(|| format!("`x` and `y` are required for `{}`", args.action))
}

/// The mouse batch of every action but `drag` (which is several batches over time).
fn mouse_batch(args: &MouseArgs) -> Result<Input, String> {
    let button = mouse_button(args.button.as_deref())?;
    let mouse = match args.action.trim().to_ascii_lowercase().as_str() {
        "move" => vec![require_point(args)?],
        "down" => point(args).into_iter().chain([MouseOp::Down { button }]).collect(),
        "up" => point(args).into_iter().chain([MouseOp::Up { button }]).collect(),
        "click" => vec![require_point(args)?, MouseOp::Click { button }],
        "double_click" => vec![require_point(args)?, MouseOp::Click { button }, MouseOp::Click { button }],
        "wheel" => vec![MouseOp::Scroll { dy: args.dy.ok_or("`dy` is required for `wheel`")? }],
        other => return Err(format!("unknown mouse action `{other}`: use move, down, up, click, double_click, drag or wheel")),
    };
    Ok(Input { mouse, ..Input::default() })
}

/// A drag is a press, `steps` moves spread over `duration_ms`, and a release: the channel schedules one
/// batch relative to *now*, so the moves must be sent over time rather than all at once.
fn drag_blocking(client: &mut DevClient, args: &MouseArgs, wait: Duration) -> Result<(), String> {
    let button = mouse_button(args.button.as_deref())?;
    let (x0, y0) = match (args.x, args.y) {
        (Some(x), Some(y)) => (x, y),
        _ => return Err("`x` and `y` are required for `drag`".into()),
    };
    let (x1, y1) = match (args.to_x, args.to_y) {
        (Some(x), Some(y)) => (x, y),
        _ => return Err("`to_x` and `to_y` are required for `drag`".into()),
    };
    let steps = args.steps.unwrap_or(DEFAULT_DRAG_STEPS).clamp(1, MAX_DRAG_STEPS);
    let total = args.duration_ms.unwrap_or(DEFAULT_DRAG_MS).min(MAX_DRAG_MS);
    let pause = Duration::from_millis((total / u64::from(steps)).max(16));
    let send = |client: &mut DevClient, mouse: Vec<MouseOp>| ask_blocking(client, Request::Input(Input { mouse, ..Input::default() }), wait).map(|_| ());
    send(client, vec![MouseOp::MoveTo { x: x0, y: y0 }, MouseOp::Down { button }])?;
    std::thread::sleep(pause);
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        send(client, vec![MouseOp::MoveTo { x: x0 + (x1 - x0) * t, y: y0 + (y1 - y0) * t }])?;
        std::thread::sleep(pause);
    }
    send(client, vec![MouseOp::Up { button }])
}

// ── The link ─────────────────────────────────────────────────────────────────────────

impl LiveTarget for DevChannelLink {
    fn kind(&self) -> TargetKind {
        TargetKind::Game
    }

    fn protocol(&self) -> &str {
        "dev"
    }

    fn label(&self) -> String {
        let who = if self.name.is_empty() { self.game.as_str() } else { self.name.as_str() };
        format!("{who} (port {})", self.port)
    }

    /// What this wire answers. Pause is not here and never will be: a game in a network session is not
    /// stopped to be read. Actions are not here either — there is no registry on this channel; a named
    /// action is run as a console line, which is `Command`.
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Info,
            Capability::Screenshot,
            Capability::Input,
            Capability::Command,
            Capability::Hud,
            Capability::Chat,
            Capability::Mods,
            Capability::World,
            Capability::ModState,
            Capability::UiTree,
            Capability::ImageAnalysis,
            Capability::Pose,
            Capability::Region,
            Capability::Props,
        ]
    }
}

#[async_trait]
impl GameLink for DevChannelLink {
    async fn info(&self) -> Result<Value, String> {
        // The world is a cheap query and says the game is really answering; its failure must not fail `info`.
        let world = match self.ask(Request::Query(Query::World)).await {
            Ok(Response::World(w)) => Some(world_json(&w)),
            _ => None,
        };
        Ok(json!({
            "protocol": "dev",
            "game": self.game,
            "name": self.name,
            "port": self.port,
            "capabilities": self.capabilities().into_iter().map(Capability::id).collect::<Vec<_>>(),
            "world": world,
            "keys": "letters, digits, Space, Enter, Escape, Tab, Backspace, Up/Down/Left/Right, Shift, Ctrl, Alt, F1..F12, -, /",
            "notes": [
                "There is no registry of named actions on this channel: a mod's or the game's commands are console lines, run with bevy_game_command.",
                "Prefer bevy_game_ui (the interface as data) and bevy_game_analyze (stats, palette, edges, diff on the screen) to bevy_game_screenshot: they answer in a few hundred bytes instead of a picture.",
                "For an animation or a movement, prefer bevy_game_pose (numbers: position, scale, rotation, opacity, running animations, optionally sampled over N frames with a summary) to a screenshot: it costs a few hundred bytes instead of thousands of tokens.",
                "To know what is in the world (blocks by type and layer, the height profile, enclosed voids) prefer bevy_game_region to a screenshot; to know whether and where a model or prop stands (and how tall it is) use bevy_game_props. Both are numbers (protocol 4); a map FILE needs no game at all: map_scheda, map_percorso.",
                "There is no ECS on this channel: entities, components and resources need the Bevy Remote Protocol (bevy_game_connect protocol brp).",
            ],
        }))
    }

    async fn screenshot(&self, args: ScreenshotArgs) -> Result<Shot, String> {
        match self.ask(Request::Screenshot { max_side: args.max_width.unwrap_or(0) }).await? {
            Response::Screenshot(s) => Ok(Shot {
                mime_type: "image/png".into(),
                data: base64::engine::general_purpose::STANDARD.encode(&s.data),
                width: u64::from(s.width),
                height: u64::from(s.height),
            }),
            other => Err(unexpected("a screenshot", &other)),
        }
    }

    async fn key(&self, args: KeyArgs) -> Result<Value, String> {
        self.input(key_batch(&args)?).await
    }

    async fn text(&self, args: TextArgs) -> Result<Value, String> {
        let count = args.text.chars().count();
        if count == 0 || count > MAX_TEXT_CHARS {
            return Err(format!("`text` must be 1 to {MAX_TEXT_CHARS} characters, not {count}"));
        }
        self.input(Input { text: Some(args.text), ..Input::default() }).await
    }

    async fn mouse(&self, args: MouseArgs) -> Result<Value, String> {
        if args.action.trim().eq_ignore_ascii_case("drag") {
            let client = Arc::clone(&self.client);
            let wait = self.wait;
            return tokio::task::spawn_blocking(move || {
                let mut c = client.lock().unwrap_or_else(|e| e.into_inner());
                drag_blocking(&mut c, &args, wait).map(|()| json!({ "protocol": "dev", "dragged": true }))
            })
            .await
            .map_err(|e| format!("the dev channel task failed: {e}"))?;
        }
        self.input(mouse_batch(&args)?).await
    }

    async fn action(&self, args: ActionArgs) -> Result<Value, String> {
        if args.name.trim().is_empty() {
            return Err("`name` is required".into());
        }
        let line = action_line(&args.name, args.arg.as_ref());
        self.command(CommandArgs { line, as_console: None }).await
    }

    async fn command(&self, args: CommandArgs) -> Result<Value, String> {
        let line = args.line.trim().trim_start_matches('/').to_string();
        if line.is_empty() {
            return Err("`line` is empty".into());
        }
        let caller = if args.as_console == Some(true) { Caller::Console } else { Caller::Host };
        match self.ask(Request::Cmd { line, caller }).await? {
            Response::Cmd(r) => Ok(json!({ "ok": r.ok, "lines": r.lines })),
            other => Err(unexpected("a command result", &other)),
        }
    }

    async fn hud(&self, mod_id: Option<String>) -> Result<Value, String> {
        match self.ask(Request::Query(Query::Hud { mod_id })).await? {
            Response::Hud(h) => Ok(json!({ "entries": h.entries.iter().map(hud_entry).collect::<Vec<_>>() })),
            other => Err(unexpected("HUD values", &other)),
        }
    }

    async fn chat(&self, last: u32) -> Result<Value, String> {
        match self.ask(Request::Query(Query::Chat { last: last.clamp(1, 200) })).await? {
            Response::Chat(c) => Ok(json!({
                "chat": c.chat.iter().map(|l| {
                    let params: serde_json::Map<String, Value> = l.params.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                    json!({ "to": l.to, "key": l.key, "params": params })
                }).collect::<Vec<_>>(),
                "console": c.console,
            })),
            other => Err(unexpected("chat lines", &other)),
        }
    }

    async fn mods(&self) -> Result<Value, String> {
        match self.ask(Request::Query(Query::Mods)).await? {
            Response::Mods(list) => Ok(Value::Array(
                list.iter().map(|m| json!({ "id": m.id, "enabled": m.enabled, "note": m.note, "hooks": m.hooks, "faults": m.faults })).collect(),
            )),
            other => Err(unexpected("a mod list", &other)),
        }
    }

    async fn world(&self) -> Result<Value, String> {
        match self.ask(Request::Query(Query::World)).await? {
            Response::World(w) => Ok(world_json(&w)),
            other => Err(unexpected("a world description", &other)),
        }
    }

    async fn ui_tree(&self, args: UiArgs) -> Result<Value, String> {
        let screen = args.screen.filter(|s| !s.trim().is_empty());
        let whole = screen.is_none();
        let query = Query::Ui { screen, visible_only: !args.include_hidden.unwrap_or(false) };
        match self.ask(Request::Query(query)).await? {
            Response::Ui(d) => Ok(crate::vision::ui_json(&d, whole)),
            other => Err(unexpected("the UI tree", &other)),
        }
    }

    async fn pose(&self, args: PoseArgs) -> Result<Value, String> {
        if args.target.trim().is_empty() {
            return Err("`target` is required: a UI node `scope/id`, `entity:Name`, or `player`".into());
        }
        match self.ask(Request::Query(Query::Pose { target: args.target.trim().to_string() })).await? {
            Response::Pose(d) => Ok(crate::pose::pose_json(&d)),
            other => Err(unexpected("a pose", &other)),
        }
    }

    async fn pose_series(&self, args: PoseArgs) -> Result<Value, String> {
        if args.target.trim().is_empty() {
            return Err("`target` is required: a UI node `scope/id`, `entity:Name`, or `player`".into());
        }
        let ticks = args.ticks.unwrap_or(30).clamp(1, 2000);
        let every = args.every.unwrap_or(1).clamp(1, 600);
        // The answer comes after the last sample: at 30 fps that is `ticks × every / 30` seconds, plus the usual patience.
        let frames = u64::from(ticks) * u64::from(every);
        let wait = self.wait + Duration::from_millis(frames * 40);
        match self.ask_for(Request::Query(Query::PoseSeries { target: args.target.trim().to_string(), ticks, every }), wait).await? {
            Response::PoseSeries(d) => Ok(crate::pose::series_json(&d)),
            other => Err(unexpected("a pose series", &other)),
        }
    }

    async fn region(&self, args: RegionArgs) -> Result<Value, String> {
        let ops = crate::numbers::parts_of(&args.parts)?;
        let (min, max) = match (args.min, args.max) {
            (Some(a), Some(b)) => ([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]),
            (None, None) => {
                // Around a player: the position is the world's (`Query::World` lists the players the process knows, a dedicated server too).
                let radius = args.around_player.unwrap_or(16).clamp(1, 64);
                let world = match self.ask(Request::Query(Query::World)).await? {
                    Response::World(w) => w,
                    other => return Err(unexpected("the world", &other)),
                };
                let who = world.players.first().ok_or_else(|| String::from("there is no player in the world: give `min` and `max`"))?;
                let (px, py, pz) = (who.pos[0].floor() as i32, who.pos[1].floor() as i32, who.pos[2].floor() as i32);
                ([px - radius, py - 8, pz - radius], [px + radius, py + 24, pz + radius])
            }
            _ => return Err("give both `min` and `max`, or neither (then the box is around the player)".into()),
        };
        match self.ask(Request::Query(Query::Region { min, max, ops })).await? {
            Response::Region(s) => Ok(crate::numbers::summary_json_value(&s)),
            other => Err(unexpected("a region", &other)),
        }
    }

    async fn props(&self, args: PropsArgs) -> Result<Value, String> {
        let filter = args.filter.unwrap_or_default().trim().to_string();
        match self.ask(Request::Query(Query::Props { filter })).await? {
            Response::Props(d) => Ok(crate::numbers::props_json(&d)),
            other => Err(unexpected("the props", &other)),
        }
    }

    async fn analyze(&self, args: AnalyzeArgs) -> Result<Value, String> {
        let detail = crate::vision::Detail::parse(args.detail.as_deref())?;
        let ops = crate::vision::parse_ops(&args.ops, args.region.as_deref())?;
        match self.ask(Request::Query(Query::Image { source: ImageSource::Screenshot, ops: ops.clone() })).await? {
            Response::Image(results) => Ok(crate::vision::results_json(&ops, &results, detail)),
            other => Err(unexpected("image analysis results", &other)),
        }
    }

    async fn mod_state(&self, mod_id: String, ty: String, who: Option<String>) -> Result<Value, String> {
        match self.ask(Request::Query(Query::State { mod_id, ty, who })).await? {
            Response::State(s) => Ok(json!({ "mod": s.mod_id, "ty": s.ty, "state": s.state })),
            other => Err(unexpected("a mod state", &other)),
        }
    }
}

fn world_json(w: &fulcrum_dev_channel::message::WorldInfo) -> Value {
    json!({
        "name": w.name,
        "seed": w.seed,
        "dimension": w.dimension,
        "players": w.players.iter().map(|p| json!({ "name": p.name, "pos": p.pos })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str, action: Option<&str>, mods: &[&str]) -> KeyArgs {
        KeyArgs { key: k.into(), action: action.map(str::to_string), modifiers: mods.iter().map(|m| m.to_string()).collect(), hold_ms: None }
    }

    fn mouse(action: &str) -> MouseArgs {
        MouseArgs { action: action.into(), x: None, y: None, to_x: None, to_y: None, button: None, steps: None, duration_ms: None, dx: None, dy: None, unit: None }
    }

    #[test]
    fn a_key_press_wraps_its_modifiers_down_before_and_up_after() {
        let b = key_batch(&key("S", None, &["ctrl", "shift"])).unwrap();
        let seq: Vec<(&str, KeyAction)> = b.keys.iter().map(|k| (k.key.as_str(), k.action)).collect();
        assert_eq!(
            seq,
            vec![
                ("ctrl", KeyAction::Down),
                ("shift", KeyAction::Down),
                ("S", KeyAction::Tap),
                ("shift", KeyAction::Up),
                ("ctrl", KeyAction::Up)
            ]
        );
    }

    #[test]
    fn a_held_key_keeps_its_modifiers_down() {
        let b = key_batch(&key("W", Some("down"), &["shift"])).unwrap();
        assert_eq!(b.keys.len(), 2);
        assert_eq!(b.keys[1], KeyPress { key: "W".into(), action: KeyAction::Down });
        assert!(key_batch(&key("W", Some("hold"), &[])).unwrap_err().contains("unknown key action"));
    }

    #[test]
    fn click_and_double_click_move_first_and_need_a_point() {
        let mut m = mouse("double_click");
        assert!(mouse_batch(&m).unwrap_err().contains("`x` and `y`"));
        m.x = Some(10.0);
        m.y = Some(20.0);
        let b = mouse_batch(&m).unwrap();
        assert_eq!(
            b.mouse,
            vec![MouseOp::MoveTo { x: 10.0, y: 20.0 }, MouseOp::Click { button: MouseButton::Left }, MouseOp::Click { button: MouseButton::Left }]
        );
    }

    #[test]
    fn a_wheel_needs_dy_and_a_button_the_channel_has() {
        assert!(mouse_batch(&mouse("wheel")).unwrap_err().contains("`dy`"));
        let mut m = mouse("wheel");
        m.dy = Some(-2.0);
        assert_eq!(mouse_batch(&m).unwrap().mouse, vec![MouseOp::Scroll { dy: -2.0 }]);
        let mut c = mouse("click");
        c.x = Some(1.0);
        c.y = Some(1.0);
        c.button = Some("back".into());
        assert!(mouse_batch(&c).unwrap_err().contains("left, right and middle"));
        assert!(mouse_batch(&mouse("tickle")).unwrap_err().contains("unknown mouse action"));
    }

    #[test]
    fn an_action_becomes_a_console_line() {
        assert_eq!(action_line("cozy", None), "cozy");
        assert_eq!(action_line("cozy", Some(&json!("on"))), "cozy on");
        assert_eq!(action_line(" tp ", Some(&json!([1, 2, 3]))), "tp [1,2,3]");
        assert_eq!(action_line("x", Some(&Value::Null)), "x");
    }

    #[test]
    fn the_hud_value_keeps_its_type() {
        assert_eq!(hud_value(&HudValue::Int(3)), json!(3));
        assert_eq!(hud_value(&HudValue::Text("a".into())), json!("a"));
        assert_eq!(hud_value(&HudValue::Unset), Value::Null);
        assert_eq!(hud_value(&HudValue::Bool(true)), json!(true));
    }

    #[test]
    fn the_dev_link_declares_no_debugger_capability() {
        // `capabilities` does not need a live client, so the line is held on a plain list too.
        let declared = [
            Capability::Info, Capability::Screenshot, Capability::Input, Capability::Command, Capability::Hud, Capability::Chat,
            Capability::Mods, Capability::World, Capability::ModState, Capability::UiTree, Capability::ImageAnalysis, Capability::Pose, Capability::Region,
            Capability::Props,
        ];
        assert!(declared.iter().all(|c| c.belongs_to(TargetKind::Game)));
    }
}
