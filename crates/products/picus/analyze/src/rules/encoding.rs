//! `ENC001` / `ENC002` — the file's encoding, and whether the fix is safe.
//!
//! The two are a pair, and the pairing is the reason `ENC002` is what it is.
//!
//! `ENC001` says a file is no longer in the encoding its folder expects — someone
//! opened it in an editor that saved UTF-8 — and offers to convert it back. That
//! offer is only safe if every character in the file **can** be written in the
//! folder's encoding. `ENC002` is the rule that checks: it reports a character
//! that windows-1252 has no byte for, which is precisely the case where taking
//! `ENC001`'s fix would replace it with a question mark and lose data that nobody
//! would notice was gone.
//!
//! So `ENC002` is not "another encoding rule". It is the guard on the corrective
//! action of the first one, and that is why it is blocking while `ENC001` is only
//! worth a look.

use arbor_fs::prelude::encoding::{check_representable, encoding_for_label, EncodingSource};

use crate::context::Context;
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    for (script, placement) in context.project.placed() {
        let file = placement.file;
        let branch_id = placement.branch.id.as_str();

        // A pinned encoding is a decision, not a drift. The user (or the project
        // file) said what this is; reporting it back at them every run is how a
        // report earns a permanent "ignore all".
        if file.encoding_drifted() && file.encoding_source != EncodingSource::Forced {
            output.findings.push(
                Finding::new(
                    RuleId::Enc001,
                    Anchor::file(script.path, branch_id),
                    format!("This file is {} where the folder is {}", file.encoding, file.expected_encoding),
                    format!(
                        "It was saved by an editor that did not know, so every accented character \
                         in it is now a different byte sequence from the one the rest of `{}` uses. \
                         The descriptions those characters are in install wrong.",
                        placement.folder.label
                    ),
                )
                .fix(format!("Convert back to {}", file.expected_encoding))
                .build(),
            );
        }

        // The check that keeps the conversion above honest.
        let expected = encoding_for_label(&file.expected_encoding);
        if let Err(offender) = check_representable(script.source, expected) {
            output.findings.push(
                Finding::new(
                    RuleId::Enc002,
                    Anchor::at(script.path, branch_id, offender.line),
                    format!(
                        "'{}' cannot be written in {}",
                        offender.ch, file.expected_encoding
                    ),
                    format!(
                        "The folder expects {expected_label}, which has no byte for U+{code:04X} at \
                         column {column}. Saving this file in the folder's encoding — including the \
                         conversion the drift finding offers — replaces the character with a \
                         question mark, and the text it belongs to installs wrong with nothing to \
                         show for it.",
                        expected_label = file.expected_encoding,
                        code = offender.ch as u32,
                        column = offender.column,
                    ),
                )
                .build(),
            );
        }
    }
}
