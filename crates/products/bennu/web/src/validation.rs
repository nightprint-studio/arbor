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

use std::path::Path;

use crate::model::{ValidationField, ValidationRecord};
use crate::xml;

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
            .filter_map(|n| n.attribute("type").map(str::to_string))
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
        assert_eq!(user.validators, vec!["requiredstring", "stringlength"]);
        assert!(user.name_offset > 0);
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
    fn non_validation_file_is_none() {
        let dir = crate::test_support::tmp_dir("val2");
        let file = dir.join("random.xml");
        std::fs::write(&file, "<validators/>").unwrap();
        assert!(parse_file(&file).is_none());
    }
}
