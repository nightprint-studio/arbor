//! Attaching to a game, whichever wire it speaks: `connect`, `disconnect` and the `status` that says what is
//! open, what could be, and what each link can do.
//!
//! ## What `auto` does
//!
//! 1. Look for a game on the **`dev` channel** (its discovery files; or `port`). If one answers, it is the
//!    link — and the Bevy Remote Protocol is *not* also opened: a Voriz game has none, and taking over the
//!    inspector's connection to some other game behind the user's back would be worse than not having ECS.
//! 2. Otherwise, the **Bevy Remote Protocol** at `endpoint` — reusing the inspector's connection if it is
//!    already open (`auto` never replaces it).
//!
//! `dev` and `brp` do only their half, and `brp` replaces the inspector's connection exactly as the panel's
//! Connect does. The protocol asked for is remembered (`bennu_link::session`): it is what the neutral tools
//! route by afterwards.

use std::sync::Arc;

use bennu_core::prelude::BennuState;
use bennu_link::prelude::{
    list_games, pick, summarize, DevChannelLink, DevTarget, GameConnectArgs, GameDisconnectArgs, Pick, Protocol, NOT_CONNECTED,
};
use bennu_link::session;
use serde_json::{json, Value};

use super::debugger::DebuggerTarget;
use crate::brp::link::BrpLink;

/// Open the link(s) the protocol asks for. `Err` when none could be opened, with one line per attempt.
pub(crate) async fn connect(ctx: &BennuState, args: GameConnectArgs) -> Result<Value, String> {
    let protocol = Protocol::parse(args.protocol.as_deref().unwrap_or(""))?;
    let mut failures: Vec<String> = Vec::new();

    let mut dev_open = false;
    if matches!(protocol, Protocol::Auto | Protocol::Dev) {
        match DevChannelLink::connect(DevTarget { game: args.game.clone(), port: args.port, dir: None }).await {
            Ok(link) => {
                session::set_dev(Arc::new(link));
                dev_open = true;
            }
            Err(e) => failures.push(format!("dev: {e}")),
        }
    }

    let mut brp_open = false;
    let try_brp = match protocol {
        Protocol::Brp => true,
        Protocol::Auto => !dev_open,
        Protocol::Dev => false,
    };
    if try_brp {
        if protocol == Protocol::Auto && crate::brp::is_connected() {
            brp_open = true;
        } else {
            match crate::brp::connect_for_link(ctx, args.endpoint.clone()).await {
                Ok(_) => brp_open = true,
                Err(e) => failures.push(format!("brp: {e}")),
            }
        }
    }

    if !dev_open && !brp_open {
        return Err(format!("Could not connect (protocol {protocol}).\n- {}", failures.join("\n- ")));
    }
    session::set_preference(protocol);
    let mut answer = status().await;
    if let Value::Object(map) = &mut answer {
        map.insert("opened".into(), json!({ "dev": dev_open, "brp": brp_open }));
        if !failures.is_empty() {
            map.insert("not_opened".into(), json!(failures));
        }
    }
    Ok(answer)
}

/// Close the link(s) the protocol names (both for `auto`). The game keeps running.
pub(crate) async fn disconnect(ctx: &BennuState, args: GameDisconnectArgs) -> Result<Value, String> {
    let protocol = Protocol::parse(args.protocol.as_deref().unwrap_or(""))?;
    let mut closed: Vec<&str> = Vec::new();
    if matches!(protocol, Protocol::Auto | Protocol::Dev) && session::clear_dev() {
        closed.push("dev");
    }
    if matches!(protocol, Protocol::Auto | Protocol::Brp) && crate::brp::endpoint().is_some() {
        crate::brp::disconnect_for_link(ctx).await?;
        closed.push("brp");
    }
    if closed.is_empty() {
        return Ok(json!({ "closed": [], "note": "no link of that protocol was open" }));
    }
    session::set_preference(Protocol::Auto);
    Ok(json!({ "closed": closed }))
}

/// What is open, what could be opened, and what each can do.
pub(crate) async fn status() -> Value {
    let found = tokio::task::spawn_blocking(|| list_games(None)).await.unwrap_or_default();

    let dev = session::dev();
    let brp = BrpLink::current().ok();
    let chosen = pick(session::preference(), dev.is_some(), brp.is_some() && crate::brp::is_connected()).ok();

    let mut links: Vec<Value> = Vec::new();
    if let Some(link) = &dev {
        let mut entry = serde_json::to_value(summarize(&**link)).unwrap_or(Value::Null);
        entry["connected"] = json!(link.is_connected());
        entry["active"] = json!(chosen == Some(Pick::Dev));
        links.push(entry);
    }
    if let Some(link) = &brp {
        let mut entry = serde_json::to_value(summarize(link)).unwrap_or(Value::Null);
        entry["connected"] = json!(crate::brp::is_connected());
        entry["active"] = json!(chosen == Some(Pick::Brp));
        links.push(entry);
    }

    let debuggers: Vec<Value> = DebuggerTarget::all()
        .into_iter()
        .map(|(id, target)| {
            let mut entry = serde_json::to_value(summarize(&target)).unwrap_or(Value::Null);
            entry["session"] = json!(id);
            entry
        })
        .collect();

    let mut out = json!({
        "protocol": session::preference().id(),
        "links": links,
        "dev_games_found": found,
        "debuggers": debuggers,
    });
    if links.is_empty() {
        out["hint"] = json!(NOT_CONNECTED);
    }
    out
}
