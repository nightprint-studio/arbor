//! Groups of tools the user can switch off.
//!
//! A tool the model is not offered is a tool it cannot call and a definition it does not pay for, so a
//! family switched off is the narrowest thing the user can say short of the reach: "do not touch my
//! files at all" is `edit` off, "do not run anything" is `run` off. Which family a tool belongs to is
//! decided by its name, in one place, so a tool added later lands somewhere sensible without a list to
//! keep in step.

/// A switchable group of tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Reading a file and searching text.
    Read,
    /// Bennu's own intelligence: symbols, usages, beans, endpoints, problems.
    Project,
    /// Changing files: exact edits (one or several), a replace over many files, create, delete.
    Edit,
    /// Rename and add-parameter, which change every file that mentions the thing.
    Refactor,
    /// Anything that builds, runs or tests the project's code.
    Run,
    /// Reading and restoring Local History.
    History,
    /// Structural search and replace.
    Structural,
    /// Running the project's services and driving them: start/stop, logs, loopback HTTP, the timeline, scenarios.
    Live,
    /// Debugging a Java program: breakpoints, launch or attach, stop, stack, values, step.
    Debug,
    /// Changing the program under the debugger: throw an exception, return early, drop a frame, write a value. Hidden
    /// unless the user turned it on, and asks once per conversation even then.
    DebugMutate,
    /// Reading a running Bevy game over BRP: connect, query, get, resources, types.
    Bevy,
    /// Changing a running Bevy game: mutate, insert, remove, spawn, despawn. Hidden unless the user turned it on.
    BevyWrite,
    /// Looking at a page of a front end on this machine: a screenshot, and the page's console and failed requests.
    Screenshot,
    /// Starting sub-agents (`spawn_agent`, `await_agents`). Hidden unless sub-agents are on, and for the agents themselves.
    Agents,
    /// Talking to the other conversations about a file (`file_owner`, `request_change`). Hidden from sub-agents.
    Peers,
}

impl Family {
    pub const ALL: [Family; 15] = [
        Family::Read, Family::Project, Family::Edit, Family::Refactor, Family::Run, Family::History, Family::Structural, Family::Live, Family::Debug, Family::DebugMutate,
        Family::Bevy, Family::BevyWrite, Family::Screenshot, Family::Agents, Family::Peers,
    ];

    /// The name it has in settings and in the run's arguments.
    pub fn id(self) -> &'static str {
        match self {
            Family::Read => "read",
            Family::Project => "project",
            Family::Edit => "edit",
            Family::Refactor => "refactor",
            Family::Run => "run",
            Family::History => "history",
            Family::Structural => "structural",
            Family::Live => "live",
            Family::Debug => "debug",
            Family::DebugMutate => "debug_mutate",
            Family::Bevy => "bevy",
            Family::BevyWrite => "bevy_write",
            Family::Screenshot => "screenshot",
            Family::Agents => "agents",
            Family::Peers => "peers",
        }
    }

    pub fn parse(id: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.id() == id)
    }
}

/// The family of a tool, or `None` for the ones that are always there: asking the user a question, the
/// help about Bennu, loading a skill the user enabled, the plan (which has its own switch) and git (which has its own, and is off by default).
pub fn family_of(tool: &str) -> Option<Family> {
    match tool {
        "ask_user" | "bennu_help" | "update_plan" | "open_ui" | "load_skill" | "show_diagram" => None,
        "search_text" | "find_files" | "bennu_read_file" | "read_files" | "file_outline" => Some(Family::Read),
        // The way to get the rest of a digest: it follows whichever tool produced one, so it is never switched off alone.
        "expand_result" => None,
        "edit_file" | "edit_files" | "replace_in_files" | "create_file" | "delete_file" => Some(Family::Edit),
        "rename_symbol" | "add_parameter" => Some(Family::Refactor),
        "local_history" => Some(Family::History),
        "structural_replace" => Some(Family::Structural),
        other if super::live::is_live_tool(other) => Some(Family::Live),
        other if super::debug::more::is_mutate_tool(other) => Some(Family::DebugMutate),
        other if super::debug::is_debug_tool(other) => Some(Family::Debug),
        other if super::brp::is_brp_write(other) => Some(Family::BevyWrite),
        other if super::brp::is_brp_read(other) => Some(Family::Bevy),
        other if super::screenshot::is_screenshot_tool(other) => Some(Family::Screenshot),
        "spawn_agent" | "await_agents" => Some(Family::Agents),
        "file_owner" | "request_change" => Some(Family::Peers),
        other if other.starts_with("corvus_") => None,
        // What executes the project's code. By name, not by a word in it: bennu_tests only lists the tests.
        "bennu_build" | "bennu_test_run" | "bennu_npm_run_script" | "bennu_cargo_run" => Some(Family::Run),
        other if other.starts_with("bennu_run_") || other.starts_with("bennu_debug") || other.starts_with("bennu_launch") => Some(Family::Run),
        _ => Some(Family::Project),
    }
}

/// The families the user has switched off.
#[derive(Debug, Clone, Default)]
pub struct Off(Vec<Family>);

impl Off {
    /// From the ids in settings; one nobody knows is ignored rather than failing the run.
    pub fn from_ids(ids: &[String]) -> Self {
        Off(ids.iter().filter_map(|i| Family::parse(i)).collect())
    }

    pub fn hides(&self, tool: &str) -> bool {
        family_of(tool).is_some_and(|f| self.0.contains(&f))
    }

    /// This filter with more families switched off (by id; unknown ids are ignored).
    pub fn with(&self, ids: &[&str]) -> Off {
        let mut all = self.0.clone();
        all.extend(ids.iter().filter_map(|i| Family::parse(i)));
        Off(all)
    }

    pub fn is_off(&self, family: Family) -> bool {
        self.0.contains(&family)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_round_trips_through_its_id() {
        for f in Family::ALL {
            assert_eq!(Family::parse(f.id()), Some(f));
        }
        assert_eq!(Family::parse("shell"), None);
    }

    #[test]
    fn the_assistants_own_tools_are_sorted_into_families() {
        assert_eq!(family_of("edit_files"), Some(Family::Edit));
        assert_eq!(family_of("replace_in_files"), Some(Family::Edit));
        assert_eq!(family_of("rename_symbol"), Some(Family::Refactor));
        assert_eq!(family_of("search_text"), Some(Family::Read));
        assert_eq!(family_of("local_history"), Some(Family::History));
        assert_eq!(family_of("structural_replace"), Some(Family::Structural));
        assert_eq!(family_of("run_start"), Some(Family::Live));
        assert_eq!(family_of("http_request"), Some(Family::Live));
        assert_eq!(family_of("timeline"), Some(Family::Live));
        assert_eq!(family_of("debug_launch"), Some(Family::Debug));
        assert_eq!(family_of("debug_evaluate"), Some(Family::Debug));
    }

    #[test]
    fn switching_debug_off_hides_the_debugger_and_nothing_else() {
        let off = Off::from_ids(&["debug".to_string()]);
        assert!(off.hides("debug_set_breakpoint") && off.hides("debug_wait_stop"));
        assert!(!off.hides("run_start") && !off.hides("bennu_build"));
    }

    #[test]
    fn reading_a_game_writing_to_it_and_looking_at_a_page_are_three_switches() {
        assert_eq!(family_of("brp_query"), Some(Family::Bevy));
        assert_eq!(family_of("brp_connect"), Some(Family::Bevy));
        assert_eq!(family_of("brp_mutate"), Some(Family::BevyWrite));
        assert_eq!(family_of("brp_despawn"), Some(Family::BevyWrite));
        assert_eq!(family_of("screenshot_page"), Some(Family::Screenshot));
        assert_eq!(family_of("page_diagnostics"), Some(Family::Screenshot));
        let write_off = Off::from_ids(&["bevy_write".to_string()]);
        assert!(write_off.hides("brp_spawn") && !write_off.hides("brp_get") && !write_off.hides("screenshot_page"));
        let read_off = Off::from_ids(&["bevy".to_string()]);
        assert!(read_off.hides("brp_types") && !read_off.hides("brp_set_resource"));
    }

    #[test]
    fn a_catalogue_tool_is_project_intelligence_unless_it_runs_something() {
        assert_eq!(family_of("bennu_find_usages"), Some(Family::Project));
        assert_eq!(family_of("bennu_test_run"), Some(Family::Run));
        assert_eq!(family_of("bennu_build"), Some(Family::Run));
        assert_eq!(family_of("bennu_tests"), Some(Family::Project), "listing the tests runs nothing");
    }

    #[test]
    fn the_questions_the_help_the_plan_and_git_are_never_switched_off_here() {
        for t in ["ask_user", "bennu_help", "update_plan", "open_ui", "load_skill", "show_diagram", "corvus_commit"] {
            assert_eq!(family_of(t), None);
            assert!(!Off::from_ids(&Family::ALL.iter().map(|f| f.id().to_string()).collect::<Vec<_>>()).hides(t));
        }
    }

    #[test]
    fn a_family_switched_off_hides_its_tools_and_only_those() {
        let off = Off::from_ids(&["edit".to_string(), "nonsense".to_string()]);
        assert!(off.hides("edit_file") && off.hides("delete_file"));
        assert!(!off.hides("search_text") && !off.hides("rename_symbol"));
        assert!(off.is_off(Family::Edit) && !off.is_off(Family::Run));
        let live_off = Off::from_ids(&["live".to_string()]);
        assert!(live_off.hides("run_start") && live_off.hides("wait_ready") && !live_off.hides("bennu_build"));
    }
}
