//! What a Bennu schedule does when it fires, and how Bennu words the schedule tools. The job, the store and the
//! rules that move a job forward are `arbor-scheduler::jobs`; the tools are `imber-schedule`.

use arbor_scheduler::prelude::JobAction;
use imber_schedule::prelude::Flavour;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a job does when it fires. Both ask Imber something; neither can change the project (see `unattended`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// "Awake": wake a conversation with a saved prompt. The conversation keeps its history, so Imber picks up
    /// where it stopped. With no conversation (it was deleted) it starts a new one.
    Awake { conversation_id: Option<String>, prompt: String },
    /// A fresh conversation each time, with this prompt.
    Prompt { prompt: String },
}

impl Action {
    /// The conversation to wake, when the job has one.
    pub fn conversation(&self) -> Option<&str> {
        match self {
            Action::Awake { conversation_id, .. } => conversation_id.as_deref().filter(|c| !c.is_empty()),
            Action::Prompt { .. } => None,
        }
    }
}

impl JobAction for Action {
    fn prompt(&self) -> Option<&str> {
        match self {
            Action::Awake { prompt, .. } | Action::Prompt { prompt } => Some(prompt),
        }
    }

    fn word(&self) -> &'static str {
        match self {
            Action::Awake { .. } => "awake",
            Action::Prompt { .. } => "prompt",
        }
    }
}

/// The schedule tools as Bennu offers them.
pub struct BennuFlavour;

impl Flavour for BennuFlavour {
    type Action = Action;

    fn app(&self) -> &'static str {
        "Bennu"
    }

    fn unit(&self) -> &'static str {
        "project"
    }

    fn actions(&self) -> &'static [&'static str] {
        &["awake", "prompt"]
    }

    fn modes(&self) -> &'static [&'static str] {
        &["manual"]
    }

    fn actions_doc(&self) -> &'static str {
        "action `awake` wakes THIS conversation with `prompt` (it keeps its history); `prompt` starts a fresh conversation with it."
    }

    fn modes_doc(&self) -> &'static str {
        "it can read and search the project and leave proposed edits in its conversation for the user to review, but it never changes the project, runs commands, deletes files or asks questions; a request the user must answer ends the run as incomplete."
    }

    fn action_from_input(&self, word: Option<&str>, prompt: Option<String>, _input: &Value, conversation_id: &str) -> Result<Action, String> {
        let need = |p: Option<String>| p.ok_or("`prompt` is required for this action");
        match word {
            Some("awake") => Ok(Action::Awake { conversation_id: Some(conversation_id.to_string()).filter(|c| !c.is_empty()), prompt: need(prompt)? }),
            Some("prompt") => Ok(Action::Prompt { prompt: need(prompt)? }),
            other => Err(format!("unknown action {other:?} (awake, prompt)")),
        }
    }

    fn with_prompt(&self, current: &Action, prompt: String) -> Action {
        match current {
            Action::Awake { conversation_id, .. } => Action::Awake { conversation_id: conversation_id.clone(), prompt },
            Action::Prompt { .. } => Action::Prompt { prompt },
        }
    }

    /// There is one mode: a scheduled run never writes to the live project.
    fn mode_word(&self, _raw: &str) -> String {
        "manual".into()
    }

    fn mode_for(&self, _requested: Option<&str>, _conversation_mode: &str) -> String {
        "manual".into()
    }

    fn lowered_note(&self) -> &'static str {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn there_is_one_mode_whatever_is_asked_and_whatever_the_conversation_may_do() {
        let f = BennuFlavour;
        for asked in [None, Some("manual"), Some("auto"), Some("accept_edits"), Some("autonomous")] {
            for conversation in ["manual", "accept_edits", "autonomous"] {
                assert_eq!(f.mode_for(asked, conversation), "manual", "{asked:?} in {conversation}");
            }
        }
        assert_eq!(f.mode_word("autonomous"), "manual");
    }

    #[test]
    fn the_actions_are_awake_and_prompt_and_nothing_else() {
        let f = BennuFlavour;
        let awake = f.action_from_input(Some("awake"), Some("status?".into()), &json!({}), "c-1").unwrap();
        assert_eq!(awake.conversation(), Some("c-1"));
        assert_eq!((awake.word(), awake.prompt()), ("awake", Some("status?")));
        assert_eq!(f.action_from_input(Some("awake"), Some("x".into()), &json!({}), "").unwrap().conversation(), None, "no conversation to wake");
        assert_eq!(f.action_from_input(Some("prompt"), Some("x".into()), &json!({}), "c-1").unwrap().conversation(), None);
        assert!(f.action_from_input(Some("fetch"), None, &json!({}), "c").unwrap_err().contains("unknown action"));
        assert!(f.action_from_input(Some("prompt"), None, &json!({}), "c").unwrap_err().contains("prompt"));
        assert_eq!(f.with_prompt(&awake, "new".into()), Action::Awake { conversation_id: Some("c-1".into()), prompt: "new".into() });
    }

    #[test]
    fn an_action_survives_a_round_trip_through_json() {
        let a = Action::Awake { conversation_id: Some("c-1".into()), prompt: "p".into() };
        let text = serde_json::to_string(&a).unwrap();
        assert!(text.contains("\"kind\":\"awake\""), "{text}");
        assert_eq!(serde_json::from_str::<Action>(&text).unwrap(), a);
    }
}
