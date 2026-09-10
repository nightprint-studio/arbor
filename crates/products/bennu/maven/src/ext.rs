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

use bennu_ext::prelude::{
    ExtEntry, ExtHighlight, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
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
        Self::with_repo(LocalRepo::discover())
    }

    /// The extension against a repository the caller names.
    ///
    /// Discovery is right for a host — `settings.xml` and `-Dmaven.repo.local` are the only
    /// answer that can be right on somebody else's machine. This is for the caller that already
    /// knows: a test that built one, and anything that has to reason about a repository other
    /// than the ambient one.
    pub fn with_repo(repo: LocalRepo) -> Self {
        Self {
            repo,
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

    /// Whether this path is a pom **in the local repository** — one followed into from a
    /// dependency, rather than one this project builds.
    ///
    /// The distinction the read-only half of the crate needs: everything that *describes* a pom
    /// answers the same for both, and everything that *judges* one only makes sense for a file
    /// somebody can edit.
    pub fn is_library_pom(&self, path: &str) -> bool {
        let root = self.repo.root();
        !root.as_os_str().is_empty() && Path::new(path).starts_with(root)
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
pub fn is_pom(file_name: &str) -> bool {
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

    /// What is wrong with this pom — for a pom that is **yours**.
    ///
    /// A pom in the local repository is silent, and the difference is not a technicality. Every
    /// check here is written against a file somebody can fix: a coordinate that is not installed,
    /// a version nothing pins. A released artifact's pom names the dependencies *it* was built
    /// against, and a machine that never needed one of them never downloaded it — so on a library
    /// pom the very check that finds a real problem in your own file produces a red underline
    /// under the ordinary, correct state of a repository, on a line nobody can edit.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if self.is_library_pom(&ctx.path_str()) {
            return Vec::new();
        }
        self.with_env(ctx, crate::check::diagnostics).unwrap_or_default()
    }

    /// The pom's own vocabulary, coloured by role — see [`crate::paint`].
    ///
    /// Answered from the document alone, so it is the one contribution here that does not wait for
    /// the repository scan and does not care whose project the file is in.
    fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        if !is_pom(&ctx.file_name()) {
            return Vec::new();
        }
        crate::paint::highlights(&Doc::new(ctx.source))
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

    /// A pom in the local repository — the file you land in by following a dependency.
    ///
    /// Described in full, judged not at all. The two halves are the point: the colouring, the
    /// hover and the jump are what you went there for, and a red underline on a line you cannot
    /// edit is noise you cannot clear.
    #[test]
    fn a_pom_in_the_repository_is_described_but_not_judged() {
        // `${nope}` is reported by a check that needs no repository at all, so the contrast below
        // is the gate and not an empty catalog.
        const SOURCE: &str = "<project><groupId>org.x</groupId><artifactId>a</artifactId>\
            <version>1.0</version><dependencies><dependency><groupId>org.y</groupId>\
            <artifactId>b</artifactId><version>${nope}</version></dependency></dependencies></project>";
        let m2 = std::env::temp_dir().join("bennu-mvn-lib-gate").join("repository");
        let ext = MavenExtension::with_repo(LocalRepo::at(&m2));

        let library = m2.join("org/y/b/1.0/b-1.0.pom");
        let there = FileCtx { path: &library, source: SOURCE };
        assert!(ext.diagnostics(&there).is_empty(), "a library pom is not judged");
        assert!(!ext.highlights(&there).is_empty(), "…but it is still coloured");

        // The same text as a file of your own: judged, because you can fix it.
        let mine = Path::new("/p/proj/pom.xml");
        let here = FileCtx { path: mine, source: SOURCE };
        assert!(!ext.diagnostics(&here).is_empty(), "your own pom is still checked");
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
