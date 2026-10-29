//! Big tool results, answered with a digest.
//!
//! The formatters are in `bennu_assistant::digest`; this is the part that has a conversation: the full text
//! is kept under an id (`digest_store`) and the digest ends by saying so, in words the model acts on.

use bennu_assistant::prelude::{digest_text, digest_value, DIGEST_MIN_CHARS};

use super::*;
use crate::assistant::digest_store;

/// The sentence that closes a digest: what it is, and how to get the rest.
fn closing_line(id: &str, chars: usize) -> String {
    format!("[This is a digest of a {chars}-character result. `expand_result` with id \"{id}\" returns the detail: `match` keeps the lines containing some text (a file, a test, a message), `offset` and `limit` page through it.]")
}

impl ToolHost {
    /// `text`, or — when it is long and `tool` has a formatter — its digest. `value` is the tool's JSON result
    /// where it had one; `None` for a tool that answers in text.
    pub(super) fn digested(&self, tool: &str, value: Option<&Value>, text: String) -> String {
        if text.chars().count() < DIGEST_MIN_CHARS {
            return text;
        }
        let digest = match value {
            Some(v) => digest_value(tool, v),
            None => digest_text(tool, &text),
        };
        let Some(d) = digest else { return text };
        // A digest that is not clearly shorter is not worth the extra step.
        if d.head.len() * 2 > text.len() {
            return text;
        }
        let id = digest_store::store(&self.session.conversation_id, d.full);
        format!("{}\n{}", d.head, closing_line(&id, text.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closing_line_names_the_id_and_what_can_be_asked() {
        let line = closing_line("d3", 12_345);
        assert!(line.contains("12345-character") && line.contains("id \"d3\"") && line.contains("`match`") && line.contains("`offset`"));
    }
}
