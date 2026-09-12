//! Where templates are kept: one directory per kind, one `<name>.<extension>.jinja` file per template.
//!
//! The name is what a template is chosen by — and, for an abbreviation, what is typed — and the
//! extension is the language it generates, which is also what the editor colours it as. A name
//! therefore has no dots in it: the first dot is where the extension starts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Directives;
use crate::kind::{Builtin, TemplateKind};

pub const TEMPLATE_EXTENSION: &str = ".jinja";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateOrigin {
    Builtin,
    Global,
}

/// A template as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    /// What it generates (`java`, `yml`); empty when it does not say.
    pub extension: String,
    pub origin: TemplateOrigin,
    /// Only a starting point to copy — see [`Builtin::starter`].
    pub starter: bool,
    /// Its `bennu.description`.
    pub description: Option<String>,
    /// Its `bennu.requires`: what a project needs for it to be offered.
    pub requires: Vec<String>,
    /// Its `bennu.abbrev`, for an abbreviation whose trigger word is not its file name.
    pub abbrev: Option<String>,
    /// The first of them the project at hand does not meet — filled in by whoever knows the project.
    pub unmet: Option<String>,
    /// The file, for a template that has one.
    pub path: Option<String>,
}

/// A template's text, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub extension: String,
    pub text: String,
    pub path: Option<PathBuf>,
}

pub fn kind_dir(root: &Path, kind: TemplateKind) -> PathBuf {
    root.join(kind.id())
}

/// `Repository.java.jinja` → `("Repository", "java")`; `notes.jinja` → `("notes", "")`.
pub fn split_file_name(file: &str) -> Option<(String, String)> {
    let stem = file.strip_suffix(TEMPLATE_EXTENSION)?;
    let (name, extension) = match stem.split_once('.') {
        Some((name, extension)) => (name, extension),
        None => (stem, ""),
    };
    (!name.is_empty()).then(|| (name.to_string(), extension.to_string()))
}

pub fn template_file_name(name: &str, extension: &str) -> String {
    match extension.is_empty() {
        true => format!("{name}{TEMPLATE_EXTENSION}"),
        false => format!("{name}.{extension}{TEMPLATE_EXTENSION}"),
    }
}

/// The templates of a kind on disk, by name.
fn on_disk(root: &Path, kind: TemplateKind) -> Vec<(String, String, PathBuf)> {
    let mut found: Vec<(String, String, PathBuf)> = std::fs::read_dir(kind_dir(root, kind))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let (name, extension) = split_file_name(&file)?;
            Some((name, extension, entry.path()))
        })
        .collect();
    found.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    found
}

/// The built-in templates of a kind, then the ones on disk. A file named like a built-in is not
/// listed: the built-in is the one that is used.
pub fn list_templates(root: &Path, kind: TemplateKind, builtins: &[Builtin]) -> Vec<TemplateInfo> {
    let mut out: Vec<TemplateInfo> = builtins
        .iter()
        .map(|b| TemplateInfo {
            name: b.name.to_string(),
            extension: b.extension.to_string(),
            origin: TemplateOrigin::Builtin,
            starter: b.starter,
            description: Directives::read(b.text).get("description").map(str::to_string),
            requires: Directives::read(b.text).list("requires"),
            abbrev: Directives::read(b.text).get("abbrev").map(str::to_string),
            unmet: None,
            path: None,
        })
        .collect();
    for (name, extension, path) in on_disk(root, kind) {
        if out.iter().any(|t| t.name == name) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        out.push(TemplateInfo {
            description: Directives::read(&text).get("description").map(str::to_string),
            requires: Directives::read(&text).list("requires"),
            abbrev: Directives::read(&text).get("abbrev").map(str::to_string),
            unmet: None,
            name,
            extension,
            origin: TemplateOrigin::Global,
            starter: false,
            path: Some(path.display().to_string()),
        });
    }
    out
}

pub fn load_template(root: &Path, kind: TemplateKind, name: &str, builtins: &[Builtin]) -> Result<Template, String> {
    if let Some(b) = builtins.iter().find(|b| b.name == name) {
        return Ok(Template {
            name: b.name.to_string(),
            extension: b.extension.to_string(),
            text: b.text.to_string(),
            path: None,
        });
    }
    let (name, extension, path) = on_disk(root, kind)
        .into_iter()
        .find(|(n, _, _)| n == name)
        .ok_or_else(|| format!("There is no {} template called `{name}`", kind.title().to_lowercase()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Template { name, extension, text, path: Some(path) })
}

/// Write a new template, refusing a name already taken. Returns its path.
pub fn create_template(
    root: &Path,
    kind: TemplateKind,
    name: &str,
    extension: &str,
    text: &str,
    builtins: &[Builtin],
) -> Result<PathBuf, String> {
    check_template_name(name)?;
    if !extension.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("`{extension}` is not an extension a template can have"));
    }
    if builtins.iter().any(|b| b.name == name) || on_disk(root, kind).iter().any(|(n, _, _)| n == name) {
        return Err(format!("A template called `{name}` already exists"));
    }
    let dir = kind_dir(root, kind);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(template_file_name(name, extension));
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Rename one of the user's templates, keeping its text and its extension. Returns its new path.
///
/// A file move rather than a copy and a delete: the template is the file, and the editor may have it
/// open — a new file beside a deleted one would leave that tab pointing at nothing.
pub fn rename_template(
    root: &Path,
    kind: TemplateKind,
    name: &str,
    new_name: &str,
    builtins: &[Builtin],
) -> Result<PathBuf, String> {
    if builtins.iter().any(|b| b.name == name) {
        return Err(format!("`{name}` is built in and cannot be renamed"));
    }
    check_template_name(new_name)?;
    let (_, extension, path) = on_disk(root, kind)
        .into_iter()
        .find(|(n, _, _)| n == name)
        .ok_or_else(|| format!("There is no template called `{name}`"))?;
    if new_name == name {
        return Ok(path);
    }
    // Compared exactly, so a change of case alone is a rename — which is what it is on the two
    // filesystems that tell `Get` from `get`, and what the user asked for on the one that does not.
    if builtins.iter().any(|b| b.name == new_name) || on_disk(root, kind).iter().any(|(n, _, _)| n == new_name) {
        return Err(format!("A template called `{new_name}` already exists"));
    }
    let target = path.with_file_name(template_file_name(new_name, &extension));
    std::fs::rename(&path, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(target)
}

pub fn delete_template(root: &Path, kind: TemplateKind, name: &str, builtins: &[Builtin]) -> Result<(), String> {
    if builtins.iter().any(|b| b.name == name) {
        return Err(format!("`{name}` is built in and cannot be deleted"));
    }
    let (_, _, path) = on_disk(root, kind)
        .into_iter()
        .find(|(n, _, _)| n == name)
        .ok_or_else(|| format!("There is no template called `{name}`"))?;
    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write a `bennu.<key>` directive into a template's text: replacing the one that is there, or
/// adding it at the top when there is none.
///
/// Directives are read from any line, so a rewrite has to find the line that is already answering
/// rather than add a second one under it — two `bennu.abbrev` lines would leave the first winning
/// and the edit looking lost. An empty value removes the directive, which is how an abbreviation
/// goes back to being named by its file.
pub fn set_directive(text: &str, key: &str, value: &str) -> String {
    let value = value.trim();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let marker = format!("bennu.{key}");
    let existing = lines.iter().position(|line| {
        let content = line.trim_start();
        let content = content.strip_prefix("{#-").or_else(|| content.strip_prefix("{#")).unwrap_or(content).trim_start();
        content.strip_prefix(&marker).is_some_and(|rest| rest.trim_start().starts_with(':'))
    });
    let written = format!("{{# bennu.{key}: {value} #}}");
    match (existing, value.is_empty()) {
        (Some(at), true) => {
            lines.remove(at);
        }
        (Some(at), false) => lines[at] = written,
        (None, true) => return text.to_string(),
        (None, false) => lines.insert(0, written),
    }
    let mut out = lines.join("\n");
    // A template is a file: it ended with a newline before this and has to end with one after.
    if text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// A name a template may have: letters, digits, `-` and `_` — a file name everywhere, and no dot,
/// which is where the extension starts.
pub fn check_template_name(name: &str) -> Result<(), String> {
    let usable = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
    match usable {
        true => Ok(()),
        false => Err("A template name uses letters, digits, `-` and `_` only".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: &[Builtin] = &[Builtin {
        name: "standard",
        extension: "java",
        text: "{# bennu.description: The standard one -#}\nclass A {}",
        starter: false,
    }];

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-templates-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_directive_is_rewritten_in_place_rather_than_added_under_itself() {
        let text = "{# bennu.abbrev: old #}\n{# bennu.description: A logger -#}\nbody\n";
        let out = set_directive(text, "abbrev", "logd");
        assert_eq!(out, "{# bennu.abbrev: logd #}\n{# bennu.description: A logger -#}\nbody\n");
        assert_eq!(Directives::read(&out).get("abbrev"), Some("logd"));
    }

    #[test]
    fn a_directive_that_is_not_there_is_added_at_the_top() {
        let out = set_directive("body\n", "abbrev", "logd");
        assert_eq!(out, "{# bennu.abbrev: logd #}\nbody\n");
    }

    /// Emptying it is how an abbreviation goes back to being named by its file.
    #[test]
    fn an_empty_value_removes_the_directive() {
        let text = "{# bennu.abbrev: logd #}\nbody\n";
        assert_eq!(set_directive(text, "abbrev", ""), "body\n");
        assert_eq!(set_directive("body\n", "abbrev", ""), "body\n");
    }

    #[test]
    fn a_file_name_splits_into_the_name_and_what_it_generates() {
        assert_eq!(split_file_name("Repository.java.jinja"), Some(("Repository".into(), "java".into())));
        assert_eq!(split_file_name("notes.jinja"), Some(("notes".into(), "".into())));
        assert_eq!(split_file_name("page.html.twig"), None);
        assert_eq!(template_file_name("builder", "java"), "builder.java.jinja");
    }

    #[test]
    fn built_ins_come_first_and_cannot_be_shadowed_or_deleted() {
        let root = root("list");
        let dir = kind_dir(&root, TemplateKind::Class);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zeta.java.jinja"), "{# bennu.description: Last -#}\nz").unwrap();
        std::fs::write(dir.join("Alpha.yml.jinja"), "a").unwrap();
        std::fs::write(dir.join("standard.java.jinja"), "not the built-in").unwrap();
        let listed = list_templates(&root, TemplateKind::Class, BUILTINS);
        let names: Vec<&str> = listed.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["standard", "Alpha", "zeta"]);
        assert_eq!(listed[0].description.as_deref(), Some("The standard one"));
        assert_eq!(listed[1].extension, "yml");
        assert_eq!(listed[2].description.as_deref(), Some("Last"));
        assert_eq!(load_template(&root, TemplateKind::Class, "standard", BUILTINS).unwrap().text, BUILTINS[0].text);
        assert!(delete_template(&root, TemplateKind::Class, "standard", BUILTINS).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_template_is_created_once_and_can_be_deleted() {
        let root = root("create");
        let path = create_template(&root, TemplateKind::Live, "logger", "java", "text", &[]).unwrap();
        assert!(path.ends_with("live/logger.java.jinja"));
        assert!(create_template(&root, TemplateKind::Live, "logger", "yml", "again", &[]).is_err());
        assert!(create_template(&root, TemplateKind::Live, "has.dot", "java", "x", &[]).is_err());
        delete_template(&root, TemplateKind::Live, "logger", &[]).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
