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
    /// Absolute path of the file (unused by the transforms; echoed for symmetry).
    #[allow(dead_code)]
    pub file: String,
    /// The current buffer text.
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// One applicable intention — a stable id, a label, and the byte-range edit to apply.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OfferWire {
    pub id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Every intention applicable at the caret (empty when none fits).
#[arbor_rpc::handler]
fn bennu_intentions_at(_ctx: &BennuState, args: IntentionsArgs) -> Result<Vec<OfferWire>, String> {
    Ok(bennu_intentions::prelude::intentions_at(&args.source, args.offset)
        .into_iter()
        .map(|o| OfferWire {
            id: o.id,
            label: o.label,
            start: o.start,
            end: o.end,
            replacement: o.replacement,
        })
        .collect())
}
