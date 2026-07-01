//! [`PropertiesFormat`] — the [`SimpleFormat`] impl that lets
//! `.properties` ride on `arbor_studio_core::DefaultBackend` for the
//! doc/history/mutation/diff/query side.
//!
//! Format-specific work is delegated to the sibling modules: `project`
//! (parse + JSON projection + `$value` sentinel + kind/preview),
//! `mutate` (the structured mutation lowering over the line view),
//! `descriptor` (the capability matrix). The backend owns all the
//! boilerplate (registry, `History<String>` with dedup ON, encoding).
//!
//! `.properties` is the SPECIAL simple format: its F12 rename is
//! key-scoped (not string-leaf) and its F13 bulk coerces every value to
//! a string with an `(empty)` sentinel — so F12/F13 do NOT route through
//! `DefaultBackend`'s default `RefactorOps`. The launcher drives those
//! through [`crate::refactor::PropertiesRefactor`] instead.

use arbor_studio_core::prelude::{
    EncodingInfo, FormatDescriptor, ParseOutcome, SimpleFormat, SimpleMutation, StudioResult,
};
use serde_json::Value;

use crate::{descriptor, mutate, project};

/// The `.properties` format primitives for `arbor_studio_core::DefaultBackend`.
pub struct PropertiesFormat {
    descriptor: FormatDescriptor,
}

impl PropertiesFormat {
    pub fn new() -> Self {
        Self { descriptor: descriptor::build_descriptor() }
    }
}

impl Default for PropertiesFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleFormat for PropertiesFormat {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn parse(&self, text: &str, _encoding: &EncodingInfo) -> ParseOutcome {
        let (value, error) = project::parse_outcome(text);
        ParseOutcome { value, error }
    }

    fn detect_indent(&self, text: &str) -> String {
        project::detect_indent(text)
    }

    fn pretty(&self, text: &str) -> StudioResult<String> {
        // `.properties` has no canonical pretty form — we already preserve
        // every byte. Returning the current buffer keeps `Ctrl+Shift+I`
        // a no-op but lets the FE call `format_doc` indiscriminately.
        Ok(text.to_string())
    }

    fn mutate(&self, text: &str, mutation: SimpleMutation) -> StudioResult<String> {
        mutate::mutate(text, mutation)
    }

    fn node_kind(&self, v: &Value) -> String {
        project::node_kind(v)
    }

    fn preview_for(&self, v: &Value) -> String {
        project::preview_for(v)
    }

    // `variant_tag` defaults to None (.properties has no variant tags).
}

#[cfg(test)]
mod tests;
