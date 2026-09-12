//! Each kind's context out of what the backend knows, and where its output goes — for the kinds that need
//! no more than a name, a class or the Spring model. The configuration class is [`crate::config_class`]'s;
//! the DTO Lab's tests, [`crate::dtolab`]'s. Called through [`crate::templates_render::render_request`].
//!
//! A kind whose output is Java renders with [`render_code`], which also says what the output imports; each kind
//! puts those where its output goes — see [`crate::templates_imports`].

use std::path::Path;

use bennu_lsp::prelude::parse_snippet;
use bennu_proto::prelude::SnippetStop;
use bennu_templates::prelude::{
    insert_members, render_code, render_with, ClassTemplateContext, ConfigPropertiesContext, ConfigProperty,
    Directives, LiveContext, NewFileContext, Template,
};

use crate::frameworks::FrameworkService;
use crate::templates_facts::{facts_at, facts_for};
use crate::templates_imports::{import_edits, imports_note, with_imports};
use crate::templates_render::{class_of, source_of, RenderArgs, Rendered};

/// A directive that reads as yes. Written out rather than "anything but empty", so a
/// `bennu.stops: later` in somebody's notes does not turn the snippet grammar on.
fn truthy(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("true" | "yes" | "on" | "1"))
}

/// A New file template, for the name typed in a directory. `bennu.file` can name the file.
pub(crate) fn new_file(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let directory = args.directory.as_deref().ok_or("A new file needs `directory`")?;
    let directory = directory.trim_end_matches(['/', '\\']);
    let typed = args.name.as_deref().unwrap_or("NewFile");
    let context = NewFileContext::new(Path::new(directory), typed, &template.extension);
    let facts = facts_at(&args.root, &format!("{directory}/{}", context.file_name));
    let parameters = args.parameters.as_ref();
    let output = render_code(&template.text, &context, &facts, parameters)?;
    let file_name = match Directives::read(&template.text).get("file") {
        Some(pattern) => render_with(pattern, &context, &facts, parameters)?.trim().to_string(),
        None => context.file_name.clone(),
    };
    let path = format!("{directory}/{file_name}");
    let text = with_imports(&path, output.text, &output.imports);
    Ok(Rendered::file(&template.name, text, path, Vec::new()))
}

/// A template run on a class: members inserted into it, a file beside it, or text — `bennu.output` says
/// which, `members` when it does not.
pub(crate) fn class(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let (file, source) = source_of(args)?;
    // The class's file for naming: members go into it, and a file beside it is in the same part of the project.
    let facts = facts_at(&args.root, &file);
    let class = class_of(args, &file, &source)?;
    let path_in_file = class.path_in_file.clone();
    let context = ClassTemplateContext::new(class, &file);
    let parameters = args.parameters.as_ref();
    let output = render_code(&template.text, &context, &facts, parameters)?;
    let directives = Directives::read(&template.text);
    match directives.get("output").unwrap_or("members") {
        "file" => {
            let pattern = directives
                .get("file")
                .ok_or("A template with `bennu.output: file` names the file with `bennu.file: …`")?;
            let relative = render_with(pattern, &context, &facts, parameters)?;
            let path = format!("{}/{}", context.directory, relative.trim().trim_start_matches('/'));
            let text = with_imports(&path, output.text, &output.imports);
            Ok(Rendered::file(&template.name, text, path, Vec::new()))
        }
        "text" => Ok(Rendered::text(&template.name, output.text, imports_note(&output.imports).into_iter().collect())),
        _ => {
            let mut insertion = insert_members(&source, Some(&path_in_file), &output.text)?;
            // Asked for, never assumed: a template that writes `${spring.datasource.url}` into a
            // class would have it silently eaten by the snippet grammar. Parsed after the members
            // have been re-indented, so the stops are offsets into the text that actually lands.
            let stops = match truthy(directives.get("stops")) {
                false => Vec::new(),
                true => {
                    let parsed = parse_snippet(&insertion.text);
                    insertion.text = parsed.text;
                    parsed
                        .stops
                        .into_iter()
                        .map(|s| SnippetStop { start: s.start, end: s.end, group: s.index })
                        .collect()
                }
            };
            Ok(Rendered {
                output: "members".to_string(),
                file: Some(file),
                exists: true,
                template: template.name.clone(),
                import_edits: import_edits(&source, &output.imports),
                text: output.text,
                insertion: Some(insertion),
                insertion_stops: stops,
                notes: Vec::new(),
                source_lines: None,
            })
        }
    }
}

/// The keys a `@ConfigurationProperties` class binds, from the Spring model, as text for a property file.
pub(crate) fn config_properties(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let facts = facts_for(&args.root);
    let (file, source) = source_of(args)?;
    let class = class_of(args, &file, &source)?;
    let prefix = class
        .annotations
        .iter()
        .find(|a| a.name == "ConfigurationProperties")
        .and_then(|a| a.attributes.get("prefix").or_else(|| a.attributes.get("value")))
        .cloned()
        .ok_or("The class at the caret is not a @ConfigurationProperties class with a prefix")?;
    let properties: Vec<ConfigProperty> = FrameworkService::global()
        .config_bindings(&args.root)
        .iter()
        .filter(|b| b.root_prefix == prefix)
        .map(|b| ConfigProperty::new(&b.path, &prefix, &b.field, &b.owner_fqcn, &b.type_text))
        .collect();
    let mut notes = Vec::new();
    if properties.is_empty() {
        notes.push(format!(
            "No key binds under `{prefix}` — the class has no fields yet, or the Spring model is still being built."
        ));
    }
    let context = ConfigPropertiesContext::new(&prefix, &class.name, &class.fqn, properties);
    let text = render_with(&template.text, &context, &facts, args.parameters.as_ref())?;
    Ok(Rendered::text(&template.name, text, notes))
}

/// An abbreviation, as it would expand in the file it is typed in.
pub(crate) fn live(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let (file, source) = source_of(args)?;
    let facts = facts_at(&args.root, &file);
    let context = LiveContext::new(&file, &source);
    let output = render_code(&template.text, &context, &facts, args.parameters.as_ref())?;
    let mut notes = vec!["An abbreviation expands in the editor, where Tab walks its $1, $2 stops.".to_string()];
    if !output.imports.is_empty() {
        notes.push("Typed in a file, it adds the imports it needs.".to_string());
    }
    Ok(Rendered::text(&template.name, output.text, notes))
}
