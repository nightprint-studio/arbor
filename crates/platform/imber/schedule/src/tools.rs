//! The `schedule_*` tools Imber uses: their definitions, and the runner that turns a call into a saved job.
//! Written once against [`Flavour`]; the approval rule and what "run" means stay with each product.

use arbor_scheduler::prelude::{new_id, when_to_recurrence, Job, JobAction, JobInput, MissedPolicy, Store};
use imber_core::prelude::ToolSpec;
use serde_json::{json, Value};

use crate::describe::job_json;
use crate::flavour::Flavour;

pub const SCHEDULE_TOOLS: [&str; 4] = ["schedule_list", "schedule_create", "schedule_update", "schedule_delete"];

/// Whether `name` is one of the schedule tools.
pub fn is_schedule_tool(name: &str) -> bool {
    SCHEDULE_TOOLS.contains(&name)
}

/// Whether a scheduled run may call `name`: reading the list is fine; anything else would let a job schedule
/// more jobs, which would outlive anyone's say in it.
pub fn unattended_may_call(name: &str) -> bool {
    name == "schedule_list"
}

fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec { name: name.into(), description: description.into(), input_schema: json!({ "type": "object", "properties": properties, "required": required }) }
}

fn when_schema() -> Value {
    json!({ "type": "object", "description": "When it runs, in local time. {\"kind\":\"once\",\"at\":\"2026-11-05T09:00\"} | {\"kind\":\"daily\",\"time\":\"09:00\"} | {\"kind\":\"weekly\",\"days\":[\"mon\",\"fri\"],\"time\":\"09:00\"} | {\"kind\":\"every\",\"minutes\":30} | {\"kind\":\"cron\",\"expr\":\"0 0 9 * * 1-5\"} (6 fields: sec min hour day month weekday)." })
}

/// The four tool definitions, worded for the product.
pub fn specs<F: Flavour>(f: &F) -> Vec<ToolSpec> {
    let s = |d: &str| json!({ "type": "string", "description": d });
    let mut create_props = json!({
        "name": s("A short name."),
        "action": json!({ "type": "string", "enum": f.actions() }),
        "prompt": s("What to ask Imber at that time (for the actions that ask it). Self-contained: say what to check and what to report."),
        "when": when_schema(),
        "mode": json!({ "type": "string", "enum": f.modes() }),
        "missed": json!({ "type": "string", "enum": ["skip", "run_once"] }),
    });
    if let (Some(props), Some(extra)) = (create_props.as_object_mut(), f.extra_create_properties().as_object()) {
        props.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    vec![
        spec("schedule_list", "List the schedules: what runs when, whether it is on, its next run and how the last one ended.", json!({}), &[]),
        spec(
            "schedule_create",
            &format!(
                "Schedule something for later or on a repeat, for THIS {}. {} A scheduled run has nobody to approve anything: {} {} must be open at the time; a run missed while it was closed is dropped unless missed=`run_once`.",
                f.unit(),
                f.actions_doc(),
                f.modes_doc(),
                f.app()
            ),
            create_props,
            &["name", "action", "when"],
        ),
        spec(
            "schedule_update",
            "Change a schedule: turn it on or off, or edit its name, prompt, time or mode.",
            json!({ "id": s("The schedule's id (from schedule_list)."), "enabled": json!({ "type": "boolean" }), "name": s("New name."), "prompt": s("New prompt."), "when": when_schema(), "mode": json!({ "type": "string", "enum": f.modes() }) }),
            &["id"],
        ),
        spec("schedule_delete", "Delete a schedule.", json!({ "id": s("The schedule's id.") }), &["id"]),
    ]
}

/// Who is calling a schedule tool.
pub struct ToolCtx<'a> {
    /// The repository or project of the conversation: schedules of another are not visible or changeable.
    pub root: &'a str,
    pub conversation_id: &'a str,
    /// The mode word of the conversation: a schedule it creates is no freer.
    pub conversation_mode: &'a str,
    pub now_ms: i64,
}

fn text(input: &Value, k: &str) -> Option<String> {
    input.get(k).and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty())
}

/// Run a schedule tool.
pub fn run_tool<F: Flavour>(f: &F, store: &Store<F::Action>, ctx: &ToolCtx<'_>, name: &str, input: &Value) -> Result<String, String> {
    let json_of = |j: &Job<F::Action>| serde_json::to_string(&job_json(j)).map_err(|e| e.to_string());
    match name {
        "schedule_list" => {
            let mine: Vec<Value> = store.list().iter().filter(|j| j.root == ctx.root).map(job_json).collect();
            serde_json::to_string(&mine).map_err(|e| e.to_string())
        }
        "schedule_create" => {
            let action = f.action_from_input(text(input, "action").as_deref(), text(input, "prompt"), input, ctx.conversation_id)?;
            let recurrence = when_to_recurrence(input.get("when").ok_or("`when` is required")?, ctx.now_ms)?;
            let missed = if text(input, "missed").as_deref() == Some("run_once") { MissedPolicy::RunOnce } else { MissedPolicy::Skip };
            let requested = text(input, "mode");
            let mode = f.mode_for(requested.as_deref(), ctx.conversation_mode);
            let job_input = JobInput { id: None, name: text(input, "name").ok_or("`name` is required")?, enabled: true, recurrence, missed, action, root: ctx.root.to_string(), mode: mode.clone() };
            let job = store.upsert(Job::from_input(None, job_input, ctx.now_ms, new_id, |m| f.mode_word(m))?)?;
            let lowered = requested.as_deref().is_some_and(|r| f.mode_word(r) != mode);
            Ok(format!("{}{}", json_of(&job)?, if lowered { f.lowered_note() } else { "" }))
        }
        "schedule_update" => {
            let id = text(input, "id").ok_or("`id` is required")?;
            let current = store.get(&id).ok_or_else(|| format!("schedule '{id}' not found"))?;
            if current.root != ctx.root {
                return Err("That schedule belongs to another project.".into());
            }
            let recurrence = match input.get("when") {
                Some(w) => when_to_recurrence(w, ctx.now_ms)?,
                None => current.recurrence.clone(),
            };
            let action = match text(input, "prompt") {
                Some(prompt) => f.with_prompt(&current.action, prompt),
                None => current.action.clone(),
            };
            let mode = match text(input, "mode") {
                Some(m) => f.mode_for(Some(&m), ctx.conversation_mode),
                None => current.mode.clone(),
            };
            let edited = JobInput {
                id: Some(id),
                name: text(input, "name").unwrap_or_else(|| current.name.clone()),
                enabled: input.get("enabled").and_then(Value::as_bool).unwrap_or(current.enabled),
                recurrence,
                missed: current.missed,
                action,
                root: current.root.clone(),
                mode,
            };
            let job = store.upsert(Job::from_input(Some(current), edited, ctx.now_ms, new_id, |m| f.mode_word(m))?)?;
            json_of(&job)
        }
        "schedule_delete" => {
            let id = text(input, "id").ok_or("`id` is required")?;
            if store.get(&id).is_some_and(|j| j.root != ctx.root) {
                return Err("That schedule belongs to another project.".into());
            }
            store.remove(&id)?;
            Ok(format!("Deleted schedule {id}."))
        }
        other => Err(format!("'{other}' is not a schedule tool.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_scheduler::prelude::Recurrence;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Act {
        Prompt { prompt: String },
        Fetch,
    }
    impl JobAction for Act {
        fn prompt(&self) -> Option<&str> {
            match self {
                Act::Prompt { prompt } => Some(prompt),
                Act::Fetch => None,
            }
        }
        fn word(&self) -> &'static str {
            match self {
                Act::Prompt { .. } => "prompt",
                Act::Fetch => "fetch",
            }
        }
    }

    struct Test;
    impl Flavour for Test {
        type Action = Act;
        fn app(&self) -> &'static str {
            "Testbed"
        }
        fn unit(&self) -> &'static str {
            "repository"
        }
        fn actions(&self) -> &'static [&'static str] {
            &["prompt", "fetch"]
        }
        fn modes(&self) -> &'static [&'static str] {
            &["manual", "auto"]
        }
        fn actions_doc(&self) -> &'static str {
            "action `prompt` asks; `fetch` fetches."
        }
        fn modes_doc(&self) -> &'static str {
            "`manual` is read-only; `auto` may change things."
        }
        fn action_from_input(&self, word: Option<&str>, prompt: Option<String>, _input: &Value, _conversation_id: &str) -> Result<Act, String> {
            match word {
                Some("prompt") => Ok(Act::Prompt { prompt: prompt.ok_or("`prompt` is required for this action")? }),
                Some("fetch") => Ok(Act::Fetch),
                other => Err(format!("unknown action {other:?}")),
            }
        }
        fn with_prompt(&self, current: &Act, prompt: String) -> Act {
            match current {
                Act::Prompt { .. } => Act::Prompt { prompt },
                other => other.clone(),
            }
        }
        fn mode_word(&self, raw: &str) -> String {
            if raw.trim().eq_ignore_ascii_case("auto") { "auto".into() } else { "manual".into() }
        }
        fn mode_for(&self, requested: Option<&str>, conversation_mode: &str) -> String {
            match (self.mode_word(requested.unwrap_or("manual")).as_str(), conversation_mode) {
                ("auto", "auto") => "auto".into(),
                _ => "manual".into(),
            }
        }
        fn lowered_note(&self) -> &'static str {
            " Created read-only."
        }
    }

    fn store(tag: &str) -> (Store<Act>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("imber-schedule-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::new(dir.join("s.json")), dir)
    }

    fn ctx(mode: &str) -> ToolCtx<'_> {
        ToolCtx { root: "/r", conversation_id: "c-1", conversation_mode: mode, now_ms: 1_000 }
    }

    #[test]
    fn the_tools_are_named_and_keyed_consistently() {
        let all = specs(&Test);
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, SCHEDULE_TOOLS);
        for t in &all {
            for r in t.input_schema["required"].as_array().unwrap() {
                assert!(t.input_schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
            }
        }
        let create = &all[1];
        assert!(create.description.contains("THIS repository") && create.description.contains("Testbed must be open"));
        assert_eq!(create.input_schema["properties"]["action"]["enum"], json!(["prompt", "fetch"]));
    }

    #[test]
    fn only_listing_is_allowed_unattended() {
        assert!(unattended_may_call("schedule_list"));
        for t in ["schedule_create", "schedule_update", "schedule_delete"] {
            assert!(!unattended_may_call(t), "{t}");
        }
        assert!(is_schedule_tool("schedule_create") && !is_schedule_tool("git_status"));
    }

    #[test]
    fn a_job_is_no_freer_than_the_conversation_that_made_it() {
        let (s, dir) = store("mode");
        let ask = json!({"name": "Check", "action": "prompt", "prompt": "status?", "when": {"kind": "daily", "time": "09:00"}, "mode": "auto"});
        let in_auto = run_tool(&Test, &s, &ctx("auto"), "schedule_create", &ask).unwrap();
        assert!(in_auto.contains("\"mode\":\"auto\"") && !in_auto.contains("Created read-only"));
        let in_manual = run_tool(&Test, &s, &ctx("manual"), "schedule_create", &ask).unwrap();
        assert!(in_manual.contains("\"mode\":\"manual\"") && in_manual.ends_with("Created read-only."));
        let unasked = json!({"name": "Plain", "action": "fetch", "when": {"kind": "daily", "time": "09:00"}});
        assert!(run_tool(&Test, &s, &ctx("auto"), "schedule_create", &unasked).unwrap().contains("\"mode\":\"manual\""), "read-only unless asked");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn create_update_list_and_delete_stay_inside_their_project() {
        let (s, dir) = store("crud");
        let made = run_tool(&Test, &s, &ctx("manual"), "schedule_create", &json!({"name": "Check", "action": "prompt", "prompt": "status?", "when": {"kind": "every", "minutes": 30}})).unwrap();
        let id = serde_json::from_str::<Value>(&made).unwrap()["id"].as_str().unwrap().to_string();
        assert_eq!(s.get(&id).unwrap().recurrence, Recurrence::Interval { secs: 1800, anchor_ms: 1_000 });

        let updated = run_tool(&Test, &s, &ctx("manual"), "schedule_update", &json!({"id": id, "prompt": "new?", "enabled": false})).unwrap();
        assert!(updated.contains("\"enabled\":false"));
        assert_eq!(s.get(&id).unwrap().action, Act::Prompt { prompt: "new?".into() });

        let elsewhere = ToolCtx { root: "/other", ..ctx("manual") };
        assert_eq!(run_tool(&Test, &s, &elsewhere, "schedule_list", &json!({})).unwrap(), "[]");
        assert!(run_tool(&Test, &s, &elsewhere, "schedule_update", &json!({"id": id})).unwrap_err().contains("another project"));
        assert!(run_tool(&Test, &s, &elsewhere, "schedule_delete", &json!({"id": id})).unwrap_err().contains("another project"));

        assert!(run_tool(&Test, &s, &ctx("manual"), "schedule_delete", &json!({"id": id})).unwrap().contains("Deleted"));
        assert!(run_tool(&Test, &s, &ctx("manual"), "nope", &json!({})).unwrap_err().contains("not a schedule tool"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_create_without_what_it_needs_says_so() {
        let (s, dir) = store("missing");
        let err = |v: Value| run_tool(&Test, &s, &ctx("manual"), "schedule_create", &v).unwrap_err();
        assert!(err(json!({"name": "x", "action": "prompt", "when": {"kind": "daily", "time": "09:00"}})).contains("prompt"));
        assert!(err(json!({"name": "x", "action": "fetch"})).contains("when"));
        assert!(err(json!({"action": "fetch", "when": {"kind": "daily", "time": "09:00"}})).contains("name"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
