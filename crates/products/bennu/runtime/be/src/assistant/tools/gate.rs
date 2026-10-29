//! The gate every tool call crosses before it runs: the access policy for each path it touches, the prompts that
//! policy raises, and the confirmation that destructive or project-running tools need.

use super::*;

impl ToolHost {
    // ── Gate ─────────────────────────────────────────────────────────────────────────────────────

    /// The paths a call touches and how, for the access policy.
    pub(super) fn touches(&self, call: &ToolCall) -> Vec<(PathBuf, Op)> {
        let input = &call.input;
        let mut out: Vec<(PathBuf, Op)> = Vec::new();
        let str_of = |k: &str| input.get(k).and_then(Value::as_str).map(|s| self.resolve(s));
        match call.name.as_str() {
            "search_text" | "find_files" => match self.text_targets(input, input.get("path").and_then(Value::as_str)) {
                Ok(targets) => out.extend(targets.into_iter().map(|(_, _, base)| (base, Op::List))),
                // A name that is not a project: the call will say so; here it is held to the project folder.
                Err(_) => out.push((self.project().to_path_buf(), Op::List)),
            },
            "edit_file" | "rename_symbol" | "add_parameter" => out.extend(str_of("file").map(|p| (p, Op::Write))),
            "create_file" => out.extend(str_of("file").map(|p| (p, Op::Create))),
            "delete_file" => {
                for p in input.get("paths").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    out.push((self.resolve(p), Op::Delete));
                }
            }
            "edit_files" => out.extend(self.edit_files_touched(input).into_iter().map(|p| (p, Op::Write))),
            // A tree-wide change needs the right to write across the folder it works in.
            "replace_in_files" => out.push((str_of("path").unwrap_or_else(|| self.project().to_path_buf()), Op::Write)),
            "structural_replace" => out.push((self.project().to_path_buf(), Op::Write)),
            n if live::is_live_tool(n) => out.extend(self.live_touched(n, input)),
            n if debug::is_debug_tool(n) => out.extend(self.debug_touched(n, input)),
            // A game and a page on this machine: no file is read or written, the loopback rule is in the tool.
            n if brp::is_brp_tool(n) || screenshot::is_screenshot_tool(n) => {}
            "local_history" => out.extend(self.history_touched(input).into_iter().map(|(p, write)| (p, if write { Op::Write } else { Op::Read }))),
            "ask_user" | "bennu_help" | "update_plan" | "open_ui" | "load_skill" | "show_diagram" => {}
            // A sub-agent's own calls cross this gate under its own, narrower policy; starting one touches nothing here.
            n if spawn::is_agent_tool(n) => {}
            n if reads::is_read_tool(n) => out.extend(self.read_touched(call).into_iter().map(|p| (p, Op::Read))),
            _ => {
                let op = match self.descriptor(&call.name).map(|d| d.safety) {
                    Some(Safety::Read) | None => Op::Read,
                    Some(_) => Op::Write,
                };
                for (path, role) in catalog_path_args(input) {
                    out.push((self.resolve(&path), if role == PathRole::Folder { Op::List } else { op }));
                }
                // The policy sees what a call is given, not what comes back. A tool that answers about the
                // whole project (usages, callers, the framework catalogue) hands back other files' paths and
                // lines even when it was pointed at one file — so under "this file" it is held to the
                // project folder as well, and asks.
                if self.session.policy().scope == Scope::File && reaches_beyond_its_file(&call.name) {
                    out.push((self.project().to_path_buf(), Op::List));
                }
            }
        }
        out
    }

    /// Ask the access policy about one path, putting the user's answer to any prompt it raises.
    pub(super) fn authorize(&self, path: &Path, op: Op) -> Result<(), String> {
        let files = if op == Op::Delete { delete_file_count(path) } else { 1 };
        self.authorize_counted(path, op, files)
    }

    /// [`Self::authorize`] for an operation that touches `files` files (a delete of a folder).
    fn authorize_counted(&self, path: &Path, op: Op, files: usize) -> Result<(), String> {
        // A grant or a yes changes the answer at most twice (read → write upgrade), so the loop is bounded.
        for _ in 0..3 {
            let decision = self.session.policy().check_counted(path, op, files);
            match decision {
                Decision::Allow => return Ok(()),
                Decision::Deny(why) => return Err(why),
                Decision::Ask(ask) => match ask.grant.clone() {
                    Some(folder) => {
                        // Autonomous opens a file the build named without asking; everything else is a question.
                        if self.open_if_the_build_named_it(path, op) {
                            continue;
                        }
                        let question = self.access_question(path, &folder, &ask);
                        self.ask_access(&folder, path, &question)?
                    }
                    // A confirmation applies to this call only and is not recorded in the policy, so a
                    // "yes" is the answer — looping back would ask the same question again.
                    None => return self.confirm("Allow this?", &ask.message, true),
                },
            }
        }
        Err("access could not be settled".into())
    }

    /// The one prompt for a path outside the allowed scope: no, reading, or reading and writing.
    pub(super) fn ask_access(&self, folder: &Path, file: &Path, reason: &str) -> Result<(), String> {
        let request_id = format!("ax-{}", folder.display());
        let block = json!({
            "type": "access",
            "request": {
                "id": request_id,
                "folder": folder.display().to_string(),
                "file": file.file_name().map(|n| n.to_string_lossy().to_string()),
                "reason": reason,
            },
        });
        let answer = self.session.ask(block, "/request/id", &request_id, |b, a| {
            b["request"]["answer"] = a.get("answer").cloned().unwrap_or(json!("denied"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some(word @ ("read" | "write")) => {
                self.session.policy().grant(folder.to_path_buf(), word == "write");
                // What a sub-agent was granted is the agent's alone: it is not remembered for the conversation.
                if self.session.prompt_via.is_none() {
                    runs::remember_grants(&self.session.conversation_id, self.session.policy().grants.clone());
                }
                Ok(())
            }
            _ => Err(format!("the user did not allow access to {}", folder.display())),
        }
    }

    /// A yes/no question. `Err` is the model-facing reason when the answer is no (or the run ended).
    pub(super) fn confirm(&self, title: &str, detail: &str, danger: bool) -> Result<(), String> {
        let id = unique_id("cf");
        let block = json!({ "type": "confirm", "confirm": { "id": id, "title": title, "detail": detail, "danger": danger } });
        let answer = self.session.ask(block, "/confirm/id", &id, |b, a| {
            b["confirm"]["answer"] = a.get("answer").cloned().unwrap_or(json!("no"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some("yes") => Ok(()),
            _ => Err("the user declined".into()),
        }
    }

    pub(super) fn gate_call(&self, call: &ToolCall) -> Gate {
        if call.name == "update_plan" && !self.plan {
            return Gate::Refuse("the step plan is switched off".into());
        }
        if self.chat && !CHAT_TOOLS.contains(&call.name.as_str()) {
            return Gate::Refuse("this is a chat: its only tools are the help about Bennu, opening a panel and showing a diagram".into());
        }
        if call.name == "load_skill" && self.skills.is_empty() {
            return Gate::Refuse("the user has not enabled any skill".into());
        }
        if self.off.hides(&call.name) {
            return Gate::Refuse("the user has switched that kind of tool off".into());
        }
        let known = self.descriptor(&call.name).is_some()
            || matches!(
                call.name.as_str(),
                "search_text" | "find_files" | "edit_file" | "edit_files" | "replace_in_files" | "create_file" | "delete_file" | "ask_user" | "rename_symbol"
                    | "add_parameter" | "bennu_help" | "update_plan" | "local_history" | "structural_replace" | "open_ui" | "load_skill" | "show_diagram"
            )
            || reads::is_read_tool(&call.name)
            || spawn::is_agent_tool(&call.name)
            || live::is_live_tool(&call.name)
            || debug::is_debug_tool(&call.name)
            || brp::is_brp_tool(&call.name)
            || screenshot::is_screenshot_tool(&call.name);
        if !known {
            return Gate::Refuse(format!("`{}` is not a tool you have", call.name));
        }
        for (path, op) in self.touches(call) {
            if let Err(why) = self.authorize(&path, op) {
                return Gate::Refuse(why);
            }
        }
        // Starting processes, running recipes and scenarios, and writing to a port we did not start ask first.
        if live::is_live_tool(&call.name) {
            if let Err(why) = self.live_gate(&call.name, &call.input) {
                return Gate::Refuse(why);
            }
        }
        // Launching or attaching a debugger asks first; the rest of the debugger acts on the session that approval opened.
        if debug::is_debug_tool(&call.name) {
            if let Err(why) = self.debug_gate(&call.name, &call.input) {
                return Gate::Refuse(why);
            }
        }
        // Changing a running game asks once per conversation: it has no proposal and no undo.
        if brp::is_brp_write(&call.name) {
            if let Err(why) = self.brp_gate(&call.name, &call.input) {
                return Gate::Refuse(why);
            }
        }
        // A catalogue tool that changes things, or runs project code, asks first (writes only in manual mode).
        if let Some(d) = self.descriptor(&call.name) {
            // The project's own build and tests follow the mode (every call, once per run, or never); anything else
            // that runs code asks every time.
            let own_command = bennu_assistant::prelude::is_own_command(&d.name);
            let must_ask = match d.safety {
                Safety::Read => false,
                Safety::Write => !self.session.applies_edits() || git::always_asks(&d.name),
                Safety::Destructive if own_command => bennu_assistant::prelude::must_ask_own_command(self.session.mode, self.session.own_commands_approved()),
                Safety::Destructive => true,
            };
            if must_ask {
                if let Err(why) = self.confirm(&format!("Run {}?", d.title), &d.description.lines().next().unwrap_or(""), d.safety == Safety::Destructive) {
                    return Gate::Refuse(why);
                }
                if own_command {
                    self.session.approve_own_commands();
                }
            }
        }
        Gate::Run
    }
}

/// How many files a delete of `path` removes, counted only as far as the unasked-delete cap needs (and with
/// a bound on the entries visited, so a huge tree is not walked to count it).
fn delete_file_count(path: &Path) -> usize {
    const VISIT_BUDGET: usize = 5_000;
    let (mut files, mut visited) = (0usize, 0usize);
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        visited += 1;
        if files > bennu_assistant::prelude::MAX_UNASKED_DELETE_FILES || visited > VISIT_BUDGET {
            // Over the cap either way; the exact number does not matter.
            return files.max(bennu_assistant::prelude::MAX_UNASKED_DELETE_FILES + 1);
        }
        if p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&p) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        } else {
            files += 1;
        }
    }
    files.max(1)
}

/// The catalogue tools that look at one file and say nothing about the rest of the project.
const FILE_LOCAL_TOOLS: [&str; 4] = ["bennu_read_file", "bennu_outline", "bennu_check_file", "bennu_symbol_at"];

/// Whether a catalogue tool's answer can include things from outside the file it was pointed at.
fn reaches_beyond_its_file(tool: &str) -> bool {
    !FILE_LOCAL_TOOLS.contains(&tool)
}

/// What a path argument of a catalogue tool names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathRole {
    File,
    Folder,
}

/// The paths a catalogue tool's arguments name, for the access policy.
///
/// `root` is the project the tool works in — it is how a tool finds the project's encoding and index, not
/// something it reads — so it only counts when the call names nothing narrower. A call that reads one
/// allowed file must not be refused because that file lives in a project the user did not open up; a call
/// that has only a `root` is a project-wide question, and is held to the reach as one.
fn catalog_path_args(input: &Value) -> Vec<(String, PathRole)> {
    let mut out: Vec<(String, PathRole)> = Vec::new();
    let text = |k: &str| input.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    for key in ["file", "path", "new_path"] {
        out.extend(text(key).map(|p| (p, PathRole::File)));
    }
    out.extend(text("dir").map(|p| (p, PathRole::Folder)));
    for key in ["files", "paths"] {
        for p in input.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            out.push((p.to_string(), PathRole::File));
        }
    }
    if out.is_empty() {
        out.extend(text("root").map(|p| (p, PathRole::Folder)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_this_file_only_the_file_local_tools_stay_inside_it() {
        for local in ["bennu_read_file", "bennu_outline", "bennu_check_file", "bennu_symbol_at"] {
            assert!(!reaches_beyond_its_file(local), "{local}");
        }
        for wide in ["bennu_references", "bennu_callers", "bennu_catalog", "bennu_rename_plan", "bennu_problems", "bennu_frameworks"] {
            assert!(reaches_beyond_its_file(wide), "{wide}");
        }
    }

    #[test]
    fn a_call_that_names_a_file_is_not_held_to_its_project_folder() {
        let got = catalog_path_args(&json!({"root": "C:/p", "file": "C:/p/src/A.java"}));
        assert_eq!(got, vec![("C:/p/src/A.java".to_string(), PathRole::File)]);
    }

    #[test]
    fn a_call_with_only_a_root_is_a_project_wide_question() {
        let got = catalog_path_args(&json!({"root": "C:/p", "symbol": "Order"}));
        assert_eq!(got, vec![("C:/p".to_string(), PathRole::Folder)]);
    }

    #[test]
    fn every_path_a_call_names_is_collected() {
        let got = catalog_path_args(&json!({"files": ["a", "b"], "dir": "d", "path": ""}));
        assert_eq!(got.len(), 3, "the empty path is ignored: {got:?}");
        assert!(got.contains(&("d".to_string(), PathRole::Folder)));
    }
}
