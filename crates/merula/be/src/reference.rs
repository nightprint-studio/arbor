//! `reference` domain — the canonical DSL reference, exposed to the frontend.
//!
//! `merula_lang_reference` returns the whole `.merula` language catalogue (every
//! combinator, generator, signal, transform, mini-notation operator, …) from the
//! authoritative source in `merula-lang` ([`reference`]). The frontend loads
//! it once into a store and drives autocomplete, hover docs, and the Docs panel
//! off it — so the editor's language intelligence and the evaluator can never
//! drift. Static + cheap (a `Vec` of borrowed-static data); no state, no I/O.
//!
//! `merula-lang` stays serde-free (its only deps are `tree-sitter` + `cc`), so the
//! serde shape lives **here**, at the IPC boundary. The JSON field names / the
//! `kind` tag string are the contract the frontend `referenceStore` parses.

use merula::prelude::{reference, DslEntry, DslParam};
use serde::Serialize;

use merula_core::prelude::MerulaState;

/// IPC view of a [`DslParam`].
#[derive(Serialize)]
pub struct DslParamDto {
    name: &'static str,
    optional: bool,
    summary: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<&'static str>,
}

impl From<&DslParam> for DslParamDto {
    fn from(p: &DslParam) -> Self {
        DslParamDto { name: p.name, optional: p.optional, summary: p.summary, default: p.default }
    }
}

/// IPC view of a [`DslEntry`]. `kind` is the lowercase tag from
/// [`DslKind::as_str`](merula::prelude::DslKind::as_str).
#[derive(Serialize)]
pub struct DslEntryDto {
    name: &'static str,
    kind: &'static str,
    signature: &'static str,
    summary: &'static str,
    params: Vec<DslParamDto>,
    example: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns: Option<&'static str>,
}

impl From<DslEntry> for DslEntryDto {
    fn from(e: DslEntry) -> Self {
        DslEntryDto {
            name: e.name,
            kind: e.kind.as_str(),
            signature: e.signature,
            summary: e.summary,
            params: e.params.iter().map(DslParamDto::from).collect(),
            example: e.example,
            returns: e.returns,
        }
    }
}

/// Return the full `.merula` DSL reference catalogue.
#[arbor_rpc::handler]
fn merula_lang_reference(_ctx: &MerulaState) -> Result<Vec<DslEntryDto>, String> {
    Ok(reference().into_iter().map(DslEntryDto::from).collect())
}
