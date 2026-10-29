//! Showing the user a diagram.
//!
//! A picture explains an architecture, a flow or a set of entities better than a paragraph, and Mermaid is
//! text a model writes well. What the backend cannot do is read it: only Mermaid itself can say whether a
//! source parses, and it lives in the panel. So the tool is a round trip, like a question: the diagram
//! goes to the thread as a block waiting for a verdict, the panel parses it and answers, and the model is
//! told either that it is shown or exactly what was wrong (Mermaid's own message, line included) so it can
//! fix it and call again. A source that keeps failing is given up on after a few tries and shown as text.
//!
//! The failed attempts are noise the model should not carry: [`super::diagram_context`] removes them from
//! what is sent once a diagram has gone through.

use std::time::Duration;

use bennu_assistant::prelude::ToolSpec;
use serde_json::{json, Value};

use super::{spec, unique_id, ToolHost};

/// The tool's name, which the history pruning and the thread both key on.
pub(super) const NAME: &str = "show_diagram";

/// How many sources may fail in one run before the tool stops trying.
const MAX_FAILURES: u32 = 3;
/// The largest source accepted; the panel draws no more than this either.
const MAX_SOURCE_CHARS: usize = 20_000;
/// How long the panel has to answer. It parses in milliseconds once Mermaid is loaded; this is for a panel
/// that is gone, so that the run goes on instead of waiting for it.
const VERDICT_WAIT: Duration = Duration::from_secs(20);

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![spec(
        NAME,
        "Show the user a diagram, written in Mermaid (flowchart, sequenceDiagram, classDiagram, stateDiagram-v2, erDiagram, gantt, …). Use it when a picture explains better than text: an architecture, a flow, a call chain, the entities of a model and how they relate. Keep it small and readable — a dozen nodes, short labels. The source is checked before it is shown: if it does not parse you get Mermaid's error with the line, so fix the source and call again. Send only the Mermaid source, with no ``` fence, and do not repeat the diagram in your text.",
        json!({"type":"object","properties":{
            "mermaid":{"type":"string","description":"The Mermaid source, starting with the diagram type (`flowchart TD`, `sequenceDiagram`, …)."},
            "title":{"type":"string","description":"A short caption shown above the diagram."}
        },"required":["mermaid"]}),
    )]
}

/// What the model reads when its source was refused: the reason, then what to do about it.
fn refusal(error: &str, failures: u32) -> String {
    if failures >= MAX_FAILURES {
        format!(
            "The diagram did not parse: {error}\nIt has failed {failures} times, so no more tries: the source was shown to the user as text. Tell them briefly that the diagram could not be drawn; do not call show_diagram again."
        )
    } else {
        format!(
            "The diagram did not parse: {error}\nFix the Mermaid source and call show_diagram again ({} {} left).",
            MAX_FAILURES - failures,
            if MAX_FAILURES - failures == 1 { "try" } else { "tries" }
        )
    }
}

impl ToolHost {
    pub(super) fn run_show_diagram(&self, input: &Value) -> Result<String, String> {
        let source = input.get("mermaid").and_then(Value::as_str).map(str::trim).unwrap_or("");
        if source.is_empty() {
            return Err("`mermaid` is required: the diagram's source".into());
        }
        if source.chars().count() > MAX_SOURCE_CHARS {
            return Err(format!("The diagram is too large (over {MAX_SOURCE_CHARS} characters). Make it smaller: fewer nodes, shorter labels."));
        }
        if *self.diagram_failures.lock().unwrap_or_else(|p| p.into_inner()) >= MAX_FAILURES {
            return Err(refusal("(gave up earlier in this run)", MAX_FAILURES));
        }

        let id = unique_id("dg");
        let mut diagram = json!({ "id": id, "source": source, "status": "checking" });
        if let Some(title) = input.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) {
            diagram["title"] = json!(title);
        }
        let block = json!({ "type": "diagram", "diagram": diagram });
        let answer = self.session.ask_within(block, "/diagram/id", &id, Some(VERDICT_WAIT), |b, a| {
            if a.get("ok").and_then(Value::as_bool) == Some(true) {
                b["diagram"]["status"] = json!("ok");
            }
        });

        let Some(answer) = answer else {
            if self.session.cancelled() {
                return Err("cancelled before the diagram was checked".into());
            }
            // The panel did not answer in time. The diagram stays in the thread and the panel shows the
            // reason itself if it cannot draw it; the run is not held up for it.
            self.session.view().update("/diagram/id", &id, |b| b["diagram"]["status"] = json!("ok"));
            self.session.push_message();
            return Ok("Diagram shown to the user (it could not be checked in time). Do not repeat it in text.".into());
        };

        match answer.get("error").and_then(Value::as_str) {
            None => Ok("Diagram shown to the user. Do not repeat it in text.".into()),
            Some(error) => {
                let failures = {
                    let mut n = self.diagram_failures.lock().unwrap_or_else(|p| p.into_inner());
                    *n += 1;
                    *n
                };
                let mut view = self.session.view();
                // A diagram that does not parse is not worth the user's eye: the receipt line records the attempt.
                view.remove("/diagram/id", &id);
                if failures >= MAX_FAILURES {
                    view.push(json!({ "type": "code", "language": "mermaid", "code": source }));
                    view.push(json!({ "type": "notice", "level": "warning", "text": "Imber could not write a diagram that parses; its source is shown as text." }));
                }
                drop(view);
                self.session.push_message();
                Err(refusal(error, failures))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_asks_for_the_source_and_the_description_asks_for_a_small_one() {
        let spec = specs().remove(0);
        assert_eq!(spec.name, NAME);
        assert_eq!(spec.input_schema["required"], json!(["mermaid"]));
        assert!(spec.description.contains("Mermaid") && spec.description.contains("fix the source"));
    }

    #[test]
    fn a_refusal_says_how_many_tries_are_left_and_the_last_one_says_stop() {
        let first = refusal("Parse error on line 3", 1);
        assert!(first.contains("line 3") && first.contains("2 tries left"));
        assert!(refusal("x", 2).contains("1 try left"));
        let last = refusal("x", MAX_FAILURES);
        assert!(last.contains("no more tries") && last.contains("do not call show_diagram again"));
    }
}
