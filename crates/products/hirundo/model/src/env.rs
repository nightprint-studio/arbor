//! Environments and their **types**.
//!
//! An *environment* is one place an API runs — `staging-eu`, `prod`, your laptop. Its **type** says what kind
//! of place that is, and carries what every place of that kind shares:
//!
//! * a **colour**, so the title bar and the request bar say where a request is about to go;
//! * **protection** — a type can ask for confirmation before a method that changes things is sent;
//! * a **set of variables**, the defaults for every environment of the type (`timeout`, `logLevel`…), which an
//!   environment overrides one by one.
//!
//! `local`, `dev`, `staging` and `prod` come with Hirundo; the user can change them, delete them, and define
//! their own (`qa`, `sandbox`, `customer-acme`…). An environment whose type no longer exists is not an error:
//! it simply has no type, no colour and no protection.

use serde::{Deserialize, Serialize};

use crate::kv::KeyValue;
use crate::method::Method;
use crate::vars::{VarLayer, VarScope, VarSource};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvType {
    pub id: String,
    pub name: String,
    /// `#rgb` or `#rrggbb`.
    pub color: String,
    /// Ask before sending a method that changes things.
    #[serde(default)]
    pub confirm_mutating: bool,
    /// The variables every environment of this type starts from.
    #[serde(default)]
    pub vars: Vec<KeyValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    /// The [`EnvType::id`] it is an instance of; may name a type that no longer exists.
    #[serde(default)]
    pub type_id: String,
    #[serde(default)]
    pub vars: Vec<KeyValue>,
}

/// The types Hirundo starts with.
pub fn default_types() -> Vec<EnvType> {
    let t = |id: &str, name: &str, color: &str, confirm: bool| EnvType { id: id.into(), name: name.into(), color: color.into(), confirm_mutating: confirm, vars: Vec::new() };
    vec![t("local", "Local", "#3fb950", false), t("dev", "Dev", "#2f81f7", false), t("staging", "Staging", "#d29922", false), t("prod", "Prod", "#f85149", true)]
}

/// Whether sending `method` in an environment of type `ty` needs the user's confirmation.
pub fn needs_confirmation(ty: Option<&EnvType>, method: Method) -> bool {
    ty.is_some_and(|t| t.confirm_mutating) && method.is_mutating()
}

/// An id is a file name: lower-case letters, digits, `-` and `_`, starting with a letter or digit.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.len() <= 48
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

pub fn valid_color(color: &str) -> bool {
    let hex = color.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether a set of types may be saved, with the first thing wrong when not.
pub fn validate_types(types: &[EnvType]) -> Result<(), String> {
    for (i, t) in types.iter().enumerate() {
        if !valid_id(&t.id) {
            return Err(format!("'{}' is not a valid type id (lower-case letters, digits, - and _)", t.id));
        }
        if t.name.trim().is_empty() {
            return Err(format!("the type '{}' needs a name", t.id));
        }
        if !valid_color(&t.color) {
            return Err(format!("'{}' is not a colour for the type '{}' (use #rgb or #rrggbb)", t.color, t.id));
        }
        if types[..i].iter().any(|o| o.id == t.id) {
            return Err(format!("two types are called '{}'", t.id));
        }
    }
    Ok(())
}

pub fn validate_environment(env: &Environment) -> Result<(), String> {
    if !valid_id(&env.id) {
        return Err(format!("'{}' is not a valid environment id (lower-case letters, digits, - and _)", env.id));
    }
    if env.name.trim().is_empty() {
        return Err(format!("the environment '{}' needs a name", env.id));
    }
    Ok(())
}

/// The variable layers, outermost first: the collection's (and its folders'), then the type's, the
/// environment's, this machine's own, and what a script set at run time.
pub fn layers(inherited: Vec<VarLayer>, ty: Option<&EnvType>, env: Option<&Environment>, local: &[KeyValue], runtime: &[KeyValue]) -> Vec<VarLayer> {
    let mut out = inherited;
    if let Some(t) = ty {
        out.push(VarLayer { source: VarSource::new(VarScope::Type, &t.id), vars: t.vars.clone() });
    }
    if let Some(e) = env {
        out.push(VarLayer { source: VarSource::new(VarScope::Environment, &e.id), vars: e.vars.clone() });
    }
    out.push(VarLayer { source: VarSource::new(VarScope::Local, "local"), vars: local.to_vec() });
    out.push(VarLayer { source: VarSource::new(VarScope::Runtime, "run"), vars: runtime.to_vec() });
    out
}

/// The type an environment belongs to, if it still exists.
pub fn type_of<'a>(types: &'a [EnvType], env: &Environment) -> Option<&'a EnvType> {
    types.iter().find(|t| t.id == env.type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vars::resolve;

    fn kv(k: &str, v: &str) -> KeyValue {
        KeyValue::new(k, v)
    }

    #[test]
    fn four_types_come_with_hirundo_and_only_prod_is_protected() {
        let types = default_types();
        assert_eq!(types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["local", "dev", "staging", "prod"]);
        assert_eq!(types.iter().filter(|t| t.confirm_mutating).map(|t| t.id.as_str()).collect::<Vec<_>>(), ["prod"]);
        assert!(validate_types(&types).is_ok());
    }

    #[test]
    fn protection_asks_only_for_methods_that_change_things() {
        let types = default_types();
        let prod = types.iter().find(|t| t.id == "prod");
        let dev = types.iter().find(|t| t.id == "dev");
        assert!(needs_confirmation(prod, Method::Post));
        assert!(needs_confirmation(prod, Method::Delete));
        assert!(!needs_confirmation(prod, Method::Get), "reading is not asked about");
        assert!(!needs_confirmation(prod, Method::Head));
        assert!(!needs_confirmation(dev, Method::Delete), "an unprotected type never asks");
        assert!(!needs_confirmation(None, Method::Delete), "no type, no protection");
    }

    #[test]
    fn a_user_type_can_be_protected_and_carry_variables() {
        let qa = EnvType { id: "qa".into(), name: "QA".into(), color: "#a371f7".into(), confirm_mutating: true, vars: vec![kv("timeout", "5000")] };
        assert!(needs_confirmation(Some(&qa), Method::Put));
        assert!(validate_types(&[qa]).is_ok());
    }

    #[test]
    fn types_are_checked_before_they_are_saved() {
        let t = |id: &str, name: &str, color: &str| EnvType { id: id.into(), name: name.into(), color: color.into(), confirm_mutating: false, vars: vec![] };
        assert!(validate_types(&[t("Bad Id", "x", "#fff")]).unwrap_err().contains("not a valid type id"));
        assert!(validate_types(&[t("a", " ", "#fff")]).unwrap_err().contains("needs a name"));
        assert!(validate_types(&[t("a", "A", "red")]).unwrap_err().contains("not a colour"));
        assert!(validate_types(&[t("a", "A", "#12345")]).is_err());
        assert!(validate_types(&[t("a", "A", "#abc"), t("a", "B", "#abcdef")]).unwrap_err().contains("two types"));
        assert!(validate_environment(&Environment { id: "".into(), name: "x".into(), type_id: String::new(), vars: vec![] }).is_err());
    }

    #[test]
    fn the_environment_overrides_its_type_which_overrides_the_collection() {
        let ty = EnvType { id: "staging".into(), name: "Staging".into(), color: "#d29922".into(), confirm_mutating: false, vars: vec![kv("timeout", "5000"), kv("baseUrl", "https://type.example")] };
        let env = Environment { id: "stg-eu".into(), name: "Staging EU".into(), type_id: "staging".into(), vars: vec![kv("baseUrl", "https://stg-eu.example")] };
        let inherited = vec![VarLayer { source: VarSource::new(VarScope::Collection, "bandi"), vars: vec![kv("baseUrl", "http://localhost"), kv("page", "0")] }];
        let resolved = resolve(&layers(inherited, Some(&ty), Some(&env), &[kv("page", "7")], &[]), &|_, _| None);
        assert_eq!(resolved.values["baseUrl"].value, "https://stg-eu.example");
        assert_eq!(resolved.values["timeout"].value, "5000", "the type's own default shows through");
        assert_eq!(resolved.values["page"].value, "7", "local beats the collection");
    }

    #[test]
    fn an_environment_whose_type_is_gone_has_none() {
        let env = Environment { id: "e".into(), name: "E".into(), type_id: "retired".into(), vars: vec![] };
        assert!(type_of(&default_types(), &env).is_none());
        let known = Environment { type_id: "dev".into(), ..env };
        assert_eq!(type_of(&default_types(), &known).map(|t| t.name.as_str()), Some("Dev"));
    }
}
