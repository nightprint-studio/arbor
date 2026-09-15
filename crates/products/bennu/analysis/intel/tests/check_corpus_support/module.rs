//! One corpus module (`java8`, `java21`, and the clean ones through [`validate_module`]): index its
//! sources against the machine's JDK, validate every file the way the editor does, and score the
//! result against the committed golden file.

use std::path::{Path, PathBuf};

use bennu_check::prelude::{check_file_resolved, FileContext};
use bennu_classpath::prelude::{
    resolve_jdk_classpath, ClassMembers, ClassSource, MemberIndex, SourceMemberIndex,
};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::build_project_index_from_sources;
use bennu_query::prelude::IndexResolver;

use super::anchor::Anchors;
use super::golden::{self, Disagreement, JavacDiagnostic};
use super::verdict::{self, BennuError, Verdicts};

/// A Maven module of the corpus and the language level it compiles at.
pub struct CorpusModule {
    pub name: &'static str,
    /// What `resolve_jdk_classpath` is asked for (it falls back to the newest JDK installed).
    pub release: &'static str,
    pub major: u32,
}

/// Everything scored for one module.
pub struct ModuleResult {
    pub name: &'static str,
    pub files: usize,
    pub verdicts: Verdicts,
    pub disagreements: Vec<Disagreement>,
}

pub struct SourceFile {
    /// Relative to `src/main/java`, forward slashes.
    pub rel: String,
    pub text: String,
}

/// Which Bennu diagnostics a run keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// Errors only: warnings are not compile errors, so javac has no opinion on them.
    Errors,
    /// Everything: on a clean module every diagnostic, error or warning, is a false positive.
    All,
}

/// A module's sources, its golden file, and what Bennu reported on it.
pub struct Validated {
    pub sources: Vec<SourceFile>,
    pub golden: Vec<JavacDiagnostic>,
    pub diagnostics: Vec<BennuError>,
}

struct JdkIndex(SourceMemberIndex<Box<dyn ClassSource>>);

impl MemberIndex for JdkIndex {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        self.0.members_of(binary_name)
    }
    // Forwarded like the editor's `JdkMemberIndex` does: a defaulted method left out here would
    // score a product whose annotation and package questions are never answered.
    fn class_annotations(&self, binary_name: &str) -> Option<bennu_classpath::prelude::ClassAnnotations> {
        self.0.class_annotations(binary_name)
    }
    fn package_exists(&self, package: &str) -> Option<bool> {
        self.0.package_exists(package)
    }
}

/// Run and score one error module; `Err` carries the reason it was skipped.
pub fn run_module(root: &Path, module: &CorpusModule) -> Result<ModuleResult, String> {
    let validated = validate_module(root, module, Kept::Errors)?;
    let markers: Vec<_> =
        validated.sources.iter().flat_map(|s| golden::markers_in(&s.rel, &s.text)).collect();
    Ok(ModuleResult {
        name: module.name,
        files: validated.sources.len(),
        disagreements: golden::disagreements(&markers, &validated.golden),
        verdicts: verdict::score(&validated.golden, &validated.diagnostics),
    })
}

/// Index one module against the JDK and validate every file; `Err` carries the reason it was skipped.
pub fn validate_module(root: &Path, module: &CorpusModule, kept: Kept) -> Result<Validated, String> {
    let module_dir = root.join(module.name);
    let golden_path = module_dir.join("expected.txt");
    let golden = golden::read_golden(&golden_path).ok_or_else(|| {
        format!("no golden file at {} (run refresh-expected first)", golden_path.display())
    })?;
    let src_root = module_dir.join("src").join("main").join("java");
    let sources = read_sources(&src_root);
    if sources.is_empty() {
        return Err(format!("no .java sources under {}", src_root.display()));
    }
    let jdk = resolve_jdk_classpath(module.release)
        .map_err(|why| format!("no JDK resolvable for Java {} ({why})", module.release))?;

    // Declared before the resolver `validate_all` builds, so the index files outlive their mmap.
    let temp = TempDir::new(module.name)?;
    let diagnostics = validate_all(&src_root, &sources, jdk, temp.path(), module.major, kept)?;
    Ok(Validated { sources, golden, diagnostics })
}

/// Every Bennu diagnostic `kept` asks for, in every file of the module.
fn validate_all(
    src_root: &Path,
    sources: &[SourceFile],
    jdk: Box<dyn ClassSource>,
    index_dir: &Path,
    major: u32,
    kept: Kept,
) -> Result<Vec<BennuError>, String> {
    let disk: Vec<(PathBuf, String)> =
        sources.iter().map(|s| (src_root.join(&s.rel), s.text.clone())).collect();
    let built = build_project_index_from_sources(&disk, index_dir);
    built.builder.persist().map_err(|why| format!("persist the project index: {why:?}"))?;
    let persisted = PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path())
        .map_err(|why| format!("open the project index: {why:?}"))?;
    let mut resolver = IndexResolver::new(persisted, JdkIndex(SourceMemberIndex::new(jdk)));
    for (simple, binary) in built.type_map.iter() {
        resolver.add_simple_hint(simple, binary);
    }

    // `BENNU_CORPUS_DUMP=ArgsCtorBad` prints every diagnostic, warnings included, of the files whose
    // path contains the value: what to reach for once the report names a miss.
    let dump = std::env::var("BENNU_CORPUS_DUMP").ok().filter(|d| !d.is_empty());
    let mut errors = Vec::new();
    for source in sources {
        let ctx = FileContext {
            file_stem: Path::new(&source.rel).file_stem().map(|s| s.to_string_lossy().into_owned()),
            expected_package: package_of(&source.rel),
            java_major: Some(major),
            // JDK-only corpus: the classpath is exactly what javac compiled against.
            classpath_complete: true,
        };
        let anchors = Anchors::new(&source.text);
        let dumped = dump.as_deref().is_some_and(|d| source.rel.contains(d));
        for diagnostic in check_file_resolved(&source.text, &ctx, &resolver, true) {
            let (first_line, last_line) = anchors.statement_lines(diagnostic.start);
            if dumped {
                eprintln!(
                    "dump {}:{first_line} {} [{}] {}",
                    source.rel, diagnostic.severity, diagnostic.code, diagnostic.message
                );
            }
            if kept == Kept::Errors && diagnostic.severity != "error" {
                continue;
            }
            let code = if diagnostic.code.is_empty() { "<uncoded>".to_string() } else { diagnostic.code };
            errors.push(BennuError {
                file: source.rel.clone(),
                first_line,
                last_line,
                code,
                message: diagnostic.message,
            });
        }
    }
    Ok(errors)
}

/// `corpus/args/ArgsTypeBad.java` → `corpus.args`.
fn package_of(rel: &str) -> Option<String> {
    let (dir, _) = rel.rsplit_once('/')?;
    Some(dir.replace('/', "."))
}

fn read_sources(src_root: &Path) -> Vec<SourceFile> {
    let mut paths = Vec::new();
    collect_java(src_root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let rel = path.strip_prefix(src_root).ok()?.to_string_lossy().replace('\\', "/");
            Some(SourceFile { rel, text })
        })
        .collect()
}

fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

/// A self-cleaning temp dir for the module's persisted index.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("bennu-check-corpus-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).map_err(|why| format!("create {}: {why}", path.display()))?;
        Ok(TempDir(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort: on Windows the index mmap may hold the directory for a moment.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
