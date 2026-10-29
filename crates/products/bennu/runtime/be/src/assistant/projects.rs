//! The projects a conversation works across.
//!
//! Bennu keeps several projects open at once, each already indexed, so "work across projects" needs no
//! loading of its own: it needs the assistant to know which are in play, to call them by name, and to send
//! a change to the project that owns the file. This is that bookkeeping, kept pure: a short, capped list
//! (the project the user is in first), a name for each, and the two translations the tools need — a name
//! or `name:relative/path` into a real path, and a real path into the project that owns it.

use std::path::Path;

/// How many projects a conversation works in unless the user says otherwise.
pub const DEFAULT_CAP: usize = 6;
/// The most the setting may ask for: every project is a line in the instructions and a place a search may go.
pub const MAX_CAP: usize = 20;

/// The build manifests of a project are found by `bennu-project`, which the tree and the tool windows
/// share; the assistant only reads them.
pub use bennu_project::prelude::{modules_of, Module};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// What the model calls it: the folder's name, made unique when two share one.
    pub name: String,
    pub root: String,
    /// Every build manifest under the root (see [`modules_of`]). Empty for a plain folder.
    pub modules: Vec<Module>,
    /// A Cargo manifest of the project depends on Bevy.
    pub bevy: bool,
}

impl Project {
    pub fn has(&self, kind: &str) -> bool {
        self.modules.iter().any(|m| m.kind == kind)
    }

    /// Where the manifests of one kind are, relative to the root.
    pub fn dirs(&self, kind: &str) -> Vec<&str> {
        self.modules.iter().filter(|m| m.kind == kind).map(|m| m.dir.as_str()).collect()
    }

    /// What the project is, in a word or two: `Cargo + Node`, or `folder` when nothing builds it.
    pub fn kind(&self) -> String {
        let mut kinds: Vec<&str> = Vec::new();
        for m in &self.modules {
            if !kinds.contains(&m.kind) {
                kinds.push(m.kind);
            }
        }
        if kinds.is_empty() { "folder".into() } else { kinds.join(" + ") }
    }
}

fn normal(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// The projects in play: `root` first, then the other open ones in the order given, without repeats, at
/// most `cap` of them. A cap of 0 means the default.
pub fn working_set(root: &str, open: &[String], cap: usize) -> Vec<Project> {
    let cap = if cap == 0 { DEFAULT_CAP } else { cap.min(MAX_CAP) };
    let mut roots: Vec<&str> = Vec::new();
    for r in std::iter::once(root).chain(open.iter().map(String::as_str)) {
        if !r.is_empty() && !roots.iter().any(|seen| normal(seen) == normal(r)) {
            roots.push(r);
        }
        if roots.len() == cap {
            break;
        }
    }
    let mut out: Vec<Project> = Vec::new();
    for r in roots {
        let base = Path::new(r).file_name().map(|n| n.to_string_lossy().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "project".into());
        let mut name = base.clone();
        let mut n = 2;
        while out.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
            name = format!("{base}-{n}");
            n += 1;
        }
        let modules = modules_of(Path::new(r));
        let bevy = modules.iter().filter(|m| m.kind == "Cargo").any(|m| is_bevy(&Path::new(r).join(&m.dir)));
        out.push(Project { name, root: r.to_string(), modules, bevy });
    }
    out
}

/// Whether the manifest in `dir` names Bevy as a dependency. A light read — a line that starts with the crate's
/// name or its table — because the exact detection belongs to the capability scan and this only chooses a
/// sentence of the instructions.
pub fn is_bevy(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else { return false };
    text.lines().map(str::trim).any(|l| {
        l.strip_prefix("bevy").is_some_and(|rest| rest.starts_with([' ', '=', '.']))
            || l.starts_with("[dependencies.bevy]")
    })
}

/// A project's name, or `name:relative/path` inside one, as the path it stands for. `None` for anything
/// else — an ordinary path is left as it is. A single letter before the colon is a drive, not a project.
pub fn expand(projects: &[Project], raw: &str) -> Option<String> {
    let find = |name: &str| projects.iter().find(|p| p.name.eq_ignore_ascii_case(name));
    if let Some(p) = find(raw) {
        return Some(p.root.clone());
    }
    let (name, rest) = raw.split_once(':')?;
    if name.chars().count() < 2 {
        return None;
    }
    let p = find(name)?;
    let rest = rest.trim_start_matches(['/', '\\']);
    Some(format!("{}/{}", p.root.trim_end_matches(['/', '\\']), rest))
}

/// The project a file belongs to: the one whose root is the longest prefix of its path.
pub fn owner<'a>(projects: &'a [Project], file: &Path) -> Option<&'a Project> {
    let f = normal(&file.to_string_lossy());
    projects
        .iter()
        .filter(|p| {
            let r = normal(&p.root);
            f == r || f.starts_with(&format!("{r}/"))
        })
        .max_by_key(|p| p.root.len())
}

/// `name:relative/path` for a file in one of the projects, for showing where a change lands.
pub fn labelled(projects: &[Project], file: &Path) -> Option<String> {
    let p = owner(projects, file)?;
    let f = file.to_string_lossy().replace('\\', "/");
    let r = p.root.replace('\\', "/");
    let rel = f.get(r.trim_end_matches('/').len()..)?.trim_start_matches('/');
    Some(format!("{}:{rel}", p.name))
}

/// The block of the instructions that names the projects, when there is more than one.
pub fn describe(projects: &[Project]) -> String {
    if projects.len() < 2 {
        return String::new();
    }
    let mut s = String::from("\nThis conversation spans several open projects. The first is the one the user is in; call any of them by name wherever a tool takes a `root` or a path (`name` or `name:relative/path`), and use `project: \"all\"` or a name on the text search and the file finder to look across them. A change goes to the project that owns the file. Never assume the same class in two projects is the same class.\n");
    for (i, p) in projects.iter().enumerate() {
        s.push_str(&format!("- `{}` ({}){} — `{}`\n", p.name, p.kind(), if i == 0 { ", the one the user is in" } else { "" }, p.root));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_projects_in_play_start_with_the_one_the_user_is_in() {
        let set = working_set("C:/work/app", &roots(&["C:/work/lib", "C:/work/app", "c:\\work\\LIB\\"]), 6);
        assert_eq!(set.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["app", "lib"]);
    }

    #[test]
    fn the_cap_holds_and_zero_means_the_default() {
        let many: Vec<String> = (0..10).map(|i| format!("C:/w/p{i}")).collect();
        assert_eq!(working_set("C:/w/main", &many, 3).len(), 3);
        assert_eq!(working_set("C:/w/main", &many, 0).len(), DEFAULT_CAP);
        assert_eq!(working_set("C:/w/main", &many, 500).len(), MAX_CAP.min(11));
        assert_eq!(working_set("C:/w/main", &many, 3)[0].name, "main");
    }

    #[test]
    fn two_folders_with_one_name_get_distinct_names() {
        let set = working_set("C:/a/api", &roots(&["C:/b/api"]), 6);
        assert_eq!((set[0].name.as_str(), set[1].name.as_str()), ("api", "api-2"));
    }

    #[test]
    fn a_name_or_a_name_with_a_path_becomes_a_real_path() {
        let set = working_set("C:/work/app", &roots(&["C:/work/lib"]), 6);
        assert_eq!(expand(&set, "LIB").as_deref(), Some("C:/work/lib"));
        assert_eq!(expand(&set, "lib:src/A.java").as_deref(), Some("C:/work/lib/src/A.java"));
        assert_eq!(expand(&set, "app:/src/B.java").as_deref(), Some("C:/work/app/src/B.java"));
    }

    #[test]
    fn a_drive_or_an_ordinary_path_is_left_alone() {
        let set = working_set("C:/work/app", &roots(&["C:/work/lib"]), 6);
        assert_eq!(expand(&set, "C:/work/lib/src"), None);
        assert_eq!(expand(&set, "src/A.java"), None);
        assert_eq!(expand(&set, "other:src/A.java"), None);
    }

    #[test]
    fn a_file_belongs_to_the_deepest_project_that_contains_it() {
        let set = working_set("C:/work/app", &roots(&["C:/work/app/modules/core", "C:/work/lib"]), 6);
        assert_eq!(owner(&set, Path::new("C:\\work\\app\\modules\\core\\A.java")).map(|p| p.name.as_str()), Some("core"));
        assert_eq!(owner(&set, Path::new("C:/work/app/B.java")).map(|p| p.name.as_str()), Some("app"));
        assert_eq!(owner(&set, Path::new("C:/work/application/C.java")), None, "a sibling that shares a prefix is not inside");
        assert_eq!(labelled(&set, Path::new("C:/work/lib/src/X.java")).as_deref(), Some("lib:src/X.java"));
    }

    fn tree(name: &str, files: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-modules-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for f in files {
            let full = dir.join(f);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, "").unwrap();
        }
        dir
    }

    #[test]
    fn a_project_says_what_it_is_in_a_few_words() {
        let dir = tree("kind", &["package.json", "src-tauri/Cargo.toml"]);
        let set = working_set(&dir.to_string_lossy(), &[], 1);
        assert_eq!(set[0].kind(), "Node + Cargo");
        assert!(set[0].has("Cargo") && !set[0].has("Maven"));
        assert_eq!(set[0].dirs("Cargo"), ["src-tauri"]);
        let empty = tree("empty", &["README.md"]);
        assert_eq!(working_set(&empty.to_string_lossy(), &[], 1)[0].kind(), "folder");
    }

    #[test]
    fn the_instructions_name_the_projects_only_when_there_are_several() {
        assert_eq!(describe(&working_set("C:/work/app", &[], 6)), "");
        let text = describe(&working_set("C:/work/app", &roots(&["C:/work/lib"]), 6));
        assert!(text.contains("`app`") && text.contains("`lib`") && text.contains("the one the user is in"));
    }
}
