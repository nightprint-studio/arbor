//! The neutral game tools: the MCP `bevy_game_*` family, each routed to whichever link is open.
//!
//! Before the `dev` channel these were the `mcp(...)` attribute of the Bevy-Remote-Protocol handlers in
//! `crate::brp::drive`; the names, the arguments and the safety classes are the same, and with only a BRP
//! game connected they do exactly what they did. What is new is the second link, a few tools only it can
//! answer (`command`, `hud`, `chat`, `mods`, `world`, `mod_state`), and the connecting gesture.
//!
//! ## Classification (what an MCP host allows, asks or denies)
//!
//! * `read` — looking: info, screenshot, stats, actions, status, world, mods, HUD, chat, mod state;
//! * `write` — the game reacts, nothing on disk changes: key, type, mouse, connect, disconnect;
//! * `destructive` — what the game does with it is its own business and may not be undoable: a named action
//!   and, above all, **a console command** (which with `as_console` has every permission group).
//!
//! The same line is drawn for Imber in `assistant/tools/brp.rs` (the `bevy` and `bevy_write` families, off by
//! default for writes, approved once per conversation).
//!
//! A tool whose link lacks the capability does not fail with a code: it answers the sentence
//! `bennu_link::unsupported` builds, naming the link, the missing capability, the way out and what the link
//! does offer.

use bennu_core::prelude::BennuState;
use bennu_link::prelude::{
    ActionArgs, AnalyzeArgs, ChatArgs, CommandArgs, GameConnectArgs, GameDisconnectArgs, HudArgs, ImageAnalyzeArgs, KeyArgs, MapPercorsoArgs,
    MapSchedaArgs, ModStateArgs, MouseArgs, PoseArgs, PropsArgs, RegionArgs, ScreenshotArgs, Shot, StatsArgs, TextArgs, UiArgs,
};
use serde_json::Value;

use super::{active, connect};

// ── Attaching ────────────────────────────────────────────────────────────────

/// Attach to a running game and remember how. `protocol` is `auto` (default: the fulcrum `dev` channel first,
/// then the Bevy Remote Protocol), `dev` or `brp`; the choice is kept, and the other `bevy_game_*` tools use
/// that link from now on. A Voriz/fulcrum game must run with `--dev-channel`; a Bevy game needs
/// `RemotePlugin` + `RemoteHttpPlugin`. Loopback only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_connect",
    title = "Attach to a running game (dev channel or Bevy Remote Protocol)",
    safety = write,
))]
async fn bennu_game_connect(ctx: &BennuState, args: GameConnectArgs) -> Result<Value, String> {
    connect::connect(ctx, args).await
}

/// Close the link to the game (`dev`, `brp`, or both). The game keeps running.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_disconnect",
    title = "Detach from the running game",
    safety = write,
))]
async fn bennu_game_disconnect(ctx: &BennuState, args: GameDisconnectArgs) -> Result<Value, String> {
    connect::disconnect(ctx, args).await
}

/// What is attached and what could be: the open links with their capabilities (what each can do — screenshot,
/// input, command, ECS…), the games found on the `dev` channel, and the debug sessions of the editor. Start
/// here when unsure which tool the game in front of you supports.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_status",
    title = "Which games and debuggers are attached, and what each can do",
    safety = read,
))]
async fn bennu_game_status(_ctx: &BennuState) -> Result<Value, String> {
    Ok(connect::status().await)
}

// ── Looking ──────────────────────────────────────────────────────────────────

/// What the link to the running game is and what the game offers: its protocol, capabilities, the key names it
/// accepts and, on the `dev` channel, the world.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_info",
    title = "What the running game lets you drive",
    safety = read,
))]
async fn bennu_game_info(_ctx: &BennuState) -> Result<Value, String> {
    active().await?.info().await
}

/// A screenshot of the running game's window. **An image is the most expensive way to look at a game** (a 1280×720
/// picture is thousands of tokens): use it only for what is purely visual and cannot be asked. To know what the
/// interface says or whether a panel is there, use `bevy_game_ui`; to know whether the look changed, what colours
/// dominate or whether an outline is continuous, use `bevy_game_analyze`; to check an animation or a movement, use
/// `bevy_game_pose` (numbers over time, with a summary). When you do need the picture, pass a small
/// `max_width` (500–800).
#[arbor_rpc::handler(mcp(
    name = "bevy_game_screenshot",
    title = "Screenshot of the running game",
    safety = read,
    output = image,
))]
async fn bennu_game_screenshot(_ctx: &BennuState, args: ScreenshotArgs) -> Result<Shot, String> {
    active().await?.screenshot(args).await
}

/// The game's **interface as data, without a picture**: every UI node with its identity (`scope/id`), the text it
/// shows (already in the player's language), its rectangle in window points, whether it is visible, its opacity
/// and its colours, plus the raised UI flags. **Prefer it to `bevy_game_screenshot`** for "does the banner say X?",
/// "is the cozy pill on screen and at what opacity?", "where is the button I want to click?" — the answer is a few
/// hundred bytes and exact. Narrow it with `screen` (`pausa`, `pausa/riprendi`, `hud/matassa*`): a whole HUD is
/// hundreds of nodes. By default only what the player can see; `include_hidden` adds the switched-off nodes.
/// `dev` channel (protocol 2) only; a game without UI documents answers an empty tree.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_ui",
    title = "The UI of the running game as data (instead of a screenshot)",
    safety = read,
))]
async fn bennu_game_ui(_ctx: &BennuState, args: UiArgs) -> Result<Value, String> {
    active().await?.ui_tree(args).await
}

/// **The pose of a UI node or of an entity as numbers, instead of a screenshot.** For a node `scope/id` (`hud/banner`):
/// the translation, scale and rotation written on its transform, its opacity, whether it is visible, its laid-out
/// rectangle and the animations running on it with their time. For an entity (`entity:Name`, or `player` where the
/// game offers it): position, angles, scale and, when the game knows them, velocity. **Prefer it to
/// `bevy_game_screenshot` for any animation or movement** ("does the life card shake when I am hurt and go back to
/// 3° of tilt?", "is the crystal spinning?"): without `ticks` it answers the pose right now in a few lines; with
/// `ticks` (and `every` frames between samples) the game samples N consecutive frames in ONE round trip and the
/// answer carries the table plus a summary — peak, overshoot, settling time, reversals, whether it returns to its
/// start, warnings (a NaN, a jump, a node left invisible). The same table a screenshot sheet of eight poses would
/// need thousands of tokens to show. `dev` channel (protocol 3) only; the numbers are the last finished frame.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_pose",
    title = "Pose of a UI node or entity as numbers (instead of a screenshot of the animation)",
    safety = read,
))]
async fn bennu_game_pose(_ctx: &BennuState, args: PoseArgs) -> Result<Value, String> {
    let link = active().await?;
    if args.ticks.is_some_and(|t| t > 1) {
        link.pose_series(args).await
    } else {
        link.pose(args).await
    }
}

/// **What is in a region of the live world, as numbers, instead of a screenshot.** Give a box in blocks (`min`/`max`)
/// or none (then it is the box around the local player, `around_player` blocks to each side): the game counts the
/// blocks by type and by layer, draws the **height profile** (one character per few columns, `' '` nothing → `'@'`
/// highest), finds the **enclosed voids** (air that no outside air reaches) and says it all in one sentence
/// ("world@(0,60,0)..(31,90,31) 32×31×32, 41% stone, nessun segno, 2 vuoti interni (30 celle)"). **Prefer it to
/// `bevy_game_screenshot` for "is the terrain there?", "what is this made of?", "is there a hole?", "did my
/// structure come out the size I wanted?"** — a summary is a few hundred bytes to ~2 KB, a screenshot ~1200 tokens
/// and no number. `parts` narrows it (`counts`, `layers`, `heights`, `sections`, `voids`, `marks`). Chunks not loaded
/// count as air and the answer says how many. Saved summaries can be compared with the Studio (`mappa scheda
/// --confronta`); the same shape comes from `map_scheda` for a map FILE. `dev` channel (protocol 4) only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_region",
    title = "Summary of a region of the live world as numbers (instead of a screenshot)",
    safety = read,
))]
async fn bennu_game_region(_ctx: &BennuState, args: RegionArgs) -> Result<Value, String> {
    active().await?.region(args).await
}

/// **Which models and props stand in the world, where and how big, instead of a screenshot.** For every prop: where it
/// comes from (`script`: a model a mod placed with `model.place`; `block`: a `prop_…` block of the voxel world), its
/// name, cell, rotation (yaw degrees) and **box in metres** (`min`, `max`, `size`) — enough to know *whether* the goal
/// stone is there, *where*, that it is 3 cells tall, or that the crystal floats 2 m above the pedestal. `filter` keeps
/// the names containing a fragment, or `mod:<id>` for one mod's models. Models live on the **client**: a dedicated
/// server answers the blocks only and says so in `notes`. A script model's box comes from its skins at rest (the
/// running clip is not followed). `dev` channel (protocol 4) only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_props",
    title = "Models and props placed in the world: name, cell, rotation, box (instead of a screenshot)",
    safety = read,
))]
async fn bennu_game_props(_ctx: &BennuState, args: PropsArgs) -> Result<Value, String> {
    active().await?.props(args).await
}

/// Resolve a map path the caller gave: scope-checked inside `root` when it names one, as it is otherwise.
fn scoped_map_path(root: Option<&str>, path: &str) -> Result<String, String> {
    match root.map(str::trim).filter(|r| !r.is_empty()) {
        Some(root) => crate::project_paths::in_root(root, path),
        None => Ok(path.to_string()),
    }
}

/// **The numbers of a map FILE, instead of an isometric photo of it** (no game needed): size, blocks by type and by
/// layer, the height profile, enclosed voids, the marks (`start`, `checkpoint:3`…) with their positions, the props with
/// their cells, unknown block names, and one sentence ("track 293×33×9, 30% mossy_cobblestone, 9 segni, nessun blocco
/// sconosciuto"). With `compare` (another map, or a summary saved earlier) it answers what changed from `path` to it:
/// blocks by type, layers, marks moved or added, props. Reads the `.fmap.ron` (or the source next to a cooked `.fmap`).
/// The same shape as `bevy_game_region`, so a map and the live world built from it can be compared.
#[arbor_rpc::handler(mcp(
    name = "map_scheda",
    title = "Numbers of a map file: size, blocks, heights, voids, marks (instead of a photo)",
    safety = read,
))]
async fn bennu_map_scheda(_ctx: &BennuState, args: MapSchedaArgs) -> Result<Value, String> {
    let mut args = args;
    args.path = scoped_map_path(args.root.as_deref(), &args.path)?;
    if let Some(other) = args.compare.clone().filter(|c| !c.trim().is_empty()) {
        args.compare = Some(scoped_map_path(args.root.as_deref(), &other)?);
    }
    tokio::task::spawn_blocking(move || bennu_link::prelude::map_scheda(&args))
        .await
        .map_err(|e| format!("the map task failed: {e}"))?
}

/// **Can the player walk from A to B on this map? — without playing it.** Uses the game's body (jump 1.26 blocks, a
/// running jump reaches 3.7 on level ground, a 0.6 step, 1.8 tall; override with `jump`, `run`, `step`, `pad`) and the
/// map file's blocks: from a mark or a position to a mark or a chain (`checkpoint:*+finish`). The answer is
/// `reachable` per leg, the polyline, the steps that are jumps with their **margin** ("gap 3.61 of 3.70"), the
/// three tightest ones, and — when it fails — the **impossible steps**: which pair of cells, what is wrong (jump too
/// long by x, too high by y), the blocks at both ends (a launcher like `bouncy` explains a step the model cannot
/// do), or the nearest point reached when nothing connects. The proof an obby or a parkour map needs. Pass `polyline` for
/// the route as points too (half the bytes of a long route). Not modelled: the run-up, ceilings over the arc, moving
/// blocks, launchers.
#[arbor_rpc::handler(mcp(
    name = "map_percorso",
    title = "Is a map walkable from A to B? Impossible steps, tightest jumps (instead of playing it)",
    safety = read,
))]
async fn bennu_map_percorso(_ctx: &BennuState, args: MapPercorsoArgs) -> Result<Value, String> {
    let mut args = args;
    args.path = scoped_map_path(args.root.as_deref(), &args.path)?;
    tokio::task::spawn_blocking(move || bennu_link::prelude::map_percorso(&args))
        .await
        .map_err(|e| format!("the map task failed: {e}"))?
}

/// **Questions about the screen, answered inside the game**: the game takes its own screenshot, computes the
/// operations you list and sends back only the results (the picture never crosses). Operations: `stats`
/// (luminance, saturation, mean colour, % near black/white), `palette:k` (dominant colours with names and %),
/// `count:#rrggbb~tol` / `bbox:#rrggbb~tol` (how many pixels of a colour and the rectangle that holds them),
/// `edges` (outline density, thickness, direction, continuity %), `grid:CxR` (a tiny character picture, e.g.
/// `grid:16x6`), `thumb:side` (a downscaled PNG as last resort), `diff:file.png` (against a saved picture).
/// Regions: fractions `x0,y0,x1,y1` of the picture (`0.7,0,1,0.15` = top-right corner), `px:x,y,w,h`, or a UI node
/// (`ui:cozy/pill`, resolved with the UI tree). **Prefer it to `bevy_game_screenshot`** whenever the question has
/// a number or a yes/no in it: "is it darker than before?", "is the stitching continuous?", "how much amber is in
/// the corner?". Each answer is one sentence (`detail: summary`) or structured (`json`). `dev` channel (protocol 2)
/// only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_analyze",
    title = "Analyse the running game's screen (statistics, palette, edges, diff) without a screenshot",
    safety = read,
))]
async fn bennu_game_analyze(_ctx: &BennuState, args: AnalyzeArgs) -> Result<Value, String> {
    active().await?.analyze(args).await
}

/// The same operations as `bevy_game_analyze` (`stats`, `palette`, `count`, `bbox`, `edges`, `grid`, `thumb`,
/// `diff`), on a **PNG file** instead of the live screen — for the pictures a tool already saved (a Studio
/// `ui shot`, a `mappa foto`, an earlier screenshot) so you can interrogate them **without reading them**. With
/// `root` the file is a path inside that project (scope-checked); without, an absolute `.png` in the temporary
/// folder. Needs no running game; a `ui:` region is refused (it names a node of a live game).
#[arbor_rpc::handler(mcp(
    name = "image_analyze",
    title = "Analyse a PNG file (statistics, palette, edges, diff) without reading the image",
    safety = read,
))]
async fn bennu_image_analyze(_ctx: &BennuState, args: ImageAnalyzeArgs) -> Result<Value, String> {
    let file = match args.root.as_deref().filter(|r| !r.trim().is_empty()) {
        Some(root) => std::path::PathBuf::from(crate::project_paths::in_root(root, &args.file)?),
        None => bennu_link::prelude::scratch_path(&args.file)?,
    };
    let detail = bennu_link::prelude::Detail::parse(args.detail.as_deref())?;
    let ops = bennu_link::prelude::parse_ops(&args.ops, args.region.as_deref())?;
    let ops_for_answer = ops.clone();
    let results = tokio::task::spawn_blocking(move || bennu_link::prelude::analyze_file(&file, &ops))
        .await
        .map_err(|e| format!("the analysis task failed: {e}"))??;
    Ok(bennu_link::prelude::results_json(&ops_for_answer, &results, detail))
}

/// Runtime statistics of the running game: frame time (average, min, max, p50 / p95 / p99), FPS, entity count
/// and every other diagnostic the game registered, and the window size. Bevy Remote Protocol only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_stats",
    title = "Frame time, FPS and diagnostics of the running game",
    safety = read,
))]
async fn bennu_game_stats(_ctx: &BennuState, args: StatsArgs) -> Result<Value, String> {
    active().await?.stats(args).await
}

/// The actions the running game registered for remote clients, with their descriptions. Bevy Remote Protocol
/// only: on the `dev` channel the game's and the mods' commands are console lines (`bevy_game_command`).
#[arbor_rpc::handler(mcp(
    name = "bevy_game_actions",
    title = "Actions the running game offers",
    safety = read,
))]
async fn bennu_game_actions(_ctx: &BennuState) -> Result<Value, String> {
    active().await?.actions().await
}

/// The world the game is in: name, seed, dimension, and who is in it with their positions. `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_world",
    title = "The world of the running game and its players",
    safety = read,
))]
async fn bennu_game_world(_ctx: &BennuState) -> Result<Value, String> {
    active().await?.world().await
}

/// The mods the game loaded: whether each is on (and why not), how many hooks it has and how many errors it
/// has made. The first place to look when a mod does nothing. `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_mods",
    title = "Mods loaded by the running game",
    safety = read,
))]
async fn bennu_game_mods(_ctx: &BennuState) -> Result<Value, String> {
    active().await?.mods().await
}

/// The values mods wrote to the HUD (key, typed value, how many times it was written), of one mod or of all.
/// `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_hud",
    title = "HUD values written by the mods of the running game",
    safety = read,
))]
async fn bennu_game_hud(_ctx: &BennuState, args: HudArgs) -> Result<Value, String> {
    active().await?.hud(args.mod_id).await
}

/// The latest chat lines (with their translation key and parameters) and console log lines. `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_chat",
    title = "Chat and console log of the running game",
    safety = read,
))]
async fn bennu_game_chat(_ctx: &BennuState, args: ChatArgs) -> Result<Value, String> {
    active().await?.chat(args.last.unwrap_or(30)).await
}

/// The state a mod keeps, read by its type (and the player it is about), as the game prints it. `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_mod_state",
    title = "State kept by a mod of the running game",
    safety = read,
))]
async fn bennu_game_mod_state(_ctx: &BennuState, args: ModStateArgs) -> Result<Value, String> {
    active().await?.mod_state(args.mod_id, args.ty, args.who).await
}

// ── Acting ───────────────────────────────────────────────────────────────────

/// Press a key in the running game as if on the keyboard. The input is real to the game: it acts on live
/// state with no undo.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_key",
    title = "Press a key in the running game",
    safety = write,
))]
async fn bennu_game_key(_ctx: &BennuState, args: KeyArgs) -> Result<Value, String> {
    active().await?.key(args).await
}

/// Type text into the running game (a focused text field, a console).
#[arbor_rpc::handler(mcp(
    name = "bevy_game_type",
    title = "Type text in the running game",
    safety = write,
))]
async fn bennu_game_text(_ctx: &BennuState, args: TextArgs) -> Result<Value, String> {
    active().await?.text(args).await
}

/// Move, click, double-click, drag or scroll the mouse in the running game. Coordinates are logical pixels of
/// the window; take a `bevy_game_screenshot` first to know where things are.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_mouse",
    title = "Use the mouse in the running game",
    safety = write,
))]
async fn bennu_game_mouse(_ctx: &BennuState, args: MouseArgs) -> Result<Value, String> {
    active().await?.mouse(args).await
}

/// Run a named action of the running game (`restart`, `spawn_wave`, …; on the `dev` channel the name is the
/// first word of a console line). What it does is the game's own business and may not be undoable.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_action",
    title = "Run a named action in the running game",
    safety = destructive,
))]
async fn bennu_game_action(_ctx: &BennuState, args: ActionArgs) -> Result<Value, String> {
    active().await?.action(args).await
}

/// Run one line of the game's console — a verb of the game or of one of its mods, as the player would type it
/// — and read what it answered (`ok`, `lines`). It acts on the live game with no undo; with `as_console` it
/// runs with every permission group. `dev` channel only.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_command",
    title = "Run a console command in the running game",
    safety = destructive,
))]
async fn bennu_game_command(_ctx: &BennuState, args: CommandArgs) -> Result<Value, String> {
    active().await?.command(args).await
}
