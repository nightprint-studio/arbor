//! Shared end-to-end harness for the `bennu-intel` integration tests.
//!
//! Each [`Project`] builds the **real** pipeline over a set of in-memory Java files: it
//! persists a symbol index to a throwaway temp dir, opens a live `RenameEngine` (the real
//! `IndexResolver` + reference walk), and exposes go-to-declaration / find-usages exactly as
//! the backend serves them. No fakes — a test that passes here reflects production behaviour.
//!
//! Offsets are located by substring: `at(src, "foo")` is the byte offset of the first `foo`,
//! `at_last(src, "foo")` the last. Pick a distinctive needle to target a specific occurrence
//! (e.g. `at(src, "return ctx")` then `+ "return ".len()` to land on the `ctx` usage).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bennu_classpath::prelude::{ClassMembers as CpClassMembers, MemberIndex as CpMemberIndex};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{
    build_project_index_from_sources, rename_apply, CompletionItem, DeclarationLocation,
    Edit, HoverInfo, ReferencesResult, RenameEngine, RenamePlan,
};
use bennu_query::prelude::{completion, IndexResolver, InheritedMember};

/// A JDK member source that resolves nothing. The completion resolver is generic over a
/// `MemberIndex`; feeding it this stub keeps the whole suite free of a live JDK install —
/// completion is exercised purely over PROJECT-declared types, whose members are baked into
/// the persisted index (JDK-type completion is a separate, environment-dependent concern).
struct NoJdk;
impl CpMemberIndex for NoJdk {
    fn members_of(&self, _binary_name: &str) -> Option<CpClassMembers> {
        None
    }
}

/// A self-cleaning unique temp directory (no `tempfile` dependency).
pub struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "bennu-intel-it-{}-{:?}-{n}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create temp dir");
        TempDir(base)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort: on Windows the just-dropped index mmap may still hold the dir briefly.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A live project over the real index + rename engine.
///
/// Field order matters: `engine` (which mmaps the persisted index) is declared before `_temp`
/// so it drops FIRST, releasing the mapping before the temp dir is removed (Windows os error
/// 1224 otherwise).
pub struct Project {
    engine: RenameEngine,
    /// A second resolver over the SAME persisted index, kept for member-access completion.
    /// The engine's own resolver is `project_only` + private; completion wants an
    /// `IndexResolver` it can pass to `completion(...)`, so we open one here. It mmaps the
    /// index files, so it must drop before `_temp` (declared before it).
    completion_resolver: IndexResolver<NoJdk>,
    sources: HashMap<String, String>,
    _temp: TempDir,
}

impl Project {
    /// Build a project at a modern language level (all binding forms enabled).
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self::with_jdk(files, "21")
    }

    /// Build a project pinned to a specific Java language level (`"8"`, `"17"`, …) — for the
    /// version-gated constructs (records / pattern variables / inferred lambda params).
    pub fn with_jdk(files: &[(&str, &str)], jdk: &str) -> Self {
        let temp = TempDir::new();
        // A `gN` gen subdir so the engine's reference cache lands at `temp/…` (unique per
        // project), never a shared path across tests.
        let index_dir = temp.path().join("g000");
        std::fs::create_dir_all(&index_dir).expect("create index dir");

        let disk_sources: Vec<(PathBuf, String)> =
            files.iter().map(|(p, s)| (PathBuf::from(*p), s.to_string())).collect();
        let built = build_project_index_from_sources(&disk_sources, &index_dir);
        built.builder.persist().expect("persist symbol index");

        let pairs: Vec<(String, String)> =
            built.type_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let java_sources: Vec<(String, String)> =
            files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();

        let engine = RenameEngine::for_project(&index_dir, jdk, &pairs, java_sources, vec![], &|_, _| {})
            .expect("build rename engine");

        // A completion resolver over the same on-disk index (JDK-free — project types only).
        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");
        let persisted = PersistedIndex::open(&blob, &fst).expect("open index for completion");
        let mut completion_resolver = IndexResolver::new(persisted, NoJdk);
        for (simple, binary) in &pairs {
            completion_resolver.add_simple_hint(simple, binary);
        }

        let sources = files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        Self { engine, completion_resolver, sources, _temp: temp }
    }

    /// The source text of `file` (for computing expected offsets / lines).
    pub fn source(&self, file: &str) -> &str {
        self.sources.get(file).unwrap_or_else(|| panic!("no such file: {file}"))
    }

    /// Go-to-declaration for the symbol at `file`:`offset`.
    pub fn goto(&self, file: &str, offset: usize) -> Option<DeclarationLocation> {
        self.engine.declaration(file, self.source(file), offset)
    }

    /// The go-to label (`"local `x`"`, `"method p.A.foo()"`, `"class p.A"`, …), or `None`.
    pub fn goto_label(&self, file: &str, offset: usize) -> Option<String> {
        self.goto(file, offset).map(|d| d.label)
    }

    /// Find-usages for the symbol at `file`:`offset` (the use-site count is on `.usages`).
    pub fn find_usages(&self, file: &str, offset: usize) -> Option<ReferencesResult> {
        self.engine.find_usages(file, self.source(file), offset)
    }

    /// The number of recorded use sites for the symbol at `file`:`offset` (0 if unresolved).
    pub fn usage_count(&self, file: &str, offset: usize) -> usize {
        self.find_usages(file, offset).map(|r| r.usages.len()).unwrap_or(0)
    }

    /// Member-access completion at `file`:`offset` — the caret is expected to sit just after a
    /// `receiver.` (optionally with a partial prefix already typed). Returns the candidate items
    /// (sorted fields-then-methods, alpha within), exactly as the provider serves them.
    pub fn complete(&self, file: &str, offset: usize) -> Vec<CompletionItem> {
        completion(self.source(file), offset, &self.completion_resolver)
    }

    /// Just the completion labels (member names) offered at `file`:`offset`.
    pub fn complete_labels(&self, file: &str, offset: usize) -> Vec<String> {
        self.complete(file, offset).into_iter().map(|c| c.label).collect()
    }

    /// `true` if a completion candidate named `name` is offered at `file`:`offset`.
    pub fn completes_with(&self, file: &str, offset: usize, name: &str) -> bool {
        self.complete_labels(file, offset).iter().any(|l| l == name)
    }

    /// Hover card for the symbol at `file`:`offset` (`None` for a local / unresolvable caret).
    pub fn hover(&self, file: &str, offset: usize) -> Option<HoverInfo> {
        self.engine.hover(file, self.source(file), offset)
    }

    /// Rename PLAN for the symbol at `file`:`offset` → `new_name` (`None` on a junk caret).
    pub fn rename(&self, file: &str, offset: usize, new_name: &str) -> Option<RenamePlan> {
        self.engine.plan(file, self.source(file), offset, new_name)
    }

    /// The flat edit list a rename would apply (empty when the caret isn't renameable).
    pub fn rename_edits(&self, file: &str, offset: usize, new_name: &str) -> Vec<Edit> {
        self.rename(file, offset, new_name).map(|p| rename_apply(&p)).unwrap_or_default()
    }

    /// The inherited ("super") members of the type named `type_name` declared at `file`:`line`
    /// (1-based). Lists SUPERCLASS + INTERFACE members (not the type's own), project supertypes
    /// only (the engine resolver is project-only, so JDK `Object` members never appear).
    pub fn inherited(&self, file: &str, type_name: &str, line: i64) -> Vec<InheritedMember> {
        self.engine.inherited_members(file, type_name, line)
    }
}

/// Byte offset of the FIRST occurrence of `needle` in `src`.
pub fn at(src: &str, needle: &str) -> usize {
    src.find(needle).unwrap_or_else(|| panic!("needle {needle:?} not found in source"))
}

/// Byte offset of the LAST occurrence of `needle` in `src`.
pub fn at_last(src: &str, needle: &str) -> usize {
    src.rfind(needle).unwrap_or_else(|| panic!("needle {needle:?} not found in source"))
}

/// 1-based line number of the FIRST occurrence of `needle` in `src` (to assert go-to landed
/// on the right declaration line).
pub fn line_of(src: &str, needle: &str) -> u32 {
    let off = at(src, needle);
    1 + src[..off].bytes().filter(|&b| b == b'\n').count() as u32
}

/// 1-based line number of the LAST occurrence of `needle` in `src`.
pub fn line_of_last(src: &str, needle: &str) -> u32 {
    let off = at_last(src, needle);
    1 + src[..off].bytes().filter(|&b| b == b'\n').count() as u32
}
