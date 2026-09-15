//! `naming` as a template reads it: how the project names each kind of declaration where the output goes, and
//! names built that way.
//!
//! A template that makes a method name out of words — `should reject`, a field's name — cannot know whether
//! the project writes `shouldRejectBlankName` or `should_reject_blank_name`, and a project may say both:
//! camelCase in `src/main`, snake_case for its tests. `naming.method("should reject", field.name)` asks the
//! convention in force for the file being written — the project's naming rules where it checks them, the
//! language's standard where it does not.

use std::collections::BTreeMap;
use std::sync::Arc;

use bennu_naming::prelude::{packs, Convention, LanguageRules, Pack, Target};
use minijinja::value::{from_args, Enumerator, Object, ObjectRepr, Rest, Value};
use minijinja::{Error, ErrorKind, State};
use serde::{Serialize, Serializer};

/// The convention for each kind of declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingFacts {
    conventions: BTreeMap<Target, Convention>,
}

impl Default for NamingFacts {
    /// Java's standard — what a template reads outside any project.
    fn default() -> Self {
        match packs().iter().find(|pack| pack.id == "java") {
            Some(java) => Self::new(java, None),
            None => Self { conventions: BTreeMap::new() },
        }
    }
}

impl NamingFacts {
    /// `pack`'s standard, with `rules` laid over it wherever they name a convention.
    pub fn new(pack: &Pack, rules: Option<&LanguageRules>) -> Self {
        let mut conventions: BTreeMap<Target, Convention> = pack.standard.iter().copied().collect();
        for (target, convention) in rules.map(|rules| &rules.0).into_iter().flatten() {
            if !convention.is_off() {
                conventions.insert(*target, *convention);
            }
        }
        Self { conventions }
    }

    pub fn convention(&self, target: Target) -> Convention {
        self.conventions.get(&target).copied().unwrap_or_default()
    }

    /// `words` as one name under the convention for `target`: `["should reject", "blankName"]` is
    /// `shouldRejectBlankName` or `should_reject_blank_name`. Joined as written where no convention applies.
    pub fn name(&self, target: Target, words: &[String]) -> String {
        let joined = words.iter().map(|word| word.trim()).filter(|word| !word.is_empty()).collect::<Vec<_>>().join("_");
        self.convention(target).render(&joined).unwrap_or(joined)
    }

    /// Each target by the name a template uses for it — `enum_constant` for `enum-constant` — to its convention.
    pub fn by_name(&self) -> BTreeMap<String, String> {
        Target::ALL.iter().map(|target| (template_name(*target), self.convention(*target).as_str().to_string())).collect()
    }

    /// These conventions with `over` — in [`Self::by_name`]'s shape — laid over them: a preview's parameters.
    pub fn laid_over(&self, over: &BTreeMap<String, String>) -> Self {
        let mut conventions = self.conventions.clone();
        for (name, value) in over {
            let convention = Convention::ALL.iter().copied().find(|convention| convention.as_str() == value);
            if let (Some(target), Some(convention)) = (target_named(name), convention) {
                conventions.insert(target, convention);
            }
        }
        Self { conventions }
    }
}

impl Serialize for NamingFacts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.by_name().serialize(serializer)
    }
}

fn template_name(target: Target) -> String {
    target.as_str().replace('-', "_")
}

fn target_named(name: &str) -> Option<Target> {
    let slug = name.replace('_', "-");
    Target::ALL.iter().copied().find(|target| target.as_str() == slug)
}

/// What `naming.` offers, with what each is: every target, read as its convention and called to build a name.
pub fn naming_members() -> Vec<(String, String)> {
    Target::ALL
        .iter()
        .map(|target| {
            let name = template_name(*target);
            let label = target.label().to_lowercase();
            let doc = format!(
                "{name}(words…) — a {label} name made of the words, in the project's convention. `naming.{name}` alone is the convention: `camelCase`, `snake_case`…"
            );
            (name, doc)
        })
        .collect()
}

/// The facts as the object a template reads — see the module notes.
pub(crate) fn naming_value(facts: NamingFacts) -> Value {
    Value::from_object(NamingObject(facts))
}

#[derive(Debug)]
struct NamingObject(NamingFacts);

impl Object for NamingObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let target = target_named(key.as_str()?)?;
        Some(Value::from(self.0.convention(target).as_str()))
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Values(Target::ALL.iter().map(|target| Value::from(template_name(*target))).collect())
    }

    fn call_method(self: &Arc<Self>, _state: &State<'_, '_>, method: &str, args: &[Value]) -> Result<Value, Error> {
        let Some(target) = target_named(method) else {
            return Err(Error::from(ErrorKind::UnknownMethod));
        };
        let (Rest(words),): (Rest<Value>,) = from_args(args)?;
        let words: Vec<String> = words.iter().map(Value::to_string).collect();
        Ok(Value::from(self.0.name(target, &words)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render_with;
    use crate::facts::TemplateFacts;
    use serde_json::json;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn outside_a_project_names_follow_java() {
        let naming = NamingFacts::default();
        assert_eq!(naming.name(Target::Method, &words(&["should reject", "blankName"])), "shouldRejectBlankName");
        assert_eq!(naming.name(Target::Constant, &words(&["max", "retries"])), "MAX_RETRIES");
        assert_eq!(naming.name(Target::Type, &words(&["order", "line"])), "OrderLine");
    }

    #[test]
    fn a_project_rule_wins_and_a_rule_that_says_nothing_leaves_the_standard() {
        let java = packs().iter().find(|pack| pack.id == "java").unwrap();
        let rules = LanguageRules::from_pairs([(Target::Method, Convention::LowerSnake), (Target::Local, Convention::Any)]);
        let naming = NamingFacts::new(java, Some(&rules));
        assert_eq!(naming.name(Target::Method, &words(&["should reject", "blankName"])), "should_reject_blank_name");
        assert_eq!(naming.convention(Target::Local), Convention::Camel);
    }

    #[test]
    fn a_template_reads_the_convention_and_builds_a_name_and_a_parameter_can_change_it() {
        let template = "{{ naming.method(\"should reject\", \"blankName\") }} {{ naming.method }} {{ naming.enum_constant }}";
        let facts = TemplateFacts::default();
        assert_eq!(
            render_with(template, &json!({}), &facts, None).unwrap(),
            "shouldRejectBlankName camelCase UPPER_SNAKE_CASE"
        );
        let over = json!({ "naming": { "method": "snake_case" } });
        assert_eq!(
            render_with(template, &json!({}), &facts, Some(&over)).unwrap(),
            "should_reject_blank_name snake_case UPPER_SNAKE_CASE"
        );
    }
}
