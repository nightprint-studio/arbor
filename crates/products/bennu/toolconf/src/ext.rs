//! `ToolConfExtension` — the [`FrameworkExtension`] over the build-tool config files.
//!
//! ## Why one extension for two tools
//!
//! Because they are the same file with a different table in it. Both are properties syntax, both
//! have a vocabulary somebody wrote down, both have a version that decides which half of that
//! vocabulary exists — and all three of those facts live in code that has no idea which tool it is
//! serving. What is genuinely per-tool is the table and the coordinates whose version dates it,
//! which is what a [`Catalogue`] is.
//!
//! A third file of this shape (`sonar-project.properties`, `spotbugs`'s exclusions) is a table and
//! one line in [`CATALOGUES`].
//!
//! ## What it costs a project that has neither file
//!
//! Nothing measurable. [`reindex`] does no work at all — it records the root and returns — and the
//! version lookup, which is the only expensive thing here, happens **on the first answer about one
//! of these files** and is memoised from then on. A project with no `lombok.config` never asks, so
//! it never pays; a project with one pays a reactor walk once, on the first keystroke in it.
//!
//! [`reindex`]: FrameworkExtension::reindex

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use bennu_ext::prelude::{ExtHover, FileCtx, FrameworkExtension, ProjectScan};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::answers;
use crate::model::Catalogue;

/// Every tool this extension serves. One line per file.
fn catalogues() -> [&'static Catalogue; 2] {
    [crate::lombok::catalogue(), &crate::junit::CATALOGUE]
}

pub struct ToolConfExtension {
    root: RwLock<PathBuf>,
    ready: AtomicBool,
    /// Tool slug → the version this project resolves, once it has been asked for. The **outer**
    /// `Option` is "has it been looked up", the inner one is "was there an answer" — collapsing
    /// them would make a project that genuinely has no Lombok pay the lookup on every keystroke.
    versions: Mutex<HashMap<&'static str, Option<String>>>,
}

impl Default for ToolConfExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolConfExtension {
    pub fn new() -> Self {
        Self {
            root: RwLock::new(PathBuf::new()),
            ready: AtomicBool::new(false),
            versions: Mutex::new(HashMap::new()),
        }
    }

    /// The catalogue that owns this file, or `None` — which is every file in the project but two.
    fn catalogue_for(ctx: &FileCtx<'_>) -> Option<&'static Catalogue> {
        let name = ctx.file_name().to_ascii_lowercase();
        catalogues().into_iter().find(|c| c.file_name == name)
    }

    /// The version this project is on, resolved once and remembered.
    fn version_of(&self, catalogue: &Catalogue) -> Option<String> {
        if let Ok(memo) = self.versions.lock() {
            if let Some(found) = memo.get(catalogue.tool) {
                return found.clone();
            }
        }
        let root = self.root.read().ok()?.clone();
        let found = crate::version::resolve(&root, catalogue.artifacts);
        if let Ok(mut memo) = self.versions.lock() {
            memo.insert(catalogue.tool, found.clone());
        }
        found
    }

    /// Run `f` with the table and the version for this file, or `None` when the file is not one of
    /// ours.
    fn with<T>(&self, ctx: &FileCtx<'_>, f: impl FnOnce(&Catalogue, Option<&str>) -> T) -> Option<T> {
        let catalogue = Self::catalogue_for(ctx)?;
        let version = self.version_of(catalogue);
        Some(f(catalogue, version.as_deref()))
    }
}

impl FrameworkExtension for ToolConfExtension {
    fn id(&self) -> &'static str {
        "toolconf"
    }

    fn display_name(&self) -> &'static str {
        "Build tool config"
    }

    /// Always — and gated on the file rather than on a capability, for the reason the XML and Maven
    /// extensions are: the file name is a better question than the bitset, and asking it is free.
    /// A `lombok.config` in a tree whose pom does not mention Lombok is still a `lombok.config`,
    /// and is in fact exactly the file somebody needs explained.
    fn applies(&self, _caps: &CapabilitySet) -> bool {
        true
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        if let Ok(mut root) = self.root.write() {
            *root = scan.root.to_path_buf();
        }
        // The poms may have moved under the cached versions — a `mvn` upgrade is exactly the moment
        // the gate has to change.
        if let Ok(mut memo) = self.versions.lock() {
            memo.clear();
        }
        self.ready.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        self.with(ctx, |c, v| answers::diagnostics(c, v, ctx.source)).unwrap_or_default()
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        self.with(ctx, |c, v| answers::completions(c, v, ctx.source, offset)).unwrap_or_default()
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        self.with(ctx, |c, v| answers::hover(c, v, ctx.source, offset)).flatten()
    }

    fn inline_hint(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<String> {
        self.with(ctx, |c, v| answers::inline_hint(c, v, ctx.source, offset)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ctx<'a>(path: &'a Path, source: &'a str) -> FileCtx<'a> {
        FileCtx { path, source }
    }

    #[test]
    fn each_file_is_answered_from_its_own_table() {
        let lombok = Path::new("/p/lombok.config");
        let junit = Path::new("/p/src/test/resources/junit-platform.properties");
        assert_eq!(ToolConfExtension::catalogue_for(&ctx(lombok, "")).unwrap().tool, "lombok");
        assert_eq!(ToolConfExtension::catalogue_for(&ctx(junit, "")).unwrap().tool, "junit");
    }

    #[test]
    fn every_other_properties_file_in_the_project_is_left_alone() {
        // The one that matters: a legacy tree is full of `.properties`, and answering about them
        // with Lombok's vocabulary would be worse than answering nothing.
        for name in ["messages.properties", "application.properties", "log4j.properties"] {
            let path = PathBuf::from("/p").join(name);
            assert!(ToolConfExtension::catalogue_for(&ctx(&path, "")).is_none(), "{name}");
        }
    }

    #[test]
    fn a_file_we_do_not_own_costs_no_version_lookup() {
        let ext = ToolConfExtension::new();
        let path = Path::new("/p/messages.properties");
        assert!(ext.completions(&ctx(path, "a"), 1).is_empty());
        assert!(ext.hover(&ctx(path, "a"), 1).is_none());
        assert!(ext.diagnostics(&ctx(path, "a")).is_empty());
        assert!(ext.versions.lock().unwrap().is_empty());
    }

    #[test]
    fn the_version_is_resolved_once_and_remembered() {
        let ext = ToolConfExtension::new();
        let path = PathBuf::from("/definitely/not/a/project/lombok.config");
        let src = "lombok.accessors.";
        assert!(!ext.completions(&ctx(&path, src), src.len()).is_empty());
        // A root with no pom answers `None`, and the memo has to hold that too — otherwise every
        // keystroke re-walks a reactor that is not there.
        assert_eq!(ext.versions.lock().unwrap().get("lombok"), Some(&None));
    }
}
