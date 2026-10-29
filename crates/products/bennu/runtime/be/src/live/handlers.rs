//! `live` domain — what the Live panel calls. The assistant's tools are in `assistant/tools/live.rs`;
//! both drive the same [`Live`] of the project.
//!
//! Anything that can take longer than a moment (starting a stack, running a scenario, stopping) is
//! done on a thread of its own and reported through the panel's refresh event and the timeline, so a
//! request worker is never held.

use std::sync::Arc;

use bennu_core::prelude::BennuState;
use bennu_live::prelude::{run_scenario, trace, EventKind, Filter, LogQuery, Scenario, StepKind};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

use super::files::{read_recipes, read_scenario, read_scenarios, recipes_path, scenario_path, scenarios_dir, Recipes};
use super::world::{run_recipe, stop_recipe, LiveWorld};
use super::{approvals, key_of, live_for, Live};

fn live(ctx: &BennuState, root: &str) -> Arc<Live> {
    live_for(root, &ctx.event_sink())
}

#[derive(Deserialize)]
pub struct RootArgs {
    pub root: String,
}

/// Everything the panel shows, in one call: processes, recipes, scenarios with their last result.
#[arbor_rpc::handler]
fn bennu_live_state(ctx: &BennuState, args: RootArgs) -> Result<Value, String> {
    let live = live(ctx, &args.root);
    let key = key_of(&args.root);
    let recipes = match read_recipes(&args.root) {
        Recipes::None => json!({ "present": false, "path": recipes_path(&args.root), "recipes": [], "errors": [], "approved": false }),
        Recipes::Invalid { errors, hash } => json!({ "present": true, "path": recipes_path(&args.root), "recipes": [], "errors": errors, "approved": false, "hash": hash }),
        Recipes::Valid(file) => json!({
            "present": true, "path": recipes_path(&args.root), "recipes": file.recipes, "errors": [],
            "approved": approvals().is_recipe_approved(&key, &file.hash), "hash": file.hash,
        }),
    };
    let results = live.results.lock().unwrap_or_else(|p| p.into_inner());
    let scenarios: Vec<Value> = read_scenarios(&args.root)
        .into_iter()
        .map(|e| {
            let last = results.get(e.scenario.as_ref().map(|s| s.name.as_str()).unwrap_or(&e.file)).map(|r| {
                json!({
                    "passed": r.passed, "started_ms": r.started_ms, "duration_ms": r.duration_ms,
                    "failed": r.steps.iter().filter(|s| s.outcome == bennu_live::prelude::Outcome::Failed).count(),
                })
            });
            match &e.scenario {
                Ok(s) => json!({
                    "file": e.file, "name": s.name, "description": s.description, "steps": s.steps.len(),
                    "recipes_needed": recipes_of(s), "errors": [], "last": last,
                }),
                Err(errors) => json!({ "file": e.file, "name": e.file, "description": "", "steps": 0, "recipes_needed": [], "errors": errors, "last": null }),
            }
        })
        .collect();
    drop(results);
    Ok(json!({
        "processes": live.manager.list(),
        "recipes": recipes,
        "scenarios": scenarios,
        "scenarios_dir": scenarios_dir(&args.root),
        "last_seq": live.timeline.last_seq(),
        "notice": live.notice.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        "scenario_running": live.scenario_running.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        "limits": { "max_running": live.manager.limits().max_running },
    }))
}

/// The recipes a scenario starts, so the panel knows whether running it needs an approval first.
fn recipes_of(s: &Scenario) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for step in &s.steps {
        if let StepKind::StartRecipe { recipe } = &step.kind {
            if !out.contains(recipe) {
                out.push(recipe.clone());
            }
        }
    }
    out
}

#[derive(Deserialize)]
pub struct LogsArgs {
    pub root: String,
    /// A process id or name.
    pub id: String,
    pub since: Option<u64>,
    pub tail: Option<usize>,
}

#[arbor_rpc::handler]
fn bennu_live_logs(ctx: &BennuState, args: LogsArgs) -> Result<Value, String> {
    let live = live(ctx, &args.root);
    let (info, lines) = live.manager.logs(&args.id, &LogQuery { since: args.since, tail: Some(args.tail.unwrap_or(500).min(2000)), ..Default::default() })?;
    Ok(json!({ "process": info, "lines": lines }))
}

#[derive(Deserialize)]
pub struct TimelineArgs {
    pub root: String,
    pub since_seq: Option<u64>,
    pub corr: Option<String>,
    pub text: Option<String>,
    pub source: Option<String>,
    /// `log`, `http` (requests and responses), `marker` (process starts/exits) — empty for all.
    #[serde(default)]
    pub kinds: Vec<String>,
    pub limit: Option<usize>,
}

fn kinds_of(names: &[String]) -> Vec<EventKind> {
    names
        .iter()
        .flat_map(|n| match n.as_str() {
            "log" => vec![EventKind::Log],
            "http" => vec![EventKind::HttpRequest, EventKind::HttpResponse],
            "marker" => vec![EventKind::ProcessStarted, EventKind::ProcessReady, EventKind::ProcessExited, EventKind::Marker],
            _ => vec![],
        })
        .collect()
}

#[arbor_rpc::handler]
fn bennu_live_timeline(ctx: &BennuState, args: TimelineArgs) -> Result<Value, String> {
    let live = live(ctx, &args.root);
    let text = match args.text.as_deref().filter(|t| !t.is_empty()) {
        Some(t) => Some(Regex::new(&format!("(?i){}", regex::escape(t))).map_err(|e| e.to_string())?),
        None => None,
    };
    let events = live.timeline.query(&Filter {
        since_seq: args.since_seq,
        corr: args.corr.filter(|c| !c.is_empty()),
        text,
        source: args.source.filter(|s| !s.is_empty()),
        kinds: kinds_of(&args.kinds),
        limit: Some(args.limit.unwrap_or(1000).min(5000)),
        ..Default::default()
    });
    Ok(json!({ "events": events, "last_seq": live.timeline.last_seq() }))
}

#[derive(Deserialize)]
pub struct TraceArgs {
    pub root: String,
    pub corr: String,
    pub context: Option<usize>,
}

#[arbor_rpc::handler]
fn bennu_live_trace(ctx: &BennuState, args: TraceArgs) -> Result<Value, String> {
    let live = live(ctx, &args.root);
    let events = live.timeline.query(&Filter::default());
    Ok(json!(trace(&events, &args.corr, args.context.unwrap_or(8))))
}

#[arbor_rpc::handler]
fn bennu_live_clear_timeline(ctx: &BennuState, args: RootArgs) -> Result<(), String> {
    live(ctx, &args.root).timeline.clear();
    Ok(())
}

#[derive(Deserialize)]
pub struct NameArgs {
    pub root: String,
    pub name: String,
    /// The user confirmed the commands of the recipes file in the panel.
    #[serde(default)]
    pub approve: bool,
}

/// Approve the recipes file as it is now: what the user said yes to, bound to its content.
fn approve_current_recipes(root: &str) {
    if let Recipes::Valid(file) = read_recipes(root) {
        approvals().approve_recipes(&key_of(root), &file.hash);
    }
}

#[arbor_rpc::handler]
fn bennu_live_run_recipe(ctx: &BennuState, args: NameArgs) -> Result<(), String> {
    let live = live(ctx, &args.root);
    if args.approve {
        approve_current_recipes(&args.root);
    }
    live.set_notice(None);
    std::thread::spawn(move || {
        if let Err(e) = run_recipe(&live, &args.name, None, &|| false) {
            live.set_notice(Some(format!("Recipe `{}` did not come up: {e}", args.name)));
        } else {
            live.touch();
        }
    });
    Ok(())
}

#[arbor_rpc::handler]
fn bennu_live_stop_recipe(ctx: &BennuState, args: NameArgs) -> Result<(), String> {
    let live = live(ctx, &args.root);
    std::thread::spawn(move || {
        // The user's panel may stop a recipe whoever started it.
        if let Err(e) = stop_recipe(&live, &args.name, None) {
            live.set_notice(Some(e));
        }
        live.touch();
    });
    Ok(())
}

#[derive(Deserialize)]
pub struct ProcArgs {
    pub root: String,
    pub id: String,
}

#[arbor_rpc::handler]
fn bennu_live_stop(ctx: &BennuState, args: ProcArgs) -> Result<(), String> {
    let live = live(ctx, &args.root);
    std::thread::spawn(move || {
        let _ = live.manager.stop(&args.id);
        live.touch();
    });
    Ok(())
}

#[arbor_rpc::handler]
fn bennu_live_stop_all(ctx: &BennuState, args: RootArgs) -> Result<(), String> {
    let live = live(ctx, &args.root);
    std::thread::spawn(move || {
        live.manager.stop_all();
        live.touch();
    });
    Ok(())
}

#[arbor_rpc::handler]
fn bennu_live_run_scenario(ctx: &BennuState, args: NameArgs) -> Result<(), String> {
    let live = live(ctx, &args.root);
    let entry = read_scenario(&args.root, &args.name)?;
    let scenario = entry.scenario.map_err(|e| format!("the scenario is invalid:\n- {}", e.join("\n- ")))?;
    if args.approve {
        approve_current_recipes(&args.root);
    }
    {
        let mut running = live.scenario_running.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(other) = running.as_ref() {
            return Err(format!("scenario `{other}` is still running"));
        }
        *running = Some(scenario.name.clone());
    }
    live.set_notice(None);
    std::thread::spawn(move || {
        let result = {
            let never = || false;
            let mut world = LiveWorld { live: Arc::clone(&live), cancelled: &never, owner: None };
            live.timeline.push(EventKind::Marker, "scenario", None, None, &format!("scenario `{}` started", scenario.name), None);
            run_scenario(&mut world, &scenario)
        };
        live.timeline.push(EventKind::Marker, "scenario", None, None, &format!("scenario `{}` {}", result.name, if result.passed { "passed" } else { "FAILED" }), None);
        *live.scenario_running.lock().unwrap_or_else(|p| p.into_inner()) = None;
        live.store_result(result);
    });
    Ok(())
}

#[derive(Deserialize)]
pub struct ScenarioArgs {
    pub root: String,
    pub name: String,
}

/// The full last result of a scenario, with the timeline slice of each step.
#[arbor_rpc::handler]
fn bennu_live_scenario_result(ctx: &BennuState, args: ScenarioArgs) -> Result<Value, String> {
    let live = live(ctx, &args.root);
    let results = live.results.lock().unwrap_or_else(|p| p.into_inner());
    // Keyed by the scenario's own name; the panel passes whichever it has (name or file stem).
    Ok(results.get(&args.name).map(|r| json!(r)).unwrap_or(Value::Null))
}

#[derive(Deserialize)]
pub struct TemplateArgs {
    pub root: String,
    /// `recipes` or `scenario`.
    pub kind: String,
}

const RECIPES_TEMPLATE: &str = "# Named stacks of services Bennu can start, watch and stop (Live panel, assistant).\n# Starting a recipe asks for approval; editing this file asks again.\n\n[[recipe]]\nname = \"app\"\ndescription = \"The service under development\"\n\n  [[recipe.service]]\n  name = \"api\"\n  command = \"mvn -q spring-boot:run\"\n  # cwd = \"module\"                  # relative to the project\n  # env = { SPRING_PROFILES_ACTIVE = \"dev\" }\n  ports = [8080]\n  ready = { port = 8080, timeout_s = 120 }   # also: log = \"Started .*\", http = \"http://localhost:8080/health\"\n";

const SCENARIO_TEMPLATE: &str = "name = \"smoke\"\ndescription = \"Start the app and check it answers\"\n\n[[step]]\ntype = \"start_recipe\"\nrecipe = \"app\"\n\n[[step]]\ntype = \"http\"\nname = \"health\"\nurl = \"http://localhost:8080/actuator/health\"\nexpect_status = 200\nmax_ms = 2000\n\n[[step]]\ntype = \"assert_log\"\npattern = \"ERROR\"\npresent = false\ntimeout_ms = 500\n\n[[step]]\ntype = \"stop_all\"\n";

/// Create a starter recipes file or scenario (never overwriting one). Returns the path to open.
#[arbor_rpc::handler]
fn bennu_live_create_template(_ctx: &BennuState, args: TemplateArgs) -> Result<String, String> {
    let (path, text) = match args.kind.as_str() {
        "recipes" => (recipes_path(&args.root), RECIPES_TEMPLATE),
        "scenario" => {
            let mut n = 1;
            let path = loop {
                let name = if n == 1 { "smoke".to_string() } else { format!("smoke-{n}") };
                let p = scenario_path(&args.root, &name)?;
                if !p.exists() {
                    break p;
                }
                n += 1;
            };
            (path, SCENARIO_TEMPLATE)
        }
        other => return Err(format!("`{other}` is not a template (recipes, scenario)")),
    };
    if path.exists() {
        return Ok(path.display().to_string());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_live::prelude::{parse_recipes, parse_scenario};

    #[test]
    fn the_starter_files_are_valid() {
        assert!(parse_recipes(RECIPES_TEMPLATE).is_ok());
        let s = parse_scenario(SCENARIO_TEMPLATE).unwrap();
        assert_eq!(recipes_of(&s), vec!["app".to_string()]);
    }

    #[test]
    fn kinds_map_to_events() {
        assert_eq!(kinds_of(&["http".into()]).len(), 2);
        assert!(kinds_of(&["nonsense".into()]).is_empty());
    }
}
