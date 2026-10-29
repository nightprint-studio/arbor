//! The debugger tools beyond the basics, a child of `debug` (declared there) so it shares its session plumbing.
//!
//! * **Navigation**, which only moves the program the way a stepping user would: `debug_run_to` (a temporary
//!   breakpoint, removed afterwards) and `debug_break_on_exception`.
//! * **Changing the program** (`debug_throw`, `debug_force_return`, `debug_drop_frame`, `debug_set_value`): a
//!   separate family, `debug_mutate`, off until the user turns it on and asking once per conversation even then
//!   (Autonomous mode does not skip it). Each change is said in the thread as an *assistant action*, returned to
//!   the model as such, and sent to the Debug panel. The descriptions say plainly that the program changes and
//!   that there is no undo.

use super::*;
use crate::debug::mutate::Mutation;

const NAVIGATION: [&str; 3] = ["debug_run_to", "debug_break_on_exception", "debug_select_thread"];
const MUTATE: [&str; 4] = ["debug_throw", "debug_force_return", "debug_drop_frame", "debug_set_value"];

/// What the assistant's changes are filed under in the per-conversation approvals.
const APPROVAL_HASH: &str = "debug-mutate";
/// `arbor://bennu/assistant-debug-action`: the assistant changed the program (the Debug panel lists it).
pub(crate) const EVT_ACTION: &str = "arbor://bennu/assistant-debug-action";

/// One of the tools of this file.
pub(crate) fn is_extra_tool(name: &str) -> bool {
    NAVIGATION.contains(&name) || MUTATE.contains(&name)
}

/// A tool that changes the program under debug.
pub(crate) fn is_mutate_tool(name: &str) -> bool {
    MUTATE.contains(&name)
}

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "debug_run_to",
            "Run the program until it reaches a line of a Java file, then stop there and return where it is (like Run to Cursor). It sets a temporary breakpoint that is removed afterwards. If the program is stopped it is resumed first. If something else stops it first (another breakpoint, an exception) you get that stop instead.",
            json!({"type":"object","properties":{
                "file":{"type":"string","description":"The Java file."},
                "line":{"type":"integer","description":"1-based line."},
                "timeout_s":{"type":"integer","description":"Default 60, at most 300."}
            },"required":["file","line"]}),
        ),
        spec(
            "debug_break_on_exception",
            "Stop the program where an exception is thrown. `class` is a fully qualified throwable (`java.lang.IllegalStateException`); leave it out for any exception. `uncaught` (default true) stops on throws nothing catches; `caught` (default false) also on those a frame will catch — frameworks throw and catch thousands, so ask for caught throws of a specific class, not of everything. `remove: true` takes down the ones you set (for `class`, or all of yours without it). The user's own are never touched; yours end with the debug session.",
            json!({"type":"object","properties":{
                "class":{"type":"string"},
                "caught":{"type":"boolean"},
                "uncaught":{"type":"boolean"},
                "remove":{"type":"boolean"}
            }}),
        ),
        spec(
            "debug_select_thread",
            "Switch to another thread that is stopped. When several threads are stopped at once (a breakpoint with \"suspend only its thread\", or two threads hitting breakpoints together) you look at one at a time: the stop report lists the others under \"also stopped\". This makes `thread` (its id, its name or part of its name) the one debug_stack, debug_variables, debug_evaluate, debug_step and debug_resume work on, and returns its stack.",
            json!({"type":"object","properties":{"thread":{"type":"string"}},"required":["thread"]}),
        ),
        spec(
            "debug_throw",
            "CHANGES THE RUNNING PROGRAM, with no undo: creates an exception of `exception_class` inside it (its constructor RUNS in the program) and throws it from the line the stopped thread is on, as soon as it resumes — to see how the code handles a failure that is hard to cause for real. Needs a stop. Off unless the user turned on program changes; the user approves once per conversation. Resume (debug_resume) and wait to see what happens.",
            json!({"type":"object","properties":{
                "exception_class":{"type":"string","description":"Fully qualified, already loaded in the program, e.g. java.io.IOException."},
                "message":{"type":"string"}
            },"required":["exception_class"]}),
        ),
        spec(
            "debug_force_return",
            "CHANGES THE RUNNING PROGRAM, with no undo: the method on top of the stack returns `value` immediately when the program resumes, without running the rest of its body (finally blocks included). `value` is a literal matching the return type: a number, true/false, 'c', \"text\" (with the quotes) or null; omit it for a void method. Only primitives, String and null. Needs a stop and a VM that supports it (it says if not). Off unless the user turned on program changes; approved once per conversation.",
            json!({"type":"object","properties":{"value":{"type":"string"}}}),
        ),
        spec(
            "debug_drop_frame",
            "CHANGES THE RUNNING PROGRAM, with no undo: discards a frame and every frame above it, leaving the program at the call in the caller, which runs again when it resumes — to re-run a method after changing a variable. Side effects of what already ran (writes, calls, prints) are NOT undone. Needs a stop and a VM that supports it. Off unless the user turned on program changes; approved once per conversation.",
            json!({"type":"object","properties":{"frame":{"type":"integer","description":"0 = innermost. Frames 0 to this one are dropped."}},"required":["frame"]}),
        ),
        spec(
            "debug_set_value",
            "CHANGES THE RUNNING PROGRAM, with no undo: writes a primitive, a String or null to a local variable, an instance field, or a field reached through a path (`order.total`). `value` is a literal: a number, true/false, 'c', \"text\" (with the quotes) or null. Other objects cannot be written; final and static fields are refused. Returns the value before and after. Needs a stop. Off unless the user turned on program changes; approved once per conversation.",
            json!({"type":"object","properties":{
                "path":{"type":"string"},
                "value":{"type":"string"},
                "frame":{"type":"integer","description":"0 = innermost. Default 0."}
            },"required":["path","value"]}),
        ),
    ]
}

/// A tool's `value` argument as the text a literal is parsed from (a JSON number or boolean is accepted as is).
fn value_text(input: &Value, key: &str) -> String {
    match input.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// The class names a tool accepts for an exception to create.
fn class_arg(input: &Value, key: &str) -> Result<String, String> {
    let class = str_arg(input, key)?;
    if debug_bp::is_class_name(class) { Ok(class.to_string()) } else { Err(format!("`{class}` is not a fully-qualified class name")) }
}

impl ToolHost {
    /// Changing the program asks once per conversation, whatever the mode: nothing in it can be undone.
    pub(super) fn mutate_gate(&self, tool: &str, input: &Value) -> Result<(), String> {
        if !is_mutate_tool(tool) {
            return Ok(());
        }
        let key = format!("{}#debug-mutate", crate::live::conversation_key(&self.session.root, &self.session.conversation_id));
        if crate::live::approvals().is_recipe_approved(&key, APPROVAL_HASH) {
            return Ok(());
        }
        self.confirm(
            "Let Imber change the running program?",
            &format!(
                "First change: {}\n\nThe program changes while it runs and a change cannot be undone. Approving covers every change to the debugged program in this conversation, and each is listed in the thread and in the Debug panel.",
                describe_change(tool, input)
            ),
            true,
        )?;
        crate::live::approvals().approve_recipes(&key, APPROVAL_HASH);
        Ok(())
    }

    pub(super) fn run_debug_more(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "debug_run_to" => self.debug_run_to(input),
            "debug_break_on_exception" => self.debug_break_on_exception(input),
            "debug_select_thread" => self.debug_select_thread(input),
            t if is_mutate_tool(t) => self.debug_mutate(t, input),
            other => Err(format!("`{other}` is not a debug tool")),
        }
    }

    // ── Navigation ───────────────────────────────────────────────────────────────────────────────

    fn debug_select_thread(&self, input: &Value) -> Result<String, String> {
        let (backend, _) = self.stopped()?;
        let pause = backend.select_thread(str_arg(input, "thread")?)?;
        Ok(format!("now looking at thread `{}`.\n{}", pause.thread_name, text::stack_text(&pause, 8)))
    }

    fn debug_run_to(&self, input: &Value) -> Result<String, String> {
        let path = self.resolve(str_arg(input, "file")?);
        let line = line_arg(input)?;
        if !path.is_file() || !crate::intel::is_java_file(&path.to_string_lossy()) {
            return Err(format!("`{}` is not a Java file", path.display()));
        }
        let source = std::fs::read_to_string(&path).map_err(|e| format!("could not read the file: {e}"))?;
        let line = text::source_line(&source, line)?.line;
        let secs = text::wait_seconds(input.get("timeout_s").and_then(Value::as_u64), DEFAULT_WAIT_S, MAX_WAIT_S);
        self.held()?;

        let shown = path.to_string_lossy().to_string();
        let root = self.root_of(&path);
        let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
        // A breakpoint already there (the user's, or one of yours) does the job and is left alone.
        let temporary = !debug_bp::has_breakpoint(&cfg, &shown, line);
        if temporary {
            debug_bp::set(&mut cfg, &shown, line, "", "", &[])?;
            apply_config(&root, &cfg)?;
            self.notify_breakpoints_changed(&root);
        }
        let outcome = self.run_to_wait(secs);
        if temporary {
            let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
            if debug_bp::remove(&mut cfg, &shown, line) == Removal::Removed && apply_config(&root, &cfg).is_ok() {
                self.notify_breakpoints_changed(&root);
            }
        }
        outcome
    }

    /// Resume if stopped, then wait for the next stop.
    fn run_to_wait(&self, secs: u64) -> Result<String, String> {
        if let Ok((backend, _)) = self.stopped() {
            backend.resume()?;
        }
        match self.wait_for_stop(Duration::from_secs(secs)) {
            Wait::Stopped(p) => Ok(self.stop_report(&p)),
            Wait::Ended(why) => Ok(why),
            Wait::Cancelled => Err("the run was cancelled while waiting; the debug session will be released".into()),
            Wait::TimedOut => Ok(format!("The program did not reach that line within {secs} s: it is still RUNNING. Trigger the code path, or wait again with debug_wait_stop.")),
        }
    }

    fn debug_break_on_exception(&self, input: &Value) -> Result<String, String> {
        let root = self.session.root.clone();
        let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
        let class = input.get("class").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if input.get("remove").and_then(Value::as_bool).unwrap_or(false) {
            let n = debug_bp::remove_exceptions(&mut cfg, &class, class.is_empty());
            if n > 0 {
                apply_config(&root, &cfg)?;
                self.notify_breakpoints_changed(&root);
            }
            return Ok(format!("removed {n} exception breakpoint(s) you had set. The user's were left alone."));
        }
        let caught = input.get("caught").and_then(Value::as_bool).unwrap_or(false);
        let uncaught = input.get("uncaught").and_then(Value::as_bool).unwrap_or(true);
        let outcome = debug_bp::set_exception(&mut cfg, &class, caught, uncaught)?;
        let what = if class.is_empty() { "any exception".to_string() } else { class.clone() };
        if outcome == debug_bp::ExceptionSet::UserOwns {
            return Ok(format!("the user already stops on {what}; it was left exactly as it is."));
        }
        apply_config(&root, &cfg)?;
        self.notify_breakpoints_changed(&root);
        let when = match (caught, uncaught) {
            (true, true) => "caught or uncaught",
            (true, false) => "when caught",
            _ => "when uncaught",
        };
        Ok(format!("the program will stop on {what} {when}. Call debug_wait_stop; the stop says which exception it was. It ends with the debug session."))
    }

    // ── Changing the program ─────────────────────────────────────────────────────────────────────

    fn debug_mutate(&self, tool: &str, input: &Value) -> Result<String, String> {
        let (backend, pause) = self.stopped()?;
        let op = match tool {
            "debug_throw" => Mutation::Throw {
                class: class_arg(input, "exception_class")?,
                message: input.get("message").and_then(Value::as_str).map(str::to_string),
            },
            "debug_force_return" => Mutation::ForceReturn { value: value_text(input, "value") },
            "debug_drop_frame" => Mutation::DropFrame { frame: input.get("frame").and_then(Value::as_u64).ok_or("`frame` is required")? as usize },
            _ => {
                let path = str_arg(input, "path")?;
                text::parse_expression(path)?;
                let frame = Self::frame_arg(input, &pause)?;
                Mutation::SetValue { frame, path: path.to_string(), value: value_text(input, "value") }
            }
        };
        let done = backend.mutate(&op)?;
        let mut line = done.summary.clone();
        if let (Some(before), Some(after)) = (&done.before, &done.after) {
            line = format!("{line}: {} → {}", text::row_text(before), text::row_text(after));
        } else if let Some(after) = &done.after {
            line = format!("{line}: now {}", text::row_text(after));
        }
        self.announce_change(&line);
        Ok(format!("ASSISTANT ACTION — the program was changed (no undo): {line}"))
    }

    /// Say, in the thread and in the Debug panel, that the assistant changed the program.
    fn announce_change(&self, line: &str) {
        self.note("warning", &format!("Imber changed the debugged program: {line}"));
        let session_id = lock(&self.debug.hold).as_ref().map(|h| h.session_id.clone()).unwrap_or_default();
        self.session.sink.emit(EVT_ACTION, json!({ "session_id": session_id, "conversation_id": self.session.conversation_id, "text": line, "at": super::super::debug_keep::now_ms() }));
    }
}

/// The first change, in words, for the approval.
fn describe_change(tool: &str, input: &Value) -> String {
    match tool {
        "debug_throw" => format!("create and throw {} from the stopped line", input.get("exception_class").and_then(Value::as_str).unwrap_or("an exception")),
        "debug_force_return" => format!("make the current method return {}", match value_text(input, "value").as_str() { "" => "(nothing)".to_string(), v => v.to_string() }),
        "debug_drop_frame" => format!("drop frame {} and the ones above it", input.get("frame").and_then(Value::as_u64).unwrap_or(0)),
        _ => format!("set {} to {}", input.get("path").and_then(Value::as_str).unwrap_or("a variable"), value_text(input, "value")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_the_program_is_its_own_family_and_navigation_is_not() {
        for t in MUTATE {
            assert!(is_mutate_tool(t) && is_extra_tool(t) && super::is_debug_tool(t), "{t}");
        }
        for t in NAVIGATION {
            assert!(!is_mutate_tool(t) && is_extra_tool(t) && super::is_debug_tool(t), "{t}");
        }
        assert!(!is_extra_tool("debug_resume"));
        assert_eq!(specs().len(), NAVIGATION.len() + MUTATE.len());
    }

    #[test]
    fn every_change_says_plainly_that_there_is_no_undo() {
        for s in specs().iter().filter(|s| is_mutate_tool(&s.name)) {
            assert!(s.description.contains("CHANGES THE RUNNING PROGRAM") && s.description.contains("no undo"), "{}", s.name);
            assert!(s.description.contains("approve"), "{}", s.name);
        }
    }

    #[test]
    fn a_value_argument_may_be_text_a_number_or_missing() {
        assert_eq!(value_text(&json!({ "value": "\"hi\"" }), "value"), "\"hi\"");
        assert_eq!(value_text(&json!({ "value": 42 }), "value"), "42");
        assert_eq!(value_text(&json!({ "value": true }), "value"), "true");
        assert_eq!(value_text(&json!({ "value": null }), "value"), "");
        assert_eq!(value_text(&json!({}), "value"), "");
    }

    #[test]
    fn the_approval_names_the_first_change() {
        assert!(describe_change("debug_throw", &json!({ "exception_class": "java.io.IOException" })).contains("java.io.IOException"));
        assert!(describe_change("debug_force_return", &json!({})).contains("nothing"));
        assert!(describe_change("debug_set_value", &json!({ "path": "order.total", "value": "0" })).contains("order.total to 0"));
        assert!(class_arg(&json!({ "exception_class": "no way" }), "exception_class").is_err());
    }
}
