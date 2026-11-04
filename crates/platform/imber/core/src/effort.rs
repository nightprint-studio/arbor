//! How hard the model thinks before it answers.
//!
//! One lever with five positions, the same on every provider that has it: `low` answers quickly and
//! reaches for few tools, `max` thinks longest and costs most. It is a property of the **model**, not of
//! the conversation — a model without the setting simply has none, and asking it for one is an error
//! the provider would answer with a 400. So the question "may I send an effort?" is answered here,
//! once, rather than learned from a failed request.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    /// Written `xhigh` on the wire.
    #[serde(rename = "xhigh")]
    XHigh,
    Max,
}

impl Effort {
    pub const ALL: [Effort; 5] = [Effort::Low, Effort::Medium, Effort::High, Effort::XHigh, Effort::Max];

    /// The word a provider's request carries.
    pub fn wire(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::XHigh => "xhigh",
            Effort::Max => "max",
        }
    }

    pub fn parse(word: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|e| e.wire() == word.trim().to_lowercase())
    }

    /// The level to use when the one asked for is not available: the highest the model offers that does
    /// not exceed it, or its lowest when it offers nothing so low. `None` when the model has no levels.
    pub fn clamp_to(self, available: &[Effort]) -> Option<Effort> {
        available
            .iter()
            .copied()
            .filter(|e| *e <= self)
            .max()
            .or_else(|| available.iter().copied().min())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_words_round_trip() {
        for e in Effort::ALL {
            assert_eq!(Effort::parse(e.wire()), Some(e));
        }
        assert_eq!(Effort::parse(" XHigh "), Some(Effort::XHigh));
        assert_eq!(Effort::parse("turbo"), None);
    }

    #[test]
    fn serde_uses_the_same_words() {
        assert_eq!(serde_json::to_string(&Effort::XHigh).unwrap(), "\"xhigh\"");
        assert_eq!(serde_json::from_str::<Effort>("\"low\"").unwrap(), Effort::Low);
    }

    #[test]
    fn the_levels_are_ordered_by_how_hard_they_think() {
        assert!(Effort::Low < Effort::Medium && Effort::High < Effort::XHigh && Effort::XHigh < Effort::Max);
    }

    #[test]
    fn a_level_the_model_lacks_falls_back_to_the_nearest_one_below() {
        let only_low_to_high = [Effort::Low, Effort::Medium, Effort::High];
        assert_eq!(Effort::Max.clamp_to(&only_low_to_high), Some(Effort::High));
        assert_eq!(Effort::Medium.clamp_to(&only_low_to_high), Some(Effort::Medium));
        // Nothing at or below it: the lowest on offer is the closest thing.
        assert_eq!(Effort::Low.clamp_to(&[Effort::High, Effort::Max]), Some(Effort::High));
        assert_eq!(Effort::High.clamp_to(&[]), None);
    }
}
