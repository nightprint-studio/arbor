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
    ActionArgs, ChatArgs, CommandArgs, GameConnectArgs, GameDisconnectArgs, HudArgs, KeyArgs, ModStateArgs, MouseArgs, ScreenshotArgs, Shot,
    StatsArgs, TextArgs,
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

/// A screenshot of the running game's window. Images are expensive to read: prefer `max_width` of 800–1000
/// unless detail matters.
#[arbor_rpc::handler(mcp(
    name = "bevy_game_screenshot",
    title = "Screenshot of the running game",
    safety = read,
    output = image,
))]
async fn bennu_game_screenshot(_ctx: &BennuState, args: ScreenshotArgs) -> Result<Shot, String> {
    active().await?.screenshot(args).await
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
