//! What a template is rendered with, assembled — and checked on the JVM when there is one.
//!
//! The order matters. A valid instance is built first and validated: every case is that instance
//! with one field changed, so if the instance itself is not valid, every expectation carries its
//! violations too, and that is said once rather than discovered in every test. Then each case is
//! validated in turn, and **what the validator reports is what the test expects** — not what the
//! source suggests. The prediction from source is used only when there is no validator to ask, and
//! the context says so.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::cases::{invalid_cases, type_simple, valid_value, Case};
use crate::model::{LabClass, LabField};
use crate::protocol::Violation;
use crate::template::{CaseContext, ClassContext, FieldContext, RenderMode, TestContext, ValidAssignment};

/// Validates a JSON instance of the class on the JVM.
pub type Verifier<'a> = &'a mut dyn FnMut(&Value) -> Result<Vec<Violation>, String>;

/// What the project's build offers a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    pub java: u32,
    pub junit: u32,
    pub assertj: bool,
    pub validation: String,
}

pub struct Request<'a> {
    pub class: &'a LabClass,
    pub mode: RenderMode,
    pub test_class: String,
    pub toolchain: Toolchain,
    /// Only these fields; all constrained ones when `None`.
    pub fields: Option<&'a [String]>,
    /// The verifier binds JSON by Jackson's names rather than by field names.
    pub json_names: bool,
}

#[derive(Debug, Clone)]
pub struct Built {
    pub context: TestContext,
    /// What the user should know before trusting the result.
    pub warnings: Vec<String>,
    pub cases: usize,
}

pub fn build(request: Request<'_>, verify: Option<Verifier<'_>>, constants: &BTreeMap<String, String>) -> Built {
    let Request { class, mode, test_class, toolchain, fields: only, json_names } = request;
    let mut verify = verify;
    let mut warnings: Vec<String> = Vec::new();
    let java = toolchain.java;

    let valid: Vec<ValidAssignment> = class
        .fields
        .iter()
        .map(|f| {
            let simple = type_simple(&f.type_name);
            let value = valid_value(f);
            ValidAssignment {
                field: f.name.clone(),
                type_name: f.type_name.clone(),
                setter: f.setter.clone(),
                constrained: !f.constraints.is_empty(),
                value_java: value.to_java(&simple, java),
                value,
                type_simple: simple,
            }
        })
        .collect();

    let key = |f: &LabField| if json_names { f.json_name.clone() } else { f.name.clone() };
    // A field Jackson ignores cannot be set from a payload, so nothing about it can be checked that way.
    let settable = |f: &LabField| !(json_names && f.ignored);

    let mut instance = Map::new();
    for (f, assignment) in class.fields.iter().zip(&valid) {
        // A class leaves an unconstrained field at its default, and so does the JSON instance. A
        // record has no default to leave it at.
        if !settable(f) || (!assignment.constrained && !class.record) {
            continue;
        }
        if let Some(value) = assignment.value.to_json(&assignment.type_simple) {
            instance.insert(key(f), value);
        }
    }

    let mut verified = false;
    if let Some(check) = verify.as_deref_mut() {
        match check(&Value::Object(instance.clone())) {
            Ok(found) => {
                verified = true;
                if !found.is_empty() {
                    warnings.push(format!(
                        "The valid instance is not valid ({}), so every generated test expects those \
                         violations too. Fix the values it is built from, then generate again.",
                        summary(&found)
                    ));
                }
            }
            Err(error) => warnings.push(format!(
                "Not checked on the JVM, so the expected violations are predicted from the source: {error}"
            )),
        }
    }
    if !verified {
        verify = None;
    }

    let mut fields: Vec<FieldContext> = Vec::new();
    let mut total = 0;
    for f in &class.fields {
        if f.constraints.is_empty() || only.is_some_and(|names| !names.iter().any(|n| n == &f.name)) {
            continue;
        }
        let simple = type_simple(&f.type_name);
        let mut cases = Vec::new();
        for (index, case) in invalid_cases(f).into_iter().enumerate() {
            let value_java = case.value.to_java(&simple, java);
            let mut expected = predicted(f, &case);
            let mut case_verified = false;
            let json = case.value.to_json(&simple).filter(|_| settable(f));
            if let (Some(check), Some(json)) = (verify.as_deref_mut(), json) {
                let mut object = instance.clone();
                object.insert(key(f), json);
                match check(&Value::Object(object)) {
                    Ok(found) => {
                        case_verified = true;
                        if !found.iter().any(|v| on_field(&v.path, &f.name)) {
                            warnings.push(format!(
                                "`{}` set to {} produced no violation of @{} — adjust that value in the \
                                 generated test.",
                                f.name, value_java, case.constraint
                            ));
                        }
                        expected = found;
                    }
                    Err(error) => warnings.push(format!("`{}` ({}) was not checked: {error}", f.name, case.name)),
                }
            }
            for e in &mut expected {
                e.constant = constants.get(&e.template).or_else(|| constants.get(&e.message)).cloned();
            }
            total += 1;
            cases.push(CaseContext {
                index,
                name: case.name,
                constraint: case.constraint,
                constraint_fqn: case.constraint_fqn,
                violated: case.violated,
                attributes: case.attributes,
                value: case.value,
                value_java,
                expected,
                verified: case_verified,
            });
        }
        fields.push(FieldContext {
            index: fields.len(),
            name: f.name.clone(),
            json_name: f.json_name.clone(),
            type_name: f.type_name.clone(),
            type_simple: simple,
            setter: f.setter.clone(),
            getter: f.getter.clone(),
            cases,
        });
    }

    let context = TestContext {
        mode,
        class: ClassContext {
            name: class.name.clone(),
            package: class.package.clone(),
            fqn: class.fqn.clone(),
            binary: class.binary.clone(),
            record: class.record,
        },
        test_class,
        java,
        junit: toolchain.junit,
        assertj: toolchain.assertj,
        validation: toolchain.validation,
        verified,
        valid,
        fields,
    };
    Built { context, warnings, cases: total }
}

/// The violation a case is expected to produce when there is no validator to ask.
fn predicted(field: &LabField, case: &Case) -> Vec<Violation> {
    let template = field
        .constraints
        .iter()
        .find(|c| c.name == case.constraint)
        .and_then(|c| c.message.clone())
        .unwrap_or_else(|| format!("{{{}.message}}", case.constraint_fqn));
    vec![Violation {
        path: field.name.clone(),
        message: template.clone(),
        template,
        constraint: case.constraint_fqn.clone(),
        attributes: case.attributes.clone(),
        invalid_value: None,
        constant: None,
    }]
}

fn on_field(path: &str, field: &str) -> bool {
    path == field
        || path.strip_prefix(field).is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
}

fn summary(found: &[Violation]) -> String {
    let mut parts: Vec<String> = found.iter().take(3).map(|v| format!("{} {}", v.path, v.template)).collect();
    if found.len() > 3 {
        parts.push(format!("and {} more", found.len() - 3));
    }
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::class_at;

    const ORDER: &str = "package com.example;\n\
        public class Order {\n\
            @NotBlank private String customerName;\n\
            private String note;\n\
            public void setCustomerName(String v) {}\n\
        }\n";

    fn request(class: &LabClass) -> Request<'_> {
        Request {
            class,
            mode: RenderMode::File,
            test_class: "OrderValidationTest".into(),
            toolchain: Toolchain { java: 17, junit: 5, assertj: false, validation: "jakarta.validation".into() },
            fields: None,
            json_names: true,
        }
    }

    #[test]
    fn without_a_jvm_the_expectations_are_predicted_and_say_so() {
        let class = class_at(ORDER, None).unwrap();
        let built = build(request(&class), None, &BTreeMap::new());
        assert!(!built.context.verified);
        assert_eq!(built.cases, 2, "NotBlank: null and blank");
        let first = &built.context.fields[0].cases[0];
        assert_eq!(first.expected[0].template, "{jakarta.validation.constraints.NotBlank.message}");
        assert!(!first.verified);
        assert_eq!(built.context.fields.len(), 1, "the unconstrained field has no cases");
    }

    /// The validator's answer replaces the prediction — including a message the source could not
    /// know — and constants are matched to what it reported.
    #[test]
    fn with_a_jvm_the_expectations_are_what_the_validator_reported() {
        let class = class_at(ORDER, None).unwrap();
        let mut seen: Vec<Value> = Vec::new();
        let mut verifier = |instance: &Value| -> Result<Vec<Violation>, String> {
            seen.push(instance.clone());
            Ok(match instance.get("customerName") {
                Some(Value::String(s)) if s.trim().is_empty() => vec![violation("customerName", "{custom.blank}")],
                Some(Value::Null) => vec![violation("customerName", "{custom.blank}")],
                _ => Vec::new(),
            })
        };
        let constants = BTreeMap::from([("{custom.blank}".to_string(), "Messages.BLANK".to_string())]);
        let built = build(request(&class), Some(&mut verifier), &constants);
        assert!(built.context.verified);
        assert!(built.warnings.is_empty(), "{:?}", built.warnings);
        let case = &built.context.fields[0].cases[1];
        assert!(case.verified);
        assert_eq!(case.expected[0].template, "{custom.blank}");
        assert_eq!(case.expected[0].constant.as_deref(), Some("Messages.BLANK"));
        assert_eq!(seen.len(), 3, "the valid instance, then one per case");
        assert_eq!(seen[0], serde_json::json!({ "customerName": "x" }));
    }

    #[test]
    fn a_value_that_did_not_violate_anything_is_reported() {
        let class = class_at(ORDER, None).unwrap();
        let mut verifier = |_: &Value| -> Result<Vec<Violation>, String> { Ok(Vec::new()) };
        let built = build(request(&class), Some(&mut verifier), &BTreeMap::new());
        assert_eq!(built.warnings.len(), 2, "{:?}", built.warnings);
    }

    fn violation(path: &str, template: &str) -> Violation {
        Violation {
            path: path.into(),
            template: template.into(),
            message: "must not be blank".into(),
            constraint: "jakarta.validation.constraints.NotBlank".into(),
            attributes: BTreeMap::new(),
            invalid_value: None,
            constant: None,
        }
    }
}
