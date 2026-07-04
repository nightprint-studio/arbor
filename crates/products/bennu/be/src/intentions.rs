//! `intentions` domain — `bennu_intentions_at`: every Alt+Enter quick-fix applicable at the caret.
//!
//! One handler over the whole `bennu-intentions` catalog: it returns the applicable transforms as
//! offers (id + label + edit), so the editor makes a single round-trip per Alt+Enter and adding an
//! intention is a change in the pure crate only. Replaces the old per-transform handlers.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_intentions_at`].
#[derive(Deserialize)]
pub struct IntentionsArgs {
    /// Absolute path of the file — drives the file-context intentions (fix the package to match the
    /// file's location).
    pub file: String,
    /// The current buffer text.
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// One applicable intention — a stable id, a label, and either a byte-range edit to apply or an
/// `action` the editor runs instead (e.g. a filesystem move). `action` is `None` for a plain edit.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OfferWire {
    pub id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    /// A non-edit action id the editor dispatches instead of applying the edit (`"move-to-package"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Every intention applicable at the caret (empty when none fits).
#[arbor_rpc::handler]
fn bennu_intentions_at(_ctx: &BennuState, args: IntentionsArgs) -> Result<Vec<OfferWire>, String> {
    // The pure source transforms (caret-anchored).
    let mut offers: Vec<OfferWire> = bennu_intentions::prelude::intentions_at(&args.source, args.offset)
        .into_iter()
        .map(|o| OfferWire {
            id: o.id,
            label: o.label,
            start: o.start,
            end: o.end,
            replacement: o.replacement,
            action: None,
        })
        .collect();

    // File-context intentions on a package mismatch: (a) rewrite the declaration to match the folder,
    // or (b) move the file to the folder matching the declaration. Both anchored on the same mismatch.
    if let Some(expected) = std::path::Path::new(&args.file)
        .parent()
        .and_then(bennu_java::prelude::infer_package)
    {
        if let Some((start, end, replacement)) =
            bennu_check::prelude::change_package(&args.source, &expected)
        {
            offers.push(OfferWire {
                id: "change-package".to_string(),
                label: format!("Set package to `{expected}`"),
                start,
                end,
                replacement,
                action: None,
            });
            // The move alternative — a filesystem action, not an edit (dispatched by the editor).
            if let Some(declared) = bennu_java::prelude::extract_symbols(&args.source).package {
                offers.push(OfferWire {
                    id: "move-to-package".to_string(),
                    label: format!("Move file to package `{declared}`"),
                    start: 0,
                    end: 0,
                    replacement: String::new(),
                    action: Some("move-to-package".to_string()),
                });
            }
        }
    }

    Ok(offers)
}
