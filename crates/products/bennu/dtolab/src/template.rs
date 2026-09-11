//! Rendering a test from a template the user owns — and where templates live.
//!
//! ## What a template is
//!
//! A Jinja file (`*.java.jinja`) rendered with a [`TestContext`]. The built-in one writes plain Bean
//! Validation tests with the project's own JUnit and AssertJ; any other is a copy the user made and
//! changed, and nothing in Bennu depends on what it outputs. That is the point: a team with its own
//! assertion helpers, its own factories and its own naming writes *those* into its template, and the
//! lab supplies the data — the cases, the values as data and as Java, and the violations the project's
//! own validator produced for each.
//!
//! ## Where they live
//!
//! Globally, in the profile's `bennu/dtolab/templates/` — a template describes how *a person* writes
//! tests, which does not change between their projects. A project can **pick** one (per-repo
//! setting), and a single generation can pick another. The built-in template is named
//! [`DEFAULT_TEMPLATE_NAME`] and cannot be overwritten, so there is always one that works.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use minijinja::Environment;
use serde::{Deserialize, Serialize};

use crate::cases::SampleValue;
use crate::names::{camel, java_string, pascal, snake};
use crate::protocol::Violation;

pub const DEFAULT_TEMPLATE_NAME: &str = "default";
pub const DEFAULT_TEMPLATE: &str = include_str!("../templates/default.java.jinja");
pub const TEMPLATE_EXTENSION: &str = ".java.jinja";

/// Whether a template renders a whole test class or only members for an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    File,
    Members,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateOrigin {
    Builtin,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub origin: TemplateOrigin,
    /// The file, for a template that has one.
    pub path: Option<String>,
}

/// Everything a template is rendered with. The DTO Lab documentation lists it field by field — keep
/// the two in step.
#[derive(Debug, Clone, Serialize)]
pub struct TestContext {
    pub mode: RenderMode,
    pub class: ClassContext,
    pub test_class: String,
    /// The project's Java language level (`8`, `17`).
    pub java: u32,
    /// `4` or `5`.
    pub junit: u32,
    /// AssertJ is on the test classpath.
    pub assertj: bool,
    /// `jakarta.validation` or `javax.validation`.
    pub validation: String,
    /// The expectations were produced by the project's validator rather than predicted.
    pub verified: bool,
    /// A value for every field that satisfies its constraints, in declaration order.
    pub valid: Vec<ValidAssignment>,
    /// The constrained fields, each with its cases.
    pub fields: Vec<FieldContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassContext {
    pub name: String,
    pub package: String,
    pub fqn: String,
    pub binary: String,
    pub record: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidAssignment {
    pub field: String,
    pub type_name: String,
    pub type_simple: String,
    pub setter: Option<String>,
    /// The field has constraints. An unconstrained field of a class is best left at its default.
    pub constrained: bool,
    pub value: SampleValue,
    pub value_java: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldContext {
    /// Position among the fields being generated, from 0.
    pub index: usize,
    pub name: String,
    pub json_name: String,
    pub type_name: String,
    pub type_simple: String,
    pub setter: Option<String>,
    pub getter: Option<String>,
    pub cases: Vec<CaseContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseContext {
    /// Position within the field, from 0.
    pub index: usize,
    pub name: String,
    pub constraint: String,
    pub constraint_fqn: String,
    pub violated: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub value: SampleValue,
    pub value_java: String,
    /// Every violation an instance with this value has — the whole set, not only this field's.
    pub expected: Vec<Violation>,
    /// `expected` came from the project's validator.
    pub verified: bool,
}

/// Render `template` with `context`.
///
/// Blocks are trimmed the way hand-written templates expect — a line holding only `{% … %}` leaves
/// nothing behind — and four filters are added: `snake`, `camel`, `pascal`, `java_string`.
pub fn render(template: &str, context: &TestContext) -> Result<String, String> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    env.add_filter("snake", |value: String| snake(&value));
    env.add_filter("camel", |value: String| camel(&value));
    env.add_filter("pascal", |value: String| pascal(&value));
    env.add_filter("java_string", |value: String| java_string(&value));
    env.render_str(template, context).map_err(|error| match error.line() {
        Some(line) => format!("template line {line}: {error}"),
        None => format!("template: {error}"),
    })
}

/// The classes a template asks constants to be resolved from, declared anywhere in it as
/// `bennu.constants: com.example.Messages, com.example.Codes` — normally inside a comment.
///
/// Read before rendering, because resolving them is a question for the JVM and the answers are part
/// of what the template is rendered with.
pub fn declared_constant_classes(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in template.lines() {
        let Some((_, rest)) = line.split_once("bennu.constants:") else { continue };
        for item in rest.split(',') {
            let name: String = item
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '$' | '_'))
                .collect();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// The built-in template, then every template in `dir`, by name.
pub fn list_templates(dir: &Path) -> Vec<TemplateInfo> {
    let mut found: Vec<TemplateInfo> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = file.strip_suffix(TEMPLATE_EXTENSION)?.to_string();
            (name != DEFAULT_TEMPLATE_NAME).then(|| TemplateInfo {
                name,
                origin: TemplateOrigin::Global,
                path: Some(entry.path().display().to_string()),
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut out = vec![TemplateInfo {
        name: DEFAULT_TEMPLATE_NAME.to_string(),
        origin: TemplateOrigin::Builtin,
        path: None,
    }];
    out.extend(found);
    out
}

pub fn template_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{TEMPLATE_EXTENSION}"))
}

/// The text of the template called `name`.
pub fn load_template(dir: &Path, name: &str) -> Result<String, String> {
    if name == DEFAULT_TEMPLATE_NAME {
        return Ok(DEFAULT_TEMPLATE.to_string());
    }
    std::fs::read_to_string(template_path(dir, name))
        .map_err(|e| format!("The template `{name}` could not be read: {e}"))
}

/// A name a new template may have: a file name on every platform, and not the built-in one's.
pub fn check_template_name(name: &str) -> Result<(), String> {
    if name == DEFAULT_TEMPLATE_NAME {
        return Err(format!("`{DEFAULT_TEMPLATE_NAME}` is the built-in template's name"));
    }
    let usable = !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'));
    match usable {
        true => Ok(()),
        false => Err("A template name uses letters, digits, `-`, `_` and `.` only".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{build, Request, Toolchain};
    use crate::model::class_at;

    const ORDER: &str = "package com.example;\n\
        public class Order {\n\
            @NotBlank @Size(max = 40) private String customerName;\n\
            @Min(1) private int quantity;\n\
            public void setCustomerName(String v) {}\n\
            public void setQuantity(int v) {}\n\
        }\n";

    fn context(mode: RenderMode, junit: u32, assertj: bool) -> TestContext {
        let class = class_at(ORDER, None).unwrap();
        build(
            Request {
                class: &class,
                mode,
                test_class: "OrderValidationTest".into(),
                toolchain: Toolchain { java: 17, junit, assertj, validation: "jakarta.validation".into() },
                fields: None,
                json_names: true,
            },
            None,
            &BTreeMap::new(),
        )
        .context
    }

    #[test]
    fn the_default_template_writes_a_whole_test_class() {
        let out = render(DEFAULT_TEMPLATE, &context(RenderMode::File, 5, true)).unwrap();
        assert!(out.starts_with("package com.example;\n"), "{out}");
        assert!(out.contains("import org.junit.jupiter.api.Test;"), "{out}");
        assert!(out.contains("class OrderValidationTest {"), "{out}");
        assert!(out.contains("void customerName_not_blank_null()"), "{out}");
        assert!(out.contains("void customerName_size_max()"), "{out}");
        assert!(out.contains("instance.setCustomerName(\"x\".repeat(41));"), "{out}");
        assert!(out.contains("void quantity_min()"), "{out}");
        assert!(out.trim_end().ends_with('}'), "{out}");
    }

    /// Members go into a class that has its own imports, so they must not rely on any.
    #[test]
    fn members_for_an_existing_class_spell_every_type_in_full() {
        let out = render(DEFAULT_TEMPLATE, &context(RenderMode::Members, 4, false)).unwrap();
        assert!(!out.contains("package "), "{out}");
        assert!(!out.contains("import "), "{out}");
        assert!(out.contains("@org.junit.Test"), "{out}");
        assert!(out.contains("public void customerName_size_max()"), "{out}");
        assert!(out.contains("org.junit.Assert.assertEquals("), "{out}");
    }

    #[test]
    fn a_mistake_in_a_template_says_which_line() {
        let error = render("line one\n{{ fields | nosuchfilter }}\n", &context(RenderMode::File, 5, true)).unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn constant_classes_are_read_from_wherever_the_template_declares_them() {
        let template = "{#- bennu.constants: com.example.Messages, com.example.Codes -#}\n{# bennu.constants: com.example.Messages #}";
        assert_eq!(declared_constant_classes(template), ["com.example.Messages", "com.example.Codes"]);
    }

    #[test]
    fn the_built_in_template_is_listed_first_and_cannot_be_shadowed() {
        let dir = std::env::temp_dir().join(format!("bennu-dtolab-templates-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zeta.java.jinja"), "z").unwrap();
        std::fs::write(dir.join("Alpha.java.jinja"), "a").unwrap();
        std::fs::write(dir.join("default.java.jinja"), "not the built-in").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let names: Vec<String> = list_templates(&dir).into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["default", "Alpha", "zeta"]);
        assert_eq!(load_template(&dir, "default").unwrap(), DEFAULT_TEMPLATE);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(check_template_name("default").is_err());
        assert!(check_template_name("../x").is_err());
        assert!(check_template_name("team-style_2").is_ok());
    }
}
