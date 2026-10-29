//! Showing the user a part of Bennu.
//!
//! "Open the vulnerabilities of my project" is a request to *look*, and the answer is the window Bennu
//! already has for it — not a paraphrase of its contents in the chat. The tool does nothing to the
//! project: it tells the editor to open a panel or a dialog, by a name from a fixed list the frontend
//! knows. It needs no access to files, and it is the only tool that reaches the interface at all.

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, ToolHost};

/// What can be opened, and what each is for — the names the model may use.
const TARGETS: [(&str, &str); 12] = [
    ("vulnerabilities", "the known vulnerabilities (CVEs) of the project's dependencies"),
    ("problems", "the Problems panel: the errors and warnings of the project"),
    ("tests", "the Tests panel"),
    ("run", "the Run panel: running programs and their output"),
    ("terminal", "the Terminal"),
    ("git_changes", "the Git changes of the project"),
    ("todos", "the TODO comments of the project"),
    ("dependencies", "the project's dependencies"),
    ("module_graph", "the module (or crate) graph: who depends on whom"),
    ("structural_search", "the structural search and replace window"),
    ("find_in_files", "Find in Files, optionally with a text already typed in (`query`)"),
    ("settings", "Bennu's settings"),
];

pub(super) fn specs() -> Vec<ToolSpec> {
    let list: Vec<&str> = TARGETS.iter().map(|(id, _)| *id).collect();
    let described: Vec<String> = TARGETS.iter().map(|(id, what)| format!("`{id}` — {what}")).collect();
    vec![spec(
        "open_ui",
        &format!(
            "Open a window or panel of Bennu for the user to look at. It only shows it; it changes nothing. Use it when the user asks to see something ('show me the vulnerabilities'), rather than describing it in the chat. Targets: {}.",
            described.join("; ")
        ),
        json!({"type":"object","properties":{
            "target":{"type":"string","enum":list},
            "query":{"type":"string","description":"For `find_in_files`: the text to search for."}
        },"required":["target"]}),
    )]
}

impl ToolHost {
    pub(super) fn run_open_ui(&self, input: &Value) -> Result<String, String> {
        let target = input.get("target").and_then(Value::as_str).ok_or("`target` is required")?;
        let Some((_, what)) = TARGETS.iter().find(|(id, _)| *id == target) else {
            return Err(format!("`{target}` is not something that can be opened"));
        };
        let mut event = json!({ "target": target });
        if let Some(q) = input.get("query").and_then(Value::as_str) {
            event["query"] = json!(q);
        }
        self.session.emit("open_ui", event);
        Ok(format!("Opened {what} for the user."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_is_in_the_schema_and_described() {
        let spec = specs().remove(0);
        let listed = spec.input_schema.pointer("/properties/target/enum").and_then(Value::as_array).unwrap().len();
        assert_eq!(listed, TARGETS.len());
        for (id, _) in TARGETS {
            assert!(spec.description.contains(id), "{id} is not described");
        }
    }
}
