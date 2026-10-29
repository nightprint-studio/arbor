//! What the model is told about sub-agents. Its own file so the standing prompt in `prompt` is not touched by it;
//! the paragraph goes last (after the decision level), because it follows the conversation's own switch and
//! everything before it must stay identical, and cached, whether sub-agents are on or not.

const AGENTS_GUIDE: &str = "\nSub-agents: `spawn_agent` starts a helper with a clean context that does one task and sends back only a summary; `await_agents` collects the ones started in the background, and `resume_agent` sends a finished one a follow-up instruction on its kept thread (it remembers what it concluded, not the files it read, which it reads again) instead of starting over. Their kept threads are deleted when the conversation is compacted. They are for independent side tasks, for exploration that would fill your own context (reading many files to answer one question), and for work unrelated to what you were doing. Never use one for something a call or two answers, or for work you need to follow step by step: each one is a whole new context to pay for. Give it everything in `task` — it knows nothing else — and ask for a short summary with paths and line numbers. A read-only agent runs on a cheaper model, up to three at once; one that edits (only when your own edits are applied) runs while you wait. Read what comes back before relying on it. When you can do the work yourself, do.\n";

/// The system prompt with the sub-agents paragraph added when they are offered.
pub(super) fn with_agents(mut system: String, on: bool) -> String {
    if on {
        system.push_str(AGENTS_GUIDE);
    }
    system
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paragraph_is_added_last_and_only_when_agents_are_on() {
        assert_eq!(with_agents("base".into(), false), "base");
        let on = with_agents("base".into(), true);
        assert!(on.starts_with("base") && on.contains("spawn_agent") && on.contains("await_agents"));
        assert!(on.contains("Never use one for something a call or two answers"), "it says when not to");
    }
}
