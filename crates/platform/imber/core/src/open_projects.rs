//! Bringing another project into a conversation: when Imber asks Bennu about a project the conversation does not
//! work in, what happens — it joins at once, it is opened and indexed, or the user is asked.
//!
//! Two facts decide, besides the user's setting and the mode:
//!
//! * **is it open in the window?** A project the user has open is already indexed: bringing it in costs nothing
//!   but the reach, so it never asks to *index* — at most the access card, when it is outside the reach.
//! * **may Imber already read there?** Outside the reach the access card asks, and it is the only question: it
//!   says that a yes also opens and indexes the project, so the user is not asked twice for one thing.
//!
//! Only an external project that Imber may already read is left to the setting. Pure: the host checks the reach,
//! the window and the manifests, and does what this returns.

use crate::autonomy::AccessMode;

/// The user's choice about opening an external project Imber may already read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenProjects {
    /// Always ask.
    Ask,
    /// Open it without asking in Autonomous mode; ask in the others. The default.
    Autonomous,
    /// Open it without asking in every mode.
    Allowed,
    /// Never: an external project is only files to Imber.
    Never,
}

impl OpenProjects {
    /// The saved word; anything unknown is the default.
    pub fn parse(word: &str) -> Self {
        match word.trim() {
            "ask" => OpenProjects::Ask,
            "allowed" => OpenProjects::Allowed,
            "never" => OpenProjects::Never,
            _ => OpenProjects::Autonomous,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            OpenProjects::Ask => "ask",
            OpenProjects::Autonomous => "autonomous",
            OpenProjects::Allowed => "allowed",
            OpenProjects::Never => "never",
        }
    }
}

/// What to do about one project the conversation does not work in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenStep {
    /// Open in the window and reachable: it joins the conversation without a word.
    Join,
    /// External and reachable, and the setting lets it open: it is opened and indexed without asking.
    Open,
    /// External and reachable, and opening it asks.
    AskToOpen,
    /// Outside the reach: the access card asks; `indexes` is whether a yes also opens and indexes it (an external
    /// project) or only lets it join (one open in the window).
    AskAccess { indexes: bool },
    /// It may not be opened; the sentence the model reads.
    Refuse(String),
}

/// The step for a project, given whether it is open in the window, whether Imber may already read there, the mode
/// and the setting.
pub fn decide(setting: OpenProjects, in_window: bool, permitted: bool, mode: AccessMode) -> OpenStep {
    match (in_window, permitted) {
        (true, true) => OpenStep::Join,
        (true, false) => OpenStep::AskAccess { indexes: false },
        (false, _) if setting == OpenProjects::Never => OpenStep::Refuse(
            "the user does not let you open projects that are not open in Bennu: work with its files (read, search, edit) instead of Bennu's tools, or ask them to open it".to_string(),
        ),
        (false, false) => OpenStep::AskAccess { indexes: true },
        (false, true) => match setting {
            OpenProjects::Allowed => OpenStep::Open,
            OpenProjects::Autonomous if mode == AccessMode::Autonomous => OpenStep::Open,
            _ => OpenStep::AskToOpen,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_open_in_the_window_never_asks_to_index() {
        assert_eq!(decide(OpenProjects::Ask, true, true, AccessMode::Manual), OpenStep::Join);
        assert_eq!(decide(OpenProjects::Never, true, false, AccessMode::Manual), OpenStep::AskAccess { indexes: false });
    }

    #[test]
    fn outside_the_reach_the_access_card_is_the_only_question() {
        assert_eq!(decide(OpenProjects::Allowed, false, false, AccessMode::Autonomous), OpenStep::AskAccess { indexes: true });
    }

    #[test]
    fn a_reachable_external_project_follows_the_setting_and_the_mode() {
        assert_eq!(decide(OpenProjects::Autonomous, false, true, AccessMode::Autonomous), OpenStep::Open);
        assert_eq!(decide(OpenProjects::Autonomous, false, true, AccessMode::AcceptEdits), OpenStep::AskToOpen);
        assert_eq!(decide(OpenProjects::Ask, false, true, AccessMode::Autonomous), OpenStep::AskToOpen);
        assert_eq!(decide(OpenProjects::Allowed, false, true, AccessMode::Manual), OpenStep::Open);
        assert!(matches!(decide(OpenProjects::Never, false, true, AccessMode::Autonomous), OpenStep::Refuse(_)));
    }

    #[test]
    fn an_unknown_word_is_the_default() {
        assert_eq!(OpenProjects::parse("whatever"), OpenProjects::Autonomous);
        assert_eq!(OpenProjects::parse(OpenProjects::Never.id()), OpenProjects::Never);
    }
}
