//! Every build manifest of a project — what is built, and where.
//!
//! A project is not one kind of thing. A Tauri app is a Node project with a Cargo project in `src-tauri`;
//! a monorepo has a service in Maven, a worker in Rust and a web client in Node. Deciding the kind from the
//! root alone (what [`crate::model::open_project`] still does for the Java model) names one of them and
//! hides the rest, so the tools that depend on the kind — the activity bar, the tool windows, the
//! assistant — ask this instead: every manifest under the root, each with its folder.
//!
//! The rules are the tree's own: [`crate::tree::is_noise_dir`] decides what is never entered, so the two
//! agree on what the project contains. A manifest below one of the same kind is a **member** of it — a
//! Cargo workspace's crates, a Maven module, an npm workspace's packages — and is not listed beside it.

use std::path::Path;

use serde::Serialize;

use crate::tree::is_noise_dir;

/// One build manifest found in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Module {
    /// `Maven`, `Gradle`, `Cargo` or `Node`.
    pub kind: &'static str,
    /// The folder that holds the manifest, relative to the project's root (`.` for the root itself).
    pub dir: String,
}

/// The manifest files and what each one makes its folder. Listed in the order a folder's own manifests are
/// reported, which is also the order of precedence at the root: Java first, as `open_project` decides.
const MANIFESTS: [(&str, &str); 5] =
    [("pom.xml", "Maven"), ("build.gradle", "Gradle"), ("build.gradle.kts", "Gradle"), ("Cargo.toml", "Cargo"), ("package.json", "Node")];

/// How deep below the root a manifest is looked for, and how many are kept: a monorepo has a handful of
/// projects, and a deeper or longer list is mostly vendored or generated.
pub const SCAN_DEPTH: usize = 3;
pub const MAX_MODULES: usize = 8;

/// Every build manifest of the project at `root`, root first, then folder by folder in name order.
pub fn modules_of(root: &Path) -> Vec<Module> {
    let mut out = Vec::new();
    scan(root, root, 0, &mut out);
    out
}

fn scan(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Module>) {
    let rel = dir.strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let rel = if rel.is_empty() { ".".to_string() } else { rel };
    for (file, kind) in MANIFESTS {
        let covered = out.iter().any(|m| m.kind == kind && (m.dir == "." || m.dir == rel || rel.starts_with(&format!("{}/", m.dir))));
        if !covered && out.len() < MAX_MODULES && dir.join(file).is_file() {
            out.push(Module { kind, dir: rel.clone() });
        }
    }
    if depth >= SCAN_DEPTH || out.len() >= MAX_MODULES {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut subdirs: Vec<_> = read.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).collect();
    subdirs.sort_by_key(|e| e.file_name());
    for entry in subdirs {
        let name = entry.file_name().to_string_lossy().to_string();
        // Hidden folders are tool state (`.angular`, `.svelte-kit`, `.cache`), never a project of their own.
        if name.starts_with('.') || is_noise_dir(dir, &name) {
            continue;
        }
        scan(root, &entry.path(), depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn found(root: &Path) -> Vec<(&'static str, String)> {
        modules_of(root).into_iter().map(|m| (m.kind, m.dir)).collect()
    }

    #[test]
    fn a_web_front_end_with_a_rust_back_end_beside_it_lists_both() {
        let dir = tree("tauri", &["package.json", "src-tauri/Cargo.toml", "src/main.ts"]);
        assert_eq!(found(&dir), [("Node", ".".to_string()), ("Cargo", "src-tauri".to_string())]);
    }

    #[test]
    fn the_members_of_a_workspace_are_not_listed_beside_it() {
        let dir = tree("workspace", &["Cargo.toml", "package.json", "crates/a/Cargo.toml", "crates/b/Cargo.toml", "pkgs/ui/package.json"]);
        assert_eq!(found(&dir), [("Cargo", ".".to_string()), ("Node", ".".to_string())]);
        let maven = tree("maven", &["pom.xml", "api/pom.xml", "web/pom.xml"]);
        assert_eq!(found(&maven), [("Maven", ".".to_string())]);
    }

    #[test]
    fn dependencies_and_build_output_are_not_entered() {
        let dir = tree("skip", &["package.json", "node_modules/x/package.json", "target/debug/Cargo.toml", "dist/package.json", ".git/x/Cargo.toml", ".angular/x/package.json"]);
        assert_eq!(found(&dir), [("Node", ".".to_string())]);
    }

    #[test]
    fn a_monorepo_lists_each_ecosystem_where_it_lives() {
        let dir = tree("mono", &["services/api/pom.xml", "services/worker/Cargo.toml", "web/package.json"]);
        assert_eq!(found(&dir), [("Maven", "services/api".to_string()), ("Cargo", "services/worker".to_string()), ("Node", "web".to_string())]);
    }

    #[test]
    fn a_folder_with_two_gradle_files_is_one_module() {
        let dir = tree("gradle", &["build.gradle", "build.gradle.kts"]);
        assert_eq!(found(&dir), [("Gradle", ".".to_string())]);
    }

    #[test]
    fn a_plain_folder_has_none_and_the_list_is_bounded() {
        assert!(found(&tree("plain", &["README.md"])).is_empty());
        let many: Vec<String> = (0..20).map(|i| format!("svc{i:02}/pom.xml")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(modules_of(&tree("many", &refs)).len(), MAX_MODULES);
    }

    #[test]
    fn a_manifest_deeper_than_the_scan_is_not_found() {
        let dir = tree("deep", &["a/b/c/d/package.json"]);
        assert!(found(&dir).is_empty());
    }
}
