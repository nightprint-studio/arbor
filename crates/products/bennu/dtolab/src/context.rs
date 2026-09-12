//! What a validation-test template is rendered with, and the template Bennu ships.
//!
//! The built-in template writes plain Bean Validation tests with the project's own JUnit, and AssertJ
//! when it has it. A team with its own assertion helpers, factories and naming writes those into a
//! template of its own — the kind is `validation-tests` — and this context is the data it gets: the
//! cases, each value as data and as Java, and the violations the project's validator produced.

use std::collections::BTreeMap;

use bennu_templates::prelude::{schema_of, Builtin, Directives};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::cases::SampleValue;
use crate::protocol::Violation;

pub const DEFAULT_TEMPLATE_NAME: &str = "default";

/// The validation-test templates Bennu ships.
pub const BUILTINS: &[Builtin] = &[Builtin {
    name: DEFAULT_TEMPLATE_NAME,
    extension: "java",
    text: include_str!("../templates/default.java.jinja"),
    starter: false,
}];

/// Whether a template renders a whole test class or only members for an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    File,
    Members,
}

/// Everything a template is rendered with. The DTO Lab documentation lists it field by field — keep
/// the two in step.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ClassContext {
    pub name: String,
    pub package: String,
    pub fqn: String,
    pub binary: String,
    pub record: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ValidAssignment {
    pub field: String,
    pub type_name: String,
    pub type_simple: String,
    pub setter: Option<String>,
    /// The setter returns the object, so calls chain.
    pub setter_chains: bool,
    /// The method that returns a copy with the field changed — `withName` — when the class has one.
    pub wither: Option<String>,
    /// The field has constraints. An unconstrained field of a class is best left at its default.
    pub constrained: bool,
    pub value: SampleValue,
    pub value_java: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct FieldContext {
    /// Position among the fields being generated, from 0.
    pub index: usize,
    pub name: String,
    pub json_name: String,
    pub type_name: String,
    pub type_simple: String,
    pub setter: Option<String>,
    /// The setter returns the object, so calls chain.
    pub setter_chains: bool,
    pub getter: Option<String>,
    /// The method that returns a copy with the field changed — `withName` — when the class has one.
    pub wither: Option<String>,
    pub cases: Vec<CaseContext>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
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

/// The JSON Schema of [`TestContext`] — what the editor completes inside a validation-test template.
pub fn context_schema() -> serde_json::Value {
    schema_of::<TestContext>()
}

/// The classes a template asks constants to be resolved from: `bennu.constants: com.example.Messages`
/// on a comment line of its own. Read before rendering, because resolving them is a question for the
/// JVM, and the answers are part of what the template is rendered with.
pub fn constant_classes(template: &str) -> Vec<String> {
    Directives::read(template)
        .list("constants")
        .into_iter()
        .map(|item| item.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '$' | '_')).collect::<String>())
        .filter(|name| !name.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{build, Request, Toolchain};
    use bennu_templates::prelude::{class_at, render};

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
                rules: &[],
            },
            None,
            &BTreeMap::new(),
        )
        .context
    }

    #[test]
    fn the_default_template_writes_a_whole_test_class() {
        let out = render(BUILTINS[0].text, &context(RenderMode::File, 5, true)).unwrap();
        assert!(out.starts_with("package com.example;\n"), "{out}");
        assert!(out.contains("import org.junit.jupiter.api.Test;"), "{out}");
        assert!(out.contains("class OrderValidationTest {"), "{out}");
        assert!(out.contains("void customerName_not_blank_null()"), "{out}");
        assert!(out.contains("instance.setCustomerName(\"x\".repeat(41));"), "{out}");
        assert!(out.contains("void quantity_min()"), "{out}");
        assert!(out.trim_end().ends_with('}'), "{out}");
    }

    /// Members go into a class that has its own imports, so they must not rely on any.
    #[test]
    fn members_for_an_existing_class_spell_every_type_in_full() {
        let out = render(BUILTINS[0].text, &context(RenderMode::Members, 4, false)).unwrap();
        assert!(!out.contains("package "), "{out}");
        assert!(!out.contains("import "), "{out}");
        assert!(out.contains("@org.junit.Test"), "{out}");
        assert!(out.contains("public void customerName_size_max()"), "{out}");
    }

    #[test]
    fn the_built_in_asks_for_no_constants_and_a_template_can() {
        assert!(constant_classes(BUILTINS[0].text).is_empty(), "prose about the directive is not the directive");
        assert_eq!(constant_classes("{# bennu.constants: com.example.Messages, com.example.Codes #}"), ["com.example.Messages", "com.example.Codes"]);
    }

    #[test]
    fn the_context_schema_describes_what_a_template_reads() {
        let schema = context_schema();
        let case = &schema["properties"]["fields"]["items"]["properties"]["cases"]["items"]["properties"];
        assert!(case.get("value_java").is_some(), "{schema}");
        assert!(case["expected"]["items"]["properties"].get("constant").is_some(), "{schema}");
        assert!(schema["properties"]["mode"].to_string().contains("members"), "{schema}");
    }
}
