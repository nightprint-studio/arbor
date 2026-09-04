//! `MavenExtension` — the [`FrameworkExtension`] over a `pom.xml`.
//!
//! ## Why it is not part of the XML extension
//!
//! `bennu-xml` answers from a **grammar**: which elements are legal here, which attributes that one
//! takes. That is the right answer for the shape of a pom and it is all it can be, because a schema
//! cannot know whether `org.acme:widget:1.4` exists — no grammar in the world does.
//!
//! Everything in this crate is the other half: the answers that come from the *machine* rather than
//! from the document's vocabulary. The two live side by side on the same file and contribute
//! different things — the XML extension completes the element you are opening, this one completes
//! the value you are typing into it.
//!
//! ## What it costs when it has nothing to do
//!
//! Nothing. The extension applies to every project (there is no "is this Maven" capability, and the
//! root's `pom.xml` is a better question anyway), and every answer is gated on the file actually
//! being a pom. A Cargo project never scans a repository, because [`reindex`] leaves immediately
//! when the root holds no pom.
//!
//! [`reindex`]: FrameworkExtension::reindex

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bennu_ext::prelude::{ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan};
use bennu_proto::prelude::{CapabilitySet, CompletionItem, Diagnostic};

use crate::catalog::Catalog;
use crate::doc::Doc;
use crate::effective::Effective;
use crate::env::PomEnv;
use crate::repo::LocalRepo;

/// What the extension knows about the project it is looking at.
#[derive(Default)]
struct Project {
    /// `groupId:artifactId` → the module's pom, forward-slashed.
    reactor: HashMap<String, String>,
}

/// The Maven extension.
pub struct MavenExtension {
    repo: LocalRepo,
    /// The repository's coordinates.
    ///
    /// An `Arc` around the lock rather than a plain field, because the first scan of a cold
    /// repository is seconds and `reindex` is called on the thread that asked the first question.
    /// Blocking it would mean the first completion in a pom waits for a walk of `~/.m2`; instead
    /// the scan runs behind this handle and every answer degrades to "the repository has not said
    /// yet", which the checks already treat as *no claims* rather than *nothing is installed*.
    catalog: Arc<RwLock<Arc<Catalog>>>,
    /// Whether a scan is already running — a second project opening must not start a second walk.
    scanning: Arc<AtomicBool>,
    project: RwLock<Arc<Project>>,
    ready: AtomicBool,
    /// Per-file memo of the effective pom, keyed by a hash of the buffer.
    ///
    /// Computing one means reading the parent chain and any imported BOM — a handful of small files
    /// — and every answer for a keystroke needs the same one. Without the memo, a hover and the
    /// diagnostics that follow it read the same four poms twice.
    effective: Mutex<HashMap<String, (u64, Arc<Effective>)>>,
}

impl Default for MavenExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl MavenExtension {
    pub fn new() -> Self {
        Self {
            repo: LocalRepo::discover(),
            catalog: Arc::new(RwLock::new(Arc::new(Catalog::default()))),
            scanning: Arc::new(AtomicBool::new(false)),
            project: RwLock::new(Arc::new(Project::default())),
            ready: AtomicBool::new(false),
            effective: Mutex::new(HashMap::new()),
        }
    }

    /// The local repository this project resolves against — the answer to "where is it even
    /// looking", which is otherwise invisible on a machine with a relocated one.
    pub fn repo(&self) -> &LocalRepo {
        &self.repo
    }

    /// The repository's coordinates, as scanned. Named for what it holds rather than `catalog`,
    /// which is the trait's word for a *panel* of rows and would shadow it here.
    pub fn installed(&self) -> Arc<Catalog> {
        self.catalog.read().map(|c| Arc::clone(&c)).unwrap_or_default()
    }

    /// Walk the repository on a worker, and publish the result when it arrives.
    ///
    /// At most one walk at a time across every project: two Maven projects opening together are
    /// asking about the same `~/.m2`, and the second walk would find exactly what the first is
    /// already finding.
    fn scan_in_background(&self) {
        if self.scanning.swap(true, Ordering::AcqRel) {
            return;
        }
        let repo = self.repo.clone();
        let slot = Arc::clone(&self.catalog);
        let scanning = Arc::clone(&self.scanning);
        std::thread::spawn(move || {
            let catalog = Catalog::scan(&repo);
            catalog.save();
            eprintln!(
                "bennu-maven: {} holds {} artifacts ({} versions)",
                repo.root().display(),
                catalog.len(),
                catalog.version_count()
            );
            if let Ok(mut slot) = slot.write() {
                *slot = Arc::new(catalog);
            }
            scanning.store(false, Ordering::Release);
        });
    }

    /// Run `f` with everything an answer about this buffer needs, or `None` when the file is not a
    /// pom.
    fn with_env<T>(&self, ctx: &FileCtx<'_>, f: impl FnOnce(&PomEnv<'_>, &Doc<'_>) -> T) -> Option<T> {
        if !is_pom(&ctx.file_name()) {
            return None;
        }
        let path = ctx.path_str();
        let effective = self.effective_for(&path, ctx.source);
        let catalog = self.installed();
        let project = self.project.read().ok().map(|p| Arc::clone(&p))?;
        let doc = Doc::new(ctx.source);
        let env = PomEnv {
            repo: &self.repo,
            catalog: &catalog,
            reactor: &project.reactor,
            effective: &effective,
            path: &path,
        };
        Some(f(&env, &doc))
    }

    /// The effective pom for this buffer, from the memo when the buffer has not changed.
    fn effective_for(&self, path: &str, source: &str) -> Arc<Effective> {
        let stamp = fnv(source.as_bytes());
        if let Ok(memo) = self.effective.lock() {
            if let Some((cached, effective)) = memo.get(path) {
                if *cached == stamp {
                    return Arc::clone(effective);
                }
            }
        }
        let built =
            Arc::new(crate::effective::effective_of_buffer(&self.repo, Path::new(path), source));
        if let Ok(mut memo) = self.effective.lock() {
            // One buffer per file: the memo is a cache of the *current* text, not a history of it.
            memo.insert(path.to_string(), (stamp, Arc::clone(&built)));
        }
        built
    }
}

/// Whether this file is a pom. `pom.xml` is the name Maven itself insists on; a `*.pom` is the same
/// document under the name the repository stores it as, and jumping into one is an ordinary thing
/// to do from a dependency.
fn is_pom(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case("pom.xml") || file_name.to_ascii_lowercase().ends_with(".pom")
}

impl FrameworkExtension for MavenExtension {
    fn id(&self) -> &'static str {
        "maven"
    }

    fn display_name(&self) -> &'static str {
        "Maven"
    }

    /// Always — and gated on the file being a pom rather than on a capability, for the same reason
    /// the XML extension is: the file is a better question than the bitset, and asking it is free.
    fn applies(&self, _caps: &CapabilitySet) -> bool {
        true
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        // A project with no pom is not a Maven project, and must not pay a repository scan to
        // discover that.
        if !scan.root.join("pom.xml").is_file() {
            self.ready.store(true, Ordering::Release);
            return;
        }
        let reactor = crate::resolve::reactor(scan.root)
            .into_iter()
            .map(|(dir, pom)| {
                let ga = format!("{}:{}", pom.effective_group(), pom.artifact_id);
                (ga, dir.join("pom.xml").to_string_lossy().replace('\\', "/"))
            })
            .collect();
        if let Ok(mut slot) = self.project.write() {
            *slot = Arc::new(Project { reactor });
        }
        // The disk cache is free and is the usual case. A cold repository is a walk of tens of
        // thousands of directories, and `reindex` runs on whichever thread asked the first
        // question — so that one goes behind the extension's back and lands when it lands.
        match Catalog::cached(&self.repo) {
            Some(catalog) => {
                if let Ok(mut slot) = self.catalog.write() {
                    *slot = Arc::new(catalog);
                }
            }
            None => self.scan_in_background(),
        }
        if let Ok(mut memo) = self.effective.lock() {
            memo.clear(); // the poms may have moved under every cached answer
        }
        self.ready.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        self.with_env(ctx, crate::check::diagnostics).unwrap_or_default()
    }

    fn completions(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<CompletionItem> {
        self.with_env(ctx, |env, doc| crate::complete::completions(env, doc, offset)).unwrap_or_default()
    }

    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        self.with_env(ctx, |env, doc| crate::explain::hover(env, doc, offset)).flatten()
    }

    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        self.with_env(ctx, |env, doc| crate::explain::navigate(env, doc, offset)).unwrap_or_default()
    }

    /// The repository's own contents, as a list — the answer to "do I actually have this", asked
    /// without a pom in front of you.
    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        /// A repository holds tens of thousands of artifacts; a list panel is not a database
        /// browser, and past this the payload costs more than the answer is worth.
        const MAX_ROWS: usize = 2000;
        if kind != "artifacts" {
            return Vec::new();
        }
        self.installed()
            .artifacts
            .iter()
            .take(MAX_ROWS)
            .map(|a| ExtEntry {
                id: a.ga(),
                primary: a.artifact_id.clone(),
                secondary: a.group_id.clone(),
                kind: "jar".to_string(),
                tags: a.versions.iter().take(4).cloned().collect(),
                ..ExtEntry::default()
            })
            .collect()
    }

    fn stats(&self) -> Vec<ExtStat> {
        let catalog = self.installed();
        if catalog.is_empty() {
            return Vec::new();
        }
        vec![
            ExtStat {
                label: "Local repository".into(),
                value: catalog.len(),
                catalog: Some("artifacts".into()),
            },
            ExtStat { label: "Installed versions".into(), value: catalog.version_count(), catalog: None },
        ]
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_pom_is_answered_for() {
        assert!(is_pom("pom.xml"));
        assert!(is_pom("spring-web-5.3.27.pom"));
        assert!(!is_pom("struts.xml"));
        assert!(!is_pom("App.java"));
    }

    /// A project with no pom must not scan a repository to find that out.
    #[test]
    fn a_project_with_no_pom_indexes_nothing() {
        let dir = std::env::temp_dir().join(format!("bennu-mvn-ext-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let ext = MavenExtension::new();
        ext.reindex(&ProjectScan::empty(&dir));
        assert!(ext.is_ready());
        assert!(ext.installed().is_empty());
        assert!(ext.stats().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The gate that keeps the extension free on every other file in the project.
    #[test]
    fn a_file_that_is_not_a_pom_gets_nothing() {
        let ext = MavenExtension::new();
        let ctx = FileCtx { path: Path::new("/p/src/App.java"), source: "class App {}" };
        assert!(ext.diagnostics(&ctx).is_empty());
        assert!(ext.completions(&ctx, 0).is_empty());
        assert!(ext.hover(&ctx, 0).is_none());
        assert!(ext.navigate(&ctx, 0).is_empty());
    }
}
