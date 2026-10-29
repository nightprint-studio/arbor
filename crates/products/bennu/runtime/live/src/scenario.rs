//! Scenarios: an ordered list of steps with assertions, stored as a TOML file in the project and run
//! without the AI — from the Live panel, or by the assistant.
//!
//! ```toml
//! name = "order-flow"
//! description = "Create an order and see it in the orders service"
//!
//! [[step]]
//! type = "start_recipe"
//! recipe = "shop"
//!
//! [[step]]
//! type = "http"
//! name = "login"
//! method = "POST"
//! url = "http://localhost:8080/api/login"
//! headers = { "Content-Type" = "application/json" }
//! body = '{"user":"demo"}'
//! expect_status = 200
//! max_ms = 800
//!   [[step.expect_json]]
//!   path = "$.token"
//!   exists = true
//!
//! [[step]]
//! type = "http"
//! url = "http://localhost:8080/api/orders"
//! headers = { Authorization = "Bearer ${login:$.token}" }    # a value saved from an earlier response
//! expect_status = 200
//!
//! [[step]]
//! type = "assert_log"
//! process = "orders"
//! pattern = "Order \\d+ created"
//! present = true
//! timeout_ms = 3000
//!
//! [[step]]
//! type = "assert_running"
//! process = "orders"
//! ```
//!
//! ## Shape, for a later export
//!
//! A scenario is a *test suite*, a step a *test case*, and each assertion a check inside it with its
//! own pass/fail and message — the shape JUnit XML and Playwright specs share. [`StepResult`] carries
//! that flat list (`assertions`) so an exporter needs no knowledge of how a step ran.
//!
//! The runner is generic over [`World`]: the real one starts processes and sends requests, a test's
//! one scripts them, and the logic of "what counts as a pass" is the same code either way.

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::{parse_local_url, HttpRequest, HttpResponse};
use crate::jsonpath;
use crate::timeline::{Event, EventKind, Filter};

/// Most steps in a scenario.
pub const MAX_STEPS: usize = 100;
/// The slice of timeline kept with a step.
const SLICE_EVENTS: usize = 60;

// ── File shape ───────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    step: Vec<RawStep>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    continue_on_fail: bool,
    // start_recipe / stop_recipe
    #[serde(default)]
    recipe: Option<String>,
    // http
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    save_as: Option<String>,
    #[serde(default)]
    expect_status: Option<u16>,
    #[serde(default)]
    expect_body: Option<String>,
    #[serde(default)]
    expect_json: Vec<RawJsonExpect>,
    #[serde(default)]
    max_ms: Option<u64>,
    // wait
    #[serde(default)]
    ms: Option<u64>,
    // assert_log / assert_running
    #[serde(default)]
    process: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    present: Option<bool>,
    #[serde(default)]
    window_ms: Option<u64>,
    #[serde(default)]
    running: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
struct RawJsonExpect {
    path: String,
    #[serde(default)]
    exists: Option<bool>,
    #[serde(default)]
    equals: Option<Value>,
    #[serde(default)]
    matches: Option<String>,
    #[serde(default)]
    gt: Option<f64>,
    #[serde(default)]
    lt: Option<f64>,
}

// ── Validated shape ──────────────────────────────────────────────────────────────────────────────

/// One assertion on a JSON value of the response body.
#[derive(Debug, Clone, Serialize)]
pub struct JsonExpect {
    pub path: String,
    pub exists: Option<bool>,
    pub equals: Option<Value>,
    pub matches: Option<String>,
    pub gt: Option<f64>,
    pub lt: Option<f64>,
}

/// What a response must satisfy.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Expect {
    pub status: Option<u16>,
    /// A regex the body must match.
    pub body_matches: Option<String>,
    pub json: Vec<JsonExpect>,
    /// The response must arrive within this many milliseconds.
    pub max_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    StartRecipe { recipe: String },
    StopRecipe { recipe: String },
    StopAll,
    Http { method: String, url: String, headers: Vec<(String, String)>, body: Option<String>, timeout_ms: u64, save_as: Option<String>, expect: Expect },
    Wait { ms: u64 },
    /// A log pattern is (or is not) present in a window of the timeline.
    AssertLog { process: Option<String>, pattern: String, present: bool, window_ms: Option<u64>, timeout_ms: u64 },
    AssertRunning { process: String, running: bool },
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub name: Option<String>,
    pub continue_on_fail: bool,
    #[serde(flatten)]
    pub kind: StepKind,
}

impl Step {
    /// What the step is called in a result: its name, or a description of what it does.
    pub fn label(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        match &self.kind {
            StepKind::StartRecipe { recipe } => format!("start {recipe}"),
            StepKind::StopRecipe { recipe } => format!("stop {recipe}"),
            StepKind::StopAll => "stop all".to_string(),
            StepKind::Http { method, url, .. } => format!("{method} {url}"),
            StepKind::Wait { ms } => format!("wait {ms} ms"),
            StepKind::AssertLog { pattern, present, .. } => format!("log {} /{pattern}/", if *present { "has" } else { "lacks" }),
            StepKind::AssertRunning { process, running } => format!("{process} {}", if *running { "running" } else { "not running" }),
        }
        .chars()
        .take(120)
        .collect::<String>()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub steps: Vec<Step>,
}

/// Parse and validate. Every problem is reported at once.
pub fn parse_scenario(text: &str) -> Result<Scenario, Vec<String>> {
    let raw: RawScenario = toml::from_str(text).map_err(|e| vec![format!("scenario file: {}", e.message())])?;
    let mut errors = Vec::new();
    if raw.name.trim().is_empty() {
        errors.push("the scenario needs a name".to_string());
    }
    if raw.step.is_empty() {
        errors.push("the scenario has no [[step]]".to_string());
    }
    if raw.step.len() > MAX_STEPS {
        errors.push(format!("a scenario has at most {MAX_STEPS} steps"));
    }
    let mut steps = Vec::new();
    for (i, s) in raw.step.iter().enumerate() {
        let ctx = format!("step {} ({})", i + 1, s.name.clone().unwrap_or_else(|| s.kind.clone()));
        match convert_step(s, &ctx, &mut errors) {
            Some(kind) => steps.push(Step { name: s.name.clone(), continue_on_fail: s.continue_on_fail, kind }),
            None => continue,
        }
    }
    if errors.is_empty() {
        Ok(Scenario { name: raw.name, description: raw.description, steps })
    } else {
        Err(errors)
    }
}

fn convert_step(s: &RawStep, ctx: &str, errors: &mut Vec<String>) -> Option<StepKind> {
    let need = |v: &Option<String>, field: &str, errors: &mut Vec<String>| -> Option<String> {
        match v.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) => Some(t.to_string()),
            None => {
                errors.push(format!("{ctx}: `{field}` is required"));
                None
            }
        }
    };
    let regex_ok = |p: &str, field: &str, errors: &mut Vec<String>| {
        if let Err(e) = Regex::new(p) {
            errors.push(format!("{ctx}: `{field}` is not a valid regex: {e}"));
        }
    };
    match s.kind.as_str() {
        "start_recipe" => need(&s.recipe, "recipe", errors).map(|recipe| StepKind::StartRecipe { recipe }),
        "stop_recipe" => need(&s.recipe, "recipe", errors).map(|recipe| StepKind::StopRecipe { recipe }),
        "stop_all" => Some(StepKind::StopAll),
        "wait" => {
            let ms = s.ms.unwrap_or(0);
            if ms == 0 || ms > 60_000 {
                errors.push(format!("{ctx}: `ms` must be between 1 and 60000"));
                return None;
            }
            Some(StepKind::Wait { ms })
        }
        "http" => {
            let url = need(&s.url, "url", errors)?;
            // A `${...}` placeholder is resolved at run time; validate the URL's shape only without one.
            if !url.contains("${") {
                if let Err(e) = parse_local_url(&url) {
                    errors.push(format!("{ctx}: url: {e}"));
                }
            }
            if let Some(re) = &s.expect_body {
                regex_ok(re, "expect_body", errors);
            }
            for j in &s.expect_json {
                if !jsonpath::is_valid(&j.path) {
                    errors.push(format!("{ctx}: expect_json path `{}` is not a valid JSON path", j.path));
                }
                if let Some(re) = &j.matches {
                    regex_ok(re, "expect_json.matches", errors);
                }
                if j.exists.is_none() && j.equals.is_none() && j.matches.is_none() && j.gt.is_none() && j.lt.is_none() {
                    errors.push(format!("{ctx}: expect_json `{}` checks nothing (give exists, equals, matches, gt or lt)", j.path));
                }
            }
            let timeout_ms = s.timeout_ms.unwrap_or(10_000);
            if timeout_ms == 0 || timeout_ms > 120_000 {
                errors.push(format!("{ctx}: `timeout_ms` must be between 1 and 120000"));
            }
            Some(StepKind::Http {
                method: s.method.clone().unwrap_or_else(|| "GET".into()).to_ascii_uppercase(),
                url,
                headers: s.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                body: s.body.clone(),
                timeout_ms,
                save_as: s.save_as.clone().or_else(|| s.name.clone()),
                expect: Expect {
                    status: s.expect_status,
                    body_matches: s.expect_body.clone(),
                    json: s
                        .expect_json
                        .iter()
                        .map(|j| JsonExpect { path: j.path.clone(), exists: j.exists, equals: j.equals.clone(), matches: j.matches.clone(), gt: j.gt, lt: j.lt })
                        .collect(),
                    max_ms: s.max_ms,
                },
            })
        }
        "assert_log" => {
            let pattern = need(&s.pattern, "pattern", errors)?;
            regex_ok(&pattern, "pattern", errors);
            let timeout_ms = s.timeout_ms.unwrap_or(0);
            if timeout_ms > 120_000 {
                errors.push(format!("{ctx}: `timeout_ms` is at most 120000"));
            }
            Some(StepKind::AssertLog { process: s.process.clone(), pattern, present: s.present.unwrap_or(true), window_ms: s.window_ms, timeout_ms })
        }
        "assert_running" => need(&s.process, "process", errors).map(|process| StepKind::AssertRunning { process, running: s.running.unwrap_or(true) }),
        other => {
            errors.push(format!("{ctx}: `{other}` is not a step type (start_recipe, stop_recipe, stop_all, http, wait, assert_log, assert_running)"));
            None
        }
    }
}

// ── Running ──────────────────────────────────────────────────────────────────────────────────────

/// Everything a scenario does to the outside world.
pub trait World {
    fn start_recipe(&mut self, name: &str) -> Result<String, String>;
    fn stop_recipe(&mut self, name: &str) -> Result<String, String>;
    fn stop_all(&mut self) -> String;
    /// Send the request (which already carries a correlation id) and return the response.
    fn http(&mut self, req: &HttpRequest) -> Result<HttpResponse, String>;
    fn sleep(&mut self, ms: u64);
    /// `None` when no process of that name was ever started.
    fn process_running(&self, name: &str) -> Option<bool>;
    fn events(&self, filter: &Filter) -> Vec<Event>;
    fn now_ms(&self) -> u64;
    fn cancelled(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Passed,
    Failed,
    /// Not run because an earlier step failed.
    Skipped,
}

/// One check inside a step.
#[derive(Debug, Clone, Serialize)]
pub struct AssertionResult {
    pub description: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub index: usize,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub outcome: Outcome,
    pub message: String,
    pub duration_ms: u64,
    pub assertions: Vec<AssertionResult>,
    /// The correlation id of an HTTP step: `trace_request` takes it.
    pub corr: Option<String>,
    /// The timeline while the step ran.
    pub slice: Vec<Event>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub passed: bool,
    pub started_ms: u64,
    pub duration_ms: u64,
    pub steps: Vec<StepResult>,
}

impl ScenarioResult {
    /// A compact text the model (or a log) can read.
    pub fn summary(&self) -> String {
        let mut out = format!("scenario `{}`: {} in {} ms\n", self.name, if self.passed { "PASSED" } else { "FAILED" }, self.duration_ms);
        for s in &self.steps {
            let mark = match s.outcome {
                Outcome::Passed => "pass",
                Outcome::Failed => "FAIL",
                Outcome::Skipped => "skip",
            };
            out.push_str(&format!("  {}. [{mark}] {} ({} ms){}\n", s.index + 1, s.name, s.duration_ms, if s.message.is_empty() { String::new() } else { format!(" — {}", s.message) }));
            for a in s.assertions.iter().filter(|a| !a.passed) {
                out.push_str(&format!("       x {}: {}\n", a.description, a.detail));
            }
            if let Some(c) = &s.corr {
                if s.outcome == Outcome::Failed {
                    out.push_str(&format!("       correlation id {c} (trace_request shows every log line for it)\n"));
                }
            }
        }
        out
    }
}

/// Run `scenario` against `world`. Stops at the first failing step unless it says `continue_on_fail`;
/// the steps after it are reported as skipped.
pub fn run_scenario(world: &mut dyn World, scenario: &Scenario) -> ScenarioResult {
    let started = world.now_ms();
    let mut saved: HashMap<String, Saved> = HashMap::new();
    let mut results = Vec::new();
    let mut aborted = false;
    for (index, step) in scenario.steps.iter().enumerate() {
        let name = step.label();
        let kind = kind_name(&step.kind).to_string();
        if aborted || world.cancelled() {
            let message = if aborted { "not run: an earlier step failed" } else { "not run: cancelled" };
            results.push(StepResult { index, name, kind, outcome: Outcome::Skipped, message: message.into(), duration_ms: 0, assertions: Vec::new(), corr: None, slice: Vec::new() });
            continue;
        }
        let t0 = world.now_ms();
        let mut r = run_step(world, step, started, &mut saved);
        r.index = index;
        r.name = name;
        r.kind = kind;
        r.duration_ms = world.now_ms().saturating_sub(t0);
        r.slice = world.events(&Filter { after_ms: Some(t0), limit: Some(SLICE_EVENTS), ..Default::default() });
        if r.outcome == Outcome::Failed && !step.continue_on_fail {
            aborted = true;
        }
        results.push(r);
    }
    let passed = results.iter().all(|r| r.outcome != Outcome::Failed) && !results.iter().any(|r| r.outcome == Outcome::Skipped);
    ScenarioResult { name: scenario.name.clone(), passed, started_ms: started, duration_ms: world.now_ms().saturating_sub(started), steps: results }
}

fn kind_name(k: &StepKind) -> &'static str {
    match k {
        StepKind::StartRecipe { .. } => "start_recipe",
        StepKind::StopRecipe { .. } => "stop_recipe",
        StepKind::StopAll => "stop_all",
        StepKind::Http { .. } => "http",
        StepKind::Wait { .. } => "wait",
        StepKind::AssertLog { .. } => "assert_log",
        StepKind::AssertRunning { .. } => "assert_running",
    }
}

/// A response kept for `${name:...}` in later steps.
struct Saved {
    status: u16,
    json: Option<Value>,
    body: String,
}

fn blank(outcome: Outcome, message: String) -> StepResult {
    StepResult { index: 0, name: String::new(), kind: String::new(), outcome, message, duration_ms: 0, assertions: Vec::new(), corr: None, slice: Vec::new() }
}

fn run_step(world: &mut dyn World, step: &Step, scenario_start: u64, saved: &mut HashMap<String, Saved>) -> StepResult {
    match &step.kind {
        StepKind::StartRecipe { recipe } => action(world.start_recipe(recipe)),
        StepKind::StopRecipe { recipe } => action(world.stop_recipe(recipe)),
        StepKind::StopAll => action(Ok(world.stop_all())),
        StepKind::Wait { ms } => {
            world.sleep(*ms);
            blank(Outcome::Passed, String::new())
        }
        StepKind::Http { method, url, headers, body, timeout_ms, save_as, expect } => run_http(world, saved, method, url, headers, body.as_deref(), *timeout_ms, save_as.as_deref(), expect),
        StepKind::AssertLog { process, pattern, present, window_ms, timeout_ms } => run_assert_log(world, scenario_start, process.as_deref(), pattern, *present, *window_ms, *timeout_ms),
        StepKind::AssertRunning { process, running } => {
            let desc = format!("{process} is {}", if *running { "running" } else { "not running" });
            let actual = world.process_running(process);
            let (passed, detail) = match (actual, running) {
                (None, true) => (false, format!("no process named `{process}` was started")),
                (None, false) => (true, "never started".to_string()),
                (Some(a), want) => (a == *want, format!("it is {}", if a { "running" } else { "not running" })),
            };
            let mut r = blank(if passed { Outcome::Passed } else { Outcome::Failed }, if passed { String::new() } else { detail.clone() });
            r.assertions.push(AssertionResult { description: desc, passed, detail });
            r
        }
    }
}

fn action(result: Result<String, String>) -> StepResult {
    match result {
        Ok(m) => blank(Outcome::Passed, m),
        Err(e) => blank(Outcome::Failed, e),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_http(
    world: &mut dyn World,
    saved: &mut HashMap<String, Saved>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_ms: u64,
    save_as: Option<&str>,
    expect: &Expect,
) -> StepResult {
    let sub = |text: &str| substitute(text, saved);
    let mut req = match (sub(url), headers.iter().map(|(k, v)| sub(v).map(|v| (k.clone(), v))).collect::<Result<Vec<_>, _>>(), body.map(sub).transpose()) {
        (Ok(url), Ok(headers), Ok(body)) => HttpRequest { method: method.to_string(), url, headers, body, timeout_ms },
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return blank(Outcome::Failed, e),
    };
    let corr = req.ensure_correlation();
    let mut r = match world.http(&req) {
        Ok(resp) => {
            let mut r = check_response(&resp, expect);
            if let Some(name) = save_as {
                saved.insert(name.to_string(), Saved { status: resp.status, json: serde_json::from_str(&resp.body).ok(), body: resp.body.clone() });
            }
            r.message = format!("{} {} in {} ms", resp.status, resp.reason, resp.elapsed_ms).trim().to_string()
                + &r.assertions.iter().filter(|a| !a.passed).map(|a| format!("; {}", a.description)).collect::<String>();
            r
        }
        Err(e) => blank(Outcome::Failed, e),
    };
    r.corr = Some(corr);
    r
}

fn check_response(resp: &HttpResponse, expect: &Expect) -> StepResult {
    let mut checks: Vec<AssertionResult> = Vec::new();
    let mut add = |description: String, passed: bool, detail: String| checks.push(AssertionResult { description, passed, detail });
    if let Some(want) = expect.status {
        add(format!("status is {want}"), resp.status == want, format!("got {}", resp.status));
    }
    if let Some(max) = expect.max_ms {
        add(format!("answers within {max} ms"), resp.elapsed_ms <= max, format!("took {} ms", resp.elapsed_ms));
    }
    if let Some(re) = &expect.body_matches {
        let ok = Regex::new(re).map(|r| r.is_match(&resp.body)).unwrap_or(false);
        add(format!("body matches /{re}/"), ok, format!("body starts: {}", preview(&resp.body)));
    }
    if !expect.json.is_empty() {
        let parsed: Option<Value> = serde_json::from_str(&resp.body).ok();
        for j in &expect.json {
            let (passed, detail) = match &parsed {
                None => (false, format!("the body is not JSON: {}", preview(&resp.body))),
                Some(v) => check_json(v, j),
            };
            add(describe_json(j), passed, detail);
        }
    }
    let passed = checks.iter().all(|c| c.passed);
    let mut r = blank(if passed { Outcome::Passed } else { Outcome::Failed }, String::new());
    r.assertions = checks;
    r
}

fn describe_json(j: &JsonExpect) -> String {
    let mut parts = Vec::new();
    if let Some(e) = j.exists {
        parts.push(if e { "exists".to_string() } else { "is absent".to_string() });
    }
    if let Some(v) = &j.equals {
        parts.push(format!("equals {v}"));
    }
    if let Some(m) = &j.matches {
        parts.push(format!("matches /{m}/"));
    }
    if let Some(g) = j.gt {
        parts.push(format!("> {g}"));
    }
    if let Some(l) = j.lt {
        parts.push(format!("< {l}"));
    }
    format!("{} {}", j.path, parts.join(" and "))
}

fn check_json(root: &Value, j: &JsonExpect) -> (bool, String) {
    let found = jsonpath::select(root, &j.path);
    let shown = found.map(|v| v.to_string()).unwrap_or_else(|| "(missing)".to_string());
    let mut ok = true;
    if let Some(want) = j.exists {
        ok &= found.is_some() == want;
    }
    if let Some(eq) = &j.equals {
        ok &= found.is_some_and(|v| json_equal(v, eq));
    }
    if let Some(re) = &j.matches {
        let text = found.map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        ok &= match (text, Regex::new(re)) {
            (Some(t), Ok(r)) => r.is_match(&t),
            _ => false,
        };
    }
    if let Some(g) = j.gt {
        ok &= found.and_then(Value::as_f64).is_some_and(|n| n > g);
    }
    if let Some(l) = j.lt {
        ok &= found.and_then(Value::as_f64).is_some_and(|n| n < l);
    }
    (ok, format!("found {shown}"))
}

/// Equality that does not care whether TOML wrote an integer and the server a float.
fn json_equal(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn preview(text: &str) -> String {
    let t: String = text.chars().take(120).collect();
    t.replace(['\r', '\n'], " ")
}

fn run_assert_log(world: &mut dyn World, scenario_start: u64, process: Option<&str>, pattern: &str, present: bool, window_ms: Option<u64>, timeout_ms: u64) -> StepResult {
    let Ok(re) = Regex::new(pattern) else { return blank(Outcome::Failed, format!("`{pattern}` is not a valid regex")) };
    let desc = format!("log {}{} /{pattern}/", if present { "has" } else { "lacks" }, process.map(|p| format!(" ({p})")).unwrap_or_default());
    let waited_from = world.now_ms();
    let find = |world: &dyn World| -> Option<Event> {
        // A window reaches back from now; without one the whole scenario so far counts.
        let after = window_ms.map(|w| world.now_ms().saturating_sub(w)).unwrap_or(scenario_start);
        world
            .events(&Filter { kinds: vec![EventKind::Log], source: process.map(str::to_string), text: Some(re.clone()), after_ms: Some(after), ..Default::default() })
            .into_iter()
            .last()
    };
    let hit = if present {
        // Wait for it to show up, up to the timeout.
        loop {
            if let Some(e) = find(world) {
                break Some(e);
            }
            if world.now_ms().saturating_sub(waited_from) >= timeout_ms || world.cancelled() {
                break None;
            }
            world.sleep(100);
        }
    } else {
        // Absence is only meaningful after giving it time to appear.
        if timeout_ms > 0 {
            world.sleep(timeout_ms);
        }
        find(world)
    };
    let passed = hit.is_some() == present;
    let detail = match (&hit, present) {
        (Some(e), true) => format!("found: {}", preview(&e.text)),
        (None, true) => "no matching line appeared".to_string(),
        (Some(e), false) => format!("unexpected line from {}: {}", e.source, preview(&e.text)),
        (None, false) => "no matching line".to_string(),
    };
    let mut r = blank(if passed { Outcome::Passed } else { Outcome::Failed }, if passed { String::new() } else { detail.clone() });
    r.assertions.push(AssertionResult { description: desc, passed, detail });
    r
}

/// Replace `${name:$.path}` (a value from the JSON body saved by an earlier step), `${name:status}` and
/// `${name:body}` in `text`. An unknown name or a missing value is an error, never an empty string: a
/// request with a silently blank token would fail in a confusing way two steps later.
fn substitute(text: &str, saved: &HashMap<String, Saved>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}').ok_or_else(|| format!("unclosed `${{` in `{}`", preview(text)))?;
        let (name, what) = after[..end].split_once(':').ok_or_else(|| format!("`${{{}}}` needs the form ${{step:$.path}}", &after[..end]))?;
        let saved = saved.get(name.trim()).ok_or_else(|| format!("`${{{name}:…}}`: no earlier step saved a response as `{name}`"))?;
        out.push_str(&saved.lookup(what.trim()).ok_or_else(|| format!("`${{{name}:{what}}}`: that value is not in the saved response"))?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl Saved {
    fn lookup(&self, what: &str) -> Option<String> {
        match what {
            "status" => Some(self.status.to_string()),
            "body" => Some(self.body.clone()),
            path => {
                let v = jsonpath::select(self.json.as_ref()?, path)?;
                Some(v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Timeline;

    /// A world with a virtual clock and scripted answers.
    struct Fake {
        now: u64,
        timeline: Timeline,
        responses: Vec<Result<HttpResponse, String>>,
        sent: Vec<HttpRequest>,
        running: HashMap<String, bool>,
        log_on_sleep: Option<(u64, String, String)>,
    }

    impl Fake {
        fn new() -> Self {
            Fake { now: 1_000, timeline: Timeline::new(500), responses: Vec::new(), sent: Vec::new(), running: HashMap::new(), log_on_sleep: None }
        }
        fn reply(mut self, status: u16, body: &str, elapsed: u64) -> Self {
            self.responses.push(Ok(HttpResponse { status, reason: "OK".into(), headers: vec![], body: body.into(), truncated: false, elapsed_ms: elapsed }));
            self
        }
        fn log(&self, at: u64, source: &str, text: &str) {
            self.timeline.push_at(at, EventKind::Log, source, Some(source), None, text, None);
        }
    }

    impl World for Fake {
        fn start_recipe(&mut self, name: &str) -> Result<String, String> {
            if name == "missing" { Err("no recipe `missing`".into()) } else { Ok(format!("started {name}")) }
        }
        fn stop_recipe(&mut self, name: &str) -> Result<String, String> {
            Ok(format!("stopped {name}"))
        }
        fn stop_all(&mut self) -> String {
            "stopped all".into()
        }
        fn http(&mut self, req: &HttpRequest) -> Result<HttpResponse, String> {
            self.sent.push(req.clone());
            self.now += 5;
            self.responses.remove(0)
        }
        fn sleep(&mut self, ms: u64) {
            self.now += ms;
            if let Some((at, source, text)) = self.log_on_sleep.clone() {
                if self.now >= at {
                    self.log(self.now, &source, &text);
                    self.log_on_sleep = None;
                }
            }
        }
        fn process_running(&self, name: &str) -> Option<bool> {
            self.running.get(name).copied()
        }
        fn events(&self, filter: &Filter) -> Vec<Event> {
            self.timeline.query(filter)
        }
        fn now_ms(&self) -> u64 {
            self.now
        }
        fn cancelled(&self) -> bool {
            false
        }
    }

    const SCENARIO: &str = r#"
name = "flow"
description = "demo"

[[step]]
type = "start_recipe"
recipe = "shop"

[[step]]
type = "http"
name = "login"
method = "post"
url = "http://localhost:8080/login"
headers = { "Content-Type" = "application/json" }
body = '{"u":1}'
expect_status = 200
max_ms = 500
  [[step.expect_json]]
  path = "$.token"
  exists = true
  [[step.expect_json]]
  path = "$.user.id"
  equals = 7

[[step]]
type = "http"
url = "http://localhost:8080/orders"
headers = { Authorization = "Bearer ${login:$.token}" }
expect_status = 200
expect_body = "orders"

[[step]]
type = "assert_log"
process = "orders"
pattern = "created order \\d+"
timeout_ms = 1000

[[step]]
type = "assert_running"
process = "orders"
"#;

    #[test]
    fn a_valid_scenario_parses_with_defaults() {
        let s = parse_scenario(SCENARIO).unwrap();
        assert_eq!(s.steps.len(), 5);
        match &s.steps[1].kind {
            StepKind::Http { method, save_as, expect, .. } => {
                assert_eq!(method, "POST");
                assert_eq!(save_as.as_deref(), Some("login"), "a named step is saved under its name");
                assert_eq!(expect.json.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&s.steps[3].kind, StepKind::AssertLog { present: true, .. }));
    }

    #[test]
    fn mistakes_are_all_reported() {
        let bad = r#"
name = "x"
[[step]]
type = "http"
url = "http://example.com/"
expect_body = "("
  [[step.expect_json]]
  path = "nope"
  [[step.expect_json]]
  path = "$.a"
[[step]]
type = "wait"
ms = 0
[[step]]
type = "assert_log"
[[step]]
type = "dance"
"#;
        let e = parse_scenario(bad).unwrap_err().join(" | ");
        for needle in ["not loopback", "expect_body", "not a valid JSON path", "checks nothing", "between 1 and 60000", "`pattern` is required", "not a step type"] {
            assert!(e.contains(needle), "missing `{needle}` in {e}");
        }
        assert!(parse_scenario("name = \"x\"\n").unwrap_err()[0].contains("no [[step]]"));
        assert!(parse_scenario("name=\"x\"\n[[step]]\ntype=\"wait\"\nms=5\nbogus=1\n").is_err());
    }

    #[test]
    fn a_passing_run_substitutes_saved_values_and_checks_everything() {
        let s = parse_scenario(SCENARIO).unwrap();
        let mut w = Fake::new().reply(200, r#"{"token":"abc","user":{"id":7.0}}"#, 40).reply(200, "{\"orders\":[]}", 10);
        w.running.insert("orders".into(), true);
        w.log(1_001, "orders", "created order 55");
        let r = run_scenario(&mut w, &s);
        assert!(r.passed, "{}", r.summary());
        assert_eq!(w.sent[1].header("authorization"), Some("Bearer abc"));
        assert_eq!(w.sent[0].method, "POST");
        assert!(w.sent[0].header("x-correlation-id").is_some());
        assert!(r.steps[1].corr.is_some());
        assert_eq!(r.steps[1].assertions.len(), 4);
        assert_eq!(r.steps.iter().map(|s| s.outcome).collect::<Vec<_>>(), vec![Outcome::Passed; 5]);
    }

    #[test]
    fn a_failed_assertion_fails_the_step_and_skips_the_rest() {
        let s = parse_scenario(SCENARIO).unwrap();
        let mut w = Fake::new().reply(500, r#"{"error":"boom"}"#, 900);
        let r = run_scenario(&mut w, &s);
        assert!(!r.passed);
        assert_eq!(r.steps[1].outcome, Outcome::Failed);
        let failed: Vec<&str> = r.steps[1].assertions.iter().filter(|a| !a.passed).map(|a| a.description.as_str()).collect();
        assert!(failed.iter().any(|d| d.contains("status is 200")) && failed.iter().any(|d| d.contains("within 500 ms")), "{failed:?}");
        assert!(r.steps[2..].iter().all(|s| s.outcome == Outcome::Skipped));
        assert!(r.summary().contains("[FAIL]") && r.summary().contains("trace_request"));
    }

    #[test]
    fn continue_on_fail_keeps_going() {
        let text = "name=\"x\"\n[[step]]\ntype=\"assert_running\"\nprocess=\"ghost\"\ncontinue_on_fail=true\n[[step]]\ntype=\"wait\"\nms=10\n";
        let r = run_scenario(&mut Fake::new(), &parse_scenario(text).unwrap());
        assert_eq!(r.steps[0].outcome, Outcome::Failed);
        assert_eq!(r.steps[1].outcome, Outcome::Passed);
        assert!(!r.passed, "a failure anywhere still fails the scenario");
    }

    #[test]
    fn a_missing_saved_value_fails_the_step_instead_of_sending_a_blank() {
        let text = "name=\"x\"\n[[step]]\ntype=\"http\"\nurl=\"http://localhost:1/${nobody:$.t}\"\n";
        let mut w = Fake::new();
        let r = run_scenario(&mut w, &parse_scenario(text).unwrap());
        assert_eq!(r.steps[0].outcome, Outcome::Failed);
        assert!(r.steps[0].message.contains("no earlier step saved"), "{}", r.steps[0].message);
        assert!(w.sent.is_empty());
    }

    #[test]
    fn a_log_assertion_waits_for_a_line_that_arrives_and_gives_up_otherwise() {
        let text = "name=\"x\"\n[[step]]\ntype=\"assert_log\"\npattern=\"ready now\"\ntimeout_ms=2000\n";
        let mut w = Fake::new();
        w.log_on_sleep = Some((1_500, "api".into(), "service ready now".into()));
        assert!(run_scenario(&mut w, &parse_scenario(text).unwrap()).passed);

        let mut never = Fake::new();
        let r = run_scenario(&mut never, &parse_scenario(text).unwrap());
        assert_eq!(r.steps[0].outcome, Outcome::Failed);
        assert!(r.steps[0].message.contains("no matching line appeared"));
    }

    #[test]
    fn an_absence_assertion_looks_after_the_wait() {
        let text = "name=\"x\"\n[[step]]\ntype=\"assert_log\"\npattern=\"ERROR\"\npresent=false\ntimeout_ms=500\n";
        let mut clean = Fake::new();
        clean.log(1_001, "api", "all good");
        assert!(run_scenario(&mut clean, &parse_scenario(text).unwrap()).passed);

        let mut dirty = Fake::new();
        dirty.log_on_sleep = Some((1_400, "api".into(), "ERROR kaboom".into()));
        let r = run_scenario(&mut dirty, &parse_scenario(text).unwrap());
        assert_eq!(r.steps[0].outcome, Outcome::Failed);
        assert!(r.steps[0].message.contains("unexpected line from api"));
    }

    #[test]
    fn json_checks_cover_each_operator() {
        let v: Value = serde_json::from_str(r#"{"n":5,"s":"hello","a":[1,2]}"#).unwrap();
        let mk = |path: &str| JsonExpect { path: path.into(), exists: None, equals: None, matches: None, gt: None, lt: None };
        assert!(check_json(&v, &JsonExpect { gt: Some(4.0), lt: Some(6.0), ..mk("$.n") }).0);
        assert!(!check_json(&v, &JsonExpect { gt: Some(5.0), ..mk("$.n") }).0);
        assert!(check_json(&v, &JsonExpect { matches: Some("^hel".into()), ..mk("$.s") }).0);
        assert!(check_json(&v, &JsonExpect { equals: Some(serde_json::json!(5.0)), ..mk("$.n") }).0, "5 equals 5.0");
        assert!(check_json(&v, &JsonExpect { exists: Some(false), ..mk("$.zzz") }).0);
        assert!(check_json(&v, &JsonExpect { equals: Some(serde_json::json!(2)), ..mk("$.a[1]") }).0);
        assert!(!check_json(&v, &JsonExpect { exists: Some(true), ..mk("$.zzz") }).0);
    }

    #[test]
    fn substitution_handles_status_body_and_plain_text() {
        let mut saved = HashMap::new();
        saved.insert("r".to_string(), Saved { status: 201, json: serde_json::from_str("{\"id\":9}").ok(), body: "{\"id\":9}".into() });
        assert_eq!(substitute("/x/${r:$.id}/s${r:status}", &saved).unwrap(), "/x/9/s201");
        assert_eq!(substitute("no placeholders", &saved).unwrap(), "no placeholders");
        assert!(substitute("${r:$.nope}", &saved).is_err());
        assert!(substitute("${r", &saved).is_err());
        assert!(substitute("${nameonly}", &saved).is_err());
    }
}
