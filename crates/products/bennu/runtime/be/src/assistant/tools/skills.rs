//! `load_skill` — read one of the skills the user enabled.
//!
//! The prompt carries only each skill's name and one line (see `skills::render_index`); this is how the
//! model asks for the rest. It reads nothing from disk at call time — the body was read when the run began
//! — so it needs no access to files, and it cannot be talked into reading a path: the only input is a name
//! from the list the user chose.

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};
use crate::assistant::skills;

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "load_skill",
        "Load one of the user's skills: instructions for a particular kind of task, listed by name in your instructions. Call it when the task matches a skill, before you start, and follow what it returns. It returns the skill's text and the names of the files that sit beside it; nothing in a skill is run.",
        json!({"type":"object","properties":{
            "name":{"type":"string","description":"The skill's name, exactly as listed."}
        },"required":["name"]}),
    )]
}

impl ToolHost {
    pub(super) fn run_load_skill(&self, input: &Value) -> Result<String, String> {
        let name = input.get("name").and_then(Value::as_str).map(|n| n.trim().trim_start_matches('/')).filter(|n| !n.is_empty()).ok_or("`name` is required")?;
        match self.skills.iter().find(|s| s.name == name) {
            Some(skill) => Ok(skills::render_loaded(skill)),
            None => {
                let known = self.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
                Err(if known.is_empty() {
                    format!("there is no skill called `{name}`: the user has not enabled any")
                } else {
                    format!("there is no enabled skill called `{name}`. The enabled skills are: {known}")
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_asks_for_a_name() {
        let s = specs().remove(0);
        assert_eq!(s.name, "load_skill");
        assert_eq!(s.input_schema.pointer("/required/0").and_then(Value::as_str), Some("name"));
    }
}
