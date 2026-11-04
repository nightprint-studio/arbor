//! The window's own preferences: workspaces (named groups of projects, like Bennu's and Corvus's), which one
//! is active, and which project the window reopens on. Per profile, in `profiles/<p>/hirundo/config.toml`
//! (never in the project: it is about this machine, not the repo).

use std::path::PathBuf;

use hirundo_core::prelude::HirundoState;
use serde::{Deserialize, Serialize};

/// Most recent projects remembered.
const MAX_RECENTS: usize = 12;
/// Colours a workspace can have: the theme's `--ws-color-N` tokens.
const COLOR_COUNT: u8 = 8;
/// The workspace a project lands in when none exists yet.
const DEFAULT_WORKSPACE: &str = "Projects";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color_idx: u8,
    #[serde(default)]
    pub projects: Vec<String>,
}

// Field order matters for TOML: scalars and value arrays before the array of tables.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HirundoConfig {
    /// The project the window reopens on; empty for none.
    pub last_project: String,
    pub active_workspace: String,
    pub recent_projects: Vec<String>,
    /// The Bennu projects whose endpoints "Detected from code" lists (empty: just the project open here).
    pub detected_roots: Vec<String>,
    pub workspaces: Vec<Workspace>,
}

fn config_path() -> PathBuf {
    arbor_core::prelude::product_path("hirundo", "config.toml")
}

fn load() -> HirundoConfig {
    std::fs::read_to_string(config_path()).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
}

fn save(cfg: &HirundoConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, toml::to_string_pretty(cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    norm(a) == norm(b)
}

fn new_id(cfg: &HirundoConfig) -> String {
    let mut n = cfg.workspaces.len() + 1;
    loop {
        let id = format!("ws{n}");
        if cfg.workspaces.iter().all(|w| w.id != id) {
            return id;
        }
        n += 1;
    }
}

/// `project` was opened: first of the recents, the one to reopen on, and a member of the active workspace
/// (created if there is none, so a project always has somewhere to be).
fn remember(cfg: &mut HirundoConfig, project: &str) {
    cfg.recent_projects.retain(|p| !same_path(p, project));
    cfg.recent_projects.insert(0, project.to_string());
    cfg.recent_projects.truncate(MAX_RECENTS);
    cfg.last_project = project.to_string();

    if cfg.workspaces.is_empty() {
        let id = new_id(cfg);
        cfg.workspaces.push(Workspace { id: id.clone(), name: DEFAULT_WORKSPACE.into(), color_idx: 0, projects: Vec::new() });
        cfg.active_workspace = id;
    }
    if !cfg.workspaces.iter().any(|w| w.id == cfg.active_workspace) {
        cfg.active_workspace = cfg.workspaces[0].id.clone();
    }
    // Already in some workspace: opening it there switches to that workspace rather than copying it.
    if let Some(owner) = cfg.workspaces.iter().find(|w| w.projects.iter().any(|p| same_path(p, project))) {
        cfg.active_workspace = owner.id.clone();
        return;
    }
    let active = cfg.active_workspace.clone();
    if let Some(w) = cfg.workspaces.iter_mut().find(|w| w.id == active) {
        w.projects.push(project.to_string());
    }
}

/// Move a workspace to a gap of the list **as it is now** (dragged item included): the board's drop position.
fn move_workspace(cfg: &mut HirundoConfig, id: &str, gap: usize) -> Result<(), String> {
    let from = cfg.workspaces.iter().position(|w| w.id == id).ok_or_else(|| format!("no workspace '{id}'"))?;
    let item = cfg.workspaces.remove(from);
    let at = if gap > from { gap - 1 } else { gap }.min(cfg.workspaces.len());
    cfg.workspaces.insert(at, item);
    Ok(())
}

/// Move a project to a gap of the target workspace's list as it is now; within one workspace that reorders it.
/// A project already in the target (by path) is just taken out of the source.
fn move_project(cfg: &mut HirundoConfig, from_id: &str, root: &str, to_id: &str, gap: usize) -> Result<(), String> {
    if !cfg.workspaces.iter().any(|w| w.id == to_id) {
        return Err(format!("no workspace '{to_id}'"));
    }
    let source = workspace_mut(cfg, from_id)?;
    let pos = source.projects.iter().position(|p| same_path(p, root)).ok_or("that project is not in the workspace")?;
    let project = source.projects.remove(pos);
    let target = workspace_mut(cfg, to_id)?;
    if target.projects.iter().any(|p| same_path(p, root)) {
        return Ok(());
    }
    let at = if from_id == to_id && gap > pos { gap - 1 } else { gap }.min(target.projects.len());
    target.projects.insert(at, project);
    Ok(())
}

fn workspace_mut<'a>(cfg: &'a mut HirundoConfig, id: &str) -> Result<&'a mut Workspace, String> {
    cfg.workspaces.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("no workspace '{id}'"))
}

#[arbor_rpc::handler]
fn get_hirundo_config(_state: &HirundoState) -> Result<HirundoConfig, String> {
    Ok(load())
}

/// The window opened `project`.
#[arbor_rpc::handler]
fn hirundo_project_opened(_state: &HirundoState, project: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    remember(&mut cfg, &project);
    save(&cfg)?;
    Ok(cfg)
}

/// Forget a project everywhere (it was moved or deleted).
#[arbor_rpc::handler]
fn hirundo_project_forget(_state: &HirundoState, project: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    cfg.recent_projects.retain(|p| !same_path(p, &project));
    for w in &mut cfg.workspaces {
        w.projects.retain(|p| !same_path(p, &project));
    }
    if same_path(&cfg.last_project, &project) {
        cfg.last_project.clear();
    }
    save(&cfg)?;
    Ok(cfg)
}

/// Which Bennu projects "Detected from code" looks at.
#[arbor_rpc::handler]
fn hirundo_detected_roots_set(_state: &HirundoState, roots: Vec<String>) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    cfg.detected_roots = roots;
    save(&cfg)?;
    Ok(cfg)
}

#[arbor_rpc::handler]
fn hirundo_workspace_create(_state: &HirundoState, name: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    let id = new_id(&cfg);
    let color_idx = (cfg.workspaces.len() as u8) % COLOR_COUNT;
    cfg.workspaces.push(Workspace { id: id.clone(), name: if name.trim().is_empty() { "New workspace".into() } else { name }, color_idx, projects: Vec::new() });
    cfg.active_workspace = id;
    save(&cfg)?;
    Ok(cfg)
}

#[arbor_rpc::handler]
fn hirundo_workspace_update(_state: &HirundoState, id: String, name: String, color_idx: u8) -> Result<HirundoConfig, String> {
    if name.trim().is_empty() {
        return Err("the workspace needs a name".into());
    }
    let mut cfg = load();
    let w = workspace_mut(&mut cfg, &id)?;
    w.name = name;
    w.color_idx = color_idx.min(COLOR_COUNT - 1);
    save(&cfg)?;
    Ok(cfg)
}

#[arbor_rpc::handler]
fn hirundo_workspace_delete(_state: &HirundoState, id: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    cfg.workspaces.retain(|w| w.id != id);
    if cfg.active_workspace == id {
        cfg.active_workspace = cfg.workspaces.first().map(|w| w.id.clone()).unwrap_or_default();
    }
    save(&cfg)?;
    Ok(cfg)
}

#[arbor_rpc::handler]
fn hirundo_workspace_switch(_state: &HirundoState, id: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    workspace_mut(&mut cfg, &id)?;
    cfg.active_workspace = id;
    save(&cfg)?;
    Ok(cfg)
}

/// Reorder the workspaces: `gap` is the drop position in the list as it is now.
#[arbor_rpc::handler]
fn hirundo_workspace_move(_state: &HirundoState, id: String, gap: usize) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    move_workspace(&mut cfg, &id, gap)?;
    save(&cfg)?;
    Ok(cfg)
}

/// Move a project between workspaces, or reorder it inside one.
#[arbor_rpc::handler]
fn hirundo_workspace_move_project(_state: &HirundoState, from_id: String, project: String, to_id: String, gap: usize) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    move_project(&mut cfg, &from_id, &project, &to_id, gap)?;
    save(&cfg)?;
    Ok(cfg)
}

/// A new project: a folder with its `.hirundo/` inside. `name` empty uses `location` itself (turn an existing
/// folder into a project); otherwise `location/name` is created. Returns the project's path.
#[arbor_rpc::handler]
fn hirundo_project_create(_state: &HirundoState, location: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.contains(['/', '\\']) || name == ".." || name == "." {
        return Err("the project name cannot contain a path separator".into());
    }
    let dir = if name.is_empty() { PathBuf::from(&location) } else { PathBuf::from(&location).join(name) };
    if name.is_empty() && !dir.is_dir() {
        return Err(format!("'{location}' is not a folder"));
    }
    if !name.is_empty() && !PathBuf::from(&location).is_dir() {
        return Err(format!("'{location}' is not a folder"));
    }
    std::fs::create_dir_all(dir.join(hirundo_core::prelude::DIR).join("collections")).map_err(|e| format!("could not create the project: {e}"))?;
    Ok(dir.to_string_lossy().into_owned())
}

#[arbor_rpc::handler]
fn hirundo_workspace_add_project(_state: &HirundoState, id: String, project: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    let w = workspace_mut(&mut cfg, &id)?;
    if !w.projects.iter().any(|p| same_path(p, &project)) {
        w.projects.push(project);
    }
    save(&cfg)?;
    Ok(cfg)
}

#[arbor_rpc::handler]
fn hirundo_workspace_remove_project(_state: &HirundoState, id: String, project: String) -> Result<HirundoConfig, String> {
    let mut cfg = load();
    let w = workspace_mut(&mut cfg, &id)?;
    w.projects.retain(|p| !same_path(p, &project));
    save(&cfg)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_remembered_once_and_first() {
        let mut cfg = HirundoConfig::default();
        remember(&mut cfg, "/a");
        remember(&mut cfg, "/b");
        remember(&mut cfg, "/a");
        assert_eq!(cfg.recent_projects, ["/a", "/b"]);
        assert_eq!(cfg.last_project, "/a");
    }

    #[test]
    fn the_recents_are_capped() {
        let mut cfg = HirundoConfig::default();
        for i in 0..30 {
            remember(&mut cfg, &format!("/p{i}"));
        }
        assert_eq!(cfg.recent_projects.len(), MAX_RECENTS);
        assert_eq!(cfg.recent_projects[0], "/p29");
    }

    #[test]
    fn the_first_project_creates_a_workspace_to_live_in() {
        let mut cfg = HirundoConfig::default();
        remember(&mut cfg, "/a");
        assert_eq!(cfg.workspaces.len(), 1);
        assert_eq!(cfg.workspaces[0].projects, ["/a"]);
        assert_eq!(cfg.active_workspace, cfg.workspaces[0].id);
    }

    #[test]
    fn opening_a_project_of_another_workspace_switches_to_it_without_copying() {
        let mut cfg = HirundoConfig::default();
        remember(&mut cfg, "/a");
        cfg.workspaces.push(Workspace { id: "ws9".into(), name: "Other".into(), color_idx: 1, projects: vec!["/b".into()] });
        remember(&mut cfg, "/B/");
        assert_eq!(cfg.active_workspace, "ws9");
        assert_eq!(cfg.workspaces.iter().map(|w| w.projects.len()).sum::<usize>(), 2, "no duplicate");
    }

    fn config_with(projects: &[&[&str]]) -> HirundoConfig {
        HirundoConfig {
            workspaces: projects
                .iter()
                .enumerate()
                .map(|(i, ps)| Workspace { id: format!("w{i}"), name: format!("W{i}"), color_idx: 0, projects: ps.iter().map(|p| p.to_string()).collect() })
                .collect(),
            ..HirundoConfig::default()
        }
    }

    #[test]
    fn workspaces_reorder_by_gap_of_the_current_list() {
        let mut cfg = config_with(&[&[], &[], &[]]);
        move_workspace(&mut cfg, "w0", 3).unwrap(); // after the last
        assert_eq!(cfg.workspaces.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["w1", "w2", "w0"]);
        move_workspace(&mut cfg, "w0", 0).unwrap(); // before the first
        assert_eq!(cfg.workspaces[0].id, "w0");
        move_workspace(&mut cfg, "w0", 1).unwrap(); // its own gap: nothing moves
        assert_eq!(cfg.workspaces.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["w0", "w1", "w2"]);
        assert!(move_workspace(&mut cfg, "nope", 0).is_err());
    }

    #[test]
    fn a_project_reorders_inside_a_workspace_and_moves_between_them() {
        let mut cfg = config_with(&[&["/a", "/b", "/c"], &["/z"]]);
        move_project(&mut cfg, "w0", "/a", "w0", 3).unwrap();
        assert_eq!(cfg.workspaces[0].projects, ["/b", "/c", "/a"]);
        move_project(&mut cfg, "w0", "/c", "w1", 0).unwrap();
        assert_eq!((cfg.workspaces[0].projects.clone(), cfg.workspaces[1].projects.clone()), (vec!["/b".to_string(), "/a".to_string()], vec!["/c".to_string(), "/z".to_string()]));
        assert!(move_project(&mut cfg, "w0", "/missing", "w1", 0).is_err());
        assert!(move_project(&mut cfg, "w0", "/b", "nope", 0).is_err());
    }

    #[test]
    fn moving_a_project_to_a_workspace_that_has_it_does_not_duplicate_it() {
        let mut cfg = config_with(&[&["/a"], &["/A/"]]);
        move_project(&mut cfg, "w0", "/a", "w1", 0).unwrap();
        assert!(cfg.workspaces[0].projects.is_empty());
        assert_eq!(cfg.workspaces[1].projects.len(), 1);
    }

    #[test]
    fn paths_compare_loosely() {
        assert!(same_path("C:\\Work\\Api\\", "c:/work/api"));
        assert!(!same_path("/a", "/b"));
    }

    #[test]
    fn the_config_round_trips_through_toml() {
        let mut cfg = HirundoConfig::default();
        remember(&mut cfg, "/a");
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: HirundoConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.workspaces, cfg.workspaces);
        assert_eq!(back.recent_projects, cfg.recent_projects);
    }
}
