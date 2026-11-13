//! What a run with nobody there may do — the table.
//!
//! A scheduled run has no one to approve, answer or undo anything, and Bennu has more power to misuse than a git
//! client (it builds, runs, debugs and edits many files). So the rule is not "ask less" but "never reach what
//! needs a person": the live project is never written, commands and processes never start, nothing is deleted,
//! no schedule is created, and a question cannot be put.
//!
//! Pure on purpose: the glue (`tools::gate`) names a call's [`Kind`]; this decides. A table is what a test can
//! pin down, and a kind added later has to be placed in it.

/// What a tool call amounts to, as far as an unattended run is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Reads or searches the project, its index or its history; touches nothing.
    Read,
    /// An edit. The run is in manual mode, so it becomes a proposal card; nothing is written.
    Propose,
    /// A question for the user.
    Question,
    /// Deleting files.
    Delete,
    /// Starting, driving or looking into a live process: services, a debugger, a profiler, a game, a browser.
    Process,
    /// The project's own build or test command.
    OwnCommand,
    /// A catalogue tool that changes things.
    CatalogWrite,
    /// A catalogue tool that is destructive or runs project code, and is not the project's own command.
    CatalogDestructive,
    /// Starting a sub-agent.
    Agent,
    /// `schedule_list`.
    ScheduleList,
    /// `schedule_create`, `schedule_update`, `schedule_delete`.
    ScheduleChange,
}

pub const ALL_KINDS: [Kind; 11] = [
    Kind::Read, Kind::Propose, Kind::Question, Kind::Delete, Kind::Process, Kind::OwnCommand, Kind::CatalogWrite, Kind::CatalogDestructive, Kind::Agent,
    Kind::ScheduleList, Kind::ScheduleChange,
];

/// Whether an unattended run may make a call of this kind, and what the model is told when it may not. The
/// message is written to be acted on: it says what to do instead.
pub fn verdict(kind: Kind) -> Result<(), &'static str> {
    match kind {
        Kind::Read | Kind::Propose | Kind::ScheduleList => Ok(()),
        Kind::Question => Err("This is a scheduled run: nobody is there to answer. Decide conservatively, or say in your answer that this needs a person."),
        Kind::Delete => Err("A scheduled run never deletes files. Say in your answer which files you would have removed."),
        Kind::Process => Err("A scheduled run does not start, drive or debug processes. Work from the code, or say what would need running."),
        Kind::OwnCommand => Err("A scheduled run does not run the project's build or tests unless the schedule allows it, and this one does not. Say what you would have run."),
        Kind::CatalogWrite | Kind::CatalogDestructive => Err("A scheduled run does not change the project or run project code. Describe the change as a proposal instead."),
        Kind::Agent => Err("A scheduled run does not start sub-agents. Do the work yourself."),
        Kind::ScheduleChange => Err("A scheduled run cannot create or change schedules. Say so in your answer if one is needed."),
    }
}

/// The tool families a scheduled run is not even offered: the tools are not described to the model and so cost
/// nothing, and the model does not waste a turn on a call the gate would refuse. The gate remains the rule.
pub const FAMILIES_OFF: [&str; 10] = ["run", "live", "debug", "debug_mutate", "bevy", "bevy_write", "screenshot", "agents", "peers", "profile"];

/// The outcome line of a run that stopped at a question: what it needed a person for.
pub fn needs_a_person(input: &serde_json::Value) -> String {
    let first = input.get("questions").and_then(|q| q.get(0));
    let said = |k: &str| first.and_then(|q| q.get(k)).and_then(serde_json::Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let what = said("text").or_else(|| said("header")).unwrap_or("a decision");
    let mut line: String = what.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
    if what.chars().count() > 160 {
        line.push('…');
    }
    format!("Needs a person: {line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unattended_run_reads_and_proposes_and_nothing_else_gets_through() {
        let allowed: Vec<Kind> = ALL_KINDS.into_iter().filter(|k| verdict(*k).is_ok()).collect();
        assert_eq!(allowed, [Kind::Read, Kind::Propose, Kind::ScheduleList]);
    }

    #[test]
    fn every_refusal_says_what_to_do_instead() {
        for kind in ALL_KINDS {
            if let Err(why) = verdict(kind) {
                assert!(why.len() > 40 && why.ends_with('.'), "{kind:?}: {why}");
                assert!(why.contains("scheduled run") || why.contains("Scheduled run") || why.contains("A scheduled"), "{kind:?} names the situation: {why}");
            }
        }
    }

    #[test]
    fn a_question_is_refused_in_words_that_let_the_run_finish_or_stop_cleanly() {
        let why = verdict(Kind::Question).unwrap_err();
        assert!(why.contains("nobody is there") && why.contains("needs a person"));
    }

    #[test]
    fn the_families_a_scheduled_run_is_not_offered_hide_the_right_tools() {
        let off = crate::assistant::tools::families_off(&FAMILIES_OFF.map(String::from));
        for hidden in ["bennu_check", "bennu_build", "bennu_test_run", "run_start", "cargo_run", "run_tool", "run_script", "spawn_agent"] {
            assert!(off.hides(hidden), "{hidden}");
        }
        for offered in ["edit_file", "search_text", "bennu_read_file", "ask_user"] {
            assert!(!off.hides(offered), "{offered}");
        }
    }

    #[test]
    fn the_outcome_of_a_run_stopped_by_a_question_names_the_question() {
        use serde_json::json;
        assert_eq!(needs_a_person(&json!({"questions": [{"header": "Scope", "text": "Rename   only\nthe public API?"}]})), "Needs a person: Rename only the public API?");
        assert_eq!(needs_a_person(&json!({"questions": [{"header": "Scope"}]})), "Needs a person: Scope");
        assert_eq!(needs_a_person(&json!({})), "Needs a person: a decision");
        assert!(needs_a_person(&json!({"questions": [{"text": "x".repeat(300)}]})).ends_with('…'));
    }
}
