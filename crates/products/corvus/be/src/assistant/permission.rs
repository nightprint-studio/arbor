//! What Imber may do to a repository without asking, and what the user is shown before saying yes.
//!
//! Pure on purpose: the rule is a table (tool → risk) and a two-line decision, and a table is what a
//! test can pin down. The run asks [`needs_approval`] for every call; if the answer is yes it shows
//! [`preview`] — the exact git command and its effect — and waits.
//!
//! * **Read** never asks. It only looks.
//! * **Write** changes this repository or adds to the remote (stage, commit, fetch, pull, branch,
//!   checkout, merge, push, open an MR). In *manual* mode each one is proposed; in *auto* it runs.
//! * **Destructive** asks in every mode: a force push or a branch deletion cannot be undone from here.
//!   A tool this table does not know is treated the same way, so a tool added later is cautious until
//!   someone decides otherwise.

use serde_json::Value;

/// How much freedom a run has. The user's choice, shown as a switch in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Every write is a proposal the user approves.
    Manual,
    /// Writes run; destructive calls still ask.
    Auto,
}

impl Mode {
    /// Anything but `auto` is the cautious one.
    pub fn parse(word: &str) -> Self {
        if word.trim().eq_ignore_ascii_case("auto") {
            Mode::Auto
        } else {
            Mode::Manual
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Mode::Manual => "manual",
            Mode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Read,
    Write,
    Destructive,
}

impl Risk {
    pub fn word(self) -> &'static str {
        match self {
            Risk::Read => "read",
            Risk::Write => "write",
            Risk::Destructive => "destructive",
        }
    }
}

/// How risky one call is. Looks at the arguments too: a push is a write until it is forced.
pub fn classify(tool: &str, input: &Value) -> Risk {
    match tool {
        "git_status" | "git_diff" | "git_log" | "git_blame" | "git_show" | "git_branches" | "git_mr_list" | "schedule_list" | "plan_list" | "plan_show" | "undo_list" => Risk::Read,
        "git_stage" | "git_unstage" | "plan_create" | "plan_add_step" | "plan_remove_step" | "plan_set_gate" | "plan_run" | "plan_stop" | "plan_retry_step" | "plan_skip_step" | "plan_release_gate" | "schedule_create" | "schedule_update" | "schedule_delete" | "git_commit" | "git_fetch" | "git_pull" | "git_branch_create" | "git_checkout" | "git_merge" | "git_mr_create" => Risk::Write,
        "git_push" if flag(input, "force") => Risk::Destructive,
        "git_push" => Risk::Write,
        _ => Risk::Destructive,
    }
}

/// Whether the call has to be put to the user first.
pub fn needs_approval(mode: Mode, risk: Risk) -> bool {
    match risk {
        Risk::Read => false,
        Risk::Write => mode == Mode::Manual,
        Risk::Destructive => true,
    }
}

/// [`needs_approval`] for a named call. Creating a schedule is asked in every mode: it makes behaviour that will
/// happen later with nobody there to approve it, so the user sees it spelled out first.
pub fn needs_approval_for(tool: &str, mode: Mode, risk: Risk) -> bool {
    tool == "schedule_create" || needs_approval(mode, risk)
}


/// Whether a call may run in a scheduled run, where nobody is there to approve it. The user's grant is the
/// job's `mode`, given when the job was created (and approved): `auto` lets writes run, `manual` leaves the job
/// read-only. A destructive action never runs unattended whatever the mode, and a scheduled run cannot change
/// schedules — a job that could schedule more jobs would outlive anyone's say in it.
pub fn unattended_allows(mode: Mode, risk: Risk, tool: &str) -> Result<(), String> {
    if imber_schedule::prelude::is_schedule_tool(tool) && !imber_schedule::prelude::unattended_may_call(tool) {
        return Err("A scheduled run cannot create or change schedules. Say so in your answer if one is needed.".into());
    }
    match (risk, mode) {
        (Risk::Read, _) | (Risk::Write, Mode::Auto) => Ok(()),
        (Risk::Write, Mode::Manual) => Err("This is a scheduled run with no one to approve changes, and the schedule is read-only. Report what you would have done instead.".into()),
        (Risk::Destructive, _) => Err("Destructive actions never run unattended. Report that this needs a person.".into()),
    }
}
/// What the approval card shows: a title, the git commands the call amounts to, and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub title: String,
    pub commands: Vec<String>,
    pub effect: String,
}

pub fn preview(tool: &str, input: &Value) -> Preview {
    let remote = text(input, "remote").unwrap_or("origin");
    let (title, commands, effect): (&str, Vec<String>, String) = match tool {
        "git_stage" => (
            "Stage files",
            vec![format!("git add -- {}", quote_all(&list(input, "paths")))],
            "Adds these files to the index. The working tree is not touched.".into(),
        ),
        "git_unstage" => (
            "Unstage files",
            vec![format!("git restore --staged -- {}", quote_all(&list(input, "paths")))],
            "Takes these files out of the index. The working tree is not touched.".into(),
        ),
        "git_commit" => {
            let mut commands = Vec::new();
            let paths = list(input, "paths");
            if !paths.is_empty() {
                commands.push(format!("git add -- {}", quote_all(&paths)));
            }
            commands.push(format!("git commit -m {}", quote(text(input, "message").unwrap_or(""))));
            ("Commit", commands, "Creates a local commit on the current branch. Undo: git reset --soft HEAD~1.".into())
        }
        "git_fetch" => ("Fetch", vec![format!("git fetch {}", quote(remote))], "Downloads the remote's new commits. Nothing local changes.".into()),
        "git_pull" => (
            "Pull",
            vec![format!("git pull {}", quote(remote))],
            "Fetches and merges into the current branch. Local changes are stashed first and re-applied after.".into(),
        ),
        "git_push" if flag(input, "force") => (
            "Force push",
            vec![format!("git push --force {} HEAD", quote(remote))],
            "Overwrites the branch on the remote, discarding commits it has that you do not. Others who pulled it are affected.".into(),
        ),
        "git_push" => ("Push", vec![format!("git push {} HEAD", quote(remote))], "Sends the current branch's new commits to the remote.".into()),
        "git_branch_create" => {
            let name = quote(text(input, "name").unwrap_or(""));
            let from = text(input, "from").map(|f| format!(" {}", quote(f))).unwrap_or_default();
            let command = if flag(input, "checkout") { format!("git switch -c {name}{from}") } else { format!("git branch {name}{from}") };
            ("Create branch", vec![command], "Creates the branch; with checkout, switches to it (local changes are carried over safely).".into())
        }
        "git_checkout" => ("Switch branch", vec![format!("git switch {}", quote(text(input, "branch").unwrap_or("")))], "Changes the branch you are on. Local changes are stashed and re-applied.".into()),
        "git_merge" => {
            let strategy = text(input, "strategy").filter(|s| *s != "default").map(|s| format!(" --{}", s.replace('_', "-"))).unwrap_or_default();
            ("Merge", vec![format!("git merge{strategy} {}", quote(text(input, "branch").unwrap_or("")))], "Merges that branch into the current one. A conflict leaves the merge in progress; it can be aborted.".into())
        }
        "git_mr_create" => (
            "Open merge request",
            vec![format!(
                "create a merge request {} \u{2192} {}: {}",
                text(input, "source_branch").unwrap_or("(current branch)"),
                text(input, "target_branch").unwrap_or("(default branch)"),
                quote(text(input, "title").unwrap_or(""))
            )],
            "Opens the request on the hosting provider; visible to everyone with access.".into(),
        ),
        "plan_create" => {
            let phases = input.get("phases").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            let steps: usize = input.get("phases").and_then(Value::as_array).map(|a| a.iter().map(|p| p.get("steps").and_then(Value::as_array).map_or(0, Vec::len)).sum()).unwrap_or(0);
            (
                "Create a plan",
                vec![format!("create plan {} ({phases} phase(s), {steps} step(s))", quote(text(input, "name").unwrap_or("Imber plan")))],
                "Saves a draft in the Plan window (Ctrl+Shift+J). Nothing runs until it is run.".into(),
            )
        }
        "plan_add_step" => ("Add a plan step", vec![format!("plan {}: add {}", quote(text(input, "plan_id").unwrap_or("")), text(input, "kind").unwrap_or("a step"))], "Edits the draft; nothing runs.".into()),
        "plan_remove_step" => ("Remove a plan step", vec![format!("plan {}: remove step {}", quote(text(input, "plan_id").unwrap_or("")), quote(text(input, "step_id").unwrap_or("")))], "Edits the draft; nothing runs.".into()),
        "plan_set_gate" => ("Set a plan gate", vec![format!("plan {}: phase {} starts {}", quote(text(input, "plan_id").unwrap_or("")), quote(text(input, "phase_id").unwrap_or("")), input.get("gate").and_then(|g| g.get("kind")).and_then(Value::as_str).unwrap_or("?"))], "Edits the draft; nothing runs.".into()),
        "plan_run" => (
            "Run a plan",
            vec![format!("run plan {}", quote(text(input, "plan_id").unwrap_or("")))],
            "Starts the plan: its steps push, open and merge requests, start pipelines and tag on the remote, in the order and with the gates shown. A step that ran stays done; the plan can be stopped, not rewound.".into(),
        ),
        "plan_stop" => ("Stop a plan", vec![format!("stop plan {}", quote(text(input, "plan_id").unwrap_or("")))], "The steps in flight finish, then the plan's final steps run.".into()),
        "plan_retry_step" => ("Retry a plan step", vec![format!("plan {}: queue step {} again", quote(text(input, "plan_id").unwrap_or("")), quote(text(input, "step_id").unwrap_or("")))], "Puts the step back in the queue; it runs when the plan is resumed.".into()),
        "plan_skip_step" => ("Skip a plan step", vec![format!("plan {}: skip step {}", quote(text(input, "plan_id").unwrap_or("")), quote(text(input, "step_id").unwrap_or("")))], "Later phases stop waiting for it.".into()),
        "plan_release_gate" => ("Open a plan gate", vec![format!("plan {}: open the gate before phase {}", quote(text(input, "plan_id").unwrap_or("")), quote(text(input, "phase_id").unwrap_or("")))], "A paused plan resumes; a waiting gate stops waiting.".into()),
        "undo_last" => (
            "Undo Imber's last action",
            vec![match text(input, "action_id") { Some(id) => format!("undo Imber action {}", quote(id)), None => "undo Imber's last reversible action".to_string() }],
            "Reverses a commit, a branch created / switched / deleted, a stage, or a merge or pull (a recovery snapshot is taken before a hard reset). It cannot reverse a push, a merge request or a plan run.".into(),
        ),
        "schedule_create" => {
            let action = text(input, "action").unwrap_or("awake");
            let mode = text(input, "mode").unwrap_or("manual");
            (
                "Create a schedule",
                vec![format!("schedule {} \u{2014} {action}, {} ({})", quote(text(input, "name").unwrap_or("Imber job")), describe_when(input.get("when")), if mode == "auto" { "may change things unattended" } else { "read-only" })],
                "Saves the schedule; Corvus runs it at the time, if it is open. It appears in Imber \u{203a} Schedules, where it can be edited or turned off.".into(),
            )
        }
        "schedule_update" => ("Change a schedule", vec![format!("update schedule {}", quote(text(input, "id").unwrap_or("")))], "Edits or turns the schedule on or off.".into()),
        "schedule_delete" => ("Delete a schedule", vec![format!("delete schedule {}", quote(text(input, "id").unwrap_or("")))], "Removes it from Imber \u{203a} Schedules. Nothing it already did is undone.".into()),
        "git_branch_delete" => ("Delete branch", vec![format!("git branch -D {}", quote(text(input, "name").unwrap_or("")))], "Deletes the local branch. Commits only it held become unreachable (the reflog keeps them for a while).".into()),
        other => ("Unknown action", vec![other.to_string()], "This action is not one Imber knows; it is shown so you can refuse it.".into()),
    };
    Preview { title: title.into(), commands, effect }
}


use imber_schedule::prelude::describe_when;

fn text<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn flag(input: &Value, key: &str) -> bool {
    input.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn list(input: &Value, key: &str) -> Vec<String> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// An argument as a shell would need it: bare when it is plain, double-quoted otherwise.
fn quote(arg: &str) -> String {
    let plain = !arg.is_empty() && arg.chars().all(|c| c.is_alphanumeric() || "-_./:@+=,".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn quote_all(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const READS: [&str; 7] = ["git_status", "git_diff", "git_log", "git_blame", "git_show", "git_branches", "git_mr_list"];
    const WRITES: [&str; 18] = ["plan_create", "plan_add_step", "plan_remove_step", "plan_set_gate", "plan_run", "plan_stop", "plan_retry_step", "plan_skip_step", "plan_release_gate", "git_stage", "git_unstage", "git_commit", "git_fetch", "git_pull", "git_branch_create", "git_checkout", "git_merge", "git_mr_create"];

    #[test]
    fn reads_never_ask_in_either_mode() {
        for tool in READS {
            let risk = classify(tool, &json!({}));
            assert_eq!(risk, Risk::Read, "{tool}");
            assert!(!needs_approval(Mode::Manual, risk) && !needs_approval(Mode::Auto, risk), "{tool}");
        }
    }

    #[test]
    fn writes_ask_in_manual_and_run_in_auto() {
        for tool in WRITES {
            let risk = classify(tool, &json!({}));
            assert_eq!(risk, Risk::Write, "{tool}");
            assert!(needs_approval(Mode::Manual, risk), "{tool} must be proposed in manual");
            assert!(!needs_approval(Mode::Auto, risk), "{tool} runs in auto");
        }
    }

    #[test]
    fn a_plain_push_is_a_write_and_a_forced_one_is_destructive() {
        assert_eq!(classify("git_push", &json!({"remote": "origin"})), Risk::Write);
        assert_eq!(classify("git_push", &json!({"force": false})), Risk::Write);
        assert_eq!(classify("git_push", &json!({"force": true})), Risk::Destructive);
    }

    #[test]
    fn destructive_calls_ask_in_every_mode() {
        for risk in [classify("git_push", &json!({"force": true})), classify("git_branch_delete", &json!({}))] {
            assert_eq!(risk, Risk::Destructive);
            assert!(needs_approval(Mode::Manual, risk) && needs_approval(Mode::Auto, risk));
        }
    }

    #[test]
    fn a_tool_nobody_classified_is_treated_as_destructive() {
        assert_eq!(classify("git_reset_hard", &json!({})), Risk::Destructive);
        assert!(needs_approval(Mode::Auto, classify("anything_else", &json!({}))));
    }

    #[test]
    fn the_mode_word_round_trips_and_defaults_to_the_cautious_one() {
        assert_eq!(Mode::parse("auto"), Mode::Auto);
        assert_eq!(Mode::parse(" AUTO "), Mode::Auto);
        assert_eq!(Mode::parse("manual"), Mode::Manual);
        assert_eq!(Mode::parse("accept_edits"), Mode::Manual, "Bennu's words are not Corvus's");
        assert_eq!(Mode::parse(""), Mode::Manual);
        assert_eq!(Mode::parse(Mode::Auto.word()), Mode::Auto);
    }

    #[test]
    fn a_commit_preview_shows_the_exact_command_with_the_message_quoted() {
        let p = preview("git_commit", &json!({"message": "fix: handle \"empty\" cart", "paths": ["src/a b.rs", "README.md"]}));
        assert_eq!(p.commands[0], "git add -- \"src/a b.rs\" README.md");
        assert_eq!(p.commands[1], "git commit -m \"fix: handle \\\"empty\\\" cart\"");
        assert!(p.effect.contains("reset --soft"));
    }

    #[test]
    fn a_force_push_preview_says_what_it_overwrites() {
        let p = preview("git_push", &json!({"remote": "upstream", "force": true}));
        assert_eq!(p.title, "Force push");
        assert_eq!(p.commands, ["git push --force upstream HEAD"]);
        assert!(p.effect.contains("Overwrites"));
    }

    #[test]
    fn branch_and_merge_previews_name_their_arguments() {
        assert_eq!(preview("git_branch_create", &json!({"name": "feature/x", "checkout": true, "from": "main"})).commands, ["git switch -c feature/x main"]);
        assert_eq!(preview("git_branch_create", &json!({"name": "feature/x"})).commands, ["git branch feature/x"]);
        assert_eq!(preview("git_merge", &json!({"branch": "dev", "strategy": "no_ff"})).commands, ["git merge --no-ff dev"]);
        assert_eq!(preview("git_merge", &json!({"branch": "dev"})).commands, ["git merge dev"]);
    }

    #[test]
    fn an_unknown_tool_still_gets_a_card_so_it_can_be_refused() {
        let p = preview("git_reset_hard", &json!({}));
        assert_eq!(p.title, "Unknown action");
        assert_eq!(p.commands, ["git_reset_hard"]);
    }

    #[test]
    fn schedule_tools_are_classified_and_previewed() {
        assert_eq!(classify("schedule_list", &json!({})), Risk::Read);
        for t in ["schedule_create", "schedule_update", "schedule_delete"] {
            assert_eq!(classify(t, &json!({})), Risk::Write, "{t}");
            assert!(needs_approval(Mode::Manual, Risk::Write));
        }
        assert!(needs_approval_for("schedule_create", Mode::Auto, Risk::Write), "a new schedule is asked about in every mode");
        assert!(!needs_approval_for("schedule_update", Mode::Auto, Risk::Write), "editing one follows the write rule");
        let p = preview("schedule_create", &json!({"name": "Morning check", "action": "awake", "mode": "auto", "when": {"kind": "weekly", "days": ["mon", "fri"], "time": "09:00"}}));
        assert!(p.commands[0].contains("\"Morning check\"") && p.commands[0].contains("every mon, fri at 09:00") && p.commands[0].contains("may change things unattended"), "{:?}", p.commands);
        assert!(preview("schedule_create", &json!({"name": "x", "when": {"kind": "once", "at": "2026-11-05T09:00"}})).commands[0].contains("read-only"));
    }

    #[test]
    fn unattended_runs_read_always_write_only_in_auto_and_never_destroy() {
        assert!(unattended_allows(Mode::Manual, Risk::Read, "git_status").is_ok());
        assert!(unattended_allows(Mode::Auto, Risk::Read, "git_log").is_ok());
        assert!(unattended_allows(Mode::Auto, Risk::Write, "git_fetch").is_ok());
        assert!(unattended_allows(Mode::Manual, Risk::Write, "git_fetch").unwrap_err().contains("read-only"));
        for mode in [Mode::Manual, Mode::Auto] {
            assert!(unattended_allows(mode, Risk::Destructive, "git_push").unwrap_err().contains("never run unattended"));
        }
    }

    #[test]
    fn a_scheduled_run_cannot_schedule() {
        for t in ["schedule_create", "schedule_update", "schedule_delete"] {
            assert!(unattended_allows(Mode::Auto, Risk::Write, t).unwrap_err().contains("cannot create or change"), "{t}");
        }
        assert!(unattended_allows(Mode::Manual, Risk::Read, "schedule_list").is_ok(), "reading the list is fine");
    }

    #[test]
    fn plans_are_writes_to_build_and_to_run_and_reading_them_is_free() {
        for t in ["plan_list", "plan_show", "undo_list"] {
            assert_eq!(classify(t, &json!({})), Risk::Read, "{t}");
        }
        for t in ["plan_create", "plan_add_step", "plan_remove_step", "plan_set_gate", "plan_run", "plan_stop", "plan_retry_step", "plan_skip_step", "plan_release_gate"] {
            assert_eq!(classify(t, &json!({})), Risk::Write, "{t}");
        }
        assert!(needs_approval(Mode::Manual, classify("plan_run", &json!({}))), "manual proposes the run");
        assert!(!needs_approval(Mode::Auto, classify("plan_run", &json!({}))), "auto runs it");
    }

    #[test]
    fn undoing_always_asks() {
        let risk = classify("undo_last", &json!({}));
        assert_eq!(risk, Risk::Destructive);
        assert!(needs_approval(Mode::Auto, risk));
        assert!(unattended_allows(Mode::Auto, risk, "undo_last").is_err(), "never unattended");
    }

    #[test]
    fn a_plan_run_preview_names_the_plan_and_says_it_cannot_be_rewound() {
        let p = preview("plan_run", &json!({"plan_id": "plan-7"}));
        assert_eq!(p.commands, ["run plan plan-7"]);
        assert!(p.effect.contains("not rewound"));
        let u = preview("undo_last", &json!({}));
        assert!(u.effect.contains("cannot reverse a push"));
        assert!(preview("undo_last", &json!({"action_id": "act-1"})).commands[0].contains("act-1"));
    }

    #[test]
    fn a_scheduled_run_with_changes_allowed_may_build_and_run_plans() {
        assert!(unattended_allows(Mode::Auto, Risk::Write, "plan_run").is_ok());
        assert!(unattended_allows(Mode::Manual, Risk::Write, "plan_run").is_err(), "read-only schedules run nothing");
    }
}
