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
}

impl Family {
    pub const ALL: [Family; 7] =
        [Family::Read, Family::Project, Family::Edit, Family::Refactor, Family::Run, Family::History, Family::Structural];

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
        }
    }

    pub fn parse(id: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.id() == id)
    }
}

/// The family of a tool, or `None` for the ones that are always there: asking the user a question, the
/// help about Bennu, the plan (which has its own switch) and git (which has its own, and is off by default).
pub fn family_of(tool: &str) -> Option<Family> {
    match tool {
        "ask_user" | "bennu_help" | "update_plan" | "open_ui" => None,
        "search_text" | "find_files" | "bennu_read_file" => Some(Family::Read),
        "edit_file" | "edit_files" | "replace_in_files" | "create_file" | "delete_file" => Some(Family::Edit),
        "rename_symbol" | "add_parameter" => Some(Family::Refactor),
        "local_history" => Some(Family::History),
        "structural_replace" => Some(Family::Structural),
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
        for t in ["ask_user", "bennu_help", "update_plan", "open_ui", "corvus_commit"] {
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
    }
}
