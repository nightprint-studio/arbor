//! The model naming its conversation.
//!
//! Until it does, a conversation is titled with the first words of its first message — which is usually a
//! request ("Leggi engine/fulcrum/docs/specifica…") and makes a poor tab label. The model, having read the
//! request, can name the *task* in a few words. It does so once, in its first reply, and again only when the user
//! asks: a title that changes under the user every few messages is a tab they cannot find. A title the user gave
//! by hand is theirs, so it too is changed only on request.

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};

pub(super) const NAME: &str = "set_title";

/// Longest title kept: what fits a tab and the conversation list.
const MAX_CHARS: usize = 60;

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        NAME,
        "Give this conversation its title, shown on its tab and in the conversation list: a few words (at most 60 characters) naming the task, in the user's language, without quotes or a full stop — `Spec PTN nello Studio`, `Fix the nextest parser`. Call it once, in your first response of a conversation — before or together with your first other tool call, never at the end of the work. Afterwards call it only when the user asks for a new title.",
        json!({"type":"object","properties":{
            "title":{"type":"string","description":"The title, a few words."}
        },"required":["title"]}),
    )]
}

/// Words that mean the user is asking about the title, in the languages a request is likely to come in.
const ASKING: [&str; 9] = ["title", "titolo", "titre", "título", "titel", "rename", "rinomina", "renomme", "nome della"];

/// Does `text` ask for the conversation to be (re)named?
pub(in crate::assistant) fn asks_for_title(text: &str) -> bool {
    let lower = text.to_lowercase();
    ASKING.iter().any(|w| lower.contains(w))
}

/// The title as it will be shown: one line, no wrapping quotes or trailing full stop, bounded.
fn clean(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let unquoted = line.trim_matches(|c| matches!(c, '"' | '\'' | '`' | '“' | '”' | '«' | '»')).trim().trim_end_matches('.').trim();
    (!unquoted.is_empty()).then(|| unquoted.chars().take(MAX_CHARS).collect())
}

impl ToolHost {
    pub(super) fn run_set_title(&self, input: &Value) -> Result<String, String> {
        if self.session.prompt_via.is_some() {
            return Err("a sub-agent does not name the conversation".into());
        }
        if !self.session.may_title {
            return Err("this conversation already has its title; change it only when the user asks for a new one".into());
        }
        let title = clean(input.get("title").and_then(Value::as_str).unwrap_or("")).ok_or("`title` is empty")?;
        *self.session.new_title.lock().unwrap_or_else(|p| p.into_inner()) = Some(title.clone());
        self.session.emit("title", json!({ "title": title }));
        Ok(format!("Titled “{title}”."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_one_clean_bounded_line() {
        assert_eq!(clean("  “Fix the parser.”  \nmore").as_deref(), Some("Fix the parser"));
        assert_eq!(clean(&"x".repeat(100)).map(|t| t.chars().count()), Some(60));
        assert!(clean(" \n ").is_none());
    }

    #[test]
    fn the_user_asking_for_a_title_is_recognised() {
        assert!(asks_for_title("Cambia il titolo in qualcosa di più chiaro"));
        assert!(asks_for_title("rename this conversation"));
        assert!(!asks_for_title("continue"));
    }
}
