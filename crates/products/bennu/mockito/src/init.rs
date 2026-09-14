//! `@Mock` fields nothing initialises — and the one annotation that would.
//!
//! An annotation that does nothing on its own is the easiest thing in Java to forget. `@Mock` is read
//! by MockitoExtension, by MockitoJUnitRunner, or by `MockitoAnnotations.openMocks(this)`; without
//! one of them the field is simply null, and the test dies with a `NullPointerException` on the first
//! stubbing — which reads like a bug in the code under test, not in the test's header.
//!
//! Initialisation can come from a long way off: a superclass, an interface carrying `@ExtendWith`, a
//! composed annotation, a Spring test slice, a rule, a helper passed `this`. So the report is made
//! only where the class says plainly that it has none of those, and silence is the answer to every
//! doubt.

use bennu_ext::prelude::ExtEdit;
use bennu_facts::prelude::{AnnotationTable, JavaFacts, KnownAnnotation, TypeFacts};
use bennu_intentions::prelude::insert_import_edit;
use bennu_java::prelude::{node_text, type_declarations};
use tree_sitter::Node;

use crate::edits::{from_edit, merge_inserts};
use crate::file::{assigned_identifier, descendants, JavaFile};

const MOCKITO_ANNOTATIONS: AnnotationTable = AnnotationTable::new(&[
    KnownAnnotation { simple: "Mock", packages: &["org.mockito"] },
    KnownAnnotation { simple: "Spy", packages: &["org.mockito"] },
    KnownAnnotation { simple: "Captor", packages: &["org.mockito"] },
    KnownAnnotation { simple: "InjectMocks", packages: &["org.mockito"] },
]);
const INITIALISED_FIELDS: &[&str] = &["Mock", "Spy", "Captor", "InjectMocks"];
const MOCKED_FIELDS: &[&str] = &["Mock", "Spy"];

const JUPITER: &str = "org.junit.jupiter.api";
const JUNIT: AnnotationTable = AnnotationTable::new(&[
    KnownAnnotation { simple: "Test", packages: &[JUPITER, "org.junit"] },
    KnownAnnotation { simple: "ParameterizedTest", packages: &["org.junit.jupiter.params"] },
    KnownAnnotation { simple: "RepeatedTest", packages: &[JUPITER] },
    KnownAnnotation { simple: "TestFactory", packages: &[JUPITER] },
    KnownAnnotation { simple: "TestTemplate", packages: &[JUPITER] },
    KnownAnnotation { simple: "DisplayName", packages: &[JUPITER] },
    KnownAnnotation { simple: "DisplayNameGeneration", packages: &[JUPITER] },
    KnownAnnotation { simple: "Tag", packages: &[JUPITER] },
    KnownAnnotation { simple: "Tags", packages: &[JUPITER] },
    KnownAnnotation { simple: "TestInstance", packages: &[JUPITER] },
    KnownAnnotation { simple: "TestMethodOrder", packages: &[JUPITER] },
    KnownAnnotation { simple: "TestClassOrder", packages: &[JUPITER] },
    KnownAnnotation { simple: "Nested", packages: &[JUPITER] },
    KnownAnnotation { simple: "Disabled", packages: &[JUPITER] },
    KnownAnnotation { simple: "Timeout", packages: &[JUPITER] },
    KnownAnnotation { simple: "Execution", packages: &["org.junit.jupiter.api.parallel"] },
]);
const TEST_METHODS: &[&str] =
    &["Test", "ParameterizedTest", "RepeatedTest", "TestFactory", "TestTemplate"];

/// Class-level annotations that certainly register nothing. A closed list on purpose: anything else —
/// `@ExtendWith` of any extension, `@RunWith` of any runner, `@SpringBootTest`, a project's composed
/// annotation — might initialise mocks, and a check cannot tell which. `SuppressWarnings` is
/// `java.lang`'s, needs no import, and is matched by name.
const INERT_CLASS_ANNOTATIONS: &[&str] = &[
    "DisplayName", "DisplayNameGeneration", "Tag", "Tags", "TestInstance", "TestMethodOrder",
    "TestClassOrder", "Nested", "Disabled", "Timeout", "Execution", "SuppressWarnings",
];

/// Text that means somebody initialises mocks somewhere in the file, in a way not worth modelling.
const INITIALISERS: &[&str] = &[
    "openMocks(", "initMocks(", "MockitoAnnotations", "MockitoJUnit", "MockitoSession",
    "mockitoSession", "MockitoExtension", "@Rule", "@ClassRule", "@RegisterExtension",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Runner {
    Jupiter,
    JUnit4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsetField {
    /// The annotation's simple name (`Mock`), and its span — the squiggle.
    pub annotation: String,
    pub field: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UninitialisedClass {
    pub class: String,
    /// Where the class declaration starts — its first modifier or annotation.
    pub class_start: usize,
    pub runner: Runner,
    pub fields: Vec<UnsetField>,
}

pub(crate) fn uninitialised_classes(file: &JavaFile<'_>) -> Vec<UninitialisedClass> {
    let Some(runner) = runner_of(file) else { return Vec::new() };
    if INITIALISERS.iter().any(|marker| file.source.contains(*marker)) || passes_this(file) {
        return Vec::new();
    }
    let assigned = assigned_names(file);
    let declarations = type_declarations(file.root());
    let mut out = Vec::new();
    for class in &file.facts.types {
        if class.kind != "class" || class.is_abstract || !has_own_tests(&file.facts, class) {
            continue;
        }
        let fields = unset_fields(&file.facts, class, &assigned);
        if fields.is_empty() {
            continue;
        }
        let Some(node) = declarations.iter().copied().find(|n| declares(*n, class)) else { continue };
        if !self_contained(file, node) {
            continue;
        }
        out.push(UninitialisedClass {
            class: class.name.clone(),
            class_start: node.start_byte(),
            runner,
            fields,
        });
    }
    out
}

/// How many `@Mock` / `@Spy` fields a file declares — the headline number.
pub(crate) fn mocked_field_count(facts: &JavaFacts) -> usize {
    facts
        .types
        .iter()
        .flat_map(|t| &t.fields)
        .filter(|f| f.annotations.iter().any(|a| MOCKITO_ANNOTATIONS.is_any(a, facts, MOCKED_FIELDS).is_some()))
        .count()
}

/// The edits that add the extension (or, on JUnit 4, the runner) to the class, with its imports.
pub(crate) fn add_extension(file: &JavaFile<'_>, target: &UninitialisedClass) -> Vec<ExtEdit> {
    let (annotation, imports) = match target.runner {
        Runner::Jupiter => (
            "@ExtendWith(MockitoExtension.class)",
            ["org.junit.jupiter.api.extension.ExtendWith", "org.mockito.junit.jupiter.MockitoExtension"],
        ),
        Runner::JUnit4 => (
            "@RunWith(MockitoJUnitRunner.class)",
            ["org.junit.runner.RunWith", "org.mockito.junit.MockitoJUnitRunner"],
        ),
    };
    let source = file.source;
    let line_start = source[..target.class_start].rfind('\n').map_or(0, |i| i + 1);
    let before = &source[line_start..target.class_start];
    // The class's own indentation, so a nested class gets its annotation lined up with it.
    let indent = if before.chars().all(|c| c == ' ' || c == '\t') { before } else { "" };
    let mut edits: Vec<ExtEdit> = imports
        .iter()
        .copied()
        .filter(|fqn| !already_imported(&file.facts, fqn))
        .filter_map(|fqn| insert_import_edit(source, fqn))
        .map(from_edit)
        .collect();
    edits.push(ExtEdit::insert(target.class_start, format!("{annotation}\n{indent}")));
    merge_inserts(edits)
}

/// JUnit 5 or JUnit 4 — and neither when the file is TestNG, or mixes the two.
fn runner_of(file: &JavaFile<'_>) -> Option<Runner> {
    if file.source.contains("org.testng") {
        return None;
    }
    let imports = &file.facts.imports;
    let jupiter = imports.iter().any(|i| i.starts_with("org.junit.jupiter."));
    let junit4 = imports.iter().any(|i| i == "org.junit.Test" || i == "org.junit.*");
    match (jupiter, junit4) {
        (true, false) => Some(Runner::Jupiter),
        (false, true) => Some(Runner::JUnit4),
        _ => None,
    }
}

/// Whether anything in the file hands `this` to a call — `TestSupport.setUp(this)` may well be
/// `openMocks` one method away.
fn passes_this(file: &JavaFile<'_>) -> bool {
    descendants(file.root()).into_iter().any(|n| {
        if n.kind() != "argument_list" {
            return false;
        }
        let mut cursor = n.walk();
        let found = n.named_children(&mut cursor).any(|a| a.kind() == "this");
        found
    })
}

/// Every name assigned or initialised anywhere in the file. A `@Mock` field set by hand in a
/// `@BeforeEach` is not null, whatever its annotation says.
fn assigned_names<'s>(file: &JavaFile<'s>) -> Vec<&'s str> {
    let source = file.source;
    descendants(file.root())
        .into_iter()
        .filter_map(|n| match n.kind() {
            "assignment_expression" => n.child_by_field_name("left").and_then(assigned_identifier),
            "variable_declarator" if n.child_by_field_name("value").is_some() => {
                n.child_by_field_name("name")
            }
            _ => None,
        })
        .map(|n| node_text(&n, source))
        .collect()
}

fn unset_fields(facts: &JavaFacts, class: &TypeFacts, assigned: &[&str]) -> Vec<UnsetField> {
    let mut out: Vec<UnsetField> = Vec::new();
    for field in class.fields.iter().filter(|f| !f.is_static && !assigned.contains(&f.name.as_str())) {
        for annotation in &field.annotations {
            let Some(name) = MOCKITO_ANNOTATIONS.is_any(annotation, facts, INITIALISED_FIELDS) else {
                continue;
            };
            // `@Mock Repo a, b;` is one annotation over two fields — one squiggle.
            if out.iter().any(|f| f.start == annotation.start) {
                continue;
            }
            out.push(UnsetField {
                annotation: name.to_string(),
                field: field.name.clone(),
                start: annotation.start,
                end: annotation.end,
            });
        }
    }
    out
}

/// A class with no tests of its own is a base someone else runs — with an extension this file does
/// not show.
fn has_own_tests(facts: &JavaFacts, class: &TypeFacts) -> bool {
    class
        .methods
        .iter()
        .any(|m| m.annotations.iter().any(|a| JUNIT.is_any(a, facts, TEST_METHODS).is_some()))
}

fn declares(node: Node<'_>, class: &TypeFacts) -> bool {
    node.kind() == "class_declaration"
        && node.child_by_field_name("name").is_some_and(|name| name.start_byte() == class.name_offset)
}

fn type_facts_of<'f>(file: &'f JavaFile<'_>, class: Node<'_>) -> Option<&'f TypeFacts> {
    let name = class.child_by_field_name("name")?.start_byte();
    file.facts.types.iter().find(|t| t.name_offset == name)
}

/// Whether the class and every class around it (a `@Nested` test inherits its outer class's
/// extensions) say plainly that nothing initialises their mocks: no superclass, no interface, nothing
/// abstract, and no class-level annotation outside the inert list.
fn self_contained(file: &JavaFile<'_>, class: Node<'_>) -> bool {
    let mut node = class;
    loop {
        if node.kind() != "class_declaration" {
            return false;
        }
        let Some(facts) = type_facts_of(file, node) else { return false };
        let inert = facts
            .annotations
            .iter()
            .all(|a| JUNIT.is_any(a, &file.facts, INERT_CLASS_ANNOTATIONS).is_some());
        if !inert || facts.is_abstract || !facts.extends.is_empty() || !facts.implements.is_empty() {
            return false;
        }
        let Some(parent) = node.parent() else { return false };
        match parent.kind() {
            "program" => return true,
            "class_body" => match parent.parent() {
                Some(outer) => node = outer,
                None => return false,
            },
            // A local or anonymous class: whatever runs it is out of sight.
            _ => return false,
        }
    }
}

fn already_imported(facts: &JavaFacts, fqn: &str) -> bool {
    let package = fqn.rsplit_once('.').map_or("", |(p, _)| p);
    facts.imports.iter().any(|i| i == fqn || i.strip_suffix(".*") == Some(package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::apply;

    const JUPITER_TEST: &str = "package p;\n\nimport org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\n\nclass OrderServiceTest {\n    @Mock\n    OrderRepository repo;\n\n    @Test\n    void saves() {\n    }\n}\n";

    fn found(src: &str) -> Vec<UninitialisedClass> {
        uninitialised_classes(&JavaFile::parse(src).expect("parses"))
    }

    fn fixed(src: &str) -> String {
        let file = JavaFile::parse(src).expect("parses");
        let target = uninitialised_classes(&file).into_iter().next().expect("a finding");
        apply(src, &add_extension(&file, &target))
    }

    #[test]
    fn a_jupiter_test_with_mocks_and_no_extension_is_reported() {
        let found = found(JUPITER_TEST);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].runner, Runner::Jupiter);
        let field = &found[0].fields[0];
        assert_eq!((field.annotation.as_str(), field.field.as_str()), ("Mock", "repo"));
        assert_eq!(&JUPITER_TEST[field.start..field.end], "@Mock");
    }

    #[test]
    fn the_fix_adds_the_extension_and_both_imports_in_order() {
        assert_eq!(
            fixed(JUPITER_TEST),
            "package p;\n\nimport org.junit.jupiter.api.Test;\nimport org.junit.jupiter.api.extension.ExtendWith;\nimport org.mockito.Mock;\nimport org.mockito.junit.jupiter.MockitoExtension;\n\n@ExtendWith(MockitoExtension.class)\nclass OrderServiceTest {\n    @Mock\n    OrderRepository repo;\n\n    @Test\n    void saves() {\n    }\n}\n"
        );
    }

    #[test]
    fn junit4_gets_the_runner() {
        let src = "import org.junit.Test;\nimport org.mockito.Mock;\n\npublic class LegacyTest {\n    @Mock private Repo repo;\n\n    @Test public void t() { }\n}\n";
        assert_eq!(
            fixed(src),
            "import org.junit.Test;\nimport org.junit.runner.RunWith;\nimport org.mockito.Mock;\nimport org.mockito.junit.MockitoJUnitRunner;\n\n@RunWith(MockitoJUnitRunner.class)\npublic class LegacyTest {\n    @Mock private Repo repo;\n\n    @Test public void t() { }\n}\n"
        );
    }

    #[test]
    fn a_nested_test_gets_the_annotation_at_its_own_indentation() {
        let src = "import org.junit.jupiter.api.Nested;\nimport org.junit.jupiter.api.Tag;\nimport org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\n\n@Tag(\"slow\")\nclass OuterTest {\n    @Nested\n    class Inner {\n        @Mock Repo repo;\n        @Test void t() { }\n    }\n}\n";
        let out = fixed(src);
        assert!(
            out.contains("\n    @ExtendWith(MockitoExtension.class)\n    @Nested\n    class Inner {"),
            "{out}"
        );
    }

    #[test]
    fn anything_that_might_initialise_the_mocks_silences_the_check() {
        let cases = [
            ("the extension", "@ExtendWith(MockitoExtension.class)\nclass T {"),
            ("another extension", "@ExtendWith(SpringExtension.class)\nclass T {"),
            ("a Spring test slice", "@SpringBootTest\nclass T {"),
            ("a superclass", "class T extends BaseTest {"),
            ("an interface", "class T implements WithMocks {"),
            ("abstract", "abstract class T {"),
        ];
        for (why, header) in cases {
            let src = format!(
                "import org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\n{header}\n    @Mock Repo repo;\n    @Test void t() {{ }}\n}}\n"
            );
            assert!(found(&src).is_empty(), "{why}");
        }
    }

    #[test]
    fn initialisation_in_the_body_silences_the_check() {
        let bodies = [
            "@BeforeEach void setUp() { MockitoAnnotations.openMocks(this); }",
            "@BeforeEach void setUp() { TestSupport.init(this); }",
            "@BeforeEach void setUp() { repo = org.mockito.Mockito.mock(Repo.class); }",
        ];
        for body in bodies {
            let src = format!(
                "import org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\nclass T {{\n    @Mock Repo repo;\n    {body}\n    @Test void t() {{ }}\n}}\n"
            );
            assert!(found(&src).is_empty(), "{body}");
        }
    }

    #[test]
    fn testng_and_classes_without_tests_are_not_judged() {
        let testng = "import org.testng.annotations.Test;\nimport org.mockito.Mock;\nclass T { @Mock Repo repo; @Test void t() { } }";
        assert!(found(testng).is_empty());
        let base = "import org.junit.jupiter.api.Test;\nimport org.mockito.Mock;\nclass Base { @Mock Repo repo; }";
        assert!(found(base).is_empty(), "a base class is run by someone else");
    }

    #[test]
    fn somebody_elses_mock_annotation_is_not_mockitos() {
        let src = "import org.junit.jupiter.api.Test;\nimport com.acme.Mock;\nclass T { @Mock Repo repo; @Test void t() { } }";
        assert!(found(src).is_empty());
    }

    #[test]
    fn the_headline_counts_mocks_and_spies() {
        let facts = JavaFile::parse("import org.mockito.*;\nclass T { @Mock A a; @Spy B b; @Captor C c; D d; }").unwrap().facts;
        assert_eq!(mocked_field_count(&facts), 2);
    }
}
