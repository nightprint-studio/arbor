//! What every template can read about where it is used, whatever its kind: the **project** — its Java
//! level and its dependencies — the **style** the user writes Java in, and how the project **names** things
//! where the output goes ([`crate::naming`]).
//!
//! Beside each kind's own context rather than inside it, because it is the same question for all of them:
//! "`@Builder` only with Lombok", "`record` only from Java 16", "`val` when that is how I write locals".
//! The backend knows the answers; [`crate::engine::render_with`] puts them next to the kind's fields as
//! `project` and `style`, and [`crate::requires`] reads the same facts to decide whether a template is
//! offered at all.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::schema_of;
use crate::naming::{naming_members, NamingFacts};
use crate::style::STYLE_METHODS;

/// The project a template is rendered for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProjectFacts {
    /// The Java language level: `8`, `17`. `0` while it is not known.
    pub java: u32,
    /// Every dependency on the classpath as `group:artifact` —
    /// `{% if "org.projectlombok:lombok" in project.dependencies %}`.
    pub dependencies: Vec<String>,
    /// The same dependencies by artifact alone — `{% if "lombok" in project.artifacts %}`.
    pub artifacts: Vec<String>,
    /// The version of each dependency, by `group:artifact`.
    pub versions: BTreeMap<String, String>,
    /// The classpath has been resolved. Until it has, nothing is known to be missing, so no template is
    /// held back for a dependency.
    pub resolved: bool,
}

impl ProjectFacts {
    /// From the Java level and the classpath's `(group, artifact, version)` coordinates.
    pub fn new(java: u32, coordinates: impl IntoIterator<Item = (String, String, String)>, resolved: bool) -> Self {
        let mut facts = ProjectFacts { java, resolved, ..ProjectFacts::default() };
        for (group, artifact, version) in coordinates {
            let id = format!("{group}:{artifact}");
            if facts.versions.contains_key(&id) {
                continue;
            }
            facts.dependencies.push(id.clone());
            if !facts.artifacts.contains(&artifact) {
                facts.artifacts.push(artifact);
            }
            facts.versions.insert(id, version);
        }
        facts.dependencies.sort();
        facts.artifacts.sort();
        facts
    }

    /// The version of a dependency named `group:artifact`, or by its artifact alone; `Some("")` for one
    /// present without a known version.
    pub fn version_of(&self, name: &str) -> Option<String> {
        let name = name.trim().to_ascii_lowercase();
        self.versions.iter().find_map(|(id, version)| {
            let id = id.to_ascii_lowercase();
            let artifact = id.rsplit(':').next().unwrap_or(&id);
            (id == name || artifact == name).then(|| version.clone())
        })
    }
}

/// How the user writes Java — Settings › Java Style. A template reads it as `style`, which also writes
/// declarations the way it says: [`crate::style`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StyleFacts {
    /// Generated parameters and locals are `final`.
    pub final_params: bool,
    /// Locals are declared with Lombok's `val`. True only when that is the preference **and** the project
    /// has Lombok, so a template needs no second check.
    pub lombok_val: bool,
    /// Locals are declared with `var`. True only when that is the preference **and** the project is on Java 10
    /// or later; `style.local(type)` writes `val` instead where both are true.
    pub local_var: bool,
    /// A switch that yields a value is written as an arrow-style switch expression.
    pub switch_with_return: bool,
    /// A single-line body has a space inside its braces: `{ return x; }`.
    pub space_in_braces: bool,
    /// Generated members are separated by a blank line.
    pub blank_line_between_members: bool,
}

/// Both, as a render receives them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TemplateFacts {
    pub project: ProjectFacts,
    pub style: StyleFacts,
    /// Java's standard until the backend knows the project and the file.
    pub naming: NamingFacts,
}

/// A kind's context schema with `project`, `style` and `naming` beside its fields — what the editor completes.
pub fn with_fact_schemas(mut schema: Value) -> Value {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert("project".to_string(), schema_of::<ProjectFacts>());
        properties.insert("style".to_string(), style_schema());
        properties.insert("naming".to_string(), naming_schema());
    }
    schema
}

/// Every target of `naming`, each marked `x-method`: read alone it is a convention, called it builds a name.
fn naming_schema() -> Value {
    let properties: serde_json::Map<String, Value> = naming_members()
        .into_iter()
        .map(|(name, doc)| (name, serde_json::json!({ "type": "string", "description": doc, "x-method": true })))
        .collect();
    serde_json::json!({
        "type": "object",
        "description": "How the project names each kind of declaration, where the output goes",
        "properties": properties,
    })
}

/// The style's fields, and its methods marked `x-method` — which the editor offers as calls, not fields.
fn style_schema() -> Value {
    let mut schema = schema_of::<StyleFacts>();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        for (name, doc) in STYLE_METHODS {
            properties.insert(name.to_string(), serde_json::json!({ "description": doc, "x-method": true }));
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dependency_is_found_by_its_coordinate_or_its_artifact() {
        let facts = ProjectFacts::new(
            17,
            [
                ("org.projectlombok".to_string(), "lombok".to_string(), "1.18.30".to_string()),
                ("org.springframework.boot".to_string(), "spring-boot".to_string(), "3.2.1".to_string()),
            ],
            true,
        );
        assert_eq!(facts.dependencies, ["org.projectlombok:lombok", "org.springframework.boot:spring-boot"]);
        assert_eq!(facts.artifacts, ["lombok", "spring-boot"]);
        assert_eq!(facts.version_of("lombok").as_deref(), Some("1.18.30"));
        assert_eq!(facts.version_of("org.springframework.boot:spring-boot").as_deref(), Some("3.2.1"));
        assert_eq!(facts.version_of("jackson-databind"), None);
    }

    #[test]
    fn the_schema_of_every_kind_offers_the_facts() {
        let schema = with_fact_schemas(serde_json::json!({ "properties": { "class": {} } }));
        assert!(schema["properties"]["project"]["properties"].get("dependencies").is_some(), "{schema}");
        assert!(schema["properties"]["style"]["properties"].get("lombok_val").is_some(), "{schema}");
        assert_eq!(schema["properties"]["style"]["properties"]["local"]["x-method"], true, "{schema}");
        assert_eq!(schema["properties"]["naming"]["properties"]["method"]["x-method"], true, "{schema}");
    }
}
