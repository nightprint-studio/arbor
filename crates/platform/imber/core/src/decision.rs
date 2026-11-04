//! How much the assistant decides for itself — the user's "decision level" and their own words about it.
//!
//! Three levels: ask at every fork (the default), ask only about big decisions, decide everything it can and
//! report afterwards. On top of any level the user can write free-text instructions about decisions; there is
//! a global text (settings) and one per conversation, and the conversation's is appended to the global one.
//!
//! Pure on purpose: the system prompt is built from [`paragraph`], and the cap on questions from
//! [`question_cap`], so both are tested without a run.

/// Questions a `Decide` run may still put to the user: genuine blockers only.
pub const DECIDE_QUESTION_CAP: u32 = 2;

/// Longest free-text note kept, in characters (the note is sent with every request).
pub const MAX_NOTES_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecisionLevel {
    /// Ask at forks: names, scope, trade-offs, two reasonable ways to build something.
    #[default]
    AskForks,
    /// Ask only about big decisions: architecture, data loss, anything costly to undo.
    AskBig,
    /// Decide alone; ask only when truly blocked, and report the decisions at the end.
    Decide,
}

impl DecisionLevel {
    /// From its saved word; anything unknown is the default.
    pub fn parse(word: &str) -> Self {
        match word.trim().to_ascii_lowercase().as_str() {
            "ask_big" | "ask-big" => DecisionLevel::AskBig,
            "decide" => DecisionLevel::Decide,
            _ => DecisionLevel::AskForks,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            DecisionLevel::AskForks => "ask_forks",
            DecisionLevel::AskBig => "ask_big",
            DecisionLevel::Decide => "decide",
        }
    }
}

/// How many times `ask_user` may be used in one run, or `None` for no cap.
pub fn question_cap(level: DecisionLevel) -> Option<u32> {
    (level == DecisionLevel::Decide).then_some(DECIDE_QUESTION_CAP)
}

/// The global text, then the conversation's; blank parts dropped, the whole cut to [`MAX_NOTES_CHARS`].
pub fn merge_notes(global: &str, chat: &str) -> String {
    let parts: Vec<&str> = [global.trim(), chat.trim()].into_iter().filter(|p| !p.is_empty()).collect();
    let joined = parts.join("\n");
    joined.chars().take(MAX_NOTES_CHARS).collect()
}

/// The closing paragraph of the system prompt for this level and these notes; empty for the default level with
/// no notes, so the common case adds nothing.
pub fn paragraph(level: DecisionLevel, notes: &str) -> String {
    let mut s = String::new();
    match level {
        DecisionLevel::AskForks => {}
        DecisionLevel::AskBig => s.push_str(
            "\nDecision level: ask only big decisions. This replaces the rule above about forks: choose names, structure and small trade-offs yourself and say what you chose. Stop and use `ask_user` only for what is hard to undo or changes the shape of the project — architecture, a new dependency, deleting or rewriting a lot, a requirement you cannot guess.\n",
        ),
        DecisionLevel::Decide => s.push_str(&format!(
            "\nDecision level: decide yourself. This replaces the rule above about forks: pick the option you judge best and go on. Use `ask_user` only when you are genuinely blocked and nothing in the project, the tools or your judgement settles it — at most {DECIDE_QUESTION_CAP} questions in a run. Keep a list of the decisions you made and report them to the user at the end, each in a line with the alternative you did not take.\n"
        )),
    }
    if !notes.trim().is_empty() {
        s.push_str("\nThe user's own instructions about decisions (they apply on top of the level; follow them):\n");
        s.push_str(notes.trim());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_round_trip_and_unknown_words_are_the_default() {
        for l in [DecisionLevel::AskForks, DecisionLevel::AskBig, DecisionLevel::Decide] {
            assert_eq!(DecisionLevel::parse(l.id()), l);
        }
        assert_eq!(DecisionLevel::parse("whatever"), DecisionLevel::AskForks);
        assert_eq!(DecisionLevel::parse(" DECIDE "), DecisionLevel::Decide);
    }

    #[test]
    fn only_decide_caps_the_questions() {
        assert_eq!(question_cap(DecisionLevel::AskForks), None);
        assert_eq!(question_cap(DecisionLevel::AskBig), None);
        assert_eq!(question_cap(DecisionLevel::Decide), Some(DECIDE_QUESTION_CAP));
    }

    #[test]
    fn the_chat_text_is_appended_to_the_global_one() {
        assert_eq!(merge_notes("always ask about naming", "this chat: no tests"), "always ask about naming\nthis chat: no tests");
        assert_eq!(merge_notes("  ", "only chat"), "only chat");
        assert_eq!(merge_notes("", ""), "");
        assert_eq!(merge_notes(&"x".repeat(MAX_NOTES_CHARS + 50), "").chars().count(), MAX_NOTES_CHARS);
    }

    #[test]
    fn the_default_level_without_notes_adds_nothing() {
        assert_eq!(paragraph(DecisionLevel::AskForks, ""), "");
        assert_eq!(paragraph(DecisionLevel::AskForks, "  \n"), "");
    }

    #[test]
    fn notes_apply_at_every_level_and_decide_says_it_reports() {
        for l in [DecisionLevel::AskForks, DecisionLevel::AskBig, DecisionLevel::Decide] {
            assert!(paragraph(l, "never rename public API").contains("never rename public API"), "{l:?}");
        }
        let p = paragraph(DecisionLevel::Decide, "");
        assert!(p.contains("report them to the user at the end"));
        assert!(p.contains(&DECIDE_QUESTION_CAP.to_string()));
        assert!(paragraph(DecisionLevel::AskBig, "").contains("big decisions"));
    }
}
