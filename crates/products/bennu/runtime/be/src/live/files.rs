//! Where a project keeps its recipes and scenarios, and reading them.
//!
//! `<root>/.arbor/bennu/live/recipes.toml` and `<root>/.arbor/bennu/live/scenarios/<name>.toml`, next to
//! the project's other Bennu settings (see `repo_config`). Plain files the user can read, edit and
//! commit; the assistant writes them through the normal edit door, so review mode applies.

use std::path::{Path, PathBuf};

use bennu_live::prelude::{content_hash, parse_recipes, parse_scenario, RecipeFile, Scenario};

pub(crate) fn live_dir(root: &str) -> PathBuf {
    Path::new(root).join(".arbor").join("bennu").join("live")
}

pub(crate) fn recipes_path(root: &str) -> PathBuf {
    live_dir(root).join("recipes.toml")
}

pub(crate) fn scenarios_dir(root: &str) -> PathBuf {
    live_dir(root).join("scenarios")
}

/// The file for a scenario of this name, refusing a name that is not a plain file stem.
pub(crate) fn scenario_path(root: &str, name: &str) -> Result<PathBuf, String> {
    let ok = !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) && !name.starts_with('.');
    if !ok {
        return Err(format!("`{name}` is not a valid scenario name: use letters, digits, `-`, `_` and `.` (no path)"));
    }
    Ok(scenarios_dir(root).join(format!("{name}.toml")))
}

/// What reading the recipes file found.
pub(crate) enum Recipes {
    /// No file: a project that never used recipes.
    None,
    Valid(RecipeFile),
    Invalid { errors: Vec<String>, hash: String },
}

pub(crate) fn read_recipes(root: &str) -> Recipes {
    let path = recipes_path(root);
    let Ok(text) = std::fs::read_to_string(&path) else { return Recipes::None };
    match parse_recipes(&text) {
        Ok(file) => Recipes::Valid(file),
        Err(errors) => Recipes::Invalid { errors, hash: content_hash(&text) },
    }
}

/// One scenario file, valid or not.
pub(crate) struct ScenarioEntry {
    /// The file stem: what `run_scenario` is given.
    pub file: String,
    pub hash: String,
    pub scenario: Result<Scenario, Vec<String>>,
}

pub(crate) fn read_scenarios(root: &str) -> Vec<ScenarioEntry> {
    let Ok(dir) = std::fs::read_dir(scenarios_dir(root)) else { return Vec::new() };
    let mut out: Vec<ScenarioEntry> = dir
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .filter_map(|e| {
            let path = e.path();
            let file = path.file_stem()?.to_string_lossy().to_string();
            let text = std::fs::read_to_string(&path).ok()?;
            Some(ScenarioEntry { hash: content_hash(&text), scenario: parse_scenario(&text), file })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

pub(crate) fn read_scenario(root: &str, name: &str) -> Result<ScenarioEntry, String> {
    let path = scenario_path(root, name)?;
    let text = std::fs::read_to_string(&path).map_err(|_| format!("no scenario `{name}` (looked for {})", path.display()))?;
    Ok(ScenarioEntry { hash: content_hash(&text), scenario: parse_scenario(&text), file: name.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_name_is_a_plain_file_stem() {
        assert!(scenario_path("/p", "order-flow.v2").is_ok());
        for bad in ["", "../x", "a/b", "a\\b", ".hidden", "with space", &"x".repeat(80)] {
            assert!(scenario_path("/p", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn files_are_read_from_the_projects_live_folder() {
        let dir = std::env::temp_dir().join(format!("bennu-live-files-{}", std::process::id()));
        let root = dir.to_string_lossy().to_string();
        assert!(matches!(read_recipes(&root), Recipes::None));
        std::fs::create_dir_all(scenarios_dir(&root)).unwrap();
        std::fs::write(recipes_path(&root), "[[recipe]]\nname=\"a\"\n[[recipe.service]]\nname=\"s\"\ncommand=\"c\"\n").unwrap();
        std::fs::write(scenarios_dir(&root).join("one.toml"), "name=\"one\"\n[[step]]\ntype=\"wait\"\nms=5\n").unwrap();
        std::fs::write(scenarios_dir(&root).join("bad.toml"), "name=\"bad\"\n").unwrap();
        assert!(matches!(read_recipes(&root), Recipes::Valid(f) if f.recipes.len() == 1));
        let all = read_scenarios(&root);
        assert_eq!(all.iter().map(|s| s.file.as_str()).collect::<Vec<_>>(), vec!["bad", "one"]);
        assert!(all[0].scenario.is_err() && all[1].scenario.is_ok());
        assert!(read_scenario(&root, "missing").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
