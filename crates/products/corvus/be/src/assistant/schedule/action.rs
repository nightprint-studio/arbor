//! What a Corvus schedule does when it fires, and how Corvus words the schedule tools. The job, the store and
//! the rules that move a job forward are `arbor-scheduler::jobs`; the tools are `imber-schedule`.

use arbor_scheduler::prelude::JobAction;
use imber_schedule::prelude::Flavour;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::assistant::permission::Mode;

/// What a job does when it fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// "Awake": wake a conversation with a saved prompt. The conversation keeps its history, so Imber picks up
    /// where it stopped. With no conversation (it is gone after a restart) it starts a new one.
    Awake { conversation_id: Option<String>, prompt: String },
    /// A fresh conversation each time, with this prompt.
    Prompt { prompt: String },
    /// `git fetch` — no model, no cost.
    Fetch { remote: Option<String> },
}

impl JobAction for Action {
    fn prompt(&self) -> Option<&str> {
        match self {
            Action::Awake { prompt, .. } | Action::Prompt { prompt } => Some(prompt),
            Action::Fetch { .. } => None,
        }
    }

    fn word(&self) -> &'static str {
        match self {
            Action::Awake { .. } => "awake",
            Action::Prompt { .. } => "prompt",
            Action::Fetch { .. } => "fetch",
        }
    }
}

/// The schedule tools as Corvus offers them.
pub struct CorvusFlavour;

impl Flavour for CorvusFlavour {
    type Action = Action;

    fn app(&self) -> &'static str {
        "Corvus"
    }

    fn unit(&self) -> &'static str {
        "repository"
    }

    fn actions(&self) -> &'static [&'static str] {
        &["awake", "prompt", "fetch"]
    }

    fn modes(&self) -> &'static [&'static str] {
        &["manual", "auto"]
    }

    fn actions_doc(&self) -> &'static str {
        "action `awake` wakes THIS conversation with `prompt` (it keeps its history); `prompt` starts a fresh conversation with it; `fetch` just fetches."
    }

    fn modes_doc(&self) -> &'static str {
        "with mode `manual` (default) it is read-only; `auto` lets it make changes, but never destructive ones, and no more than this conversation may do."
    }

    fn extra_create_properties(&self) -> Value {
        json!({ "remote": { "type": "string", "description": "For fetch: the remote (default origin)." } })
    }

    fn action_from_input(&self, word: Option<&str>, prompt: Option<String>, input: &Value, conversation_id: &str) -> Result<Action, String> {
        let need = |p: Option<String>| p.ok_or("`prompt` is required for this action");
        match word {
            Some("awake") => Ok(Action::Awake { conversation_id: Some(conversation_id.to_string()), prompt: need(prompt)? }),
            Some("prompt") => Ok(Action::Prompt { prompt: need(prompt)? }),
            Some("fetch") => Ok(Action::Fetch { remote: input.get("remote").and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty()) }),
            other => Err(format!("unknown action {other:?} (awake, prompt, fetch)")),
        }
    }

    fn with_prompt(&self, current: &Action, prompt: String) -> Action {
        match current {
            Action::Awake { conversation_id, .. } => Action::Awake { conversation_id: conversation_id.clone(), prompt },
            Action::Prompt { .. } => Action::Prompt { prompt },
            other => other.clone(),
        }
    }

    fn mode_word(&self, raw: &str) -> String {
        Mode::parse(raw).word().to_string()
    }

    fn mode_for(&self, requested: Option<&str>, conversation_mode: &str) -> String {
        match (Mode::parse(requested.unwrap_or("manual")), Mode::parse(conversation_mode)) {
            (Mode::Auto, Mode::Auto) => Mode::Auto,
            _ => Mode::Manual,
        }
        .word()
        .to_string()
    }

    fn lowered_note(&self) -> &'static str {
        " Created read-only: this conversation is in manual mode, so a schedule cannot be given more freedom than it has."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_mode_word_is_the_read_only_one() {
        assert_eq!(CorvusFlavour.mode_word("autonomous"), "manual");
        assert_eq!(CorvusFlavour.mode_word(" AUTO "), "auto");
    }

    #[test]
    fn a_job_is_no_freer_than_the_conversation_that_made_it() {
        let f = CorvusFlavour;
        assert_eq!(f.mode_for(Some("auto"), "auto"), "auto");
        assert_eq!(f.mode_for(Some("auto"), "manual"), "manual");
        assert_eq!(f.mode_for(None, "auto"), "manual", "read-only unless asked");
        assert_eq!(f.mode_for(Some("manual"), "auto"), "manual");
    }

    #[test]
    fn the_action_words_and_prompts_are_what_the_tools_say() {
        let f = CorvusFlavour;
        let awake = f.action_from_input(Some("awake"), Some("status?".into()), &json!({}), "c-1").unwrap();
        assert_eq!(awake, Action::Awake { conversation_id: Some("c-1".into()), prompt: "status?".into() });
        assert_eq!((awake.word(), awake.prompt()), ("awake", Some("status?")));
        let fetch = f.action_from_input(Some("fetch"), None, &json!({"remote": "upstream"}), "c-1").unwrap();
        assert_eq!(fetch, Action::Fetch { remote: Some("upstream".into()) });
        assert_eq!(fetch.prompt(), None);
        assert!(f.action_from_input(Some("prompt"), None, &json!({}), "c").unwrap_err().contains("prompt"));
        assert!(f.action_from_input(Some("dance"), None, &json!({}), "c").unwrap_err().contains("unknown action"));
        assert_eq!(f.with_prompt(&awake, "new".into()), Action::Awake { conversation_id: Some("c-1".into()), prompt: "new".into() }, "an awake keeps its conversation");
        assert_eq!(f.with_prompt(&fetch, "new".into()), fetch, "a fetch has no prompt to change");
    }
}
