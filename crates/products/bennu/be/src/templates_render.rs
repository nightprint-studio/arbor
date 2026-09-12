//! Rendering a code template, and writing what it produced — see [`crate::templates`].

use std::collections::BTreeMap;
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::SnippetStop;
use bennu_templates::prelude::{carry_lines, class_at, class_named, trace_lines, ClassModel, Insertion, Template, TemplateKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::templates::{kind_of, load, project_choice};
use crate::templates_imports::TemplateEdit;
use crate::templates_kinds as kinds;

/// Args for [`bennu_render_template`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RenderArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// `new-file`, `class`, `config-properties`, `config-class`, `validation-tests` or `live`.
    pub kind: String,
    /// The template's name; the one the project uses for the kind when omitted.
    #[serde(default)]
    pub template: Option<String>,
    /// For `class`, `config-properties`, `validation-tests` and `live`: the Java file holding the class.
    #[serde(default)]
    pub file: Option<String>,
    /// That file's text, when it differs from what is on disk.
    #[serde(default)]
    pub source: Option<String>,
    /// A byte offset inside the class to use; the file's first class when omitted.
    #[serde(default)]
    pub offset: Option<usize>,
    /// The simple name of the class to use in `file`, nested ones included — for one that is not the
    /// file's first. `offset` wins when both are given.
    #[serde(default)]
    pub class: Option<String>,
    /// For `config-class`: the key whose group of keys the class binds — the innermost group at `offset`
    /// when omitted. `file` is then the configuration file.
    #[serde(default)]
    pub prefix: Option<String>,
    /// For `config-class`: read the profile files beside `file` too (`application-dev.yml`). `true` when
    /// omitted.
    #[serde(default)]
    pub profiles: Option<bool>,
    /// For `config-class`: the keys the class gets, relative to `prefix` (`host`, `smtp.auth`), each with
    /// everything below it. Every key when omitted.
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    /// For `config-class`: the shape of a group, by its key below `prefix` — `true` a `Map<String, …>`, `false`
    /// a type per key. A group not named here is a `Map` when its entries look like one thing.
    #[serde(default)]
    pub maps: Option<BTreeMap<String, bool>>,
    /// For `new-file`: the directory the file goes in.
    #[serde(default)]
    pub directory: Option<String>,
    /// For `new-file`: the name typed for it.
    #[serde(default)]
    pub name: Option<String>,
    /// A template's text to render instead of a saved template's — an unsaved buffer being previewed.
    #[serde(default)]
    pub text: Option<String>,
    /// The extension of that text's output, when `text` is given (`java`, `yml`).
    #[serde(default)]
    pub extension: Option<String>,
    /// Values laid over what the template is rendered with, as a JSON object — to see it take a branch
    /// the class does not: `{"mode": "members"}`, `{"class": {"name": "Order"}}`. Objects merge key by
    /// key; anything else replaces what was there.
    #[serde(default)]
    pub parameters: Option<Value>,
    /// For a preview: say in `source_lines` which line of the template wrote each line of the output. It costs
    /// a second render.
    #[serde(default)]
    pub trace_lines: bool,
}

/// What a template produced, and where it would go. Nothing is written.
#[derive(Debug, Clone, Serialize)]
pub struct Rendered {
    pub template: String,
    pub text: String,
    /// `file` — a new file at `file`; `members` — inside the class in `file`, at `insertion`; `text` —
    /// to be copied or appended somewhere.
    pub output: String,
    pub file: Option<String>,
    /// The file already exists.
    pub exists: bool,
    /// For `members`: the byte offset in the class's file and the text, re-indented to the class.
    pub insertion: Option<Insertion>,
    /// The tab stops of `insertion.text`, for a template that asked for them with `bennu.stops: true`.
    ///
    /// Off unless asked for, and that is not timidity: snippet syntax and generated code share `$`,
    /// and parsing every template would quietly eat the `${…}` out of a Spring property file.
    #[serde(default)]
    pub insertion_stops: Vec<SnippetStop>,
    /// For `members`: the imports the members need, as edits to the class's file computed against the same text as
    /// `insertion` — applied with it. A new file has its imports in `text` already.
    pub import_edits: Vec<TemplateEdit>,
    /// What is worth knowing about the result.
    pub notes: Vec<String>,
    /// With `trace_lines`: for each line of what is shown — `insertion.text` when there is one, else `text` —
    /// the 1-based line of the template that wrote it, `0` for a line none did. `None` when it cannot be told.
    pub source_lines: Option<Vec<u32>>,
}

impl Rendered {
    /// A new file at `path` — or the one already there, which the caller opens instead.
    pub(crate) fn file(template: &str, text: String, path: String, notes: Vec<String>) -> Self {
        Self {
            exists: Path::new(&path).exists(),
            file: Some(path),
            output: "file".to_string(),
            template: template.to_string(),
            text,
            insertion: None,
            insertion_stops: Vec::new(),
            import_edits: Vec::new(),
            notes,
            source_lines: None,
        }
    }

    /// Text to copy, or to append to a file of the project.
    pub(crate) fn text(template: &str, text: String, notes: Vec<String>) -> Self {
        Self {
            output: "text".to_string(),
            file: None,
            exists: false,
            template: template.to_string(),
            text,
            insertion: None,
            insertion_stops: Vec::new(),
            import_edits: Vec::new(),
            notes,
            source_lines: None,
        }
    }
}

/// Render one of the user's code templates and say where the result goes — writing nothing.
///
/// `class` templates run on the class in `file` (at `offset`, or named by `class`): a builder inside it, a Spring Data
/// repository beside an entity, whatever the user's templates make. `config-properties` writes the keys
/// a `@ConfigurationProperties` class binds as a property file; `config-class` writes such a class from the
/// keys under `prefix` of the configuration `file`, into `directory` as `name`. `new-file` needs `directory`
/// and `name`.
/// `validation-tests` renders the DTO Lab's tests with the expected violations predicted from the
/// source — the DTO Lab itself checks them on the project's JVM.
///
/// Prefer this to writing such code yourself when a template exists for it: it is written the way the
/// user writes it. List the templates first with `bennu_list_templates`.
#[arbor_rpc::handler(mcp(title = "Render a code template", safety = read))]
fn bennu_render_template(_ctx: &BennuState, args: RenderArgs) -> Result<Rendered, String> {
    render_request(&args)
}

pub(crate) fn render_request(args: &RenderArgs) -> Result<Rendered, String> {
    let kind = kind_of(&args.kind)?;
    let template = template_for(args, kind)?;
    if !args.trace_lines {
        return render_kind(args, kind, &template);
    }
    let (rendered, traces) = trace_lines(|| render_kind(args, kind, &template));
    let mut rendered = rendered?;
    // The kind renders more than its template — a `bennu.file` name, say — so the trace is the template's own.
    let shown = rendered.insertion.as_ref().map_or(rendered.text.as_str(), |insertion| insertion.text.as_str());
    let lines = traces.iter().find(|trace| trace.template == template.text).map(|trace| carry_lines(&trace.output, &trace.lines, shown));
    rendered.source_lines = lines;
    Ok(rendered)
}

fn render_kind(args: &RenderArgs, kind: TemplateKind, template: &Template) -> Result<Rendered, String> {
    match kind {
        TemplateKind::NewFile => kinds::new_file(args, template),
        TemplateKind::Class => kinds::class(args, template),
        TemplateKind::ConfigProperties => kinds::config_properties(args, template),
        TemplateKind::ConfigClass => crate::config_class::render(args, template),
        TemplateKind::ValidationTests => crate::dtolab::render_predicted(args, template),
        TemplateKind::Live => kinds::live(args, template),
    }
}

/// What is rendered: the unsaved text being previewed, else the template named, else the project's.
fn template_for(args: &RenderArgs, kind: TemplateKind) -> Result<Template, String> {
    if let Some(text) = &args.text {
        return Ok(Template {
            name: args.template.clone().unwrap_or_else(|| "unsaved".to_string()),
            extension: args.extension.clone().unwrap_or_else(|| kind.default_extension().to_string()),
            text: text.clone(),
            path: None,
        });
    }
    let name = match &args.template {
        Some(name) => name.clone(),
        None => project_choice(&args.root, kind)
            .ok_or_else(|| format!("There is no {} template to use", kind.title().to_lowercase()))?,
    };
    load(kind, &name)
}

/// The class a render reads: the innermost around `offset`, else the one named `class`, else the
/// first the file declares.
pub(crate) fn class_of(args: &RenderArgs, file: &str, source: &str) -> Result<ClassModel, String> {
    let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or(file);
    match (args.offset, args.class.as_deref()) {
        (Some(at), _) => class_at(source, Some(at)).ok_or_else(|| "There is no class at the caret".to_string()),
        (None, Some(class)) => {
            class_named(source, class).ok_or_else(|| format!("{name} declares no class or record named `{class}`"))
        }
        (None, None) => class_at(source, None).ok_or_else(|| format!("{name} declares no class or record")),
    }
}

/// The file a render reads a class from, and its text.
pub(crate) fn source_of(args: &RenderArgs) -> Result<(String, String), String> {
    let file = args.file.clone().ok_or("This kind of template needs `file`, the Java file holding the class")?;
    let source = match &args.source {
        Some(source) => source.clone(),
        None => std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?,
    };
    Ok((file, source))
}

#[derive(Deserialize)]
pub struct CreateFileArgs {
    pub root: String,
    pub file: String,
    pub text: String,
}

/// Write generated text that is a new file, creating its directories. Refuses a file that exists —
/// that one is written through the editor, which knows whether it has unsaved changes.
#[arbor_rpc::handler]
fn bennu_template_create_file(_ctx: &BennuState, args: CreateFileArgs) -> Result<(), String> {
    let path = Path::new(&args.file);
    if !path.starts_with(&args.root) {
        return Err(format!("{} is outside the project", args.file));
    }
    if path.exists() {
        return Err(format!("{} already exists", args.file));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, args.text).map_err(|e| format!("{}: {e}", args.file))
}
