//! The game Arbor is attached to, over either wire — and the debugger beside it, seen the same way.
//!
//! Bennu attaches to two kinds of live thing (see `bennu-link::target`): a **debugger** session
//! (`crate::debug_backend`: JDWP, DAP) and a **game**. A game can be reached over two protocols, and Arbor
//! supports both:
//!
//! * the fulcrum **`dev` channel** (`bennu_link::dev::DevChannelLink`) — the one our own games speak
//!   (Voriz: `--dev-channel`): console commands, HUD, chat, mods, world, screenshot, input;
//! * the **Bevy Remote Protocol** (`crate::brp::link::BrpLink`) — any Bevy game with `RemotePlugin`: the ECS,
//!   resources, states, and (with the `arbor-bevy-remote` companion) screenshot, input, stats, actions.
//!
//! | File | What |
//! |---|---|
//! | `mod.rs` | [`active`]: which link a neutral call goes to; the sentence when the BRP-only tools find none |
//! | `connect.rs` | `bevy_game_connect` / `_disconnect` / `_status`: the one gesture that attaches, whichever wire |
//! | `debugger.rs` | [`debugger::DebuggerTarget`]: a debug session as a `LiveTarget`, for the listing |
//! | `tools.rs` | the neutral MCP tools (`bevy_game_*`) and their handlers |
//!
//! ## Why a trait, here, now
//!
//! `GameLink` has two implementations that genuinely answer the same questions (a screenshot, a key press, an
//! action) with different wires, and the tools above it did not want to know which. That is the case the
//! project rules call a justified trait. What is **not** unified is the debugger's operation set: it shares
//! the identity and the capability vocabulary (`LiveTarget`), not the verbs — see `bennu-link::target`.
//!
//! ## One session, one choice
//!
//! The `dev` link is one process-wide slot (`bennu_link::session`), the BRP one is the inspector's static
//! (`crate::brp`). Both can be open; `bevy_game_connect` records the *protocol* the user chose (`auto`: the
//! `dev` channel first, then BRP), and [`active`] applies it with the pure `bennu_link::pick`.

pub(crate) mod connect;
pub(crate) mod debugger;
pub(crate) mod tools;

use std::sync::Arc;

use bennu_link::prelude::{pick, GameLink, Pick, NOT_CONNECTED};
use bennu_link::session;

use crate::brp::link::BrpLink;

/// The link the neutral game tools talk to right now, by the user's protocol choice.
pub(crate) async fn active() -> Result<Arc<dyn GameLink>, String> {
    let dev = session::dev();
    let brp = crate::brp::is_connected();
    match pick(session::preference(), dev.is_some(), brp)? {
        Pick::Dev => {
            let link = dev.ok_or_else(|| NOT_CONNECTED.to_string())?;
            Ok(link)
        }
        Pick::Brp => Ok(Arc::new(BrpLink::current()?)),
    }
}

/// What the Remote-Protocol-only tools (entities, gamepad, presence) answer when the inspector has no
/// connection: the old sentence, and — when the game is on the `dev` channel — the reason that is *why*.
pub(crate) fn not_connected_to_brp() -> String {
    if session::dev().is_some() {
        "The game is connected on the `dev` channel, which has no ECS, gamepad or presence indicator: those belong to the Bevy Remote Protocol. \
Connect it with bevy_game_connect protocol `brp` (the game needs RemotePlugin + RemoteHttpPlugin)."
            .to_string()
    } else {
        "Not connected to a Bevy game.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brp_only_sentence_is_the_old_one_when_nothing_is_open() {
        // The `dev` slot is process-wide; this test only reads it, and no test in this crate fills it.
        if session::dev().is_none() {
            assert_eq!(not_connected_to_brp(), "Not connected to a Bevy game.");
        }
    }
}
