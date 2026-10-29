//! After a failed build: whose errors are these, and who else may have caused them?
//!
//! The pure rules (the journal of writes, the split, the wording, the notes) are `bennu_assistant`'s
//! `attribution` module. This is the glue that applies them to a build a conversation ran: it resolves the
//! compiler's paths, reads the process-wide journal and, besides telling the asking conversation, leaves a
//! system note in the inbox of every conversation that may have caused the failure. Bennu writes those
//! notes, not the model, and they reach their conversation with its next tool result or its next prompt —
//! an agent cannot be interrupted mid-stream.

use std::path::Path;

use bennu_assistant::prelude::{attribute, attribution_for_requester, attribution_note_for, Attribution, BuildError, Note, Suspect};

/// One error as the build facade has it: file, line, message.
pub(crate) type RawError = (Option<String>, Option<u32>, String);

/// The compiler's path as an absolute one: Cargo names files relative to the workspace it ran in.
fn absolute(root: &str, file: &str) -> String {
    let p = Path::new(file);
    if p.is_absolute() { file.to_string() } else { Path::new(root).join(p).to_string_lossy().into_owned() }
}

/// Everyone who should hear about the failure: the conversations that wrote since the last green build, and
/// the ones whose files the errors are in (the same people in the usual case).
fn recipients(a: &Attribution) -> Vec<Suspect> {
    let mut out = a.suspects.clone();
    for other in &a.elsewhere {
        if !out.iter().any(|s| s.conversation == other.conversation) {
            let files = other.errors.iter().filter_map(|e| e.file.clone()).collect();
            out.push(Suspect { conversation: other.conversation.clone(), title: other.title.clone(), files });
        }
    }
    out
}

/// Attribute a failed build to the conversations that may have broken it. Leaves notes for them and returns
/// what the asking conversation is told. `None` when the caller is not a conversation (the user, an external
/// client) or there is nobody else to point at.
pub(crate) fn explain(root: &str, target: &str, errors: Vec<RawError>) -> Option<String> {
    let me = crate::build_queue::current()?;
    if me.owner.is_user() {
        return None;
    }
    let errs: Vec<BuildError> = errors.into_iter().map(|(file, line, message)| BuildError { file: file.map(|f| absolute(root, &f)), line, message }).collect();
    let a = attribute(&me.owner.conversation, &errs, crate::build_queue::journal(), target);
    for suspect in recipients(&a) {
        if let Some((text, hash)) = attribution_note_for(&suspect, &me.owner.title, &me.owner.conversation, &errs) {
            crate::build_queue::inbox().push(&suspect.conversation, Note { text, hash });
        }
    }
    attribution_for_requester(&a, &|c| crate::assistant::conversation_active(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_assistant::prelude::Journal;

    #[test]
    fn a_relative_path_is_taken_from_the_root_and_an_absolute_one_kept() {
        let abs = if cfg!(windows) { r"C:\p\src\a.rs" } else { "/p/src/a.rs" };
        assert_eq!(absolute("/w", abs), abs);
        let joined = absolute("/w", "crates/core/src/lib.rs");
        assert!(joined.replace('\\', "/").ends_with("w/crates/core/src/lib.rs"), "{joined}");
    }

    #[test]
    fn the_owner_of_an_error_file_is_told_even_if_the_journal_calls_them_no_suspect() {
        let j = Journal::default();
        j.record("/p/a.rs", "other", "O");
        j.mark_green("/p", j.epoch());
        let errs = [BuildError { file: Some("/p/a.rs".into()), line: Some(1), message: "x".into() }];
        let a = attribute("me", &errs, &j, "/p");
        assert!(a.suspects.is_empty());
        let r = recipients(&a);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].conversation.as_str(), r[0].files.as_slice()), ("other", ["/p/a.rs".to_string()].as_slice()));
    }

    #[test]
    fn nobody_is_listed_twice() {
        let j = Journal::default();
        j.mark_green("/p", 0);
        j.record("/p/a.rs", "other", "O");
        let errs = [BuildError { file: Some("/p/a.rs".into()), line: None, message: "x".into() }];
        assert_eq!(recipients(&attribute("me", &errs, &j, "/p")).len(), 1);
    }
}
