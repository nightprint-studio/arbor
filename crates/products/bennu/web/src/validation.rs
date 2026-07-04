//! Struts2 validation-config parser (`<Action>-validation.xml`).
//!
//! Struts binds a validation ruleset to an action by **file name convention**: a file
//! `FooAction-validation.xml` sitting next to the action class validates `FooAction`; the
//! alias form `FooAction-<aliasName>-validation.xml` validates one action alias. Each
//! `<field name="bar">` names a **property** of the action class (resolved to `getBar`/
//! `setBar` by the Java index), and each `<field-validator type="…">` names a validator.
//!
//! This crate owns the *parse + the file-name→class-simple-name convention* only; binding
//! the derived simple-name to a concrete project FQCN and a `<field>` to a getter/setter is
//! Java-index work done in `bennu-intel` (the property lives in `ClassMembers`, not here).

use std::path::{Path, PathBuf};

use crate::model::{
    FieldValidator, ValidationField, ValidationRecord, ValidatorMessage, ValidatorParam,
};
use crate::xml;

pub use crate::validation_author::{
    append_validator, author_field_block, author_field_validator, author_validation_skeleton,
    AuthoredMessage, AuthoredValidator,
};

/// Parse a single `<Action>-validation.xml` `file`. Returns `None` when the file name is
/// not a validation file, the file can't be read, or it doesn't parse (skip-and-continue).
pub fn parse_file(file: &Path) -> Option<ValidationRecord> {
    let (action_class, alias) = split_validation_filename(file)?;
    let text = std::fs::read_to_string(file).ok()?;
    let doc = xml::parse(&text)?;
    let root = doc.root_element();

    let mut fields = Vec::new();
    for field in root.children().filter(|n| n.has_tag_name("field")) {
        let Some(name) = field.attribute("name") else { continue };
        let validators = field
            .children()
            .filter(|n| n.has_tag_name("field-validator"))
            .filter_map(parse_field_validator)
            .collect();
        let name_offset = field
            .attributes()
            .find(|a| a.name() == "name")
            .map(|a| a.range_value().start)
            .unwrap_or(0);
        fields.push(ValidationField { name: name.to_string(), validators, name_offset });
    }

    Some(ValidationRecord {
        action_class,
        alias,
        fields,
        source_file: file.display().to_string(),
    })
}

/// The `<Class>-validation.xml` path bound to a Java **action-class file** by the Struts naming
/// convention: same directory, class simple-name + `-validation.xml`. `None` when the path isn't a
/// `.java` file. Pure path algebra — the caller checks existence + reads/writes.
pub fn validation_file_for_class(java_file: &Path) -> Option<PathBuf> {
    if java_file.extension()?.to_str()? != "java" {
        return None;
    }
    let stem = java_file.file_stem()?.to_str()?;
    let parent = java_file.parent()?;
    Some(parent.join(format!("{stem}-validation.xml")))
}

/// Parse a single `<field-validator type=…>` node into a [`FieldValidator`] (type + ordered
/// params + message + short-circuit + offset). `None` when it has no `type` attribute.
fn parse_field_validator(n: roxmltree::Node) -> Option<FieldValidator> {
    let type_name = n.attribute("type")?.to_string();
    let type_offset = n
        .attributes()
        .find(|a| a.name() == "type")
        .map(|a| a.range_value().start)
        .unwrap_or(0);
    let short_circuit = n
        .attribute("short-circuit")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let params = n
        .children()
        .filter(|c| c.has_tag_name("param"))
        .filter_map(|c| {
            let name = c.attribute("name")?.to_string();
            let value = c.text().unwrap_or("").trim().to_string();
            Some(ValidatorParam { name, value })
        })
        .collect();
    let message = n.children().find(|c| c.has_tag_name("message")).map(|m| ValidatorMessage {
        key: m.attribute("key").map(str::to_string),
        text: m.text().unwrap_or("").trim().to_string(),
    });
    Some(FieldValidator { type_name, params, message, short_circuit, type_offset })
}

/// Split a validation file name into `(action_class_simple_name, alias)`.
/// `FooAction-validation.xml` → `("FooAction", "")`;
/// `FooAction-input-validation.xml` → `("FooAction", "input")`.
/// Returns `None` if the name doesn't end in `-validation.xml`.
pub fn split_validation_filename(file: &Path) -> Option<(String, String)> {
    let name = file.file_name()?.to_str()?;
    let base = name.strip_suffix("-validation.xml")?;
    // A Java simple-name has no `-`, so the class is the head up to the first `-`; the rest
    // (if any) is the action alias.
    match base.split_once('-') {
        Some((class, alias)) => Some((class.to_string(), alias.to_string())),
        None => Some((base.to_string(), String::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fields_and_validators() {
        let xml = r#"<!DOCTYPE validators PUBLIC "-//Apache Struts//XWork Validator 1.0.3//EN" "http://struts.apache.org/dtds/xwork-validator-1.0.3.dtd">
            <validators>
              <field name="username">
                <field-validator type="requiredstring"><message>req</message></field-validator>
                <field-validator type="stringlength"><message>len</message></field-validator>
              </field>
              <field name="email">
                <field-validator type="email"><message>bad</message></field-validator>
              </field>
            </validators>"#;
        let dir = crate::test_support::tmp_dir("val");
        let file = dir.join("LoginAction-validation.xml");
        std::fs::write(&file, xml).unwrap();

        let rec = parse_file(&file).unwrap();
        assert_eq!(rec.action_class, "LoginAction");
        assert_eq!(rec.alias, "");
        assert_eq!(rec.fields.len(), 2);
        let user = rec.fields.iter().find(|f| f.name == "username").unwrap();
        let types: Vec<&str> = user.validators.iter().map(|v| v.type_name.as_str()).collect();
        assert_eq!(types, vec!["requiredstring", "stringlength"]);
        assert!(user.name_offset > 0);
    }

    #[test]
    fn captures_params_message_and_short_circuit() {
        let xml = r#"<validators>
              <field name="age">
                <field-validator type="int" short-circuit="true">
                  <param name="min">18</param>
                  <param name="max">120</param>
                  <message key="age.range">Age must be 18–120</message>
                </field-validator>
              </field>
            </validators>"#;
        let dir = crate::test_support::tmp_dir("val-params");
        let file = dir.join("SignupAction-validation.xml");
        std::fs::write(&file, xml).unwrap();

        let rec = parse_file(&file).unwrap();
        let age = &rec.fields[0];
        assert_eq!(age.validators.len(), 1);
        let v = &age.validators[0];
        assert_eq!(v.type_name, "int");
        assert!(v.short_circuit);
        assert_eq!(v.type_offset > 0, true);
        // Params keep document order.
        assert_eq!(v.params.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect::<Vec<_>>(),
            vec![("min", "18"), ("max", "120")]);
        let msg = v.message.as_ref().unwrap();
        assert_eq!(msg.key.as_deref(), Some("age.range"));
        assert_eq!(msg.text, "Age must be 18–120");
    }

    #[test]
    fn short_circuit_defaults_false_and_message_optional() {
        let xml = r#"<validators>
              <field name="email"><field-validator type="email"/></field>
            </validators>"#;
        let dir = crate::test_support::tmp_dir("val-def");
        let file = dir.join("FooAction-validation.xml");
        std::fs::write(&file, xml).unwrap();
        let rec = parse_file(&file).unwrap();
        let v = &rec.fields[0].validators[0];
        assert!(!v.short_circuit);
        assert!(v.message.is_none());
        assert!(v.params.is_empty());
    }

    #[test]
    fn splits_alias_filename() {
        assert_eq!(
            split_validation_filename(Path::new("/a/b/FooAction-validation.xml")),
            Some(("FooAction".to_string(), String::new()))
        );
        assert_eq!(
            split_validation_filename(Path::new("/a/b/FooAction-input-validation.xml")),
            Some(("FooAction".to_string(), "input".to_string()))
        );
        assert_eq!(split_validation_filename(Path::new("/a/b/struts.xml")), None);
    }

    #[test]
    fn validation_file_for_class_follows_convention() {
        assert_eq!(
            validation_file_for_class(Path::new("/src/com/acme/LoginAction.java")),
            Some(PathBuf::from("/src/com/acme/LoginAction-validation.xml"))
        );
        // Not a .java file → None.
        assert_eq!(validation_file_for_class(Path::new("/src/com/acme/Login.xml")), None);
        assert_eq!(validation_file_for_class(Path::new("/src/README")), None);
    }

    #[test]
    fn non_validation_file_is_none() {
        let dir = crate::test_support::tmp_dir("val2");
        let file = dir.join("random.xml");
        std::fs::write(&file, "<validators/>").unwrap();
        assert!(parse_file(&file).is_none());
    }
}
