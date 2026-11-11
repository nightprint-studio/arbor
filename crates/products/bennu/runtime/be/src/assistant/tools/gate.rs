//! The gate every tool call crosses before it runs: the access policy for each path it touches, the prompts that
//! policy raises, and the confirmation that destructive or project-running tools need.

use super::*;

use crate::assistant::schedule::request::creation_card;
use crate::assistant::schedule::unattended::{needs_a_person, verdict as unattended_verdict, Kind as UnattendedKind};
use imber_schedule::prelude::{is_schedule_tool, unattended_may_call};

/// What a call is, for the table of what a scheduled run may do (`schedule::unattended`). `safety` is the
/// catalogue's word for the tool; `None` for the assistant's own tools, which are named here.
fn classify_unattended(name: &str, safety: Option<Safety>) -> UnattendedKind {
    match name {
        "ask_user" => UnattendedKind::Question,
        "delete_file" => UnattendedKind::Delete,
        // All of these end in the proposal door in manual mode: a card, never a write.
        "edit_file" | "edit_files" | "replace_in_files" | "create_file" | "rename_symbol" | "add_parameter" | "structural_replace" | "local_history" => UnattendedKind::Propose,
        n if is_schedule_tool(n) && unattended_may_call(n) => UnattendedKind::ScheduleList,
        n if is_schedule_tool(n) => UnattendedKind::ScheduleChange,
        n if spawn::is_agent_tool(n) => UnattendedKind::Agent,
        n if live::is_live_tool(n) || debug::is_debug_tool(n) || brp::is_brp_tool(n) || screenshot::is_screenshot_tool(n) || profile::is_profile_tool(n) => UnattendedKind::Process,
        n => match safety {
            Some(Safety::Write) => UnattendedKind::CatalogWrite,
            Some(Safety::Destructive) if bennu_assistant::prelude::is_own_command(n) => UnattendedKind::OwnCommand,
            Some(Safety::Destructive) => UnattendedKind::CatalogDestructive,
            // Reads, searches, the help, the plan, a skill, a diagram.
            Some(Safety::Read) | None => UnattendedKind::Read,
        },
    }
}

/// The confirmation a schedule tool needs from the user, as a title and a text. Creating one is asked in every
/// mode; changing or deleting one follows the write rule (asked unless edits are applied as they are made).
fn schedule_approval(call: &ToolCall, applies_edits: bool) -> Option<(String, String)> {
    match call.name.as_str() {
        "schedule_create" => Some(creation_card(&call.input)),
        "schedule_update" if !applies_edits => Some(("Change this schedule?".into(), format!("Schedule {}: {}", call.input.get("id").and_then(Value::as_str).unwrap_or("?"), "its time, prompt or on/off state changes."))),
        "schedule_delete" if !applies_edits => Some(("Delete this schedule?".into(), format!("Schedule {} is removed. Nothing it already did is undone.", call.input.get("id").and_then(Value::as_str).unwrap_or("?")))),
        _ => None,
    }
}

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
            // Reads source across the whole project and changes nothing.
            "structural_search" => out.push((self.project().to_path_buf(), Op::List)),
            n if live::is_live_tool(n) => out.extend(self.live_touched(n, input)),
            n if debug::is_debug_tool(n) => out.extend(self.debug_touched(n, input)),
            // `live_attach` aimed at a debugger is `debug_attach`: it reads the project folder like it.
            "live_attach" if brp::attaches_debugger(input) => out.extend(self.debug_touched("debug_attach", input)),
            // A game and a page on this machine: no file is read or written, the loopback rule is in the tool.
            n if brp::is_brp_tool(n) || screenshot::is_screenshot_tool(n) => {}
            "local_history" => out.extend(self.history_touched(input).into_iter().map(|(p, write)| (p, if write { Op::Write } else { Op::Read }))),
            "ask_user" | "bennu_help" | "update_plan" | "open_ui" | "load_skill" | "show_diagram" => {}
            n if is_schedule_tool(n) => {}
            // Showing a picture from a file is a read of that file; one the model wrote itself touches nothing.
            "show_image" => out.extend(str_of("path").map(|p| (p, Op::Read))),
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
                        self.ask_access(&folder, path, &question, ask.upgrade)?
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
    ///
    /// `upgrade` is the question of writing in a folder already opened for reading: anything but a yes leaves the
    /// folder read-only, and the model is told to adapt.
    pub(super) fn ask_access(&self, folder: &Path, file: &Path, reason: &str, upgrade: bool) -> Result<(), String> {
        if self.session.unattended {
            return Err(format!("this is a scheduled run and nobody can allow access to {}: stay inside the project, or say that this needs a person", folder.display()));
        }
        // Unique per question, not per folder: asking again for the same folder (reading first, then writing) puts a
        // second card in the thread, and both the panel and the view find a card by this id — with a repeated one the
        // answer goes to the first card, which nothing waits on any more, and the run stays parked on the second.
        let request_id = unique_id("ax");
        let block = json!({
            "type": "access",
            "request": {
                "id": request_id,
                "folder": folder.display().to_string(),
                "file": file.file_name().map(|n| n.to_string_lossy().to_string()),
                "reason": reason,
                "upgrade": upgrade,
            },
        });
        let answer = self.session.ask(block, "/request/id", &request_id, |b, a| {
            b["request"]["answer"] = a.get("answer").cloned().unwrap_or(json!("denied"));
        });
        match answer.as_ref().and_then(|a| a.get("answer")).and_then(Value::as_str) {
            Some("write") => {
                self.session.policy().grant(folder.to_path_buf(), true);
                self.remember_grants();
                Ok(())
            }
            // Writing was the question and the answer is no: the folder stays as it was opened.
            _ if upgrade => Err(format!(
                "the user kept {} read-only and did not allow writing there. Adapt: work from what you can read and describe \
                 the change instead of making it",
                folder.display()
            )),
            Some("read") => {
                self.session.policy().grant(folder.to_path_buf(), false);
                self.remember_grants();
                Ok(())
            }
            _ => Err(format!("the user did not allow access to {}", folder.display())),
        }
    }

    /// Keep this conversation's grants for its later runs. What a sub-agent was granted is the agent's alone:
    /// it is not remembered for the conversation.
    fn remember_grants(&self) {
        if self.session.prompt_via.is_none() {
            runs::remember_grants(&self.session.conversation_id, self.session.policy().grants.clone());
        }
    }

    /// A yes/no question. `Err` is the model-facing reason when the answer is no (or the run ended).
    pub(super) fn confirm(&self, title: &str, detail: &str, danger: bool) -> Result<(), String> {
        if self.session.unattended {
            return Err("this is a scheduled run: nobody is there to approve it, so it was not done".into());
        }
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
            return Gate::Refuse("this is a chat: its only tools are the help about Bennu, opening a panel and showing a diagram or a picture".into());
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
                    | "add_parameter" | "bennu_help" | "update_plan" | "local_history" | "structural_replace" | "structural_search" | "open_ui" | "load_skill" | "show_diagram" | "show_image"
            )
            || reads::is_read_tool(&call.name)
            || spawn::is_agent_tool(&call.name)
            || live::is_live_tool(&call.name)
            || debug::is_debug_tool(&call.name)
            || brp::is_brp_tool(&call.name)
            || screenshot::is_screenshot_tool(&call.name)
            || profile::is_profile_tool(&call.name)
            || is_schedule_tool(&call.name);
        if !known {
            return Gate::Refuse(format!("`{}` is not a tool you have", call.name));
        }
        // A scheduled run has nobody to approve anything: the table decides, before any path is looked at.
        if self.session.unattended {
            let kind = classify_unattended(&call.name, self.descriptor(&call.name).map(|d| d.safety));
            if let Err(why) = unattended_verdict(kind) {
                if kind == UnattendedKind::Question {
                    self.session.set_end_note(needs_a_person(&call.input));
                }
                return Gate::Refuse(why.into());
            }
        } else if let Some((title, detail)) = schedule_approval(call, self.session.applies_edits()) {
            // What Imber schedules happens later with nobody there, so the user reads it first.
            if let Err(why) = self.confirm(&title, &detail, false) {
                return Gate::Refuse(why);
            }
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
        // `live_attach` at a debugger is `debug_attach`'s door, so it asks what `debug_attach` asks.
        if call.name == "live_attach" && brp::attaches_debugger(&call.input) {
            if let Err(why) = self.debug_gate("debug_attach", &call.input) {
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
    fn a_scheduled_run_can_read_and_propose_and_nothing_that_needs_a_person() {
        let allowed = |name: &str, safety: Option<Safety>| unattended_verdict(classify_unattended(name, safety)).is_ok();
        for ok in ["search_text", "find_files", "bennu_read_file", "read_files", "file_outline", "bennu_help", "update_plan", "load_skill", "show_diagram", "schedule_list"] {
            assert!(allowed(ok, None), "{ok}");
        }
        for proposal in ["edit_file", "edit_files", "replace_in_files", "create_file", "rename_symbol", "add_parameter", "structural_replace", "local_history"] {
            assert!(allowed(proposal, None), "{proposal} becomes a card");
        }
        assert!(allowed("bennu_references", Some(Safety::Read)), "the catalogue's reads");
        for refused in ["ask_user", "delete_file", "run_start", "spawn_agent", "schedule_create", "schedule_update", "schedule_delete"] {
            assert!(!allowed(refused, None), "{refused}");
        }
        assert!(!allowed("bennu_rename_apply", Some(Safety::Write)), "a catalogue write");
        assert!(!allowed("bennu_build", Some(Safety::Destructive)), "the project's own build");
        assert!(!allowed("bennu_test_run", Some(Safety::Destructive)));
        assert!(!allowed("bennu_delete_paths", Some(Safety::Destructive)));
        assert_eq!(classify_unattended("ask_user", None), UnattendedKind::Question);
        assert_eq!(classify_unattended("bennu_build", Some(Safety::Destructive)), UnattendedKind::OwnCommand);
    }

    #[test]
    fn creating_a_schedule_always_asks_and_changing_one_follows_the_write_rule() {
        let call = |name: &str| ToolCall { id: "t".into(), name: name.into(), input: json!({"id": "sch-1", "name": "Check", "action": "prompt", "when": {"kind": "daily", "time": "09:00"}}) };
        for applies in [false, true] {
            assert!(schedule_approval(&call("schedule_create"), applies).is_some(), "applies_edits = {applies}");
            assert!(schedule_approval(&call("schedule_list"), applies).is_none());
        }
        assert!(schedule_approval(&call("schedule_update"), false).is_some() && schedule_approval(&call("schedule_update"), true).is_none());
        assert!(schedule_approval(&call("schedule_delete"), false).is_some() && schedule_approval(&call("schedule_delete"), true).is_none());
    }

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
