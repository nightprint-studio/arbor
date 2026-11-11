//! The Bevy Remote Protocol connection, seen as a [`GameLink`].
//!
//! Nothing here is new behaviour: every method is a call the inspector's own handlers already make
//! (`drive.rs`), put behind the trait the `dev` channel also implements, so the neutral `bevy_game_*` tools
//! and Imber's can ask one question and let `crate::link::active` choose who answers. A BRP game without the
//! `arbor-bevy-remote` companion still has the ECS and the states — that is what [`Capability::Ecs`] and
//! [`Capability::BevyStates`] are for — and says so for the rest ("add the plugin"), through `drive::arbor`.
//!
//! What this link does **not** have, it says in the shared sentence ([`bennu_link::prelude::unsupported`]):
//! a console command, the HUD, the chat, the mod list, the world, a mod's state. Those are the `dev`
//! channel's, because a Bevy game has no notion of a "mod" for the protocol to ask about.

use async_trait::async_trait;
use bennu_link::prelude::{
    ActionArgs, Capability, GameLink, KeyArgs, LiveTarget, MouseArgs, ScreenshotArgs, Shot, StatsArgs, TargetKind, TextArgs,
};
use serde_json::{json, Value};

use super::drive::{arbor, params, take_screenshot};

/// The open Remote Protocol connection. A snapshot of what the game's `rpc.discover` said, taken when the
/// call was routed: the connection itself stays the inspector's single static.
pub(crate) struct BrpLink {
    endpoint: String,
    capabilities: Vec<Capability>,
}

impl BrpLink {
    /// The link to the game the inspector is connected to, or the sentence that says there is none.
    pub(crate) fn current() -> Result<BrpLink, String> {
        let live = super::live()?;
        // `has_method` is soft-true when the discovery call gave nothing, which is the right bias: the call
        // itself answers "add the plugin" if it is not there.
        let companion = |method: &str| live.caps.has_method(method);
        let mut capabilities = Vec::new();
        for (capability, method) in [
            (Capability::Info, "arbor.info"),
            (Capability::Screenshot, "arbor.screenshot"),
            (Capability::Input, "arbor.input.key"),
            (Capability::Actions, "arbor.actions.list"),
            (Capability::Stats, "arbor.stats"),
        ] {
            if companion(method) {
                capabilities.push(capability);
            }
        }
        capabilities.push(Capability::Ecs);
        capabilities.push(Capability::BevyStates);
        Ok(BrpLink { endpoint: live.endpoint.clone(), capabilities })
    }
}

impl LiveTarget for BrpLink {
    fn kind(&self) -> TargetKind {
        TargetKind::Game
    }

    fn protocol(&self) -> &str {
        "brp"
    }

    fn label(&self) -> String {
        self.endpoint.clone()
    }

    fn capabilities(&self) -> Vec<Capability> {
        self.capabilities.clone()
    }
}

#[async_trait]
impl GameLink for BrpLink {
    async fn info(&self) -> Result<Value, String> {
        let mut info = arbor("arbor.info", json!({})).await?;
        if let Value::Object(map) = &mut info {
            map.insert("protocol".into(), json!("brp"));
            map.insert("endpoint".into(), json!(self.endpoint));
            map.insert("capabilities".into(), json!(self.capabilities.iter().map(|c| c.id()).collect::<Vec<_>>()));
        }
        Ok(info)
    }

    async fn screenshot(&self, args: ScreenshotArgs) -> Result<Shot, String> {
        take_screenshot(args).await
    }

    async fn key(&self, args: KeyArgs) -> Result<Value, String> {
        arbor("arbor.input.key", params(&args)?).await
    }

    async fn text(&self, args: TextArgs) -> Result<Value, String> {
        arbor("arbor.input.text", params(&args)?).await
    }

    async fn mouse(&self, args: MouseArgs) -> Result<Value, String> {
        arbor("arbor.input.mouse", params(&args)?).await
    }

    async fn stats(&self, args: StatsArgs) -> Result<Value, String> {
        arbor("arbor.stats", params(&args)?).await
    }

    async fn actions(&self) -> Result<Value, String> {
        arbor("arbor.actions.list", json!({})).await
    }

    async fn action(&self, args: ActionArgs) -> Result<Value, String> {
        arbor("arbor.action", params(&args)?).await
    }
}
