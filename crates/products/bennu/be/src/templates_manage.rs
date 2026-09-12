//! Listing, creating, deleting and choosing code templates — see [`crate::templates`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use bennu_templates::prelude::{
    check_template_name, create_template, delete_template, kind_dir, list_templates, rename_template, set_directive,
    TemplateInfo, TemplateKind, CUSTOM_FILTERS,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::templates::{builtins_of, kind_of, load, project_choice, root, schema, with_unmet, SECTION};

/// Args for [`bennu_list_templates`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTemplatesArgs {
    /// Absolute path to a project root, to say which template that project uses for each kind.
    #[serde(default)]
    pub root: Option<String>,
    /// One kind — `new-file`, `class`, `config-properties`, `config-class`, `validation-tests` or `live`. Every kind
    /// when omitted.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KindTemplates {
    pub kind: String,
    pub title: String,
    pub description: String,
    /// The extension a new template of this kind gets.
    pub extension: String,
    /// Where this kind's templates are kept.
    pub directory: String,
    pub templates: Vec<TemplateInfo>,
    /// The template the project generates with, when a project was given.
    pub project: Option<String>,
    /// The kind only runs on a Java project — it reads a class, the Spring model or Bean Validation.
    pub java_only: bool,
    /// A new template of this kind may be written for any language, so the dialog asks which.
    pub picks_language: bool,
}

/// List the code templates Bennu generates from, by kind: New file templates, templates that generate
/// from a class (a builder, a repository for an entity), configuration properties for a
/// `@ConfigurationProperties` class, the DTO Lab's validation tests, and abbreviations. Each template
/// comes with its description and whether it is built in; with a project, the one that project uses.
///
/// Use it before `bennu_render_template`, to find the template that produces what you are about to
/// write by hand — the user's templates are how they want that code to look.
#[arbor_rpc::handler(mcp(title = "List code templates", safety = read))]
fn bennu_list_templates(_ctx: &BennuState, args: ListTemplatesArgs) -> Result<Vec<KindTemplates>, String> {
    let kinds = match &args.kind {
        Some(id) => vec![kind_of(id)?],
        None => TemplateKind::ALL.to_vec(),
    };
    let root = root();
    let facts = args.root.as_deref().map(crate::templates_facts::facts_for);
    Ok(kinds
        .into_iter()
        .map(|kind| KindTemplates {
            kind: kind.id().to_string(),
            title: kind.title().to_string(),
            description: kind.description().to_string(),
            extension: kind.default_extension().to_string(),
            directory: kind_dir(&root, kind).display().to_string(),
            templates: with_unmet(list_templates(&root, kind, builtins_of(kind)), facts.as_ref()),
            project: args.root.as_deref().and_then(|project| project_choice(project, kind)),
            java_only: kind.java_only(),
            picks_language: kind.picks_language(),
        })
        .collect())
}

/// What a name has to be for a kind, on top of [`check_template_name`]: an abbreviation's name is what
/// gets typed, so it has to be a word the editor completes.
fn check_name_for(kind: TemplateKind, name: &str) -> Result<(), String> {
    check_template_name(name)?;
    match kind == TemplateKind::Live && !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        true => Err("An abbreviation is a word: letters, digits and `_`".to_string()),
        false => Ok(()),
    }
}

#[derive(Deserialize)]
pub struct NewTemplateArgs {
    pub kind: String,
    pub name: String,
    /// The template to copy; the kind's first built-in when absent.
    #[serde(default)]
    pub from: Option<String>,
    /// What it writes — `java`, `rs`, `sql`, or empty for a template whose output has no language.
    /// The copied template's own extension when absent. Only the kinds that
    /// [`TemplateKind::picks_language`] accept one.
    #[serde(default)]
    pub extension: Option<String>,
}

/// Create a template as a copy of another, and return its path for the editor to open.
#[arbor_rpc::handler]
fn bennu_template_new(_ctx: &BennuState, args: NewTemplateArgs) -> Result<String, String> {
    let kind = kind_of(&args.kind)?;
    check_name_for(kind, &args.name)?;
    let source = match args.from.as_deref().or_else(|| builtins_of(kind).first().map(|b| b.name)) {
        Some(from) => Some(load(kind, from)?),
        None => None,
    };
    let (copied, text) = match source {
        Some(template) => (template.extension, template.text),
        None => (kind.default_extension().to_string(), String::new()),
    };
    // The language is the author's for the kinds that write what they are told to; for the rest it
    // is the kind's own business, and a Java class template asked to write `.rs` would be nonsense
    // with a file name to match.
    let extension = match (kind.picks_language(), args.extension) {
        (true, Some(chosen)) => chosen.trim().trim_start_matches('.').to_lowercase(),
        _ => copied,
    };
    let path = create_template(&root(), kind, &args.name, &extension, &text, builtins_of(kind))?;
    Ok(path.display().to_string())
}

#[derive(Deserialize)]
pub struct TemplateRefArgs {
    pub kind: String,
    pub name: String,
}

/// Where a built-in is written out to be read: the data directory, outside the templates the user owns, so
/// no list of theirs shows it and nothing they save lands on it.
fn builtin_copy(kind: TemplateKind, name: &str, extension: &str) -> PathBuf {
    let file = match extension.is_empty() {
        true => format!("{name}.jinja"),
        false => format!("{name}.{extension}.jinja"),
    };
    arbor_core::prelude::bennu_data_dir().join("builtin-templates").join(kind.id()).join(file)
}

/// The file to open a template from: the user's own, or a built-in written out first — which the editor
/// opens read-only, with its preview beside it like any other template.
#[arbor_rpc::handler]
fn bennu_template_open(_ctx: &BennuState, args: TemplateRefArgs) -> Result<String, String> {
    let kind = kind_of(&args.kind)?;
    let template = load(kind, &args.name)?;
    if let Some(path) = template.path {
        return Ok(path.display().to_string());
    }
    let path = builtin_copy(kind, &template.name, &template.extension);
    // Rewritten whenever it differs, so the built-in read is the one this Bennu ships.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(template.text.as_str()) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, &template.text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(path.display().to_string())
}

#[derive(Deserialize)]
pub struct RenameTemplateArgs {
    pub kind: String,
    pub name: String,
    pub new_name: String,
    /// The open project, so the project that generates with this template follows the new name.
    #[serde(default)]
    pub root: Option<String>,
}

/// Rename one of the user's templates, keeping what it writes. Returns its new path.
#[arbor_rpc::handler]
fn bennu_template_rename(_ctx: &BennuState, args: RenameTemplateArgs) -> Result<String, String> {
    let kind = kind_of(&args.kind)?;
    check_name_for(kind, &args.new_name)?;
    let path = rename_template(&root(), kind, &args.name, &args.new_name, builtins_of(kind))?;
    // A project pointing at the old name would silently fall back to the kind's first template, which
    // is the one way a rename could change what a project generates.
    if let Some(project) = args.root.as_deref() {
        let mut choices: BTreeMap<String, String> = crate::repo_config::load(project, SECTION);
        if choices.get(kind.id()).is_some_and(|chosen| chosen == &args.name) {
            choices.insert(kind.id().to_string(), args.new_name.clone());
            crate::repo_config::save(project, SECTION, &choices)?;
        }
    }
    Ok(path.display().to_string())
}

/// A template's text, for showing what it writes without opening it.
///
/// The settings list is a list of names until you can see one: the body is the whole of what tells
/// `team-builder` from `builder`, and having to open a tab to find out is what made that list a
/// filing cabinet.
#[arbor_rpc::handler]
fn bennu_template_text(_ctx: &BennuState, args: TemplateRefArgs) -> Result<String, String> {
    Ok(load(kind_of(&args.kind)?, &args.name)?.text)
}

#[arbor_rpc::handler]
fn bennu_template_delete(_ctx: &BennuState, args: TemplateRefArgs) -> Result<(), String> {
    let kind = kind_of(&args.kind)?;
    delete_template(&root(), kind, &args.name, builtins_of(kind))
}

#[derive(Deserialize)]
pub struct AbbrevArgs {
    /// The template's name, which is its file name.
    pub name: String,
    /// The word that expands it. Empty puts it back to being named by its file.
    pub abbrev: String,
}

/// Set the word that expands one of your abbreviations, independently of what its file is called.
///
/// The two were the same thing, and a file name makes a poor trigger word: `logd` is what you type,
/// `logger-for-this-class` is what you want to recognise in a list of thirty templates.
#[arbor_rpc::handler]
fn bennu_template_set_abbrev(_ctx: &BennuState, args: AbbrevArgs) -> Result<(), String> {
    let word = args.abbrev.trim();
    if !word.is_empty() && !word.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("An abbreviation is a word: letters, digits and `_`".to_string());
    }
    let template = load(TemplateKind::Live, &args.name)?;
    let path = template
        .path
        .ok_or_else(|| format!("`{}` is built in and cannot be changed", args.name))?;
    let text = set_directive(&template.text, "abbrev", word);
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Deserialize)]
pub struct ProjectTemplateArgs {
    pub root: String,
    pub kind: String,
    pub name: String,
}

/// Choose the template a project generates a kind with.
#[arbor_rpc::handler]
fn bennu_template_set_project(_ctx: &BennuState, args: ProjectTemplateArgs) -> Result<(), String> {
    let kind = kind_of(&args.kind)?;
    let mut choices: BTreeMap<String, String> = crate::repo_config::load(&args.root, SECTION);
    choices.insert(kind.id().to_string(), args.name);
    crate::repo_config::save(&args.root, SECTION, &choices)
}

#[derive(Deserialize)]
pub struct KindArgs {
    pub kind: String,
}

#[derive(Serialize)]
pub struct TemplateSchema {
    /// The JSON Schema of everything a template of the kind is rendered with.
    pub context: Value,
    /// The filters Bennu adds to Jinja's own.
    pub filters: Vec<FilterDoc>,
}

#[derive(Serialize)]
pub struct FilterDoc {
    pub name: &'static str,
    pub doc: &'static str,
}

/// What a template of a kind can read, and the filters Bennu adds — for completion inside a template.
#[arbor_rpc::handler]
fn bennu_template_schema(_ctx: &BennuState, args: KindArgs) -> Result<TemplateSchema, String> {
    Ok(TemplateSchema {
        context: schema(kind_of(&args.kind)?),
        filters: CUSTOM_FILTERS.iter().map(|&(name, doc)| FilterDoc { name, doc }).collect(),
    })
}
