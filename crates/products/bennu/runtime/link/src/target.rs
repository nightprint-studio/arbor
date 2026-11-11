//! What every live thing Arbor can attach to has in common: who it is, and what it can do.
//!
//! Bennu attaches to two kinds of running thing, and they were written apart:
//!
//! * a **debugger** session (JDWP for a JVM, DAP for a native binary — `crate::debug_backend` in the
//!   backend): a program *stopped* on purpose, where the questions are stack, variables, watches and
//!   stepping;
//! * a **game** (Bevy Remote Protocol, or the fulcrum `dev` channel): a program that keeps running
//!   whatever Arbor does, where the questions are a screenshot, a key press, a console command, the HUD.
//!
//! The *operations* of the two share nothing — a `step` has no meaning for a game loop in a network
//! session, a `screenshot` none for a stopped JVM — so there is deliberately **no common operation trait**:
//! forcing one would be a signature that fits neither. What they do share is the identity and the answer to
//! "what can you do?", and that is this: [`LiveTarget`] and [`Capability`]. It is the one place a tool that
//! lists targets, an assistant that attaches to one, or a panel that draws its toolbar can ask without
//! knowing which protocol is behind.
//!
//! ## The rule the vocabulary encodes
//!
//! Pause, step, stack and evaluate are capabilities **only of a debugger** ([`Capability::belongs_to`]):
//! stopping a game that other players are connected to would desynchronise it, so no game link may declare
//! them, and a test per link holds that line.

use serde::Serialize;

/// The two families of live target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// A program stopped on purpose, to be read: a JDWP or DAP session.
    Debugger,
    /// A program that keeps running, to be driven: a Bevy game (BRP) or a fulcrum game (`dev` channel).
    Game,
}

impl TargetKind {
    pub fn id(self) -> &'static str {
        match self {
            TargetKind::Debugger => "debugger",
            TargetKind::Game => "game",
        }
    }
}

/// One thing a live target can do. The set is closed and small on purpose: a tool decides what to offer by
/// asking for one of these, never by comparing a protocol name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // ── Games ────────────────────────────────────────────────────────────────────────
    /// What the link is and what the game offers.
    Info,
    /// A picture of the game window.
    Screenshot,
    /// Keys, text and mouse as if the player pressed them.
    Input,
    /// One line of the game's console (or of a mod's command), run and answered.
    Command,
    /// A registry of named actions the game published (BRP companion).
    Actions,
    /// Frame time and the game's diagnostics.
    Stats,
    /// The HUD values the mods wrote.
    Hud,
    /// The chat and the console log.
    Chat,
    /// The loaded mods, whether each is on, how many hooks and faults.
    Mods,
    /// The world: name, seed, dimension, who is in it.
    World,
    /// The state a mod keeps, read by type.
    ModState,
    /// Entities, components and resources (BRP only: the `dev` channel has no ECS, by design).
    Ecs,
    /// Bevy `State<T>` machines (BRP only).
    BevyStates,
    // ── Debuggers ────────────────────────────────────────────────────────────────────
    /// Stop the program, let it run, step.
    Pause,
    /// Evaluate a watch or an expression in a stopped frame.
    Eval,
    /// The stack and the variables of a stopped thread.
    Stack,
    /// The debugged program's own output.
    Output,
}

impl Capability {
    pub const ALL: [Capability; 17] = [
        Capability::Info, Capability::Screenshot, Capability::Input, Capability::Command, Capability::Actions, Capability::Stats,
        Capability::Hud, Capability::Chat, Capability::Mods, Capability::World, Capability::ModState, Capability::Ecs,
        Capability::BevyStates, Capability::Pause, Capability::Eval, Capability::Stack, Capability::Output,
    ];

    /// The name it has on the wire and in a tool's answer.
    pub fn id(self) -> &'static str {
        match self {
            Capability::Info => "info",
            Capability::Screenshot => "screenshot",
            Capability::Input => "input",
            Capability::Command => "command",
            Capability::Actions => "actions",
            Capability::Stats => "stats",
            Capability::Hud => "hud",
            Capability::Chat => "chat",
            Capability::Mods => "mods",
            Capability::World => "world",
            Capability::ModState => "mod_state",
            Capability::Ecs => "ecs",
            Capability::BevyStates => "bevy_states",
            Capability::Pause => "pause",
            Capability::Eval => "eval",
            Capability::Stack => "stack",
            Capability::Output => "output",
        }
    }

    /// A noun phrase for a sentence: "has no <this>".
    pub fn what(self) -> &'static str {
        match self {
            Capability::Info => "info",
            Capability::Screenshot => "screenshot",
            Capability::Input => "keyboard, text and mouse input",
            Capability::Command => "console command",
            Capability::Actions => "registry of named actions",
            Capability::Stats => "frame statistics",
            Capability::Hud => "HUD values",
            Capability::Chat => "chat and console log",
            Capability::Mods => "mod list",
            Capability::World => "world description",
            Capability::ModState => "mod state",
            Capability::Ecs => "entity and component access (ECS)",
            Capability::BevyStates => "Bevy state machines",
            Capability::Pause => "pause or stepping",
            Capability::Eval => "expression evaluation",
            Capability::Stack => "stack and variables",
            Capability::Output => "program output",
        }
    }

    /// Whether a target of `kind` may declare this capability. A game never pauses.
    pub fn belongs_to(self, kind: TargetKind) -> bool {
        let debugger_only = matches!(self, Capability::Pause | Capability::Eval | Capability::Stack | Capability::Output);
        match kind {
            TargetKind::Debugger => debugger_only,
            TargetKind::Game => !debugger_only,
        }
    }
}

/// A running thing Arbor is (or could be) attached to.
pub trait LiveTarget: Send + Sync {
    fn kind(&self) -> TargetKind;

    /// The wire it speaks: `dev`, `brp`, `jdwp`, `dap`.
    fn protocol(&self) -> &str;

    /// For a status line: the game's name and port, the JVM's version.
    fn label(&self) -> String;

    /// Everything this link can do, in a stable order.
    fn capabilities(&self) -> Vec<Capability>;

    fn supports(&self, capability: Capability) -> bool {
        self.capabilities().contains(&capability)
    }
}

/// A target as a tool answers about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetSummary {
    pub kind: TargetKind,
    pub protocol: String,
    pub label: String,
    pub capabilities: Vec<&'static str>,
}

pub fn summarize<T: LiveTarget + ?Sized>(target: &T) -> TargetSummary {
    TargetSummary {
        kind: target.kind(),
        protocol: target.protocol().to_string(),
        label: target.label(),
        capabilities: target.capabilities().into_iter().map(Capability::id).collect(),
    }
}

/// The sentence a tool answers when the link it was given cannot do what was asked: it names the link, says
/// what is missing, adds `hint` (what to do instead) and lists what the link does offer — so the caller
/// plans its next call from the answer instead of retrying the same one.
pub fn unsupported<T: LiveTarget + ?Sized>(target: &T, capability: Capability, hint: &str) -> String {
    let offered: Vec<&str> = target.capabilities().into_iter().map(Capability::id).collect();
    let hint = if hint.is_empty() { String::new() } else { format!(" {hint}") };
    format!(
        "The {} link to {} has no {}.{hint} It offers: {}.",
        target.protocol(),
        target.label(),
        capability.what(),
        if offered.is_empty() { "nothing".to_string() } else { offered.join(", ") }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(TargetKind, Vec<Capability>);
    impl LiveTarget for Fake {
        fn kind(&self) -> TargetKind {
            self.0
        }
        fn protocol(&self) -> &str {
            "fake"
        }
        fn label(&self) -> String {
            "the fake".into()
        }
        fn capabilities(&self) -> Vec<Capability> {
            self.1.clone()
        }
    }

    #[test]
    fn a_game_never_pauses_and_a_debugger_never_takes_screenshots() {
        for c in Capability::ALL {
            let game = c.belongs_to(TargetKind::Game);
            let debugger = c.belongs_to(TargetKind::Debugger);
            assert!(game != debugger, "{c:?} must belong to exactly one kind");
        }
        assert!(!Capability::Pause.belongs_to(TargetKind::Game));
        assert!(Capability::Pause.belongs_to(TargetKind::Debugger));
        assert!(Capability::Screenshot.belongs_to(TargetKind::Game));
        assert!(!Capability::Screenshot.belongs_to(TargetKind::Debugger));
    }

    #[test]
    fn every_capability_has_a_distinct_id() {
        let mut ids: Vec<&str> = Capability::ALL.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Capability::ALL.len());
    }

    #[test]
    fn the_wire_name_of_a_capability_is_its_id() {
        for c in Capability::ALL {
            assert_eq!(serde_json::to_value(c).unwrap(), serde_json::Value::String(c.id().into()), "{c:?}");
        }
    }

    #[test]
    fn the_unsupported_sentence_names_the_link_and_what_it_does_offer() {
        let t = Fake(TargetKind::Game, vec![Capability::Screenshot, Capability::Command]);
        let s = unsupported(&t, Capability::Stats, "Use the console instead.");
        assert!(s.contains("fake link to the fake has no frame statistics"), "{s}");
        assert!(s.contains("Use the console instead.") && s.ends_with("It offers: screenshot, command."), "{s}");
        assert!(t.supports(Capability::Command) && !t.supports(Capability::Stats));
    }

    #[test]
    fn a_summary_lists_ids_not_variants() {
        let t = Fake(TargetKind::Debugger, vec![Capability::Pause, Capability::Eval]);
        let s = summarize(&t);
        assert_eq!(s.capabilities, vec!["pause", "eval"]);
        assert_eq!(serde_json::to_value(&s).unwrap()["kind"], "debugger");
    }
}
