//! Which command-line tools the generators can use are installed.
//!
//! Resolution is `arbor_process_ext::locate_executable` — the same lookup the language-server and
//! debug-adapter clients use, which also covers the directories a GUI-launched process does not
//! inherit on its `PATH`. Nothing is spawned: asking "is `npm` here" must be instant, and a
//! `--version` per tool would be five process launches every time the wizard opens.

use serde::Serialize;

/// The tools the wizard knows about. Order is display order.
pub const KNOWN: &[(&str, &str)] = &[
    ("git", "Git"),
    ("node", "Node.js"),
    ("npm", "npm"),
    ("npx", "npx"),
    ("pnpm", "pnpm"),
    ("yarn", "Yarn"),
    ("cargo", "Cargo"),
    ("java", "Java"),
    ("mvn", "Maven"),
];

#[derive(Serialize, Clone, Debug)]
pub struct ToolStatus {
    pub name: String,
    pub label: String,
    pub found: bool,
    pub path: Option<String>,
}

/// Resolve one tool to a runnable path.
pub fn locate(name: &str) -> Option<String> {
    arbor_process_ext::prelude::locate_executable(name, None, &[], &[])
}

/// The status of every known tool.
pub fn detect() -> Vec<ToolStatus> {
    KNOWN
        .iter()
        .map(|(name, label)| {
            let path = locate(name);
            ToolStatus { name: (*name).into(), label: (*label).into(), found: path.is_some(), path }
        })
        .collect()
}

/// The first of `required` that is missing from `status`, as a sentence — or `None` when all are
/// there. This is the reason a generator is shown disabled.
pub fn missing_reason(required: &[String], status: &[ToolStatus]) -> Option<String> {
    let missing: Vec<&str> = required
        .iter()
        .filter(|r| !status.iter().any(|s| s.name == **r && s.found))
        .map(String::as_str)
        .collect();
    match missing.as_slice() {
        [] => None,
        [one] => Some(format!("`{one}` was not found on your PATH — install it, then reopen this window")),
        many => Some(format!(
            "{} were not found on your PATH — install them, then reopen this window",
            many.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str, found: bool) -> ToolStatus {
        ToolStatus { name: name.into(), label: name.into(), found, path: None }
    }

    #[test]
    fn nothing_missing_means_no_reason() {
        assert!(missing_reason(&["npm".into()], &[st("npm", true)]).is_none());
        assert!(missing_reason(&[], &[]).is_none());
    }

    #[test]
    fn missing_tools_are_named() {
        let r = missing_reason(&["npm".into(), "npx".into()], &[st("npm", false), st("npx", true)]).unwrap();
        assert!(r.contains("`npm`") && !r.contains("`npx`"));
        let both = missing_reason(&["npm".into(), "npx".into()], &[]).unwrap();
        assert!(both.contains("`npm`, `npx`"));
    }
}
