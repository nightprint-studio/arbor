//! Who is looking at the game, and what they are doing — the state behind the on-screen indicator.
//!
//! BRP over HTTP has no notion of a connection, so presence is **declared**: the client says
//! `arbor.presence { state, label }` when it attaches, and every method that acts on the game marks it
//! *acting* for a moment. The indicator is a function of this state and the clock, which keeps it testable
//! without a window.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    /// Nobody has attached: nothing is drawn.
    Idle,
    /// A client is attached and looking.
    Connected,
    /// A client just acted on the game (input, an action, a screenshot).
    Acting,
    /// The client asked to be treated as paused: it is attached but will not act.
    Paused,
}

/// RGBA, 0..1.
pub type Rgba = [f32; 4];

#[derive(Debug, Clone, PartialEq)]
pub struct PresenceColors {
    pub connected: Rgba,
    pub acting: Rgba,
    pub paused: Rgba,
}

impl Default for PresenceColors {
    fn default() -> Self {
        Self { connected: [0.25, 0.55, 1.0, 1.0], acting: [1.0, 0.55, 0.15, 1.0], paused: [0.6, 0.6, 0.65, 1.0] }
    }
}

/// How long one action keeps the indicator on *acting* once the actions stop.
pub const ACTING_HOLD: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub struct Presence {
    /// The user's switch: `false` hides the indicator whatever the state.
    pub enabled: bool,
    pub label: String,
    pub colors: PresenceColors,
    base: PresenceState,
    acting_until: f64,
}

impl Default for Presence {
    fn default() -> Self {
        Self { enabled: true, label: String::new(), colors: PresenceColors::default(), base: PresenceState::Idle, acting_until: f64::NEG_INFINITY }
    }
}

impl Presence {
    /// A client declared itself. `Acting` is not declarable: it is what *acting* does.
    pub fn declare(&mut self, state: PresenceState, label: Option<String>) {
        self.base = if state == PresenceState::Acting { PresenceState::Connected } else { state };
        if let Some(label) = label {
            self.label = label;
        }
    }

    /// A method acted on the game at `now` (seconds). A client that never declared itself is taken to be
    /// connected: an agent that only ever clicks should still be visible.
    pub fn mark_acting(&mut self, now: f64) {
        if self.base == PresenceState::Idle {
            self.base = PresenceState::Connected;
        }
        if self.base != PresenceState::Paused {
            self.acting_until = now + ACTING_HOLD.as_secs_f64();
        }
    }

    /// What to draw at `now`; `None` when nothing should be.
    pub fn visible(&self, now: f64) -> Option<(PresenceState, Rgba)> {
        if !self.enabled || self.base == PresenceState::Idle {
            return None;
        }
        let state = match self.base {
            PresenceState::Paused => PresenceState::Paused,
            _ if now < self.acting_until => PresenceState::Acting,
            other => other,
        };
        let color = match state {
            PresenceState::Acting => self.colors.acting,
            PresenceState::Paused => self.colors.paused,
            _ => self.colors.connected,
        };
        Some((state, color))
    }

    pub fn base_state(&self) -> PresenceState {
        self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_drawn_until_a_client_shows_up() {
        assert_eq!(Presence::default().visible(0.0), None);
    }

    #[test]
    fn connecting_shows_the_connected_colour() {
        let mut p = Presence::default();
        p.declare(PresenceState::Connected, Some("Imber".into()));
        let (state, color) = p.visible(1.0).unwrap();
        assert_eq!(state, PresenceState::Connected);
        assert_eq!(color, PresenceColors::default().connected);
        assert_eq!(p.label, "Imber");
    }

    #[test]
    fn acting_lasts_a_moment_then_returns_to_connected() {
        let mut p = Presence::default();
        p.declare(PresenceState::Connected, None);
        p.mark_acting(10.0);
        assert_eq!(p.visible(10.5).unwrap().0, PresenceState::Acting);
        assert_eq!(p.visible(12.0).unwrap().0, PresenceState::Connected);
    }

    #[test]
    fn a_client_that_only_acts_is_still_shown() {
        let mut p = Presence::default();
        p.mark_acting(0.0);
        assert_eq!(p.visible(0.1).unwrap().0, PresenceState::Acting);
    }

    #[test]
    fn paused_wins_over_acting_and_blocks_it() {
        let mut p = Presence::default();
        p.declare(PresenceState::Paused, None);
        p.mark_acting(0.0);
        assert_eq!(p.visible(0.1).unwrap().0, PresenceState::Paused);
    }

    #[test]
    fn the_switch_hides_everything() {
        let mut p = Presence::default();
        p.declare(PresenceState::Connected, None);
        p.enabled = false;
        assert_eq!(p.visible(0.0), None);
    }

    #[test]
    fn acting_cannot_be_declared() {
        let mut p = Presence::default();
        p.declare(PresenceState::Acting, None);
        assert_eq!(p.base_state(), PresenceState::Connected);
    }
}
