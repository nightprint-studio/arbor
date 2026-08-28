//! Shared end-to-end harness for the `bennu-intel` integration tests.
//!
//! Each [`Project`] builds the **real** pipeline over a set of in-memory Java files: it
//! persists a symbol index to a throwaway temp dir, opens a live `SemanticEngine` (the real
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
use std::sync::Arc;

use bennu_classpath::prelude::{
    ClassFlags as CpClassFlags, ClassMembers as CpClassMembers, Member as CpMember,
    MemberIndex as CpMemberIndex, MemberKind as CpMemberKind, TypeRef as CpTypeRef,
    Visibility as CpVisibility,
};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{
    build_project_index_from_sources, rename_apply, CompletionItem, DeclarationLocation, Edit,
    HierarchyDirection, HierarchyItem, HoverInfo, ReferencesResult, RenamePlan, SemanticEngine,
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

/// A live project over the real index + semantic engine.
///
/// Field order matters: `engine` (which mmaps the persisted index) is declared before `_temp`
/// so it drops FIRST, releasing the mapping before the temp dir is removed (Windows os error
/// 1224 otherwise).
pub struct Project {
    engine: SemanticEngine,
    /// A second resolver over the SAME persisted index, kept for member-access completion.
    /// The engine's own resolver is `project_only` + private; completion wants an
    /// `IndexResolver` it can pass to `completion(...)`, so we open one here. It mmaps the
    /// index files, so it must drop before `_temp` (declared before it).
    completion_resolver: IndexResolver<StreamJdk>,
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
        Self::build(files, jdk, false)
    }

    /// Like [`Project::new`], but the semantic engine also resolves the faked JDK stream types
    /// ([`StreamJdk`]) — i.e. it gets the kind of fully-resolving resolver the provider lends it
    /// in production, instead of a project-only one. Use for anything that types a receiver
    /// THROUGH a library generic.
    pub fn with_stream_jdk(files: &[(&str, &str)]) -> Self {
        Self::build(files, "21", true)
    }

    fn build(files: &[(&str, &str)], jdk: &str, stream_jdk: bool) -> Self {
        let temp = TempDir::new();
        // A `gN` gen subdir so the engine's reference cache lands at `temp/…` (unique per
        // project), never a shared path across tests.
        let index_dir = temp.path().join("g000");
        std::fs::create_dir_all(&index_dir).expect("create index dir");

        let disk_sources: Vec<(PathBuf, String)> = files
            .iter()
            .map(|(p, s)| (PathBuf::from(*p), s.to_string()))
            .collect();
        let built = build_project_index_from_sources(&disk_sources, &index_dir);
        built.builder.persist().expect("persist symbol index");

        let pairs: Vec<(String, String)> = built
            .type_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let java_sources: Vec<(String, String)> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();

        let blob = index_dir.join("symbols.blob");
        let fst = index_dir.join("names.fst");

        // The resolver the engine borrows. `None` = the production fallback (project-only, built
        // inside `for_project`); `Some` mirrors production, where the provider lends its own.
        let shared: Option<Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>> = stream_jdk
            .then(|| {
                let persisted = PersistedIndex::open(&blob, &fst).expect("open index for engine");
                let mut r = IndexResolver::new(persisted, StreamJdk);
                for (simple, binary) in &pairs {
                    r.add_simple_hint(simple, binary);
                }
                Arc::new(r) as Arc<dyn bennu_java::prelude::TypeResolver + Send + Sync>
            });

        let engine = SemanticEngine::for_project(
            &index_dir,
            jdk,
            &pairs,
            java_sources,
            vec![],
            shared,
            &|_, _| {},
        )
        .expect("build semantic engine");

        // A completion resolver over the same on-disk index, WITH the faked JDK.
        //
        // It used to be JDK-free, which read as "project types only" and was fine while every
        // completion test asked about a member of a project class. It stopped being fine the moment
        // one asked about a lambda parameter: typing that walks the receiver's hierarchy to find the
        // functional interface, the walk crosses `java/lang/Object`, and a hierarchy with an
        // unresolvable link is abandoned rather than guessed at — so the answer was empty for a
        // reason that had nothing to do with the code under test.
        //
        // Production completion runs on the provider's FULL resolver (see `IndexService::completion`
        // — "the project's FULL resolver, the one completion uses"), so a JDK-free one here was
        // testing a configuration that does not exist. `StreamJdk` is the same fake the engine
        // resolver already uses.
        let persisted = PersistedIndex::open(&blob, &fst).expect("open index for completion");
        let mut completion_resolver = IndexResolver::new(persisted, StreamJdk);
        for (simple, binary) in &pairs {
            completion_resolver.add_simple_hint(simple, binary);
        }

        let sources = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        Self {
            engine,
            completion_resolver,
            sources,
            _temp: temp,
        }
    }

    /// Whether the project index recognises `binary` as a type this project declares.
    ///
    /// This is the guard that keeps go-to from opening a decompiled stub of the user's own code:
    /// `library_binary` refuses a binary the project declares. A nested type is where it is most
    /// likely to fail, because source and bytecode spell one differently.
    pub fn is_project_type(&self, binary: &str) -> bool {
        use bennu_java::prelude::TypeResolver;
        self.completion_resolver.is_project_type(binary)
    }

    /// The source text of `file` (for computing expected offsets / lines).
    pub fn source(&self, file: &str) -> &str {
        self.sources
            .get(file)
            .unwrap_or_else(|| panic!("no such file: {file}"))
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
        self.find_usages(file, offset)
            .map(|r| r.usages.len())
            .unwrap_or(0)
    }

    /// Member-access completion at `file`:`offset` — the caret is expected to sit just after a
    /// `receiver.` (optionally with a partial prefix already typed). Returns the candidate items
    /// (sorted fields-then-methods, alpha within), exactly as the provider serves them.
    pub fn complete(&self, file: &str, offset: usize) -> Vec<CompletionItem> {
        completion(self.source(file), offset, &self.completion_resolver)
    }

    /// Just the completion labels (member names) offered at `file`:`offset`.
    pub fn complete_labels(&self, file: &str, offset: usize) -> Vec<String> {
        self.complete(file, offset)
            .into_iter()
            .map(|c| c.label)
            .collect()
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
        self.rename(file, offset, new_name)
            .map(|p| rename_apply(&p))
            .unwrap_or_default()
    }

    /// The root of the CALL hierarchy for the symbol at `file`:`offset` (empty on a caret that is
    /// not on a project method).
    pub fn call_hierarchy(&self, file: &str, offset: usize) -> Vec<HierarchyItem> {
        self.engine
            .prepare_hierarchy(file, self.source(file), offset, true)
    }

    /// The root of the TYPE hierarchy for the symbol at `file`:`offset` — a caret on a member
    /// climbs to its owner.
    pub fn type_hierarchy(&self, file: &str, offset: usize) -> Vec<HierarchyItem> {
        self.engine
            .prepare_hierarchy(file, self.source(file), offset, false)
    }

    /// One level below `item`, in `direction`.
    pub fn hierarchy_step(
        &self,
        item: &HierarchyItem,
        direction: HierarchyDirection,
    ) -> Vec<HierarchyItem> {
        self.engine.hierarchy_step(&item.handle, direction)
    }

    /// Every diagnostic the validator reports for `file`, against this project's **real** index.
    ///
    /// The corpus runs kept finding false positives that no unit test could have caught, because a
    /// check's own tests are written against a mock resolver by the person who wrote the check —
    /// and the false positives were all cases where the *index* answered something the mock never
    /// would (one binary per simple name, a nested type of the wrong outer). Reproducing one needs
    /// the real thing: several files, a real build, a real resolver.
    ///
    /// `java_major` is 21 and the classpath is declared incomplete, matching what the be layer
    /// passes for a project whose dependencies are not all resolvable.
    pub fn validate(&self, file: &str) -> Vec<bennu_proto::prelude::Diagnostic> {
        let ctx = bennu_check::prelude::FileContext {
            file_stem: std::path::Path::new(file)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string()),
            expected_package: None,
            java_major: Some(21),
            classpath_complete: false,
        };
        bennu_check::prelude::check_file_resolved(
            self.source(file),
            &ctx,
            &self.completion_resolver,
            true,
        )
    }

    /// The `code`s of the ERROR-severity diagnostics on `file` — what a false-positive test asserts
    /// is empty.
    pub fn validate_errors(&self, file: &str) -> Vec<String> {
        self.validate(file)
            .into_iter()
            .filter(|d| d.severity == "error")
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
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
    src.find(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found in source"))
}

/// Byte offset of the LAST occurrence of `needle` in `src`.
pub fn at_last(src: &str, needle: &str) -> usize {
    src.rfind(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found in source"))
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

// ── a hand-built stand-in for the JDK classes a stream chain runs through ─────────

/// The three JDK types a `list.stream().map(x -> …)` chain needs in order to type `x`, built by
/// hand so the test is deterministic and needs no JDK install.
///
/// They matter as **conduits**, not destinations: `x` is a PROJECT type that only reaches the
/// lambda by being substituted through `List<E>` → `Stream<T>` → `Function<T, R>`. Drop any link
/// and `x` is untyped, no edge is recorded for `x.foo()`, and a rename of `foo` misses that call.
pub struct StreamJdk;

fn iface(type_params: &[&str], methods: Vec<CpMember>) -> CpClassMembers {
    CpClassMembers {
        superclass: None,
        interfaces: Vec::new(),
        methods,
        fields: Vec::new(),
        flags: CpClassFlags {
            is_interface: true,
            is_abstract: true,
            ..Default::default()
        },
        type_params: type_params.iter().map(|s| s.to_string()).collect(),
    }
}

/// A CONCRETE method. Abstract is the exception and is spelled out with [`abstract_`], because
/// getting this backwards is not cosmetic: `java.lang.Object.toString()` is concrete, and a fake JDK
/// that says otherwise gives every interface in the fixture a second abstract method — so nothing
/// looks like a functional interface any more, and every lambda parameter loses its type.
fn method(name: &str, params: Vec<CpTypeRef>, ret: CpTypeRef) -> CpMember {
    CpMember {
        name: name.to_string(),
        kind: CpMemberKind::Method,
        return_type: ret,
        params,
        is_static: false,
        is_abstract: false,
        is_default: false,
        is_final: false,
        visibility: CpVisibility::Public,
        raw_signature: name.to_string(),
        throws: Vec::new(),
    }
}

/// Mark a fake-JDK method abstract — for a real SAM like `Function.apply`.
fn abstract_(mut m: CpMember) -> CpMember {
    m.is_abstract = true;
    m
}

/// A generic reference: `applied("java/util/List", ["E"])` is `List<E>`.
fn applied(binary: &str, args: &[&str]) -> CpTypeRef {
    CpTypeRef {
        binary_name: binary.to_string(),
        type_args: args.iter().map(|a| CpTypeRef::plain(*a)).collect(),
    }
}

impl CpMemberIndex for StreamJdk {
    fn members_of(&self, binary_name: &str) -> Option<CpClassMembers> {
        Some(match binary_name {
            // `interface List<E> { Stream<E> stream(); }`
            "java/util/List" => iface(
                &["E"],
                vec![method(
                    "stream",
                    vec![],
                    applied("java/util/stream/Stream", &["E"]),
                )],
            ),
            // `interface Stream<T> { <R> Stream<R> map(Function<? super T, ? extends R> f); }`
            "java/util/stream/Stream" => iface(
                &["T"],
                vec![method(
                    "map",
                    vec![applied("java/util/function/Function", &["T", "R"])],
                    applied("java/util/stream/Stream", &["R"]),
                )],
            ),
            // `interface Function<T, R> { R apply(T t); }` — the functional interface whose single
            // abstract method's parameter type IS the lambda parameter's type.
            "java/util/function/Function" => iface(
                &["T", "R"],
                vec![abstract_(method(
                    "apply",
                    vec![CpTypeRef::plain("T")],
                    CpTypeRef::plain("R"),
                ))],
            ),
            // Every enum implicitly extends this, and `name()` / `ordinal()` are declared nowhere in
            // the project — so a project enum's `e.name()` resolves only if the walk can see it.
            "java/lang/Enum" => CpClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![
                    method("name", vec![], CpTypeRef::plain("java/lang/String")),
                    method("ordinal", vec![], CpTypeRef::plain("int")),
                ],
                fields: Vec::new(),
                flags: CpClassFlags::default(),
                type_params: Vec::new(),
            },
            "java/lang/Object" => CpClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![method(
                    "toString",
                    vec![],
                    CpTypeRef::plain("java/lang/String"),
                )],
                fields: Vec::new(),
                flags: CpClassFlags::default(),
                type_params: Vec::new(),
            },
            "java/lang/String" | "java/lang/Record" => CpClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: CpClassFlags::default(),
                type_params: Vec::new(),
            },
            _ => return None,
        })
    }
}
