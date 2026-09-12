//! `templates` domain — code generated from templates the user owns.
//!
//! The kinds, the store and every context are `bennu-templates`' (validation tests: `bennu-dtolab`'s).
//! The backend's side is the seam between them and what the backend knows, in five files:
//!
//! - **this one** — where the profile keeps templates, which one each project chose, what a kind reads;
//! - [`crate::templates_manage`] — listing, creating, deleting and choosing them;
//! - [`crate::templates_render`] — rendering one and writing what it produced. One path serves the editor
//!   (Generate from a template, the New file dialog, a template's live preview) and an AI client
//!   (`bennu_render_template`), so the two cannot produce different text from the same template;
//! - [`crate::templates_kinds`] — each kind's context, out of the class at the caret or the Spring
//!   model's bindings; the configuration class is [`crate::config_class`]'s, the tests [`crate::dtolab`]'s;
//! - [`crate::templates_live`] — the user's abbreviations, for Java completion.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bennu_templates::prelude::{
    builtins, kind_dir, list_templates, load_template, schema_of, unmet, with_fact_schemas, Builtin, ClassTemplateContext, ConfigClassContext,
    ConfigPropertiesContext, LiveContext, NewFileContext, Template, TemplateFacts, TemplateInfo, TemplateKind,
};
use serde_json::Value;

/// `[templates]` in the per-repo `.arbor/bennu/config.toml`: kind id → the template the project uses.
pub(crate) const SECTION: &str = "templates";

/// `profiles/<active>/bennu/templates/` — one directory per kind.
pub(crate) fn root() -> PathBuf {
    static MIGRATED: OnceLock<()> = OnceLock::new();
    let root = arbor_core::prelude::bennu_config_path("templates");
    MIGRATED.get_or_init(|| migrate_legacy(&root));
    root
}

/// The DTO Lab kept its templates in `bennu/dtolab/templates/` before templates had kinds. Moved file
/// by file into the validation-tests directory, never over a file already there.
fn migrate_legacy(root: &Path) {
    let legacy = arbor_core::prelude::bennu_config_path("dtolab").join("templates");
    let Ok(entries) = std::fs::read_dir(&legacy) else { return };
    let target = kind_dir(root, TemplateKind::ValidationTests);
    if std::fs::create_dir_all(&target).is_err() {
        return;
    }
    for entry in entries.flatten() {
        let destination = target.join(entry.file_name());
        if !destination.exists() {
            let _ = std::fs::rename(entry.path(), destination);
        }
    }
}

pub(crate) fn builtins_of(kind: TemplateKind) -> &'static [Builtin] {
    match kind {
        TemplateKind::ValidationTests => bennu_dtolab::prelude::BUILTINS,
        other => builtins(other),
    }
}

/// What a template of `kind` is rendered with, as the JSON Schema completion reads.
pub(crate) fn schema(kind: TemplateKind) -> Value {
    with_fact_schemas(match kind {
        TemplateKind::ValidationTests => bennu_dtolab::prelude::context_schema(),
        TemplateKind::NewFile => schema_of::<NewFileContext>(),
        TemplateKind::Class => schema_of::<ClassTemplateContext>(),
        TemplateKind::ConfigProperties => schema_of::<ConfigPropertiesContext>(),
        TemplateKind::ConfigClass => schema_of::<ConfigClassContext>(),
        TemplateKind::Live => schema_of::<LiveContext>(),
    })
}

pub(crate) fn kind_of(id: &str) -> Result<TemplateKind, String> {
    TemplateKind::from_id(id).ok_or_else(|| {
        let ids: Vec<&str> = TemplateKind::ALL.iter().map(|k| k.id()).collect();
        format!("There is no template kind `{id}` — the kinds are {}", ids.join(", "))
    })
}

pub(crate) fn load(kind: TemplateKind, name: &str) -> Result<Template, String> {
    load_template(&root(), kind, name, builtins_of(kind))
}

/// The template a project generates `kind` with: its own choice while that template still exists, else the
/// first one that is not only a starter and whose `bennu.requires` the project meets.
pub(crate) fn project_choice(project: &str, kind: TemplateKind) -> Option<String> {
    let choices: BTreeMap<String, String> = crate::repo_config::load(project, SECTION);
    let templates = list_templates(&root(), kind, builtins_of(kind));
    choices
        .get(kind.id())
        .filter(|name| templates.iter().any(|t| &t.name == *name))
        .cloned()
        .or_else(|| {
            // The first the project can use — a record template that needs Java 16 is passed over on 11.
            let facts = crate::templates_facts::facts_for(project);
            templates
                .iter()
                .find(|t| !t.starter && unmet(&t.requires, &facts.project).is_none())
                .or_else(|| templates.iter().find(|t| !t.starter))
                .map(|t| t.name.clone())
        })
}

/// Each template with the first requirement the project does not meet, when a project is known.
pub(crate) fn with_unmet(mut templates: Vec<TemplateInfo>, facts: Option<&TemplateFacts>) -> Vec<TemplateInfo> {
    if let Some(facts) = facts {
        for template in &mut templates {
            template.unmet = unmet(&template.requires, &facts.project);
        }
    }
    templates
}
