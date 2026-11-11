//! The `dev` link against a **fake game**: a real `ChannelServer` (the transport the fulcrum games use) on a
//! loopback port, answering the typed protocol with canned replies. No game, no window, no Arbor: what is
//! proved here is that the link says the right thing on the wire and reads the answers back into the shapes
//! the tools promise. The same game is what a person starts by hand (`voriz --mute --dev-channel`) for the
//! one check that needs the real thing — see `docs/arbor-gioco-collegato.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bennu_link::prelude::*;
use fulcrum_debug_channel::prelude::{ChannelEvent, ChannelServer, ServerConfig};
use fulcrum_debug_channel::topic::DEV;
use fulcrum_dev_channel::message as m;
use fulcrum_dev_channel::TOPIC;
use fulcrum_vision::image::{Image as VImage, Rect as VRect};
use fulcrum_vision::ops::run_all;
use fulcrum_vision::region::NoEnv;
use serde_json::json;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("bennu-link-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What the fake answers to each request.
fn answer(request: &m::Request) -> m::Response {
    match request {
        m::Request::Cmd { line, caller } if line == "boom" => {
            let _ = caller;
            m::Response::Cmd(m::CmdResult { ok: false, lines: vec!["no such verb".into()] })
        }
        m::Request::Cmd { line, caller } => m::Response::Cmd(m::CmdResult { ok: true, lines: vec![format!("ran `{line}` as {caller:?}")] }),
        m::Request::Query(m::Query::State { mod_id, ty, who }) => {
            m::Response::State(m::StateDump { mod_id: mod_id.clone(), ty: ty.clone(), state: format!("Run {{ who: {who:?} }}") })
        }
        m::Request::Query(m::Query::Hud { mod_id }) => m::Response::Hud(m::HudDump {
            entries: vec![
                m::HudEntry { mod_id: mod_id.clone().unwrap_or_else(|| "obby".into()), key: "stage".into(), value: m::HudValue::Int(3), writes: 7 },
                m::HudEntry { mod_id: "obby".into(), key: "label".into(), value: m::HudValue::Text("go".into()), writes: 1 },
            ],
        }),
        m::Request::Query(m::Query::Chat { last }) => m::Response::Chat(m::ChatDump {
            chat: vec![m::ChatLine { to: Some(2), key: "welcome".into(), params: vec![("name".into(), "Anna".into())] }],
            console: vec![format!("last={last}")],
        }),
        m::Request::Query(m::Query::Mods) => m::Response::Mods(vec![
            m::ModInfo { id: "cozy".into(), enabled: true, note: String::new(), hooks: 4, faults: 0 },
            m::ModInfo { id: "broken".into(), enabled: false, note: "a hook failed".into(), hooks: 1, faults: 3 },
        ]),
        m::Request::Query(m::Query::World) => m::Response::World(m::WorldInfo {
            name: "Prova".into(),
            seed: Some(42),
            dimension: "overworld".into(),
            players: vec![m::PlayerInfo { name: "Anna".into(), pos: [1.0, 64.0, -3.5] }],
        }),
        m::Request::Query(m::Query::Ui { screen, visible_only }) => {
            let node = |scope: &str, id: &str, visible: bool, alpha: f32| m::UiNode {
                scope: scope.into(),
                id: id.into(),
                text: Some(format!("texto di {id}")),
                rect: [896.0, 20.0, 120.0, 28.0],
                visible,
                alpha,
                bg: Some("#1e1715".into()),
                ink: None,
            };
            let mut nodes = vec![node("cozy", "pill", true, 0.7), node("cozy", "ghost", false, 1.0), node("pausa", "riprendi", true, 1.0)];
            nodes.retain(|n| (n.visible || !visible_only) && screen.as_deref().map_or(true, |s| s == n.scope));
            m::Response::Ui(m::UiDump { viewport: [1280.0, 720.0], scale: 2.0, flags: vec!["at-aperto".into()], nodes })
        }
        m::Request::Query(m::Query::Image { source, ops }) => match source {
            m::ImageSource::Path(p) => m::Response::Error(format!("il fake non apre file: {p}")),
            // Uno schermo finto: la metà sinistra cacao, la destra ambra, con un riquadro bianco in alto a destra.
            m::ImageSource::Screenshot => {
                let mut img = VImage::from_fn(64, 32, |x, _| if x < 32 { [92, 58, 40, 255] } else { [230, 160, 40, 255] });
                img.fill_rect(VRect::new(48, 0, 16, 4), [255, 255, 255, 255]);
                m::Response::Image(run_all(&img, &ops, &NoEnv))
            }
        },
        // v3: la posa a numeri. Un banner che cala (scala 2 → 1 in 0.3 s) e sta fermo.
        m::Request::Query(m::Query::Pose { target }) => m::Response::Pose(m::PoseDump {
            target: target.clone(),
            kind: m::PoseKind::Ui,
            keys: vec!["tx".into(), "sx".into(), "alpha".into()],
            values: vec![0.0, 1.5, 1.0],
            anims: vec![m::PoseAnim { name: "preset/banner-drop".into(), t: 0.1, length: 2.8, waiting: false }],
            notes: vec![],
        }),
        m::Request::Query(m::Query::PoseSeries { target, ticks, every }) => m::Response::PoseSeries(m::PoseSeriesDump {
            target: target.clone(),
            kind: m::PoseKind::Ui,
            keys: vec!["sx".into(), "alpha".into()],
            samples: (0..*ticks)
                .map(|i| {
                    let t = (i * every) as f32 / 60.0;
                    m::PoseSample { frame: i * every, secs: t, values: vec![1.0 + (1.0 - (t / 0.3).min(1.0)).powi(2), 1.0] }
                })
                .collect(),
            anims: vec!["preset/banner-drop".into()],
            notes: vec![],
        }),
        // v4: il mondo a numeri. Una regione finta (pavimento di pietra con un tronco in un angolo) e due prop.
        m::Request::Query(m::Query::Region { min, max, ops }) => {
            use fulcrum_map_scan::prelude::*;
            let mut t = Terrain::new("finta");
            for x in min[0]..=max[0] {
                for z in min[2]..=max[2] {
                    t.add(x, z, 0, SLABS, "stone");
                }
            }
            t.add(min[0], min[2], SLABS, 3 * SLABS, "log");
            m::Response::Region(t.summary(&Options { parts: ops.clone(), ..Options::default() }))
        }
        m::Request::Query(m::Query::Props { filter }) => {
            let all = vec![
                m::PropEntry { source: "script".into(), name: "meta".into(), id: "cozy:meta".into(), cell: [3, 64, -2], pos: [3.5, 64.0, -1.5], yaw: 90.0, min: [3.0, 64.0, -2.0], max: [4.0, 67.0, -1.0] },
                m::PropEntry { source: "block".into(), name: "vaso".into(), id: "prop_vaso_n".into(), cell: [0, 1, 0], pos: [0.5, 1.0, 0.5], yaw: 0.0, min: [0.0; 3], max: [0.0; 3] },
            ];
            m::Response::Props(m::PropsDump { entries: all.into_iter().filter(|e| filter.is_empty() || e.name.contains(filter.as_str())).collect(), notes: vec![] })
        }
        // Le interrogazioni di versioni più nuove del protocollo: questo finto non le conosce.
        #[allow(unreachable_patterns)]
        m::Request::Query(_) => m::Response::Error("il fake non risponde a questa interrogazione".into()),
        m::Request::Screenshot { max_side } => {
            m::Response::Screenshot(m::Screenshot { width: *max_side.max(&1), height: 2, format: m::ImageFormat::Png, data: vec![1, 2, 3, 4] })
        }
        m::Request::Input(input) => {
            if let Some(bad) = input.keys.iter().find(|k| k.key == "Nope") {
                return m::Response::Error(format!("tasto sconosciuto: «{}»", bad.key));
            }
            m::Response::InputQueued {
                keys: input.keys.len() as u32,
                chars: input.text.as_deref().map_or(0, |t| t.chars().count() as u32),
                mouse: input.mouse.len() as u32,
            }
        }
    }
}

struct FakeGame {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    dir: TempDir,
    port: u16,
    received: Arc<Mutex<Vec<m::Request>>>,
}

impl FakeGame {
    fn start(game_id: &str) -> FakeGame {
        let dir = TempDir::new();
        let config = ServerConfig::enabled().with_game_id(game_id).with_discovery_dir(&dir.0).with_topics([TOPIC]);
        let mut server = ChannelServer::start(config).expect("the fake game listens");
        let port = server.local_addr().expect("a bound address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let received = Arc::new(Mutex::new(Vec::new()));
        let (flag, seen) = (Arc::clone(&stop), Arc::clone(&received));
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                for event in server.poll() {
                    if let ChannelEvent::Message { client, topic, payload } = event {
                        if topic != DEV {
                            continue;
                        }
                        let message: m::ClientMsg = postcard::from_bytes(&payload).expect("a message of the dev topic");
                        if let m::ClientMsg::Request { id, request } = message {
                            seen.lock().unwrap().push(request.clone());
                            let reply = m::ServerMsg::Response { id, response: answer(&request) };
                            server.send_value(client, DEV, &reply);
                        }
                    }
                }
                server.flush();
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        FakeGame { stop, thread: Some(thread), dir, port, received }
    }

    fn target(&self) -> DevTarget {
        DevTarget { game: None, port: None, dir: Some(self.dir.0.clone()) }
    }

    async fn link(&self) -> DevChannelLink {
        DevChannelLink::connect(self.target()).await.expect("the link connects").with_wait(Duration::from_secs(5))
    }

    fn requests(&self) -> Vec<m::Request> {
        self.received.lock().unwrap().clone()
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for FakeGame {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn key(k: &str, action: Option<&str>, mods: &[&str]) -> KeyArgs {
    KeyArgs { key: k.into(), action: action.map(str::to_string), modifiers: mods.iter().map(|s| s.to_string()).collect(), hold_ms: None }
}

fn mouse(action: &str) -> MouseArgs {
    MouseArgs { action: action.into(), x: None, y: None, to_x: None, to_y: None, button: None, steps: None, duration_ms: None, dx: None, dy: None, unit: None }
}

#[tokio::test]
async fn the_game_is_found_in_the_discovery_folder_and_listed() {
    let game = FakeGame::start("fakegame");
    let found = list_games(Some(&game.dir.0));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].game, "fakegame");
    assert_eq!(found[0].port, game.port);
    assert!(found[0].compatible);
    assert!(found[0].topics.contains(&format!("dev={}", fulcrum_dev_channel::VERSION)), "{}", found[0].topics);
}

#[tokio::test]
async fn connecting_by_id_by_port_and_with_nothing_there() {
    let game = FakeGame::start("alpha");
    let by_id = DevChannelLink::connect(DevTarget { game: Some("alpha".into()), port: None, dir: Some(game.dir.0.clone()) }).await.unwrap();
    assert_eq!(by_id.game(), "alpha");
    assert_eq!(by_id.port(), game.port);
    let by_port = DevChannelLink::connect(DevTarget { game: None, port: Some(game.port), dir: None }).await.unwrap();
    assert_eq!(by_port.port(), game.port);

    let empty = TempDir::new();
    let err = DevChannelLink::connect(DevTarget { game: None, port: None, dir: Some(empty.0.clone()) }).await.err().unwrap();
    assert!(err.contains("--dev-channel") && err.contains("No game answers"), "{err}");
    let err = DevChannelLink::connect(DevTarget { game: Some("ghost".into()), port: None, dir: Some(game.dir.0.clone()) }).await.err().unwrap();
    assert!(err.contains("No game answers"), "{err}");
}

#[tokio::test]
async fn two_games_without_a_choice_ask_for_one() {
    let a = FakeGame::start("one");
    // A second game in the same folder: same discovery directory, another id.
    let config = ServerConfig::enabled().with_game_id("two").with_discovery_dir(&a.dir.0).with_topics([TOPIC]);
    let mut other = ChannelServer::start(config).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let t = std::thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            other.poll();
            other.flush();
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    let err = DevChannelLink::connect(a.target()).await.err().unwrap();
    assert!(err.contains("More than one game") && err.contains("one") && err.contains("two"), "{err}");
    stop.store(true, Ordering::SeqCst);
    t.join().unwrap();
}

#[tokio::test]
async fn the_link_declares_a_game_and_nothing_a_debugger_has() {
    let game = FakeGame::start("caps");
    let link = game.link().await;
    assert_eq!(link.kind(), TargetKind::Game);
    assert_eq!(link.protocol(), "dev");
    let caps = link.capabilities();
    assert!(caps.iter().all(|c| c.belongs_to(TargetKind::Game)), "{caps:?}");
    assert!(link.supports(Capability::Command) && link.supports(Capability::Screenshot));
    assert!(!link.supports(Capability::Ecs) && !link.supports(Capability::Pause) && !link.supports(Capability::Stats));
    let summary = summarize(&link);
    assert_eq!(summary.protocol, "dev");
    assert!(summary.label.contains(&game.port.to_string()), "{}", summary.label);
}

#[tokio::test]
async fn a_command_runs_as_the_player_or_as_the_console_and_a_failure_is_data() {
    let game = FakeGame::start("cmd");
    let link = game.link().await;
    let ok = link.command(CommandArgs { line: "/cozy on".into(), as_console: None }).await.unwrap();
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["lines"][0], "ran `cozy on` as Host");
    let console = link.command(CommandArgs { line: "weather rain".into(), as_console: Some(true) }).await.unwrap();
    assert_eq!(console["lines"][0], "ran `weather rain` as Console");
    let failed = link.command(CommandArgs { line: "boom".into(), as_console: None }).await.unwrap();
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["lines"][0], "no such verb");
    assert!(link.command(CommandArgs { line: "  ".into(), as_console: None }).await.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn a_named_action_is_run_as_a_console_line() {
    let game = FakeGame::start("act");
    let link = game.link().await;
    let r = link.action(ActionArgs { name: "tp".into(), arg: Some(json!("home")) }).await.unwrap();
    assert_eq!(r["lines"][0], "ran `tp home` as Host");
    let none = link.actions().await.unwrap_err();
    assert!(none.contains("no registry of named actions") && none.contains("bevy_game_command"), "{none}");
}

#[tokio::test]
async fn key_text_and_mouse_reach_the_game_in_the_order_the_game_replays_them() {
    let game = FakeGame::start("input");
    let link = game.link().await;
    let r = link.key(key("S", None, &["ctrl"])).await.unwrap();
    assert_eq!(r["queued"]["keys"], 3);
    let r = link.text(TextArgs { text: "ciao".into(), interval_ms: Some(5) }).await.unwrap();
    assert_eq!(r["queued"]["chars"], 4);
    assert!(link.text(TextArgs { text: String::new(), interval_ms: None }).await.unwrap_err().contains("1 to 500"));
    let mut click = mouse("click");
    click.x = Some(5.0);
    click.y = Some(6.0);
    let r = link.mouse(click).await.unwrap();
    assert_eq!(r["queued"]["mouse"], 2);

    let sent = game.requests();
    let m::Request::Input(first) = &sent[0] else { panic!("{sent:?}") };
    assert_eq!(first.keys[0], m::KeyPress { key: "ctrl".into(), action: m::KeyAction::Down });
    assert_eq!(first.keys[1], m::KeyPress { key: "S".into(), action: m::KeyAction::Tap });
    assert_eq!(first.keys[2], m::KeyPress { key: "ctrl".into(), action: m::KeyAction::Up });
    let m::Request::Input(text) = &sent[1] else { panic!("{sent:?}") };
    assert_eq!(text.text.as_deref(), Some("ciao"));
}

#[tokio::test]
async fn an_unknown_key_comes_back_as_the_games_own_sentence() {
    let game = FakeGame::start("badkey");
    let link = game.link().await;
    let err = link.key(key("Nope", None, &[])).await.unwrap_err();
    assert!(err.contains("tasto sconosciuto"), "{err}");
}

#[tokio::test]
async fn a_drag_is_a_press_a_walk_and_a_release() {
    let game = FakeGame::start("drag");
    let link = game.link().await;
    let mut drag = mouse("drag");
    drag.x = Some(0.0);
    drag.y = Some(0.0);
    drag.to_x = Some(100.0);
    drag.to_y = Some(50.0);
    drag.steps = Some(4);
    drag.duration_ms = Some(64);
    let r = link.mouse(drag).await.unwrap();
    assert_eq!(r["dragged"], true);
    let sent = game.requests();
    // The press, four moves, the release.
    assert_eq!(sent.len(), 6, "{sent:?}");
    let ops = |i: usize| match &sent[i] {
        m::Request::Input(input) => input.mouse.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(ops(0), vec![m::MouseOp::MoveTo { x: 0.0, y: 0.0 }, m::MouseOp::Down { button: m::MouseButton::Left }]);
    assert_eq!(ops(4), vec![m::MouseOp::MoveTo { x: 100.0, y: 50.0 }]);
    assert_eq!(ops(5), vec![m::MouseOp::Up { button: m::MouseButton::Left }]);
}

#[tokio::test]
async fn a_drag_without_its_end_says_what_is_missing() {
    let game = FakeGame::start("dragbad");
    let link = game.link().await;
    let mut drag = mouse("drag");
    drag.x = Some(0.0);
    drag.y = Some(0.0);
    assert!(link.mouse(drag).await.unwrap_err().contains("`to_x` and `to_y`"));
    assert!(game.requests().is_empty(), "nothing was sent");
}

#[tokio::test]
async fn a_screenshot_comes_back_as_base64_png_with_its_size() {
    use base64::Engine as _;
    let game = FakeGame::start("shot");
    let link = game.link().await;
    let shot = link.screenshot(ScreenshotArgs { max_width: Some(640), window: None }).await.unwrap();
    assert_eq!(shot.mime_type, "image/png");
    assert_eq!((shot.width, shot.height), (640, 2));
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(shot.data).unwrap(), vec![1, 2, 3, 4]);
    let m::Request::Screenshot { max_side } = game.requests()[0].clone() else { panic!() };
    assert_eq!(max_side, 640);
}

#[tokio::test]
async fn the_queries_come_back_in_the_shapes_the_tools_promise() {
    let game = FakeGame::start("queries");
    let link = game.link().await;

    let hud = link.hud(Some("obby".into())).await.unwrap();
    assert_eq!(hud["entries"][0], json!({ "mod": "obby", "key": "stage", "value": 3, "writes": 7 }));
    assert_eq!(hud["entries"][1]["value"], "go");

    let chat = link.chat(500).await.unwrap();
    assert_eq!(chat["chat"][0], json!({ "to": 2, "key": "welcome", "params": { "name": "Anna" } }));
    assert_eq!(chat["console"][0], "last=200", "the count is clamped before it is sent");

    let mods = link.mods().await.unwrap();
    assert_eq!(mods[1], json!({ "id": "broken", "enabled": false, "note": "a hook failed", "hooks": 1, "faults": 3 }));

    let world = link.world().await.unwrap();
    assert_eq!(world["name"], "Prova");
    assert_eq!(world["seed"], 42);
    assert_eq!(world["players"][0]["pos"], json!([1.0, 64.0, -3.5]));

    let state = link.mod_state("cozy".into(), "Run".into(), Some("Anna".into())).await.unwrap();
    assert_eq!(state["mod"], "cozy");
    assert!(state["state"].as_str().unwrap().contains("Anna"));
}

#[tokio::test]
async fn the_ui_tree_comes_back_compact_and_filtered_by_the_game() {
    let game = FakeGame::start("uitree");
    let link = game.link().await;
    let tree = link.ui_tree(UiArgs::default()).await.unwrap();
    // By default only what is visible: the ghost is not listed.
    assert_eq!(tree["count"], 2);
    assert_eq!(tree["nodes"][0]["n"], "cozy/pill");
    assert_eq!(tree["nodes"][0]["a"], json!(0.7));
    assert!(tree["nodes"][0].get("v").is_none(), "a visible node carries no `v`");
    assert_eq!(tree["scale"], json!(2.0));
    assert_eq!(tree["nodes"][0]["r"], json!([896.0, 20.0, 120.0, 28.0]));
    assert_eq!(tree["flags"][0], "at-aperto");
    let all = link.ui_tree(UiArgs { screen: Some("cozy".into()), include_hidden: Some(true) }).await.unwrap();
    assert_eq!(all["count"], 2);
    assert_eq!(all["nodes"][1]["v"], json!(false));
    assert!(all.get("flags").is_none(), "a narrow query does not repeat the flags");
    let sent = game.requests();
    assert_eq!(sent[0], m::Request::Query(m::Query::Ui { screen: None, visible_only: true }));
    assert_eq!(sent[1], m::Request::Query(m::Query::Ui { screen: Some("cozy".into()), visible_only: false }));
}

#[tokio::test]
async fn the_pose_comes_back_as_numbers_and_a_series_brings_its_summary() {
    let game = FakeGame::start("pose");
    let link = game.link().await;
    assert!(link.supports(Capability::Pose) && Capability::Pose.belongs_to(TargetKind::Game));
    let now = link.pose(PoseArgs { target: "hud/banner".into(), ticks: None, every: None }).await.unwrap();
    assert_eq!(now["pose"]["sx"], json!(1.5));
    assert_eq!(now["anims"][0]["name"], "preset/banner-drop");
    let series = link.pose_series(PoseArgs { target: "hud/banner".into(), ticks: Some(30), every: Some(2) }).await.unwrap();
    assert_eq!(series["rows"].as_array().unwrap().len(), 30);
    assert_eq!(series["columns"], json!(["frame", "secs", "sx", "alpha"]));
    // The scale falls from 2 to 1 and stays: the summary says so; the opacity never moves and is listed once as still.
    assert_eq!(series["summary"]["channels"][0]["key"], "sx");
    assert_eq!(series["summary"]["channels"][0]["from"], json!(2.0));
    assert_eq!(series["still"], json!(["alpha"]));
    let sent = game.requests();
    assert_eq!(sent[0], m::Request::Query(m::Query::Pose { target: "hud/banner".into() }));
    assert_eq!(sent[1], m::Request::Query(m::Query::PoseSeries { target: "hud/banner".into(), ticks: 30, every: 2 }));
    // An empty target never reaches the game.
    assert!(link.pose(PoseArgs::default()).await.unwrap_err().contains("target"));
    assert_eq!(game.requests().len(), 2);
}

#[tokio::test]
async fn an_analysis_asks_the_game_for_results_and_never_for_the_picture() {
    let game = FakeGame::start("analyze");
    let link = game.link().await;
    let a = link
        .analyze(AnalyzeArgs { ops: vec!["stats".into(), "palette:2".into(), "bbox:#ffffff".into()], region: None, detail: None })
        .await
        .unwrap();
    let rows = a["results"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows[0]["summary"].as_str().unwrap().contains("regione 64x32"), "{}", rows[0]);
    assert!(rows[1]["summary"].as_str().unwrap().contains("cacao"), "{}", rows[1]);
    assert!(rows[2]["summary"].as_str().unwrap().contains("x48 y0 16x4"), "{}", rows[2]);
    let j = link.analyze(AnalyzeArgs { ops: vec!["count:#ffffff".into()], region: Some("0.5,0,1,0.5".into()), detail: Some("json".into()) }).await.unwrap();
    assert_eq!(j["results"][0]["data"]["count"]["count"], 64, "{j}");
    assert!(j["results"][0].get("summary").is_none());
    // Nothing but the query crossed: no screenshot request.
    assert!(game.requests().iter().all(|r| matches!(r, m::Request::Query(m::Query::Image { .. }))), "{:?}", game.requests());
    let m::Request::Query(m::Query::Image { source, ops }) = &game.requests()[1] else { panic!() };
    assert_eq!(*source, m::ImageSource::Screenshot);
    assert_eq!(ops.len(), 1);
}

#[tokio::test]
async fn a_bad_operation_is_refused_before_it_reaches_the_game() {
    let game = FakeGame::start("badop");
    let link = game.link().await;
    assert!(link.analyze(AnalyzeArgs { ops: vec!["boh".into()], region: None, detail: None }).await.unwrap_err().contains("sconosciuta"));
    assert!(link.analyze(AnalyzeArgs { ops: vec!["stats".into()], region: None, detail: Some("verbose".into()) }).await.unwrap_err().contains("unknown detail"));
    assert!(game.requests().is_empty());
}

#[tokio::test]
async fn the_dev_link_declares_the_new_capabilities_and_a_link_without_them_says_so() {
    let game = FakeGame::start("capsnew");
    let link = game.link().await;
    assert!(link.supports(Capability::UiTree) && link.supports(Capability::ImageAnalysis));
    assert!(Capability::UiTree.belongs_to(TargetKind::Game) && Capability::ImageAnalysis.belongs_to(TargetKind::Game));
}

#[tokio::test]
async fn info_names_the_game_its_capabilities_and_what_the_channel_does_not_have() {
    let game = FakeGame::start("infogame");
    let link = game.link().await;
    let info = link.info().await.unwrap();
    assert_eq!(info["protocol"], "dev");
    assert_eq!(info["game"], "infogame");
    assert_eq!(info["world"]["name"], "Prova");
    let caps: Vec<&str> = info["capabilities"].as_array().unwrap().iter().filter_map(|c| c.as_str()).collect();
    assert!(caps.contains(&"command") && !caps.contains(&"ecs"));
    assert!(info["notes"].to_string().contains("no ECS"));
}

#[tokio::test]
async fn what_the_channel_does_not_have_is_said_in_a_sentence_that_names_the_way_out() {
    let game = FakeGame::start("nope");
    let link = game.link().await;
    let stats = link.stats(StatsArgs::default()).await.unwrap_err();
    assert!(stats.contains("dev link") && stats.contains("has no frame statistics"), "{stats}");
    assert!(stats.contains("It offers:") && stats.contains("command"), "{stats}");
    let actions = link.actions().await.unwrap_err();
    assert!(actions.contains("bevy_game_command"), "{actions}");
    assert!(game.requests().is_empty(), "an unsupported call never reaches the game");
}

#[tokio::test]
async fn a_game_that_goes_away_answers_with_a_sentence_not_a_hang() {
    let mut game = FakeGame::start("gone");
    let link = game.link().await.with_wait(Duration::from_secs(2));
    assert!(link.world().await.is_ok());
    game.shut_down();
    let err = link.world().await.unwrap_err();
    assert!(err.contains("game"), "{err}");
}

#[tokio::test]
async fn the_session_slot_holds_one_link_and_the_choice() {
    let game = FakeGame::start("slot");
    let link = Arc::new(game.link().await);
    assert!(bennu_link::session::dev().is_none());
    bennu_link::session::set_dev(Arc::clone(&link));
    assert_eq!(bennu_link::session::dev().unwrap().port(), game.port);
    assert_eq!(link.is_connected(), Some(true));
    bennu_link::session::set_preference(Protocol::Dev);
    assert_eq!(bennu_link::session::preference(), Protocol::Dev);
    assert_eq!(pick(bennu_link::session::preference(), bennu_link::session::dev().is_some(), false), Ok(Pick::Dev));
    assert!(bennu_link::session::clear_dev());
    assert!(!bennu_link::session::clear_dev());
    bennu_link::session::set_preference(Protocol::Auto);
}

#[tokio::test]
async fn a_region_comes_back_as_a_summary_and_around_the_player_it_asks_his_position_first() {
    let game = FakeGame::start("region");
    let link = game.link().await;
    assert!(link.supports(Capability::Region) && Capability::Region.belongs_to(TargetKind::Game));
    let v = link.region(RegionArgs { min: Some([4, 0, 0]), max: Some([0, 1, 3]), parts: vec!["counts".into()], ..Default::default() }).await.unwrap();
    // The corners were given the other way round: the link puts them in order.
    assert_eq!(v["size"], json!([5, 3, 4]));
    assert_eq!(v["kinds"]["stone"], json!(20), "{v}");
    assert_eq!(v["kinds"]["log"], json!(2), "{v}");
    assert!(v.get("heights").is_none(), "only the counts were asked for");
    assert!(v["sentence"].as_str().unwrap().contains("5×3×4"), "{v}");
    let sent = game.requests();
    assert_eq!(sent[0], m::Request::Query(m::Query::Region { min: [0, 0, 0], max: [4, 1, 3], ops: vec![fulcrum_map_scan::summary::Part::Counts] }));
    // No corners: the box is around the player.
    link.region(RegionArgs::default()).await.unwrap();
    let sent = game.requests();
    assert_eq!(sent[1], m::Request::Query(m::Query::World));
    // The fake world has one player, at (1, 64, -3.5): floor = 1, 64, -4; radius 16, 8 below, 24 above.
    assert_eq!(sent[2], m::Request::Query(m::Query::Region { min: [-15, 56, -20], max: [17, 88, 12], ops: vec![] }));
    // One corner only, or a part that does not exist: refused before the wire.
    assert!(link.region(RegionArgs { min: Some([0; 3]), ..Default::default() }).await.unwrap_err().contains("both"));
    assert!(link.region(RegionArgs { parts: vec!["nope".into()], ..Default::default() }).await.unwrap_err().contains("not a part"));
    assert_eq!(game.requests().len(), 3);
}

#[tokio::test]
async fn the_props_come_back_with_their_boxes_and_the_filter_goes_to_the_game() {
    let game = FakeGame::start("props");
    let link = game.link().await;
    assert!(link.supports(Capability::Props) && Capability::Props.belongs_to(TargetKind::Game));
    let all = link.props(PropsArgs::default()).await.unwrap();
    assert_eq!(all["props"].as_array().unwrap().len(), 2);
    assert_eq!(all["props"][0]["size"], json!([1.0, 3.0, 1.0]));
    assert!(all["sentence"].as_str().unwrap().contains("tallest: meta 3.0 m"), "{all}");
    let one = link.props(PropsArgs { filter: Some(" vaso ".into()) }).await.unwrap();
    assert_eq!(one["props"].as_array().unwrap().len(), 1);
    assert!(one["props"][0].get("size").is_none(), "a prop with unknown geometry carries no box");
    assert_eq!(game.requests()[1], m::Request::Query(m::Query::Props { filter: "vaso".into() }));
}
