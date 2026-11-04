//! Running a request's tests against a response: declarative assertions first, then the sandboxed Lua after
//! a `--- lua` line.
//!
//! The Lua is the same language the Arbor plugins use, but a smaller world: no `io`, `os`, `package`,
//! `debug` or `require`, a hard cap on instructions, and only the `hirundo` table to talk to —
//! `hirundo.response`, `hirundo.expect(condition, message)`, `hirundo.env.set(name, value)` and
//! `hirundo.log(text)`. Values set with `env.set` come back to the window, which keeps them as local values
//! for the next request (chaining one request's output into the next one's input).

use std::sync::{Arc, Mutex};

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{run_assertions, split_lua, AssertionOutcome, ResponseView};
use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Value};
use serde::{Deserialize, Serialize};

/// The instruction budget of one script: generous for a test, fatal for a `while true`.
const INSTRUCTION_BUDGET: u32 = 5_000_000;

#[derive(Debug, Deserialize)]
pub struct TestResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub time_ms: u64,
    #[serde(default)]
    pub size_bytes: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CheckOut {
    pub line: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct TestReport {
    pub checks: Vec<CheckOut>,
    /// What `hirundo.env.set` was called with, in order.
    pub set_vars: Vec<(String, String)>,
    pub logs: Vec<String>,
}

impl TestReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}

/// The checks (and side effects) of `tests` against `response`.
pub fn run_tests(tests: &str, response: &TestResponse) -> TestReport {
    let view = ResponseView { status: response.status, time_ms: response.time_ms, size_bytes: response.size_bytes, headers: &response.headers, body: &response.body };
    let mut report = TestReport {
        checks: run_assertions(tests, &view).into_iter().map(|o: AssertionOutcome| CheckOut { line: o.line, passed: o.passed, detail: o.detail }).collect(),
        ..TestReport::default()
    };
    if let (_, Some(script)) = split_lua(tests) {
        if !script.trim().is_empty() {
            run_lua(&script, response, &mut report);
        }
    }
    report
}

#[derive(Default)]
struct Collected {
    checks: Vec<CheckOut>,
    set_vars: Vec<(String, String)>,
    logs: Vec<String>,
}

fn run_lua(script: &str, response: &TestResponse, report: &mut TestReport) {
    let collected = Arc::new(Mutex::new(Collected::default()));
    let outcome = execute(script, response, &collected);
    let mut c = collected.lock().unwrap_or_else(|p| p.into_inner());
    report.checks.append(&mut c.checks);
    report.set_vars.append(&mut c.set_vars);
    report.logs.append(&mut c.logs);
    if let Err(e) = outcome {
        report.checks.push(CheckOut { line: "lua".into(), passed: false, detail: e.to_string() });
    }
}

fn execute(script: &str, response: &TestResponse, collected: &Arc<Mutex<Collected>>) -> mlua::Result<()> {
    // Only the pure libraries: nothing that touches the machine.
    let lua = Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8, LuaOptions::default())?;
    lua.set_hook(HookTriggers::new().every_nth_instruction(INSTRUCTION_BUDGET), |_, _| Err(mlua::Error::runtime("the script ran too long (instruction budget exhausted)")));

    let api = lua.create_table()?;

    let expect_into = Arc::clone(collected);
    api.set(
        "expect",
        lua.create_function(move |_, (condition, message): (Value, Option<String>)| {
            let passed = !matches!(condition, Value::Nil | Value::Boolean(false));
            let line = message.unwrap_or_else(|| "expect".to_string());
            let detail = if passed { String::new() } else { "the condition was false".to_string() };
            expect_into.lock().unwrap_or_else(|p| p.into_inner()).checks.push(CheckOut { line, passed, detail });
            Ok(())
        })?,
    )?;

    let log_into = Arc::clone(collected);
    api.set(
        "log",
        lua.create_function(move |_, text: String| {
            log_into.lock().unwrap_or_else(|p| p.into_inner()).logs.push(text);
            Ok(())
        })?,
    )?;

    let env = lua.create_table()?;
    let set_into = Arc::clone(collected);
    env.set(
        "set",
        lua.create_function(move |_, (name, value): (String, Value)| {
            let text = match value {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => return Err(mlua::Error::runtime("env.set takes a string, number or boolean")),
            };
            set_into.lock().unwrap_or_else(|p| p.into_inner()).set_vars.push((name, text));
            Ok(())
        })?,
    )?;
    api.set("env", env)?;

    let res = lua.create_table()?;
    res.set("status", response.status)?;
    res.set("time", response.time_ms)?;
    res.set("size", response.size_bytes as u64)?;
    let headers = lua.create_table()?;
    for (k, v) in &response.headers {
        headers.set(k.to_lowercase(), v.as_str())?;
    }
    res.set("headers", headers)?;
    let text_for_text = response.body.clone();
    res.set("text", lua.create_function(move |_, ()| Ok(text_for_text.clone()))?)?;
    let text_for_json = response.body.clone();
    res.set(
        "json",
        lua.create_function(move |lua, ()| match serde_json::from_str::<serde_json::Value>(&text_for_json) {
            Ok(v) => json_to_lua(lua, &v),
            Err(e) => Err(mlua::Error::runtime(format!("the body is not JSON: {e}"))),
        })?,
    )?;
    api.set("response", res)?;

    lua.globals().set("hirundo", api)?;
    lua.load(script).set_name("tests").exec()
}

/// JSON → Lua by hand: `null` becomes `nil` (the serde route turns it into a truthy sentinel).
fn json_to_lua(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => Value::String(lua.create_string(s)?),
        J::Array(items) => {
            let t = lua.create_table()?;
            for (i, item) in items.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, item)?)?;
            }
            Value::Table(t)
        }
        J::Object(map) => {
            let t = lua.create_table()?;
            for (k, item) in map {
                t.set(k.as_str(), json_to_lua(lua, item)?)?;
            }
            Value::Table(t)
        }
    })
}

/// Run the tests of a request against the response it just got.
#[arbor_rpc::handler]
fn tests_run(_state: &HirundoState, tests: String, response: TestResponse) -> Result<TestReport, String> {
    Ok(run_tests(&tests, &response))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> TestResponse {
        TestResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: r#"{"content":[{"id":7}],"note":null}"#.into(),
            time_ms: 12,
            size_bytes: 40,
        }
    }

    #[test]
    fn assertions_and_lua_report_together() {
        let r = run_tests("status == 200\n--- lua\nlocal j = hirundo.response.json()\nhirundo.expect(#j.content == 1, 'one item')\nhirundo.env.set('id', j.content[1].id)", &response());
        assert!(r.passed(), "{:?}", r.checks);
        assert_eq!(r.checks.len(), 2);
        assert_eq!(r.set_vars, vec![("id".to_string(), "7".to_string())]);
    }

    #[test]
    fn a_false_expectation_fails_and_json_null_is_nil() {
        let r = run_tests("--- lua\nhirundo.expect(hirundo.response.json().note == nil, 'null is nil')\nhirundo.expect(false, 'nope')", &response());
        assert_eq!(r.checks.iter().map(|c| c.passed).collect::<Vec<_>>(), [true, false]);
    }

    #[test]
    fn the_sandbox_has_no_os_io_or_require() {
        let r = run_tests("--- lua\nhirundo.expect(os == nil and io == nil and require == nil and debug == nil, 'sandboxed')", &response());
        assert!(r.passed(), "{:?}", r.checks);
    }

    #[test]
    fn a_runaway_script_is_stopped_and_a_broken_one_is_reported() {
        let r = run_tests("--- lua\nwhile true do end", &response());
        assert!(!r.passed());
        assert!(r.checks[0].detail.contains("ran too long"), "{:?}", r.checks);
        let r = run_tests("--- lua\nthis is not lua", &response());
        assert!(!r.passed());
    }
}
