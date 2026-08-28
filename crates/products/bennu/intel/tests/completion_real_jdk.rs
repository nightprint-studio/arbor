//! Completion against the **real JDK**, not the hand-written stub the rest of the suite uses.
//!
//! Every other completion test resolves library types through a fake `MemberIndex` with a handful
//! of classes on it. That is enough to pin the project-side logic and blind to everything the real
//! bytecode brings: generic signatures, nested types spelled `Outer$Inner`, `Object`'s own members,
//! a hierarchy dozens of links deep. Three reports in a row — enum constants, `Optional`, inner
//! classes — were all about the half no fake reaches.
//!
//! Skipped, loudly, when no JDK 21 resolves: a missing JDK is an environment fact, and a run that
//! silently scored it as agreement would be a lie.

use std::path::PathBuf;

use bennu_classpath::prelude::{
    resolve_jdk_classpath, ClassMembers as CpClassMembers, ClassSource, MemberIndex as CpMemberIndex,
    SourceMemberIndex,
};
use bennu_index::prelude::PersistedIndex;
use bennu_intel::prelude::{build_project_index_from_sources, ClassNameIndex};
use bennu_query::prelude::{completion_in, IndexResolver, TypeNameCatalog};

/// A project whose library tier is the machine's own JDK.
struct RealJdkProject {
    resolver: IndexResolver<JdkIndex>,
    /// The classpath's type-name catalog — what lets a receiver you have not imported yet complete.
    /// Production hands completion the provider's; a harness without one is testing a configuration
    /// that does not exist.
    catalog: ClassNameIndex,
    sources: Vec<(String, String)>,
    _temp: TempDir,
}

struct JdkIndex(SourceMemberIndex<Box<dyn ClassSource>>);

impl CpMemberIndex for JdkIndex {
    fn members_of(&self, binary_name: &str) -> Option<CpClassMembers> {
        self.0.members_of(binary_name)
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "bennu-realjdk-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl RealJdkProject {
    /// `None` when no JDK 21 resolves on this machine — the caller skips.
    fn new(files: &[(&str, &str)]) -> Option<Self> {
        let source = match resolve_jdk_classpath("21") {
            Ok(s) => s,
            Err(why) => {
                eprintln!("SKIPPED: no JDK 21 on this machine ({why})");
                return None;
            }
        };
        let temp = TempDir::new();
        let disk: Vec<(PathBuf, String)> = files
            .iter()
            .map(|(p, s)| (PathBuf::from(*p), s.to_string()))
            .collect();
        let built = build_project_index_from_sources(&disk, &temp.0);
        built.builder.persist().expect("persist");
        let persisted =
            PersistedIndex::open(built.builder.blob_path(), built.builder.fst_path()).expect("open");
        // A second handle on the same JDK for the name catalog: enumerating class names and
        // decoding members are different reads, and `ClassSource` is not `Sync`.
        let mut catalog = ClassNameIndex::new();
        if let Ok(names) = resolve_jdk_classpath("21") {
            catalog.add_binaries(names.class_names());
        }
        for (simple, binary) in built.type_map.iter() {
            catalog.add_fqn(simple, &binary.replace('/', "."));
        }
        catalog.finalize();
        let mut resolver = IndexResolver::new(persisted, JdkIndex(SourceMemberIndex::new(source)));
        for (simple, binary) in built.type_map.iter() {
            resolver.add_simple_hint(simple, binary);
        }
        Some(RealJdkProject {
            resolver,
            catalog,
            sources: files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect(),
            _temp: temp,
        })
    }

    fn source(&self, file: &str) -> &str {
        &self.sources.iter().find(|(p, _)| p == file).expect("no such file").1
    }

    /// The completion labels offered at the caret just after `needle` in `file`.
    fn labels_after(&self, file: &str, needle: &str) -> Vec<String> {
        let src = self.source(file);
        let at = src.find(needle).unwrap_or_else(|| panic!("needle {needle:?} not found"));
        completion_in(
            src,
            at + needle.len(),
            &self.resolver,
            Some(&self.catalog as &dyn TypeNameCatalog),
        )
        .into_iter()
        .map(|c| c.label)
        .collect()
    }
}

/// Every named member must be offered; reports the whole list on failure, since "what DID it offer"
/// is the only useful thing to know here.
fn assert_offers(labels: &[String], wanted: &[&str], what: &str) {
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|w| !labels.iter().any(|l| l == w))
        .collect();
    assert!(
        missing.is_empty(),
        "{what}: missing {missing:?}\noffered ({}): {labels:?}",
        labels.len()
    );
}

macro_rules! project_or_skip {
    ($files:expr) => {
        match RealJdkProject::new($files) {
            Some(p) => p,
            None => return,
        }
    };
}

// ── enums ────────────────────────────────────────────────────────────────────

const HEADERS: &str = r#"package p;
public enum Headers {
    USERNAME("x-username"),
    TENANT("x-tenant");
    private final String header;
    Headers(String header) { this.header = header; }
    public String header_name() { return header; }
}
"#;

/// A project enum's constants, asked for through a real JDK — whose `java/lang/Enum` supertype the
/// walk now actually crosses.
#[test]
fn a_project_enum_offers_its_constants() {
    let p = project_or_skip!(&[
        ("p/Headers.java", HEADERS),
        (
            "p/Use.java",
            "package p;\npublic class Use {\n  void run() { name(Headers.USERNAME.header_name()); }\n  void name(String s) {}\n}\n",
        ),
    ]);
    let labels = p.labels_after("p/Use.java", "name(Headers.");
    assert_offers(&labels, &["USERNAME", "TENANT", "values", "valueOf"], "enum constants");
}

/// A JDK enum, whose constants come from bytecode rather than from the project index.
#[test]
fn a_jdk_enum_offers_its_constants() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.time.DayOfWeek;\npublic class Use {\n  void run() { DayOfWeek d = DayOfWeek.MONDAY; }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "= DayOfWeek.");
    assert_offers(&labels, &["MONDAY", "SUNDAY", "valueOf"], "JDK enum constants");
}

// ── Optional ─────────────────────────────────────────────────────────────────

/// `Optional`'s own members, on a variable declared with it.
#[test]
fn optional_offers_its_members() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.Optional;\npublic class Use {\n  void run(Optional<String> o) { o.toString(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "{ o.");
    assert_offers(
        &labels,
        &["get", "isPresent", "isEmpty", "orElse", "map", "filter", "ifPresent"],
        "Optional members",
    );
}

/// The TYPE ARGUMENT flows through: `Optional<String>.get()` is a `String`, so the next `.` offers
/// `String`'s members. This is generic substitution through a library generic.
#[test]
fn optional_get_is_typed_by_its_type_argument() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.Optional;\npublic class Use {\n  void run(Optional<String> o) { o.get().length(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "o.get().");
    assert_offers(&labels, &["length", "substring", "trim"], "String through Optional<String>");
}

/// `Optional.of(x)` — the type argument is inferred from the ARGUMENT, not written anywhere.
#[test]
fn optional_of_infers_its_type_argument_from_the_argument() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.Optional;\npublic class Use {\n  void run(String s) { Optional.of(s).get().length(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "Optional.of(s).get().");
    assert_offers(&labels, &["length", "trim"], "String through Optional.of");
}

// ── inner / nested types ─────────────────────────────────────────────────────

/// A project's own static nested class, reached through its outer.
#[test]
fn a_project_nested_type_completes_through_its_outer() {
    let p = project_or_skip!(&[
        (
            "p/Outer.java",
            "package p;\npublic class Outer {\n  public static class Inner {\n    public int width() { return 1; }\n    public int height() { return 2; }\n  }\n}\n",
        ),
        (
            "p/Use.java",
            "package p;\npublic class Use {\n  void run() { Outer.Inner i = new Outer.Inner(); i.width(); }\n}\n",
        ),
    ]);
    let labels = p.labels_after("p/Use.java", "i.");
    assert_offers(&labels, &["width", "height"], "members of a nested type");
}

/// The nested type NAME itself is offered after its outer's `.`.
#[test]
fn a_nested_type_name_is_offered_after_its_outer() {
    let p = project_or_skip!(&[
        (
            "p/Outer.java",
            "package p;\npublic class Outer {\n  public static class Inner { }\n  public static int SIZE = 1;\n}\n",
        ),
        (
            "p/Use.java",
            "package p;\npublic class Use {\n  void run() { int n = Outer.SIZE; }\n}\n",
        ),
    ]);
    let labels = p.labels_after("p/Use.java", "= Outer.");
    assert_offers(&labels, &["SIZE", "Inner"], "a nested type is a member of its outer");
}

/// `Map.Entry` — a JDK nested type, which bytecode spells `java/util/Map$Entry` and source spells
/// `java.util.Map.Entry`. Getting the two spellings confused is why this one is here.
#[test]
fn a_jdk_nested_type_completes() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.Map;\npublic class Use {\n  void run(Map.Entry<String, Integer> e) { e.getKey(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "{ e.");
    assert_offers(&labels, &["getKey", "getValue"], "Map.Entry members");
}

/// An INNER (non-static) class reached through an instance of its outer.
#[test]
fn an_inner_class_instance_completes() {
    let p = project_or_skip!(&[
        (
            "p/Outer2.java",
            "package p;\npublic class Outer2 {\n  public class Inner {\n    public int depth() { return 1; }\n  }\n  public Inner make() { return new Inner(); }\n}\n",
        ),
        (
            "p/Use.java",
            "package p;\npublic class Use {\n  void run(Outer2 o) { o.make().depth(); }\n}\n",
        ),
    ]);
    let labels = p.labels_after("p/Use.java", "o.make().");
    assert_offers(&labels, &["depth"], "members of an inner class instance");
}

// ── the ordinary JDK surface a legacy codebase actually touches ──────────────

#[test]
fn a_string_offers_its_members() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\npublic class Use {\n  void run(String s) { s.length(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "{ s.");
    assert_offers(&labels, &["length", "substring", "isEmpty", "equals", "hashCode"], "String");
}

/// A generic collection's element type flows into the next call.
#[test]
fn a_list_element_is_typed_by_its_type_argument() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.List;\npublic class Use {\n  void run(List<String> xs) { xs.get(0).length(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "xs.get(0).");
    assert_offers(&labels, &["length", "trim"], "String through List<String>");
}

/// A stream chain — the shape every modern call site is written in.
#[test]
fn a_stream_chain_keeps_its_element_type() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.List;\npublic class Use {\n  void run(List<String> xs) { xs.stream().filter(x -> true).findFirst().get().length(); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "findFirst().get().");
    assert_offers(&labels, &["length", "trim"], "String through a stream chain");
}

// ── the receiver whose type is not imported yet ──────────────────────────────

/// A JDK type named with NO import — the state you are in the moment you type it.
///
/// `Arrays.` with no `import java.util.Arrays;` above answered nothing at all, which is also the
/// answer for a typo, so there was no way to tell them apart. And with no completion there is no
/// accept, hence no auto-import: the one gesture that would have added the import is behind the
/// completion that the missing import suppressed.
#[test]
fn an_unimported_jdk_type_still_completes() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\npublic class Use {\n  void run(String[] xs) { Arrays.stream(xs); }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "{ Arrays.");
    assert_offers(&labels, &["stream", "asList", "sort"], "an unimported java.util.Arrays");
}

/// The same for a PROJECT type in another package, named with no import.
#[test]
fn an_unimported_project_type_still_completes() {
    let p = project_or_skip!(&[
        (
            "p/other/Codes.java",
            "package p.other;\npublic final class Codes {\n  public static final String OK = \"ok\";\n  public static String of(int n) { return \"\"; }\n}\n",
        ),
        (
            "p/Use.java",
            "package p;\npublic class Use {\n  void run() { String s = Codes.OK; }\n}\n",
        ),
    ]);
    let labels = p.labels_after("p/Use.java", "= Codes.");
    assert_offers(&labels, &["OK", "of"], "an unimported project type");
}

// ── a nested type inside an ANNOTATION type ──────────────────────────────────

/// `@AddHeader(kind = AddHeader.Kind.USERNAME)` — an enum nested inside an annotation type.
///
/// Annotation types are the rarest of the five type declarations, and the walks that gather nested
/// types list the other four often enough that a missing arm here would only show up on the one
/// project that writes them.
const ADD_HEADER: &str = r#"package p;
public @interface AddHeader {
    enum Kind {
        USERNAME,
        TENANT;
        public String header_name() { return name(); }
    }
    Kind kind();
    String value() default "";
}
"#;

const ANNOTATED: &str = r#"package p;
public class Api {
    @AddHeader(kind = AddHeader.Kind.USERNAME, value = "x-username")
    public void one() {}

    void use() {
        String s = AddHeader.Kind.USERNAME.header_name();
    }
}
"#;

/// The annotation's nested type is a member of it, like any other outer/inner pair.
#[test]
fn an_annotation_type_offers_its_nested_enum() {
    let p = project_or_skip!(&[("p/AddHeader.java", ADD_HEADER), ("p/Api.java", ANNOTATED)]);
    let labels = p.labels_after("p/Api.java", "String s = AddHeader.");
    assert_offers(&labels, &["Kind"], "the nested enum of an annotation type");
}

/// And its constants, through the qualified nested name.
#[test]
fn an_enum_nested_in_an_annotation_offers_its_constants() {
    let p = project_or_skip!(&[("p/AddHeader.java", ADD_HEADER), ("p/Api.java", ANNOTATED)]);
    let labels = p.labels_after("p/Api.java", "String s = AddHeader.Kind.");
    assert_offers(&labels, &["USERNAME", "TENANT", "valueOf"], "constants of an annotation's enum");
}

/// The same, written INSIDE the annotation's own argument list — where the caret actually is when
/// you reach for it.
#[test]
fn the_nested_enums_constants_complete_inside_an_annotation_argument() {
    let p = project_or_skip!(&[("p/AddHeader.java", ADD_HEADER), ("p/Api.java", ANNOTATED)]);
    let labels = p.labels_after("p/Api.java", "@AddHeader(kind = AddHeader.Kind.");
    assert_offers(&labels, &["USERNAME", "TENANT"], "constants inside an annotation argument");
}

/// A nested type of a LIBRARY class — `Map.Entry`, reached through its outer.
///
/// The project index knows nothing about a class in a jar, so this half comes from the classpath's
/// own name enumeration. Without it, `Outer.` offered a nested type when you had written the outer
/// and nothing when you had imported it — the same gesture answering differently for no reason the
/// reader can see.
#[test]
fn a_library_type_offers_its_nested_types() {
    let p = project_or_skip!(&[(
        "p/Use.java",
        "package p;\nimport java.util.Map;\npublic class Use {\n  void run() { Map.Entry<String, String> e = null; }\n}\n",
    )]);
    let labels = p.labels_after("p/Use.java", "{ Map.");
    assert_offers(&labels, &["Entry", "of", "entry"], "Map's nested Entry beside its statics");
}
