//! The switches and the strength of the "smart" completion signals — what Settings → Completion
//! turns on, off and up.
//!
//! ## Why a process-wide value
//!
//! Every ranking entry point (`completion_in`, `scope_completion`, `method_reference_completion`)
//! builds a [`crate::rank::Context`] deep inside, and threading a settings struct through all of
//! them would put it in every signature and every test that calls one. The completion memory
//! ([`crate::picked`]) made the same call for the same reason: it is one preference of one editor
//! process, belonging to no query. The backend refreshes it from the user's config when the setting
//! changes ([`set`]), and a [`Context`](crate::rank::Context) takes a snapshot at construction —
//! so one completion never sees two different tunings, and a unit test that wants a specific one
//! hands it to the context directly instead of touching the global.
//!
//! ## What each switch means
//!
//! Every switch turns a *signal* off, never a *candidate*: nothing here filters. Off means the term
//! contributes nothing to the order, which is why they can all default to on — a user who dislikes
//! one gets the previous ordering back, not a different list.

use std::sync::RwLock;

/// How strongly the learned signals count, in percent of their designed weight.
pub const DEFAULT_STRENGTH: u32 = 100;
/// The ceiling. Past this a learned signal would start outranking the position's type, which the
/// ranking is built to prevent — the cap is the guard rail, not a taste.
pub const MAX_STRENGTH: u32 = 250;

/// The completion tuning snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// Rank a candidate higher when its name echoes the name of the thing being filled —
    /// `Customer customer = |`, the argument of `findByCustomerId(|)`.
    pub name_affinity: bool,
    /// Rank a member higher when the PROJECT calls it often (counted while the index builds).
    pub project_usage: bool,
    /// Rank a member higher when the project calls it right after the one before it in the chain
    /// (`open()` then `close()`).
    pub sequence: bool,
    /// Offer members of members that produce the expected type (`order.getCustomer().getName()`).
    pub chain: bool,
    /// Let the shape of the code decide: setters already called on a builder sink, a never-used
    /// local rises, the exceptions a `try` throws lead a `catch`, the enum constants a `switch`
    /// has not covered lead its `case`.
    pub context_filters: bool,
    /// Offer whole-statement templates that read the surrounding code (`iter`).
    pub statements: bool,
    /// Draw the continuations that are facts as ghost text (`catch (X |` → `e`, the next `case`).
    pub ghost_text: bool,
    /// How strongly the learned terms (affinity, usage, sequence) count, in percent.
    pub strength: u32,
}

impl Tuning {
    /// Everything on, at the designed strength.
    pub const DEFAULT: Tuning = Tuning {
        name_affinity: true,
        project_usage: true,
        sequence: true,
        chain: true,
        context_filters: true,
        statements: true,
        ghost_text: true,
        strength: DEFAULT_STRENGTH,
    };

    /// Everything off — the ordering completion had before these signals existed. Handy for tests
    /// that pin the older behaviour.
    pub const OFF: Tuning = Tuning {
        name_affinity: false,
        project_usage: false,
        sequence: false,
        chain: false,
        context_filters: false,
        statements: false,
        ghost_text: false,
        strength: DEFAULT_STRENGTH,
    };

    /// `points`, scaled by [`Tuning::strength`] — what a learned term adds to a score.
    pub fn scaled(&self, points: i32) -> i32 {
        let strength = self.strength.min(MAX_STRENGTH) as i64;
        (i64::from(points) * strength / 100) as i32
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

static CURRENT: RwLock<Tuning> = RwLock::new(Tuning::DEFAULT);

/// The tuning in force right now.
pub fn current() -> Tuning {
    match CURRENT.read() {
        Ok(t) => *t,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

/// Replace the tuning — called by the backend when the setting is loaded or changed. The strength
/// is clamped on the way in, so a hand-edited config cannot exceed [`MAX_STRENGTH`].
pub fn set(tuning: Tuning) {
    let tuning = Tuning { strength: tuning.strength.min(MAX_STRENGTH), ..tuning };
    match CURRENT.write() {
        Ok(mut t) => *t = tuning,
        Err(poisoned) => *poisoned.into_inner() = tuning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_has_every_signal_on_at_full_strength() {
        let t = Tuning::default();
        assert!(t.name_affinity && t.project_usage && t.sequence && t.chain);
        assert!(t.context_filters && t.statements && t.ghost_text);
        assert_eq!(t.strength, 100);
    }

    #[test]
    fn off_turns_every_signal_off() {
        let t = Tuning::OFF;
        assert!(!(t.name_affinity || t.project_usage || t.sequence || t.chain));
        assert!(!(t.context_filters || t.statements || t.ghost_text));
    }

    #[test]
    fn strength_scales_points_linearly() {
        let half = Tuning { strength: 50, ..Tuning::DEFAULT };
        let double = Tuning { strength: 200, ..Tuning::DEFAULT };
        assert_eq!(Tuning::DEFAULT.scaled(20), 20);
        assert_eq!(half.scaled(20), 10);
        assert_eq!(double.scaled(20), 40);
        assert_eq!(Tuning { strength: 0, ..Tuning::DEFAULT }.scaled(20), 0);
    }

    #[test]
    fn scaling_works_for_negative_points_and_respects_the_ceiling() {
        let wild = Tuning { strength: 10_000, ..Tuning::DEFAULT };
        assert_eq!(wild.scaled(10), 25, "capped at 250%");
        assert_eq!(Tuning::DEFAULT.scaled(-8), -8);
    }
}
