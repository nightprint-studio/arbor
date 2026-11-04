//! The generator registry: what exists, where it applies, and how to run it.
//!
//! A generator is metadata (id, icon, title, where it is offered), a function from the project and
//! the caret to *default options*, and a pure function from options and context to [`Output`]. The
//! backend lists the metadata for the palette, asks for defaults when a dialog opens, and calls
//! `generate` on every change of an option for the live preview — so `generate` must stay cheap.

use serde::Serialize;
use serde_json::Value;

use crate::context::GenContext;
use crate::output::Output;

/// Where a generator is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// With the caret inside a class: writes members into it.
    Class,
    /// Anywhere in a Java file; the class at the caret (if any) only seeds defaults.
    Java,
    /// In a Rust file.
    Rust,
    /// On a folder of the project tree: writes into that package.
    Folder,
    /// A Spring endpoint is picked in the dialog.
    Endpoint,
    /// Always, whichever file is open.
    Project,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratorMeta {
    pub id: &'static str,
    /// Dialog title and palette verb: `Generate Spring Security configuration`.
    pub title: &'static str,
    pub description: &'static str,
    /// A lucide icon name; the frontend maps it.
    pub icon: &'static str,
    /// `Spring` | `Persistence` | `JSON` | `Class` | `Test` | `Rust`.
    pub group: &'static str,
    /// `java` | `rust` — the language of the file it writes.
    pub language: &'static str,
    pub targets: &'static [Target],
    /// Artifacts the generator is about; it is offered first when the project has one of them.
    pub related: &'static [&'static str],
}

pub trait Generator: Sync {
    fn meta(&self) -> GeneratorMeta;

    /// The options the dialog opens with, inferred from the project and the caret.
    fn defaults(&self, ctx: &GenContext) -> Value;

    /// The files and members for `options`. Pure: no I/O, so it can run on every keystroke.
    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String>;

    /// Type names (simple) whose shape `generate` reads from `GenContext::types` — the backend resolves
    /// them through the index before calling it. Most generators read only the class at the caret.
    fn wanted_types(&self, _options: &Value, _ctx: &GenContext) -> Vec<String> {
        Vec::new()
    }
}

/// Every generator, in the order the palette lists them.
pub fn all() -> &'static [&'static dyn Generator] {
    &[
        &crate::generators::security::SpringSecurity,
        &crate::generators::converter::AttributeConverter,
        &crate::generators::jackson::JacksonSerde,
        &crate::generators::http_client::HttpClient,
        &crate::generators::dto_mapper::DtoMapper,
        &crate::generators::builder::Builder,
        &crate::generators::value_members::ValueMembers,
        &crate::generators::test_skeleton::TestSkeleton,
        &crate::generators::tauri_command::TauriCommand,
        &crate::generators::bevy_skeleton::BevySkeleton,
        &crate::generators::rust_http_client::RustHttpClient,
        &crate::generators::ratatui_app::RatatuiApp,
        &crate::generators::ratatui_widget::RatatuiWidget,
        &crate::generators::ratatui_list::RatatuiList,
        &crate::generators::ratatui_keymap::RatatuiKeymap,
        &crate::generators::ratatui_dialog::RatatuiDialog,
        &crate::generators::soap_call::SoapCall,
        &crate::generators::cxf_rest::CxfRest,
    ]
}

pub fn find(id: &str) -> Option<&'static dyn Generator> {
    all().iter().copied().find(|g| g.meta().id == id)
}

/// Run one generator by id.
pub fn run(id: &str, options: &Value, ctx: &GenContext) -> Result<Output, String> {
    find(id).ok_or_else(|| format!("There is no generator `{id}`"))?.generate(options, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_generator_has_a_default_that_generates() {
        let mut seen = std::collections::BTreeSet::new();
        let ctx = crate::testing::sample_context();
        for g in all() {
            let meta = g.meta();
            assert!(seen.insert(meta.id), "duplicate generator id {}", meta.id);
            assert!(!meta.targets.is_empty(), "{} is offered nowhere", meta.id);
            let defaults = g.defaults(&ctx);
            let out = g.generate(&defaults, &ctx);
            assert!(out.is_ok(), "{} fails on its own defaults: {:?}", meta.id, out.err());
        }
    }

    #[test]
    fn an_unknown_generator_is_an_error() {
        assert!(run("nope", &Value::Null, &GenContext::default()).is_err());
    }
}
