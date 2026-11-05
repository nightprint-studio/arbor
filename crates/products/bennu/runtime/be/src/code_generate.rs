//! `code_generate` domain — the code generators of `bennu-generate`, fed with what only the backend knows.
//!
//! The generators are pure: `options + GenContext -> Output`. This is where the context comes from — the
//! project's classpath and Java level (the template engine's facts, so "has Lombok" means one thing
//! everywhere), the class at the caret, the Spring request mappings, the shapes of the types an option
//! names (through the index) — and where the output is turned into what the editor applies: the paths of
//! new files and, for the open file, byte-range edits (members, annotations and their imports) that the
//! frontend applies as one undo step.
//!
//! Three calls: [`bennu_generate_list`] (the palette's entries), [`bennu_generate_open`] (a dialog's
//! defaults and what was detected) and [`bennu_generate_preview`] (every change of an option). Writing new
//! files is `bennu_template_create_file`'s, which refuses one that exists.

use std::collections::BTreeMap;
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_generate::prelude::{
    all, annotation_insert, find, path_of, source_roots, type_names, ClassInfo, EndpointInfo, EndpointParam, Extra, FieldInfo, GenContext, Generator,
    GeneratorMeta, MemberTarget, Output, ProjectInfo,
};
use bennu_intentions::prelude::{format_source, FormatStyle};
use bennu_java::prelude::TypeRef;
use bennu_templates::prelude::{class_at, insert_members, ClassModel, FieldModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint_schema::SourceReader;
use crate::index_service::IndexService;
use crate::templates_facts::facts_for;
use crate::templates_imports::{import_edits, TemplateEdit};

/// Where the user is: the project, the file under the caret and the caret in it.
#[derive(Deserialize, Clone)]
pub struct Site {
    pub root: String,
    #[serde(default)]
    pub file: Option<String>,
    /// The buffer, possibly unsaved.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// A folder of the project tree the generator was opened on: new classes go into its package.
    #[serde(default)]
    pub directory: Option<String>,
}

// ── Context ──────────────────────────────────────────────────────────────────

fn project_info(root: &str) -> ProjectInfo {
    let facts = facts_for(root);
    ProjectInfo { java: facts.project.java, versions: facts.project.versions.clone(), resolved: facts.project.resolved }
}

fn field_info(f: &FieldModel) -> FieldInfo {
    FieldInfo {
        name: f.name.clone(),
        type_name: f.type_name.clone(),
        is_final: f.is_final,
        id: f.id,
        annotations: f.annotation_names.clone(),
        getter: f.getter.clone(),
        setter: f.setter.clone(),
    }
}

fn class_info(model: &ClassModel) -> ClassInfo {
    ClassInfo {
        name: model.name.clone(),
        package: model.package.clone(),
        kind: model.kind.clone(),
        annotations: model.annotation_names.clone(),
        fields: model.fields.iter().map(field_info).collect(),
        constants: model.constants.clone(),
        // A constructor has no return type; it is not a method to test or to call.
        methods: model.methods.iter().filter(|m| !m.returns.is_empty()).map(|m| m.name.clone()).collect(),
        superclass: model.superclass.clone(),
    }
}

fn is_java(file: &str) -> bool {
    Path::new(file).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("java"))
}

fn source_of(site: &Site) -> String {
    site.source.clone().or_else(|| site.file.as_ref().and_then(|f| std::fs::read_to_string(f).ok())).unwrap_or_default()
}

/// The package of the application class, when a scan of the project has found one.
fn application_package(root: &str) -> Option<String> {
    let file = crate::main_classes::cached_boot_application(root, root)?;
    bennu_java::prelude::infer_package(Path::new(&file).parent()?)
}

/// New classes go under the application's package when the caret is inside it, else beside the caret's class.
fn default_package(root: &str, caret_package: &str) -> String {
    match application_package(root) {
        Some(app) if caret_package.is_empty() || caret_package == app || caret_package.starts_with(&format!("{app}.")) => app,
        _ => caret_package.to_string(),
    }
}

pub(crate) fn endpoint_infos(root: &str) -> Vec<(EndpointInfo, String)> {
    let Some(model) = crate::frameworks::spring_model_of(root) else { return Vec::new() };
    model
        .endpoints
        .iter()
        .map(|e| {
            let reader = SourceReader::new(root, &e.file);
            let info = EndpointInfo {
                id: e.label(),
                methods: e.methods.clone(),
                path: e.path.clone(),
                controller: e.class_fqcn.rsplit('.').next().unwrap_or(&e.class_fqcn).to_string(),
                handler: e.handler.clone(),
                return_type: e.return_type.clone(),
                // The media types as values: Spring's constants are known, the project's are read from source.
                produces: resolve_media_types_from_source(&e.produces_raw, &reader),
                consumes: resolve_media_types_from_source(&e.consumes_raw, &reader),
                params: e
                    .params
                    .iter()
                    .map(|p| EndpointParam {
                        name: p.effective_name().to_string(),
                        type_text: p.type_text.clone(),
                        binding: p.binding.clone(),
                        required: p.required,
                    })
                    .collect(),
            };
            (info, e.file.clone())
        })
        .collect()
}

fn type_text(t: &TypeRef) -> String {
    let simple = t.binary_name.rsplit(['/', '$']).next().unwrap_or(&t.binary_name);
    let mut out = if t.type_args.is_empty() {
        simple.to_string()
    } else {
        format!("{simple}<{}>", t.type_args.iter().map(type_text).collect::<Vec<_>>().join(", "))
    };
    for _ in 0..t.dims {
        out.push_str("[]");
    }
    out
}

/// `produces` / `consumes` as media types, the project's own constants read from the sources around the controller.
fn resolve_media_types_from_source(raw: &str, reader: &SourceReader) -> String {
    bennu_spring::prelude::resolve_media_types_with_sources(raw, &|class| reader.read(class))
}

/// The fields of the type `name` as `file` means it: whether it is an enum, its constants, and (name, type) of each field.
pub(crate) fn member_fields(file: &str, name: &str) -> Option<(bool, Vec<String>, Vec<(String, String)>)> {
    let shape = shape_of(file, name)?;
    Some((shape.is_enum(), shape.constants.clone(), shape.fields.iter().map(|f| (f.name.clone(), f.type_name.clone())).collect()))
}

/// The shape of the type `name` means in `file`, through the index.
fn shape_of(file: &str, name: &str) -> Option<ClassInfo> {
    let resolved = IndexService::global().type_shape(file, name)?;
    let cm = &resolved.members;
    let binary = resolved.binary.replace('/', ".");
    let (package, simple) = binary.rsplit_once('.').map(|(p, s)| (p.to_string(), s.to_string())).unwrap_or((String::new(), binary.clone()));
    let simple = simple.rsplit('$').next().unwrap_or(&simple).to_string();
    let kind = if cm.flags.is_enum {
        "enum"
    } else if cm.flags.is_record {
        "record"
    } else if cm.flags.is_interface {
        "interface"
    } else {
        "class"
    };
    let fields = cm
        .fields
        .iter()
        .filter(|f| !f.is_static)
        .map(|f| FieldInfo { name: f.name.clone(), type_name: type_text(&f.return_type), is_final: f.is_final, ..FieldInfo::default() })
        .collect();
    let constants = if cm.flags.is_enum {
        cm.fields.iter().filter(|f| f.is_static && f.return_type.binary_name == resolved.binary).map(|f| f.name.clone()).collect()
    } else {
        Vec::new()
    };
    Some(ClassInfo {
        name: simple,
        package,
        kind: kind.to_string(),
        fields,
        constants,
        methods: cm.methods.iter().map(|m| m.name.clone()).collect(),
        ..ClassInfo::default()
    })
}

/// Resolve `names` (and, a level at a time, the types their fields mention) into `types`.
fn resolve_types(files: &[String], names: Vec<String>, types: &mut BTreeMap<String, ClassInfo>) {
    let mut pending = names;
    for _ in 0..3 {
        let mut next: Vec<String> = Vec::new();
        for name in pending.drain(..) {
            if types.contains_key(&name) {
                continue;
            }
            let Some(shape) = files.iter().find_map(|file| shape_of(file, &name)) else { continue };
            for field in &shape.fields {
                next.extend(type_names(&field.type_name));
            }
            types.insert(name, shape);
        }
        if next.is_empty() {
            break;
        }
        pending = next;
    }
}

struct Built {
    ctx: GenContext,
    model: Option<ClassModel>,
    source: String,
}

fn build(site: &Site, generator: &dyn Generator, options: Option<&Value>, with_endpoints: bool) -> Built {
    let source = source_of(site);
    let model = site.file.as_deref().filter(|f| is_java(f)).and_then(|_| class_at(&source, site.offset));
    let class = model.as_ref().map(class_info);
    let caret_package = class.as_ref().map(|c| c.package.clone()).unwrap_or_default();
    let folder_package = site.directory.as_deref().and_then(|d| bennu_java::prelude::infer_package(Path::new(d)));
    let mut ctx = GenContext {
        project: project_info(&site.root),
        file: site.file.clone(),
        package: folder_package.unwrap_or_else(|| default_package(&site.root, &caret_package)),
        app_name: Path::new(&site.root).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string(),
        class,
        ..GenContext::default()
    };
    let anchor = site.file.clone().unwrap_or_default();
    // The field types of the class at the caret: enough for a converter to know an enum from a class.
    let mut wanted: Vec<String> = ctx.class.as_ref().map(|c| c.fields.iter().flat_map(|f| type_names(&f.type_name)).take(32).collect()).unwrap_or_default();
    let mut file_of: BTreeMap<String, String> = BTreeMap::new();
    if with_endpoints {
        let rows = endpoint_infos(&site.root);
        for (info, file) in rows {
            file_of.insert(info.id.clone(), file);
            ctx.endpoints.push(info);
        }
    }
    if let Some(options) = options {
        wanted.extend(generator.wanted_types(options, &ctx));
    }
    // Names are resolved against the files that mention them: the controllers' for their types, else the caret's.
    // Only the picked endpoints' controllers (the first one before any is picked): a name that is not found in a
    // file costs a read of it, so the search stays short.
    let picked: Vec<&str> = options
        .and_then(|o| o.get("endpoints"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut files: Vec<String> = Vec::new();
    for e in ctx.endpoints.iter().filter(|e| picked.contains(&e.id.as_str())).chain(ctx.endpoints.first()) {
        if let Some(file) = file_of.get(&e.id) {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
        if files.len() >= 6 {
            break;
        }
    }
    if !anchor.is_empty() {
        files.push(anchor);
    }
    if !files.is_empty() {
        let mut types = BTreeMap::new();
        resolve_types(&files, wanted, &mut types);
        ctx.types = types;
    }
    Built { ctx, model, source }
}

// ── Listing ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListArgs {
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Serialize)]
pub struct Listing {
    #[serde(flatten)]
    pub meta: GeneratorMeta,
    /// The project has a dependency this generator is about: offered first.
    pub suggested: bool,
}

/// Every generator, for the command palette and the menus.
#[arbor_rpc::handler]
fn bennu_generate_list(_ctx: &BennuState, args: ListArgs) -> Result<Vec<Listing>, String> {
    let project = args.root.as_deref().filter(|r| !r.is_empty()).map(project_info);
    Ok(all()
        .iter()
        .map(|g| {
            let meta = g.meta();
            let suggested = project.as_ref().is_some_and(|p| meta.related.iter().any(|r| p.has(r)));
            Listing { meta, suggested }
        })
        .collect())
}

// ── Open ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OpenArgs {
    pub id: String,
    #[serde(flatten)]
    pub site: Site,
}

#[derive(Serialize)]
pub struct Opened {
    pub meta: GeneratorMeta,
    /// The options the dialog opens with.
    pub defaults: Value,
    /// What was detected, as short chips: `Spring Boot 3.2.1`, `jakarta`, `Lombok`.
    pub detected: Vec<String>,
    pub class: Option<ClassInfo>,
    pub package: String,
    /// The app's request mappings, for the generators that pick one.
    pub endpoints: Vec<EndpointInfo>,
}

fn chips(p: &ProjectInfo) -> Vec<String> {
    let mut out = Vec::new();
    if p.java > 0 {
        out.push(format!("Java {}", p.java));
    }
    let versioned = [
        ("spring-boot", "Spring Boot"),
        ("spring-security-web", "Spring Security"),
        ("spring-web", "Spring Framework"),
        ("jackson-databind", "Jackson"),
        ("hibernate-core", "Hibernate"),
        ("mapstruct", "MapStruct"),
    ];
    for (artifact, label) in versioned {
        if let Some(v) = p.versions.iter().find(|(id, _)| id.rsplit(':').next() == Some(artifact)).map(|(_, v)| v) {
            out.push(if v.is_empty() { label.to_string() } else { format!("{label} {v}") });
        }
    }
    if p.spring_boot().is_some() || p.has("jakarta.persistence-api") || p.has("javax.persistence-api") || p.has("spring-web") {
        out.push(p.ee().to_string());
    }
    for (artifact, label) in [("lombok", "Lombok"), ("spring-boot-starter-actuator", "Actuator"), ("assertj-core", "AssertJ"), ("mockito-core", "Mockito")] {
        if p.has(artifact) {
            out.push(label.to_string());
        }
    }
    out
}

/// A generator's defaults for the caret, and what the project was detected to be.
#[arbor_rpc::handler]
fn bennu_generate_open(_ctx: &BennuState, args: OpenArgs) -> Result<Opened, String> {
    let generator = find(&args.id).ok_or_else(|| format!("There is no generator `{}`", args.id))?;
    let meta = generator.meta();
    let wants_endpoints = args.id == "http-client";
    let built = build(&args.site, generator, None, wants_endpoints);
    let defaults = generator.defaults(&built.ctx);
    Ok(Opened {
        meta,
        defaults,
        detected: chips(&built.ctx.project),
        class: built.ctx.class.clone(),
        package: built.ctx.package.clone(),
        endpoints: built.ctx.endpoints.clone(),
    })
}

// ── Preview ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PreviewArgs {
    pub id: String,
    #[serde(flatten)]
    pub site: Site,
    #[serde(default)]
    pub options: Value,
}

#[derive(Serialize)]
pub struct PlannedFile {
    pub path: String,
    pub name: String,
    pub text: String,
    pub language: &'static str,
    /// Already on disk: writing it is refused, so the dialog says so before Generate.
    pub exists: bool,
}

#[derive(Serialize)]
pub struct MembersPreview {
    pub text: String,
    /// The file the members go into.
    pub file: Option<String>,
    pub class: Option<String>,
}

#[derive(Serialize)]
pub struct Preview {
    pub files: Vec<PlannedFile>,
    /// What lands in the open file: members, annotations, imports — as byte-range edits.
    pub edits: Vec<TemplateEdit>,
    pub members: Option<MembersPreview>,
    /// `@Convert(…)  on  birthDate`, one per annotation added to existing code.
    pub wiring: Vec<String>,
    pub extras: Vec<Extra>,
    pub notes: Vec<String>,
    /// `group:artifact` the output needs and the project lacks.
    pub dependencies: Vec<String>,
}

/// Run a generator and plan where its output goes.
#[arbor_rpc::handler]
fn bennu_generate_preview(_ctx: &BennuState, args: PreviewArgs) -> Result<Preview, String> {
    let generator = find(&args.id).ok_or_else(|| format!("There is no generator `{}`", args.id))?;
    let built = build(&args.site, generator, Some(&args.options), args.id == "http-client");
    let mut output = generator.generate(&args.options, &built.ctx)?;
    in_project_style(&args.site, &mut output);
    Ok(plan(&args.site, &built, output))
}

/// Lay the generated Java out the way the project's formatter would.
///
/// Every generator writes one fixed house style (four spaces, braces on the same line), so each of them
/// would otherwise have to learn the settings — indent width, tabs, brace placement, wrapping, line
/// endings — and a new generator would start out ignoring them. Doing it once, here, on the way out, means
/// no generator can forget: whatever the Format settings say (global, detected, `.editorconfig`, the
/// project's override) is what the proposal looks like. Non-Java files are left alone.
fn in_project_style(site: &Site, output: &mut Output) {
    let legacy = crate::format_style::legacy_layer(None, None);
    let at = site.file.as_deref().or(site.directory.as_deref());
    let style = crate::format_style::resolve(Some(&site.root), at, &legacy);
    for file in output.files.iter_mut().filter(|f| f.language == "java") {
        file.text = format_source(&file.text, &style);
    }
    if let Some(members) = output.members.as_mut() {
        if site.file.as_deref().is_some_and(is_java) {
            members.text = format_members(&members.text, &style);
        }
    }
}

/// Members have no file around them, so they are formatted inside a throwaway class and taken back out —
/// indented one level, as they will sit in the real one. Returned untouched when the wrapper does not come
/// back in the shape it went in (a fragment the formatter could not make sense of).
fn format_members(text: &str, style: &FormatStyle) -> String {
    let formatted = format_source(&format!("class G {{\n{text}\n}}\n"), style);
    let lines: Vec<&str> = formatted.lines().collect();
    // The opening line is `class G {`, or `class G` then `{` alone under an Allman brace style.
    let Some(open) = lines.iter().position(|l| l.trim_end().ends_with('{')) else { return text.to_string() };
    let Some(close) = lines.iter().rposition(|l| l.trim() == "}") else { return text.to_string() };
    if close <= open || !lines[0].trim_start().starts_with("class G") {
        return text.to_string();
    }
    // The wrapper adds a newline before its closing brace; whether the text ended with one is the caller's.
    let mut inside = &lines[open + 1..close];
    while let [rest @ .., last] = inside {
        if !last.trim().is_empty() {
            break;
        }
        inside = rest;
    }
    let body = inside.join("\n");
    if text.ends_with('\n') { format!("{body}\n") } else { body }
}

fn plan(site: &Site, built: &Built, output: Output) -> Preview {
    let roots = source_roots(&site.root, site.file.as_deref().or(site.directory.as_deref()));
    let files = output
        .files
        .iter()
        .map(|f| {
            let path = path_of(&roots, f, site.file.as_deref());
            PlannedFile { exists: Path::new(&path).exists(), path, name: f.name.clone(), text: f.text.clone(), language: f.language }
        })
        .collect();
    let mut notes = output.notes.clone();
    let (edits, members, wiring) = edits_for(site, built, &output, &mut notes);
    Preview { files, edits, members, wiring, extras: output.extras, notes, dependencies: output.dependencies }
}

/// The edits to the open file: members first, then annotations, then the imports all of them need — merged
/// where they land on one offset, so applying them in any order gives the same text.
fn edits_for(site: &Site, built: &Built, output: &Output, notes: &mut Vec<String>) -> (Vec<TemplateEdit>, Option<MembersPreview>, Vec<String>) {
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut members_preview = None;
    let mut wiring_lines = Vec::new();
    let source = &built.source;
    let class_name = built.ctx.class.as_ref().map(|c| c.name.clone());

    if let Some(members) = &output.members {
        members_preview = Some(MembersPreview { text: members.text.clone(), file: site.file.clone(), class: class_name.clone() });
        match members.target {
            MemberTarget::Class => match insert_members(source, built.model.as_ref().map(|m| m.path_in_file.as_str()), &members.text) {
                Ok(insertion) => {
                    inserts.push((insertion.offset, insertion.text));
                    imports.extend(members.imports.iter().cloned());
                }
                Err(e) => notes.push(e),
            },
            MemberTarget::Caret => {
                let at = site.offset.unwrap_or(source.len()).min(source.len());
                let lead = if at > 0 && !source[..at].ends_with('\n') { "\n" } else { "" };
                inserts.push((at, format!("{lead}{}\n", members.text)));
            }
        }
    }
    for wire in &output.wiring {
        let label = match &wire.target {
            bennu_generate::prelude::WireTarget::Field(name) => format!("{}  on field {name}", wire.annotation),
            bennu_generate::prelude::WireTarget::Class => format!("{}  on {}", wire.annotation, class_name.as_deref().unwrap_or("the class")),
        };
        match annotation_insert(source, &wire.target, &wire.annotation, class_name.as_deref()) {
            Some(insert) => {
                inserts.push((insert.offset, insert.text));
                imports.extend(wire.imports.iter().cloned());
                wiring_lines.push(label);
            }
            None => notes.push(format!("`{}` was not added: the declaration was not found, or it already has it.", wire.annotation)),
        }
    }

    // One edit per offset, in the order the pieces were produced.
    inserts.sort_by_key(|(offset, _)| *offset);
    let mut edits: Vec<TemplateEdit> = Vec::new();
    for (offset, text) in inserts {
        match edits.last_mut() {
            Some(last) if last.start == offset => last.replacement.push_str(&text),
            _ => edits.push(TemplateEdit { start: offset, end: offset, replacement: text }),
        }
    }
    if site.file.as_deref().is_some_and(is_java) {
        imports.sort();
        imports.dedup();
        edits.extend(import_edits(source, &imports));
    }
    (edits, members_preview, wiring_lines)
}

#[cfg(test)]
mod style_tests {
    use super::*;

    fn style(layers: &[serde_json::Value]) -> FormatStyle {
        let maps: Vec<serde_json::Map<String, Value>> = layers.iter().filter_map(|v| v.as_object().cloned()).collect();
        let refs: Vec<&serde_json::Map<String, Value>> = maps.iter().collect();
        FormatStyle::layered(&refs)
    }

    #[test]
    fn members_come_back_in_the_projects_indent_not_the_generators() {
        let generated = "    public void f() {\n        int x = 1;\n    }\n";
        let two = format_members(generated, &style(&[serde_json::json!({ "indent_width": 2, "use_tabs": false })]));
        assert!(two.contains("\n    int x = 1;\n"), "{two:?}");
        assert!(two.starts_with("  public void f() {"), "{two:?}");
        let tabs = format_members(generated, &style(&[serde_json::json!({ "use_tabs": true })]));
        assert!(tabs.contains("\t\tint x = 1;"), "{tabs:?}");
    }

    #[test]
    fn a_fragment_that_does_not_come_back_as_a_class_body_is_left_alone() {
        let broken = "    public void f( {\n";
        assert_eq!(format_members(broken, &style(&[])), broken);
    }
}
