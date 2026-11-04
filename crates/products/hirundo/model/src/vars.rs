//! Variables: where a `{{name}}` gets its value.
//!
//! Values come in **layers**, and the later layer wins:
//!
//! 1. the collection, then its folders (outermost first) — what is true of this API everywhere;
//! 2. the **environment type** (`prod`, `staging`, …) — what is true of every environment of that kind;
//! 3. the **environment** — one place the API runs;
//! 4. **local** — this machine's own values, never committed;
//! 5. **runtime** — what a test script set during this run.
//!
//! A variable whose value is `@secret` has no value in any file: it is looked up in the keyring, under the
//! layer that declares it. If the keyring has nothing the variable is *missing*, not empty — and an earlier
//! layer's plain value does not leak through, because a secret declared later is a statement that the
//! value must not be the plain one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::kv::KeyValue;

/// How deep `{{a}}` → `{{b}}` → … is followed before giving up (a variable that refers to itself).
const MAX_DEPTH: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarScope {
    Collection,
    Folder,
    Type,
    Environment,
    Local,
    Runtime,
}

/// Which layer a value came from: shown next to it in the preview, and the keyring account of a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarSource {
    pub scope: VarScope,
    pub id: String,
}

impl VarSource {
    pub fn new(scope: VarScope, id: &str) -> Self {
        Self { scope, id: id.into() }
    }
}

#[derive(Debug, Clone)]
pub struct VarLayer {
    pub source: VarSource,
    pub vars: Vec<KeyValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved {
    pub value: String,
    pub source: VarSource,
    pub secret: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResolvedVars {
    pub values: BTreeMap<String, Resolved>,
    /// Secrets declared but not found in the keyring.
    pub missing_secrets: Vec<String>,
}

/// Fold the layers into the value each name has. `secret` looks a secret up by the layer that declares it.
pub fn resolve(layers: &[VarLayer], secret: &dyn Fn(&VarSource, &str) -> Option<String>) -> ResolvedVars {
    let mut out = ResolvedVars::default();
    for layer in layers {
        for kv in layer.vars.iter().filter(|v| v.enabled && !v.key.trim().is_empty()) {
            let name = kv.key.trim().to_string();
            if kv.is_secret() {
                match secret(&layer.source, &name) {
                    Some(value) => {
                        out.missing_secrets.retain(|m| m != &name);
                        out.values.insert(name, Resolved { value, source: layer.source.clone(), secret: true });
                    }
                    None => {
                        out.values.remove(&name);
                        if !out.missing_secrets.contains(&name) {
                            out.missing_secrets.push(name);
                        }
                    }
                }
            } else {
                out.missing_secrets.retain(|m| m != &name);
                out.values.insert(name, Resolved { value: kv.value.clone(), source: layer.source.clone(), secret: false });
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolated {
    pub text: String,
    /// Names that had no value, once each, in the order they were met. Their `{{…}}` is left in the text.
    pub unresolved: Vec<String>,
}

/// Replace every `{{name}}` with its value. A value may itself contain `{{…}}`.
pub fn interpolate(text: &str, vars: &ResolvedVars) -> Interpolated {
    let mut unresolved = Vec::new();
    let text = expand(text, vars, 0, &mut unresolved);
    Interpolated { text, unresolved }
}

fn expand(text: &str, vars: &ResolvedVars, depth: u32, unresolved: &mut Vec<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        match vars.values.get(name) {
            Some(r) if depth < MAX_DEPTH => out.push_str(&expand(&r.value, vars, depth + 1, unresolved)),
            _ => {
                if !name.is_empty() && !unresolved.iter().any(|u| u == name) {
                    unresolved.push(name.to_string());
                }
                out.push_str(&rest[start..start + 2 + end + 2]);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(scope: VarScope, id: &str, pairs: &[(&str, &str)]) -> VarLayer {
        VarLayer { source: VarSource::new(scope, id), vars: pairs.iter().map(|(k, v)| KeyValue::new(k, v)).collect() }
    }

    fn no_secrets(_: &VarSource, _: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_later_layer_overrides_an_earlier_one_and_remembers_where_it_came_from() {
        let layers = [
            layer(VarScope::Collection, "bandi", &[("baseUrl", "http://localhost"), ("page", "0")]),
            layer(VarScope::Type, "prod", &[("baseUrl", "https://prod.example.it")]),
            layer(VarScope::Environment, "prod-eu", &[("baseUrl", "https://eu.example.it")]),
        ];
        let r = resolve(&layers, &no_secrets);
        assert_eq!(r.values["baseUrl"].value, "https://eu.example.it");
        assert_eq!(r.values["baseUrl"].source, VarSource::new(VarScope::Environment, "prod-eu"));
        assert_eq!(r.values["page"].source.scope, VarScope::Collection);
    }

    #[test]
    fn a_disabled_row_defines_nothing() {
        let mut l = layer(VarScope::Environment, "e", &[("x", "1")]);
        l.vars[0].enabled = false;
        assert!(resolve(&[l], &no_secrets).values.is_empty());
    }

    #[test]
    fn a_secret_is_read_from_the_keyring_under_its_layer() {
        let layers = [layer(VarScope::Environment, "staging", &[("token", "@secret")])];
        let lookup = |src: &VarSource, name: &str| (src.id == "staging" && name == "token").then(|| "s3cret".to_string());
        let r = resolve(&layers, &lookup);
        assert_eq!(r.values["token"].value, "s3cret");
        assert!(r.values["token"].secret);
        assert!(r.missing_secrets.is_empty());
    }

    #[test]
    fn a_missing_secret_is_missing_not_the_plain_value_of_an_earlier_layer() {
        let layers = [layer(VarScope::Collection, "c", &[("token", "dev-token")]), layer(VarScope::Environment, "prod", &[("token", "@secret")])];
        let r = resolve(&layers, &no_secrets);
        assert!(!r.values.contains_key("token"), "the plain value must not stand in for the secret");
        assert_eq!(r.missing_secrets, ["token"]);
        let again = resolve(&[layers[1].clone(), layer(VarScope::Local, "me", &[("token", "mine")])], &no_secrets);
        assert_eq!(again.values["token"].value, "mine");
        assert!(again.missing_secrets.is_empty(), "a later plain value settles it");
    }

    #[test]
    fn variables_are_replaced_and_may_refer_to_each_other() {
        let r = resolve(&[layer(VarScope::Collection, "c", &[("host", "api.it"), ("baseUrl", "https://{{host}}/v1"), ("id", "42")])], &no_secrets);
        let i = interpolate("{{ baseUrl }}/bandi/{{id}}?x={{id}}", &r);
        assert_eq!(i.text, "https://api.it/v1/bandi/42?x=42");
        assert!(i.unresolved.is_empty());
    }

    #[test]
    fn an_unknown_variable_stays_visible_and_is_reported_once() {
        let r = resolve(&[], &no_secrets);
        let i = interpolate("{{a}}/{{b}}/{{a}} {{ }} {{broken", &r);
        assert_eq!(i.text, "{{a}}/{{b}}/{{a}} {{ }} {{broken");
        assert_eq!(i.unresolved, ["a", "b"]);
    }

    #[test]
    fn a_variable_that_refers_to_itself_stops() {
        let r = resolve(&[layer(VarScope::Collection, "c", &[("a", "{{a}}!")])], &no_secrets);
        let i = interpolate("{{a}}", &r);
        assert!(i.text.starts_with("{{a}}") || i.text.contains('!'), "{}", i.text);
        assert!(i.text.len() < 200);
    }

    #[test]
    fn text_without_variables_is_untouched() {
        let r = ResolvedVars::default();
        assert_eq!(interpolate("plain { text } {x}", &r).text, "plain { text } {x}");
    }
}
