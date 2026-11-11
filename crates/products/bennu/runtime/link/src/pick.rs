//! Which of the two links a tool talks to.
//!
//! Two links can be open at once (a Voriz game on the `dev` channel and, say, the inspector on the Bevy
//! Remote Protocol of another game), and the tools that exist for both — a screenshot, a key press — must
//! not guess. The rule is one pure function so the backend, the assistant and the tests all agree:
//!
//! * `auto` (the default): **`dev` first** — it is the channel the games we build speak — then BRP;
//! * `dev` / `brp`: only that one, and when it is not open the answer says which *is*, instead of silently
//!   using the other one.

use std::fmt;

/// What the user (or the tool call) asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protocol {
    #[default]
    Auto,
    Dev,
    Brp,
}

impl Protocol {
    pub fn parse(text: &str) -> Result<Protocol, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Protocol::Auto),
            "dev" | "dev-channel" | "dev_channel" => Ok(Protocol::Dev),
            "brp" | "bevy" | "bevy-remote" => Ok(Protocol::Brp),
            other => Err(format!("unknown protocol `{other}`: use `auto`, `dev` or `brp`")),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Protocol::Auto => "auto",
            Protocol::Dev => "dev",
            Protocol::Brp => "brp",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// The link a call is routed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Dev,
    Brp,
}

/// The sentence when nothing is connected: it says how to connect each kind of game.
pub const NOT_CONNECTED: &str = "Not connected to a game. Call bevy_game_connect (protocol `auto` tries the `dev` channel first, then the Bevy Remote Protocol). \
A Voriz/fulcrum game must be started with `--dev-channel` (cargo feature `dev-channel`); a Bevy game needs RemotePlugin + RemoteHttpPlugin.";

/// Choose the link for a call. `dev` / `brp` say whether that link is open now.
pub fn pick(preference: Protocol, dev: bool, brp: bool) -> Result<Pick, String> {
    match (preference, dev, brp) {
        (Protocol::Auto, true, _) => Ok(Pick::Dev),
        (Protocol::Auto, false, true) => Ok(Pick::Brp),
        (Protocol::Dev, true, _) => Ok(Pick::Dev),
        (Protocol::Brp, _, true) => Ok(Pick::Brp),
        (Protocol::Dev, false, true) => Err("The protocol is set to `dev`, but only a Bevy Remote Protocol link is open. Connect the `dev` channel (the game needs `--dev-channel`), or switch with bevy_game_connect protocol `brp` / `auto`.".into()),
        (Protocol::Brp, true, false) => Err("The protocol is set to `brp`, but only a `dev` channel link is open. Connect the Bevy Remote Protocol (the game needs RemotePlugin + RemoteHttpPlugin), or switch with bevy_game_connect protocol `dev` / `auto`.".into()),
        (_, false, false) => Err(NOT_CONNECTED.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_the_dev_channel_when_both_are_open() {
        assert_eq!(pick(Protocol::Auto, true, true), Ok(Pick::Dev));
        assert_eq!(pick(Protocol::Auto, true, false), Ok(Pick::Dev));
        assert_eq!(pick(Protocol::Auto, false, true), Ok(Pick::Brp));
    }

    #[test]
    fn an_explicit_choice_is_respected_even_when_the_other_is_open() {
        assert_eq!(pick(Protocol::Brp, true, true), Ok(Pick::Brp));
        assert_eq!(pick(Protocol::Dev, true, true), Ok(Pick::Dev));
    }

    #[test]
    fn an_explicit_choice_that_is_not_open_says_which_one_is() {
        let e = pick(Protocol::Dev, false, true).unwrap_err();
        assert!(e.contains("only a Bevy Remote Protocol link is open"), "{e}");
        let e = pick(Protocol::Brp, true, false).unwrap_err();
        assert!(e.contains("only a `dev` channel link is open"), "{e}");
    }

    #[test]
    fn nothing_open_says_how_to_connect_both_kinds_of_game() {
        for p in [Protocol::Auto, Protocol::Dev, Protocol::Brp] {
            let e = pick(p, false, false).unwrap_err();
            assert!(e.contains("--dev-channel") && e.contains("RemoteHttpPlugin"), "{p}: {e}");
        }
    }

    #[test]
    fn protocols_parse_with_their_aliases_and_refuse_the_rest() {
        assert_eq!(Protocol::parse("").unwrap(), Protocol::Auto);
        assert_eq!(Protocol::parse(" DEV ").unwrap(), Protocol::Dev);
        assert_eq!(Protocol::parse("bevy").unwrap(), Protocol::Brp);
        assert!(Protocol::parse("grpc").unwrap_err().contains("unknown protocol"));
        for p in [Protocol::Auto, Protocol::Dev, Protocol::Brp] {
            assert_eq!(Protocol::parse(p.id()).unwrap(), p);
        }
    }
}
