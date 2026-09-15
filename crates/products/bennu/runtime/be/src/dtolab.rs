//! `dtolab` domain — the DTO Lab: a class tried out as JSON, validated for real, and turned into tests.
//!
//! The static half (reading the class, the cases, templates, placement) is `bennu-dtolab`; the JVM
//! half is [`crate::dtolab_jvm`]. This module is the seam between them and the editor: it decides
//! which bundle messages are interpolated with, whether Spring Boot's Jackson defaults apply, which
//! JUnit and assertion library a generated test may use, and where the result is written.

use std::collections::BTreeMap;
use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_dtolab::prelude::{
    apply_described, build, constant_classes, skeleton, test_class_name, test_file_for, violations_of,
    Described, RenderMode, Request, Toolchain, Verifier, Violation, DEFAULT_TEMPLATE_NAME,
};
use bennu_templates::prelude::{class_at, insert_members, render_code, types_in, ClassModel, Template, TemplateKind, TypeInFile};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dtolab_jvm;
use crate::frameworks::FrameworkService;
use crate::index_service::IndexService;
use crate::templates_render::{RenderArgs, Rendered};

// ── the class ─────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ClassArgs {
    pub root: String,
    /// The file the class is in.
    pub file: String,
    /// The buffer, possibly unsaved.
    pub source: String,
    /// The caret: the innermost class around it is the one read. The first class when absent.
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Serialize)]
pub struct ClassView {
    pub class: ClassModel,
    /// A JSON payload sketched from source — replaced by the real one once the JVM answers.
    pub skeleton: Value,
    /// `jakarta.validation`, `javax.validation`, or empty when neither is recognisable.
    pub validation: String,
    /// Where a new test for this class would go.
    pub test_file: String,
    /// The template this project generates with.
    pub template: String,
}

/// The class at the caret, read from source. `None` when there is no class there.
#[arbor_rpc::handler]
fn bennu_dtolab_class(_ctx: &BennuState, args: ClassArgs) -> Result<Option<ClassView>, String> {
    let Some(class) = class_at(&args.source, args.offset) else { return Ok(None) };
    Ok(Some(ClassView {
        skeleton: skeleton(&class, &crate::dtolab_values::rules()),
        validation: validation_namespace(&args.source, &args.root),
        test_file: test_file_for(&args.file, &class.package, &test_class_name(&class.name)),
        template: crate::templates::project_choice(&args.root, TemplateKind::ValidationTests)
            .unwrap_or_else(|| DEFAULT_TEMPLATE_NAME.to_string()),
        class,
    }))
}

// ── JSON and validation, on the JVM ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PayloadArgs {
    pub root: String,
    /// The class's binary name (`com.example.Order$Line`).
    pub class: String,
    pub json: String,
    /// BCP 47 tag messages are interpolated in; the JVM's default when absent.
    #[serde(default)]
    pub locale: Option<String>,
    /// `jakarta.validation` / `javax.validation`, when the caller knows which the class uses.
    #[serde(default)]
    pub validation: Option<String>,
}

/// Bind `json` to the class with the project's Jackson, and write it back: the object as it came out,
/// and the JSON that object produces — the round trip that shows what a payload loses on the way.
#[arbor_rpc::handler]
fn bennu_dtolab_read(ctx: &BennuState, args: PayloadArgs) -> Result<Value, String> {
    let boot = flag(spring_boot(&args.root));
    dtolab_jvm::call(ctx, &args.root, "read", &[&args.class, &args.json, boot])
        .map_err(|e| explain(&e, &args.class))
}

#[derive(Deserialize)]
pub struct ClassRefArgs {
    pub root: String,
    pub class: String,
}

/// The JSON the project's `ObjectMapper` writes for a new instance — the real skeleton.
#[arbor_rpc::handler]
fn bennu_dtolab_default_json(ctx: &BennuState, args: ClassRefArgs) -> Result<Value, String> {
    let boot = flag(spring_boot(&args.root));
    dtolab_jvm::call(ctx, &args.root, "default", &[&args.class, boot]).map_err(|e| explain(&e, &args.class))
}

/// Bind `json` and validate the result with the project's own validator and message bundles.
#[arbor_rpc::handler]
fn bennu_dtolab_validate(ctx: &BennuState, args: PayloadArgs) -> Result<Value, String> {
    validate_payload(
        ctx,
        &args.root,
        &args.class,
        &args.json,
        args.locale.as_deref().unwrap_or(""),
        args.validation.as_deref().unwrap_or(""),
    )
}

/// Args for [`bennu_validate_payload`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidatePayloadArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The class the payload binds to, fully qualified. A nested class is written with `$`:
    /// `com.example.Order$Line`.
    pub class: String,
    /// The JSON payload, exactly as the endpoint would receive it.
    pub json: String,
    /// The locale to interpolate messages in, as a BCP 47 tag (`it`, `en-GB`). The JVM's default
    /// when omitted.
    #[serde(default)]
    pub locale: Option<String>,
}

/// Bind a JSON payload to a project class with the project's own Jackson, validate it with the
/// project's own Bean Validation, and report every violation with its interpolated message.
///
/// The answer a unit test would give, without writing one: use it to check what an endpoint accepts
/// before changing a DTO, or to find out why a request is rejected. Runs on the project's JDK, with
/// the message bundles the project configures — including one named in a `@Bean` — so a message key
/// that does not resolve shows up as its own key, exactly as a user would see it.
///
/// The project is compiled first if it changed, and its classes are loaded and run: static
/// initialisers execute. A payload that does not bind is reported as `binding_error` rather than as
/// violations.
#[arbor_rpc::handler(mcp(title = "Validate a JSON payload against a DTO", safety = write))]
fn bennu_validate_payload(ctx: &BennuState, args: ValidatePayloadArgs) -> Result<Value, String> {
    let validation = validation_namespace("", &args.root);
    validate_payload(ctx, &args.root, &args.class, &args.json, args.locale.as_deref().unwrap_or(""), &validation)
}

fn validate_payload(
    ctx: &BennuState,
    root: &str,
    class: &str,
    json: &str,
    locale: &str,
    validation: &str,
) -> Result<Value, String> {
    let bundles = FrameworkService::global().validation_bundles(root).join(",");
    let boot = flag(spring_boot(root));
    dtolab_jvm::call(ctx, root, "validate", &[class, json, locale, &bundles, boot, validation])
        .map_err(|e| explain(&e, class))
}

// ── generating tests ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GenerateArgs {
    pub root: String,
    /// The file the class is in.
    pub file: String,
    pub source: String,
    #[serde(default)]
    pub offset: Option<usize>,
    /// For this generation only; the project's template when absent.
    #[serde(default)]
    pub template: Option<String>,
    /// An existing test class to add to; a new file beside the class's tests when absent.
    #[serde(default)]
    pub target: Option<TargetArgs>,
    /// Only these fields; every constrained one when absent.
    #[serde(default)]
    pub fields: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct TargetArgs {
    pub file: String,
    /// `Outer.Inner` within the file; its first class when absent.
    #[serde(default)]
    pub class: Option<String>,
    /// The file's buffer when it is open, so unsaved changes are kept.
    #[serde(default)]
    pub source: Option<String>,
}

/// What a generation would write — nothing is written until the editor applies it.
#[derive(Serialize)]
pub struct GeneratePreview {
    pub file: String,
    /// The file already exists, and the text is its content with the members added.
    pub exists: bool,
    /// The whole resulting file.
    pub text: String,
    /// Just what was added.
    pub inserted: String,
    /// For an existing file: the text the insertion was computed against, and the byte offset it goes
    /// at — so the editor can apply it to an open buffer as one undo step, and refuse when the buffer
    /// is no longer that text.
    pub base: Option<String>,
    pub offset: Option<usize>,
    pub template: String,
    pub cases: usize,
    /// The expectations came from the project's validator.
    pub verified: bool,
    pub warnings: Vec<String>,
    /// The classes of the target file, for choosing another one.
    pub classes: Vec<TypeInFile>,
    /// For an existing file: the imports the tests need, as edits computed against `base` — applied with the
    /// insertion. `text` has them already.
    pub import_edits: Vec<crate::templates_imports::TemplateEdit>,
}

#[arbor_rpc::handler]
fn bennu_dtolab_generate(ctx: &BennuState, args: GenerateArgs) -> Result<GeneratePreview, String> {
    let root = args.root.as_str();
    let mut class = class_at(&args.source, args.offset).ok_or("There is no class at the caret")?;
    let template_name = args
        .template
        .clone()
        .or_else(|| crate::templates::project_choice(root, TemplateKind::ValidationTests))
        .unwrap_or_else(|| DEFAULT_TEMPLATE_NAME.to_string());
    let template = crate::templates::load(TemplateKind::ValidationTests, &template_name)?;
    let validation = validation_namespace(&args.source, root);
    let mut warnings: Vec<String> = Vec::new();

    let test_class = test_class_name(&class.name);
    let (file, existing, class_path) = match &args.target {
        Some(target) => {
            let text = match &target.source {
                Some(source) => source.clone(),
                None => std::fs::read_to_string(&target.file).map_err(|e| format!("{}: {e}", target.file))?,
            };
            (target.file.clone(), Some(text), target.class.clone())
        }
        None => {
            let file = test_file_for(&args.file, &class.package, &test_class);
            let existing = std::fs::read_to_string(&file).ok();
            (file, existing, None)
        }
    };
    let mode = match existing {
        Some(_) => RenderMode::Members,
        None => RenderMode::File,
    };

    // The JVM half: the validator's own description of the class, how payloads bind, and constants.
    let bundles = FrameworkService::global().validation_bundles(root).join(",");
    let boot = flag(spring_boot(root));
    let mut jvm = match dtolab_jvm::call(ctx, root, "describe", &[&class.binary, &validation]) {
        Ok(body) => {
            match serde_json::from_value::<Described>(body) {
                Ok(described) => apply_described(&mut class, &described),
                Err(e) => warnings.push(format!("The validator's description of the class could not be read: {e}")),
            }
            true
        }
        Err(e) => {
            warnings.push(format!(
                "Not checked on the JVM, so the expected violations are predicted from the source: {}",
                explain(&e, &class.binary)
            ));
            false
        }
    };
    let mut json_names = true;
    if jvm {
        match dtolab_jvm::ask(root, "validate", &[&class.binary, "{}", "", &bundles, boot, &validation]) {
            Ok(body) => json_names = body.get("binder").and_then(Value::as_str) != Some("fields"),
            Err(e) => {
                warnings.push(format!("Not checked on the JVM: {e}"));
                jvm = false;
            }
        }
    }
    let mut constants: BTreeMap<String, String> = BTreeMap::new();
    let constant_classes = constant_classes(&template.text);
    if jvm && !constant_classes.is_empty() {
        match dtolab_jvm::ask(root, "constants", &[&constant_classes.join(",")]) {
            Ok(body) => {
                constants = body
                    .get("constants")
                    .and_then(|c| serde_json::from_value(c.clone()).ok())
                    .unwrap_or_default();
            }
            Err(e) => warnings.push(format!("The constants the template asks for could not be read: {e}")),
        }
    }

    let binary = class.binary.clone();
    let mut verifier = |instance: &Value| -> Result<Vec<Violation>, String> {
        let payload = instance.to_string();
        let body = dtolab_jvm::ask(root, "validate", &[&binary, &payload, "", &bundles, boot, &validation])?;
        if let Some(error) = body.get("binding_error").and_then(Value::as_str) {
            return Err(format!("the payload did not bind: {error}"));
        }
        Ok(violations_of(&body))
    };
    let verify: Option<Verifier<'_>> = match jvm {
        true => {
            let verifier: Verifier<'_> = &mut verifier;
            Some(verifier)
        }
        false => None,
    };
    let rules = crate::dtolab_values::rules();
    let built = build(
        Request {
            class: &class,
            mode,
            test_class,
            toolchain: toolchain(root, &validation),
            fields: args.fields.as_deref(),
            json_names,
            rules: &rules,
        },
        verify,
        &constants,
    );
    warnings.extend(built.warnings);

    let output = render_code(&template.text, &built.context, &crate::templates_facts::facts_at(root, &file), None)?;
    let (text, inserted, offset, import_edits) = match &existing {
        None => {
            let text = crate::templates_imports::with_imports(&file, output.text, &output.imports);
            (text.clone(), text, None, Vec::new())
        }
        Some(source) => {
            let insertion = insert_members(source, class_path.as_deref(), &output.text)?;
            let edits = crate::templates_imports::import_edits(source, &output.imports);
            let text = crate::templates_imports::with_imports(&file, insertion.apply(source), &output.imports);
            (text, insertion.text, Some(insertion.offset), edits)
        }
    };
    Ok(GeneratePreview {
        exists: existing.is_some(),
        classes: existing.as_deref().map(types_in).unwrap_or_default(),
        base: existing,
        offset,
        file,
        text,
        inserted,
        template: template_name,
        cases: built.cases,
        verified: built.context.verified,
        warnings,
        import_edits,
    })
}

/// A validation-test template rendered for an AI client or a preview: the expectations predicted from
/// the source, because there is no editor waiting for the project's JVM to check them.
pub(crate) fn render_predicted(args: &RenderArgs, template: &Template) -> Result<Rendered, String> {
    let (file, source) = crate::templates_render::source_of(args)?;
    let class = crate::templates_render::class_of(args, &file, &source)?;
    let validation = validation_namespace(&source, &args.root);
    let test_class = test_class_name(&class.name);
    let test_file = test_file_for(&file, &class.package, &test_class);
    let rules = crate::dtolab_values::rules();
    let built = build(
        Request {
            class: &class,
            mode: RenderMode::File,
            test_class,
            toolchain: toolchain(&args.root, &validation),
            fields: None,
            json_names: true,
            rules: &rules,
        },
        None,
        &BTreeMap::new(),
    );
    let facts = crate::templates_facts::facts_at(&args.root, &test_file);
    let output = render_code(&template.text, &built.context, &facts, args.parameters.as_ref())?;
    Ok(Rendered {
        text: crate::templates_imports::with_imports(&test_file, output.text, &output.imports),
        exists: Path::new(&test_file).exists(),
        file: Some(test_file),
        output: "file".to_string(),
        template: template.name.clone(),
        insertion: None,
        insertion_stops: Vec::new(),
        import_edits: Vec::new(),
        source_lines: None,
        notes: vec![
            "The expected violations are predicted from the source; the DTO Lab checks them on the project's JVM before writing."
                .to_string(),
        ],
    })
}

// ── what the project's build offers ─────────────────────────────────────────────────────────────

fn flag(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

/// Spring Boot's auto-configuration changes Jackson's defaults, and the lab has to bind the way the
/// application does — an unknown property is an error in plain Jackson and ignored in Boot.
fn spring_boot(root: &str) -> bool {
    IndexService::global().dep_jars_of(root).iter().any(|jar| jar.contains("spring-boot-autoconfigure"))
}

/// Which Bean Validation the class uses: its own imports first, then the jars. A project migrating
/// from `javax` has both, and validating a `javax`-annotated class with the `jakarta` provider finds
/// nothing wrong with anything.
pub(crate) fn validation_namespace(source: &str, root: &str) -> String {
    if source.contains("import javax.validation") {
        return "javax.validation".into();
    }
    if source.contains("import jakarta.validation") {
        return "jakarta.validation".into();
    }
    let jars = IndexService::global().dep_jars_of(root);
    if jars.iter().any(|j| j.contains("jakarta.validation-api")) {
        "jakarta.validation".into()
    } else if jars.iter().any(|j| j.contains("validation-api")) {
        "javax.validation".into()
    } else {
        String::new()
    }
}

pub(crate) fn toolchain(root: &str, validation: &str) -> Toolchain {
    let jars: Vec<String> =
        IndexService::global().dep_jars_of(root).into_iter().map(|j| j.replace('\\', "/")).collect();
    let has = |needle: &str| jars.iter().any(|j| j.contains(needle));
    let java = IndexService::global().jdk_version_of(root).map(|v| java_major(&v)).unwrap_or(8);
    let junit = match has("junit-jupiter-api") || !has("/junit/junit/") {
        true => 5,
        false => 4,
    };
    Toolchain {
        java,
        junit,
        assertj: has("assertj-core"),
        validation: match validation.is_empty() {
            true => "jakarta.validation".into(),
            false => validation.into(),
        },
    }
}

/// `"1.8"` → 8, `"17"` → 17.
pub(crate) fn java_major(level: &str) -> u32 {
    let level = level.trim();
    level.strip_prefix("1.").unwrap_or(level).split(['.', '_', '-']).next().and_then(|s| s.parse().ok()).unwrap_or(8)
}

/// A failure, with what to do about the one that is not self-explanatory.
fn explain(error: &str, class: &str) -> String {
    if error.contains("ClassNotFoundException") || error.contains("NoClassDefFoundError") {
        return format!(
            "{error} — the DTO Lab loads compiled classes: save the file so `{class}` is compiled, and \
             note that classes under src/test are not on its classpath."
        );
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_java_level_is_read_in_both_spellings() {
        assert_eq!(java_major("1.8"), 8);
        assert_eq!(java_major("17"), 17);
        assert_eq!(java_major("21.0.2"), 21);
        assert_eq!(java_major("nonsense"), 8);
    }

    #[test]
    fn a_class_that_does_not_load_says_what_to_do() {
        let message = explain("ClassNotFoundException: com.example.Order", "com.example.Order");
        assert!(message.contains("save the file"), "{message}");
        assert_eq!(explain("something else", "x"), "something else");
    }
}
