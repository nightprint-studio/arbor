//! `log_view` domain — the console's presentation settings, and which packages are "my code".
//!
//! The settings are presentational only (the buffer is never touched), so there is nothing to
//! validate: the frontend owns the vocabulary and this just keeps it in the profile.
//!
//! **My packages** are read from where the project's sources live, with no parse: a `.java` file's
//! package is its directory under the source root, and counting those is enough to tell
//! `it.acme.gestionale` (the code somebody wrote) from `org.springframework` (the code they call).
//! It is what lets a stack trace fold the framework frames and keep the user's own — without
//! anyone having to type a package name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bennu_core::config::{load, save, LogViewConfig};
use bennu_core::prelude::BennuState;
use bennu_intel::prelude::collect_java;

#[arbor_rpc::handler]
fn get_bennu_log_view(_state: &BennuState) -> Result<LogViewConfig, String> {
    Ok(load().log_view)
}

/// Own pair of handlers rather than a field of the settings snapshot, like the profiler's tools:
/// a table in the config file, owned by one screen.
#[arbor_rpc::handler]
fn set_bennu_log_view(_state: &BennuState, config: LogViewConfig) -> Result<(), String> {
    let mut current = load();
    current.log_view = config;
    save(&current)
}

/// The top-level packages the project's own sources declare, most populated first.
#[arbor_rpc::handler]
fn bennu_own_packages(_state: &BennuState, root: String) -> Result<Vec<String>, String> {
    let root = PathBuf::from(root);
    let mut files = Vec::new();
    collect_java(&root, &mut files);
    Ok(own_packages(files.iter().filter_map(|f| package_of(&root, f))))
}

/// The package a source file declares, from where it sits: the directories between its source root
/// (`src/main/java`, `src/test/java`, or just `src` in a legacy layout) and the file.
fn package_of(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let after = parts
        .iter()
        .enumerate()
        .find(|(i, p)| *i > 0 && p.as_str() == "java" && matches!(parts[i - 1].as_str(), "main" | "test"))
        .map(|(i, _)| i)
        .or_else(|| parts.iter().position(|p| p == "src"))?;
    let dirs = parts.get(after + 1..parts.len().saturating_sub(1))?;
    (!dirs.is_empty()).then(|| dirs.join("."))
}

/// Packages → the few prefixes that stand for "this project": the first three segments of each,
/// kept when they hold at least one file in twenty, with a prefix swallowed by a shorter one.
fn own_packages(packages: impl Iterator<Item = String>) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    for p in packages {
        total += 1;
        let prefix = p.split('.').take(3).collect::<Vec<_>>().join(".");
        *counts.entry(prefix).or_default() += 1;
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().filter(|(_, n)| n * 20 >= total).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let names: Vec<String> = ranked.into_iter().map(|(p, _)| p).collect();
    names
        .iter()
        .filter(|p| !names.iter().any(|q| q != *p && p.starts_with(&format!("{q}."))))
        .take(6)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(root: &str, file: &str) -> Option<String> {
        package_of(Path::new(root), Path::new(&format!("{root}/{file}")))
    }

    #[test]
    fn package_comes_from_the_directories_under_the_source_root() {
        assert_eq!(pkg("/p", "src/main/java/it/acme/Foo.java").as_deref(), Some("it.acme"));
        assert_eq!(pkg("/p", "core/src/test/java/it/acme/dao/FooTest.java").as_deref(), Some("it.acme.dao"));
        assert_eq!(pkg("/p", "src/it/acme/Foo.java").as_deref(), Some("it.acme"));
    }

    #[test]
    fn a_file_directly_in_the_source_root_has_no_package() {
        assert_eq!(pkg("/p", "src/main/java/Foo.java"), None);
    }

    #[test]
    fn prefixes_are_three_segments_and_rank_by_population() {
        let pkgs = ["it.acme.gestionale.dao", "it.acme.gestionale.web", "it.acme.gestionale", "it.acme.util.x"];
        let got = own_packages(pkgs.iter().map(|s| s.to_string()));
        assert_eq!(got, vec!["it.acme.gestionale".to_string(), "it.acme.util".to_string()]);
    }

    #[test]
    fn a_prefix_inside_another_is_dropped() {
        let pkgs = ["it.acme", "it.acme.gestionale.dao"];
        assert_eq!(own_packages(pkgs.iter().map(|s| s.to_string())), vec!["it.acme".to_string()]);
    }

    #[test]
    fn a_stray_package_below_the_threshold_is_ignored() {
        let mut pkgs: Vec<String> = (0..40).map(|_| "it.acme.main".to_string()).collect();
        pkgs.push("org.stray.thing".into());
        assert_eq!(own_packages(pkgs.into_iter()), vec!["it.acme.main".to_string()]);
    }
}
