//! Which classes and methods in a file are tests.
//!
//! The rule is **what the source declares**, never what the file is called. Surefire's
//! `*Test` / `Test*` / `*Tests` defaults are a scanning convention it uses when nobody told
//! it otherwise; they are not what makes a method a test, and on a legacy tree they are
//! wrong in both directions — `OrderTestUtils` is a helper, `CheckoutFlow` may be a suite.
//! What makes a method a test is:
//!
//! - an annotation from the framework in use (`@Test`, and JUnit 5's `@ParameterizedTest`,
//!   `@RepeatedTest`, `@TestFactory`, `@TestTemplate`);
//! - or, for **JUnit 3**, the shape the framework defined before annotations existed: a
//!   class extending `TestCase` and a `public void testXxx()` inside it. A codebase old
//!   enough to be interesting is old enough to contain these, and a discovery that only
//!   knows annotations reports such a class as having no tests at all;
//! - or, for **TestNG**, a class-level `@Test`, which promotes every public method.
//!
//! Which framework is in play comes from the file's imports (or from a fully-qualified
//! annotation), because a simple name alone proves nothing: `@Test` is declared by JUnit 4,
//! JUnit 5 and TestNG alike, and by projects that write their own.

use bennu_complete::prelude::line_number;
use bennu_facts::prelude::{AnnFacts, JavaFacts, MethodFacts, TypeFacts};
use serde::Serialize;

/// The cheap `contains` pre-filter to run before parsing a file (see
/// [`bennu_facts::prelude::mentions_any`]). Deliberately over-inclusive: a false hit costs
/// one tree-sitter pass, a false miss costs a test that can never be run from the UI.
pub const TEST_MARKERS: &[&str] = &[
    "@Test",
    "@ParameterizedTest",
    "@RepeatedTest",
    "@TestFactory",
    "@TestTemplate",
    "junit",
    "testng",
    "TestCase",
];

/// The testing framework a class is written against. It decides nothing about *how* the
/// run is launched — Surefire runs all of these — but it does decide what counts as a
/// test method, and it is worth showing in the UI when a project mixes two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TestFramework {
    #[serde(rename = "junit5")]
    JUnit5,
    #[serde(rename = "junit4")]
    JUnit4,
    #[serde(rename = "junit3")]
    JUnit3,
    #[serde(rename = "testng")]
    TestNg,
}

impl TestFramework {
    /// The label the UI shows.
    pub fn label(self) -> &'static str {
        match self {
            TestFramework::JUnit5 => "JUnit 5",
            TestFramework::JUnit4 => "JUnit 4",
            TestFramework::JUnit3 => "JUnit 3",
            TestFramework::TestNg => "TestNG",
        }
    }
}

/// One test method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestMethod {
    pub name: String,
    /// 1-based line of the method's name — the gutter row and the go-to target's line.
    pub line: u32,
    /// Byte offset of the method's name, for a precise caret placement on go-to.
    pub offset: usize,
    /// `@Disabled` / `@Ignore` / TestNG's `enabled = false`. Still listed — a disabled test
    /// that has vanished from the tree is a disabled test nobody ever re-enables.
    pub disabled: bool,
    /// The reason written in `@Disabled("…")` / `@Ignore("…")`, when there is one.
    pub disabled_reason: Option<String>,
    /// One declaration that produces MANY executions (`@ParameterizedTest`, `@RepeatedTest`,
    /// `@TestFactory`). The report will carry several cases for this single method, which is
    /// why the panel must not assume a 1:1 match between what was discovered and what ran.
    pub dynamic: bool,
}

/// One test class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestClass {
    /// Dotted fully-qualified name (`com.acme.Outer.Nested` for a nested class).
    pub fqcn: String,
    /// The name Surefire selects by: the class name below the package, with `$` between the
    /// outer and nested parts (`Outer$Nested`). See [`crate::selector`] for why the *simple*
    /// name and not the fully-qualified one.
    pub selector: String,
    pub package: String,
    /// Absolute path, forward-slashed.
    pub file: String,
    /// 1-based line of the class name.
    pub line: u32,
    pub offset: usize,
    pub framework: TestFramework,
    /// An abstract base class holding shared test methods. Real, and worth showing, but it
    /// cannot be *run* — Surefire instantiates concrete classes.
    pub is_abstract: bool,
    /// The whole class is `@Disabled` / `@Ignore`d.
    pub disabled: bool,
    pub methods: Vec<TestMethod>,
}

impl TestClass {
    /// Whether this class can be handed to a runner (a non-abstract class with at least one
    /// test method).
    pub fn runnable(&self) -> bool {
        !self.is_abstract && !self.methods.is_empty()
    }

    /// The simple class name, without any nesting (`Nested` of `Outer$Nested`).
    pub fn simple_name(&self) -> &str {
        self.selector.rsplit('$').next().unwrap_or(&self.selector)
    }
}

/// Every test class in one `.java` source, from its text alone — the whole pipeline: the
/// cheap marker filter, the scan, then [`discover_tests`].
///
/// This is what a caller walking a project wants, and it exists so that caller needs no
/// dependency on the scanner: on a legacy tree the filter rejects the overwhelming majority
/// of files without parsing them, and that ordering is a property of this crate rather than
/// something every call site should have to remember.
pub fn discover_in_source(file: &str, source: &str) -> Vec<TestClass> {
    if !bennu_facts::prelude::mentions_any(source, TEST_MARKERS) {
        return Vec::new();
    }
    let Some(facts) = bennu_facts::prelude::scan_java(file, source) else {
        return Vec::new();
    };
    discover_tests(&facts, source)
}

/// Every test class declared in one already-scanned file. Empty for a file that declares
/// none — which is the normal answer, and never an error.
pub fn discover_tests(facts: &JavaFacts, source: &str) -> Vec<TestClass> {
    let Some(framework) = framework_of(facts) else {
        return Vec::new();
    };
    facts
        .types
        .iter()
        .filter_map(|t| discover_class(t, facts, source, framework))
        .filter(|c| !c.methods.is_empty())
        .collect()
}

/// One type's test methods, or `None` when it is not a shape that can hold any (an
/// annotation declaration, an enum).
fn discover_class(
    ty: &TypeFacts,
    facts: &JavaFacts,
    source: &str,
    framework: TestFramework,
) -> Option<TestClass> {
    if !matches!(ty.kind, "class" | "interface") {
        return None;
    }
    // A TestNG class-level `@Test` promotes every public method; JUnit has no equivalent, so
    // the flag is only consulted for TestNG.
    let class_level_test =
        framework == TestFramework::TestNg && ty.annotations.iter().any(|a| a.name == "Test");
    let junit3 = framework == TestFramework::JUnit3 && extends_test_case(ty);

    let methods: Vec<TestMethod> = ty
        .methods
        .iter()
        .filter(|m| is_test_method(m, framework, class_level_test, junit3))
        .map(|m| test_method(m, source))
        .collect();

    Some(TestClass {
        fqcn: ty.fqcn.clone(),
        selector: selector_name(&ty.fqcn, &facts.package),
        package: facts.package.clone(),
        file: facts.file.clone(),
        line: line_number(source, ty.name_offset),
        offset: ty.name_offset,
        framework,
        is_abstract: ty.is_abstract,
        disabled: disabled_annotation(&ty.annotations).is_some(),
        methods,
    })
}

fn test_method(m: &MethodFacts, source: &str) -> TestMethod {
    let disabled = disabled_annotation(&m.annotations);
    TestMethod {
        name: m.name.clone(),
        line: line_number(source, m.name_offset),
        offset: m.name_offset,
        disabled: disabled.is_some(),
        disabled_reason: disabled.flatten(),
        dynamic: m.annotations.iter().any(|a| DYNAMIC_TEST_ANNOTATIONS.contains(&a.name.as_str())),
    }
}

/// The JUnit 5 annotations under which one declaration yields several executions.
const DYNAMIC_TEST_ANNOTATIONS: &[&str] =
    &["ParameterizedTest", "RepeatedTest", "TestFactory", "TestTemplate"];

/// Annotations that mark a *lifecycle* method (setup / teardown / data provider). These are
/// not tests, and under a class-level TestNG `@Test` they are the methods that must be
/// excluded — otherwise every `@BeforeMethod` shows up in the tree as a test that never runs.
const LIFECYCLE_ANNOTATIONS: &[&str] = &[
    "Before", "After", "BeforeClass", "AfterClass", // JUnit 4
    "BeforeEach", "AfterEach", "BeforeAll", "AfterAll", // JUnit 5
    "BeforeMethod", "AfterMethod", "BeforeTest", "AfterTest", // TestNG
    "BeforeSuite", "AfterSuite", "BeforeGroups", "AfterGroups", "DataProvider", "Factory",
];

fn is_test_method(
    m: &MethodFacts,
    framework: TestFramework,
    class_level_test: bool,
    junit3: bool,
) -> bool {
    if m.is_constructor {
        return false;
    }
    if m.annotations.iter().any(|a| LIFECYCLE_ANNOTATIONS.contains(&a.name.as_str())) {
        return false;
    }
    // The annotated case — every framework.
    if m.annotations
        .iter()
        .any(|a| a.name == "Test" || DYNAMIC_TEST_ANNOTATIONS.contains(&a.name.as_str()))
    {
        return true;
    }
    // JUnit 3: the pre-annotation shape. `public void testXxx()` with no parameters, in a
    // class extending TestCase.
    if junit3
        && m.is_public
        && !m.is_static
        && m.params.is_empty()
        && m.name.starts_with("test")
        && m.return_type.trim() == "void"
    {
        return true;
    }
    // TestNG's class-level `@Test`: every public instance method is a test.
    if class_level_test && framework == TestFramework::TestNg && m.is_public && !m.is_static {
        return true;
    }
    false
}

/// Whether the class extends JUnit 3's `TestCase` (written bare or qualified, and tolerant
/// of a generic parameter someone may have added on a custom base class).
fn extends_test_case(ty: &TypeFacts) -> bool {
    let base = ty.extends.split('<').next().unwrap_or("").trim();
    base == "TestCase" || base.ends_with(".TestCase") || base.ends_with("TestCase")
}

/// The `@Disabled` / `@Ignore` on a declaration: `Some(reason)` when one is present (the
/// inner `Option` being the reason string, which is optional), `None` when it isn't.
///
/// TestNG spells it `@Test(enabled = false)` instead, which is a *pair* on the test
/// annotation rather than an annotation of its own — read here so the three frameworks
/// answer the same question the same way.
fn disabled_annotation(anns: &[AnnFacts]) -> Option<Option<String>> {
    if let Some(a) = anns.iter().find(|a| a.name == "Disabled" || a.name == "Ignore") {
        return Some(a.value().map(|s| s.value.clone()));
    }
    let test = anns.iter().find(|a| a.name == "Test")?;
    match test.pair("enabled")?.trim() {
        "false" => Some(None),
        _ => None,
    }
}

/// The framework this file is written against, from its imports — falling back to a
/// fully-qualified annotation for the file that writes `@org.junit.Test` without importing
/// anything, and to JUnit 3's structural shape for the file that predates annotations.
///
/// `None` means "nothing here is a test", which is the answer for the overwhelming majority
/// of a project's files and must therefore be cheap and certain.
fn framework_of(facts: &JavaFacts) -> Option<TestFramework> {
    let qualified: Vec<&str> = facts
        .types
        .iter()
        .flat_map(|t| t.annotations.iter().chain(t.methods.iter().flat_map(|m| &m.annotations)))
        .map(|a| a.qualified.as_str())
        .collect();
    let mentions = |prefix: &str| {
        facts.imports.iter().any(|i| i.starts_with(prefix))
            || qualified.iter().any(|q| q.starts_with(prefix))
    };

    if mentions("org.junit.jupiter") {
        return Some(TestFramework::JUnit5);
    }
    if mentions("org.testng") {
        return Some(TestFramework::TestNg);
    }
    if mentions("junit.framework") {
        return Some(TestFramework::JUnit3);
    }
    if mentions("org.junit") {
        return Some(TestFramework::JUnit4);
    }
    // No import names a framework. A class extending `TestCase` is still unambiguously a
    // JUnit 3 test — that is precisely the era that wrote `import junit.framework.*;`, which
    // normalizes to a wildcard our prefix test above already caught, but also the era that
    // put the base class in the same package and imported nothing at all.
    facts.types.iter().any(extends_test_case).then_some(TestFramework::JUnit3)
}

/// The Surefire selector name for a dotted FQCN: the part below the package, with `$`
/// joining an outer class to a nested one (`com.acme.Outer.Nested` in package `com.acme`
/// → `Outer$Nested`).
fn selector_name(fqcn: &str, package: &str) -> String {
    let below = if package.is_empty() {
        fqcn
    } else {
        fqcn.strip_prefix(package).and_then(|r| r.strip_prefix('.')).unwrap_or(fqcn)
    };
    below.replace('.', "$")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_facts::prelude::scan_java;

    fn discover(src: &str) -> Vec<TestClass> {
        let facts = scan_java("/p/src/test/java/com/acme/T.java", src).expect("grammar loads");
        discover_tests(&facts, src)
    }

    #[test]
    fn finds_junit5_tests() {
        let src = "package com.acme;\n\
                   import org.junit.jupiter.api.Test;\n\
                   class OrderTest {\n\
                   \x20 @Test void computesTotal() {}\n\
                   \x20 @Test void appliesDiscount() {}\n\
                   \x20 void helper() {}\n\
                   }\n";
        let cs = discover(src);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].framework, TestFramework::JUnit5);
        assert_eq!(cs[0].fqcn, "com.acme.OrderTest");
        assert_eq!(cs[0].selector, "OrderTest");
        let names: Vec<&str> = cs[0].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["computesTotal", "appliesDiscount"], "a plain method is not a test");
    }

    #[test]
    fn finds_junit4_tests_and_ignores_lifecycle() {
        let src = "package com.acme;\n\
                   import org.junit.Test;\n\
                   import org.junit.Before;\n\
                   public class LegacyTest {\n\
                   \x20 @Before public void setUp() {}\n\
                   \x20 @Test public void works() {}\n\
                   }\n";
        let cs = discover(src);
        assert_eq!(cs[0].framework, TestFramework::JUnit4);
        assert_eq!(cs[0].methods.len(), 1);
        assert_eq!(cs[0].methods[0].name, "works");
    }

    /// The shape that predates annotations. A discovery that only knows `@Test` reports this
    /// class as empty, which on a legacy tree is most of the test suite.
    #[test]
    fn finds_junit3_test_case_methods() {
        let src = "package com.acme;\n\
                   import junit.framework.TestCase;\n\
                   public class OldTest extends TestCase {\n\
                   \x20 public void testAdds() {}\n\
                   \x20 public void testRemoves() {}\n\
                   \x20 public void helper() {}\n\
                   \x20 protected void setUp() {}\n\
                   }\n";
        let cs = discover(src);
        assert_eq!(cs[0].framework, TestFramework::JUnit3);
        let names: Vec<&str> = cs[0].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["testAdds", "testRemoves"]);
    }

    #[test]
    fn testng_class_level_test_promotes_public_methods() {
        let src = "package com.acme;\n\
                   import org.testng.annotations.Test;\n\
                   import org.testng.annotations.BeforeMethod;\n\
                   @Test\n\
                   public class SuiteTest {\n\
                   \x20 @BeforeMethod public void prepare() {}\n\
                   \x20 public void checksOne() {}\n\
                   \x20 private void notATest() {}\n\
                   }\n";
        let cs = discover(src);
        assert_eq!(cs[0].framework, TestFramework::TestNg);
        let names: Vec<&str> = cs[0].methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["checksOne"]);
    }

    #[test]
    fn records_disabled_with_its_reason() {
        let src = "package com.acme;\n\
                   import org.junit.jupiter.api.Test;\n\
                   import org.junit.jupiter.api.Disabled;\n\
                   class FlakyTest {\n\
                   \x20 @Test @Disabled(\"racy on CI\") void sometimes() {}\n\
                   \x20 @Test void always() {}\n\
                   }\n";
        let cs = discover(src);
        assert!(cs[0].methods[0].disabled);
        assert_eq!(cs[0].methods[0].disabled_reason.as_deref(), Some("racy on CI"));
        assert!(!cs[0].methods[1].disabled);
    }

    #[test]
    fn parameterized_tests_are_dynamic() {
        let src = "package com.acme;\n\
                   import org.junit.jupiter.api.Test;\n\
                   import org.junit.jupiter.params.ParameterizedTest;\n\
                   class ParamTest {\n\
                   \x20 @ParameterizedTest void many(int i) {}\n\
                   \x20 @Test void one() {}\n\
                   }\n";
        let cs = discover(src);
        assert!(cs[0].methods[0].dynamic, "one declaration, many executions");
        assert!(!cs[0].methods[1].dynamic);
    }

    /// A helper named like a test is not a test — the whole reason discovery reads
    /// annotations rather than file names.
    #[test]
    fn a_class_with_no_test_annotations_is_not_a_test() {
        let src = "package com.acme;\n\
                   public class OrderTestUtils {\n\
                   \x20 public static void testData() {}\n\
                   }\n";
        assert!(discover(src).is_empty());
    }

    #[test]
    fn nested_class_selector_uses_a_dollar() {
        let src = "package com.acme;\n\
                   import org.junit.jupiter.api.Test;\n\
                   import org.junit.jupiter.api.Nested;\n\
                   class OuterTest {\n\
                   \x20 @Test void outer() {}\n\
                   \x20 @Nested class Inner {\n\
                   \x20   @Test void inner() {}\n\
                   \x20 }\n\
                   }\n";
        let cs = discover(src);
        let inner = cs.iter().find(|c| c.fqcn.ends_with("Inner")).expect("nested class discovered");
        assert_eq!(inner.selector, "OuterTest$Inner");
        assert_eq!(inner.simple_name(), "Inner");
    }

    #[test]
    fn abstract_base_is_discovered_but_not_runnable() {
        let src = "package com.acme;\n\
                   import org.junit.jupiter.api.Test;\n\
                   public abstract class BaseTest {\n\
                   \x20 @Test void shared() {}\n\
                   }\n";
        let cs = discover(src);
        assert!(cs[0].is_abstract);
        assert!(!cs[0].runnable(), "Surefire instantiates concrete classes");
    }

    #[test]
    fn selector_name_handles_the_default_package() {
        assert_eq!(selector_name("FooTest", ""), "FooTest");
        assert_eq!(selector_name("com.acme.FooTest", "com.acme"), "FooTest");
        assert_eq!(selector_name("com.acme.A.B", "com.acme"), "A$B");
    }

    #[test]
    fn testng_disabled_flag_is_read_from_the_test_annotation() {
        let src = "package com.acme;\n\
                   import org.testng.annotations.Test;\n\
                   public class OffTest {\n\
                   \x20 @Test(enabled = false) public void skipped() {}\n\
                   \x20 @Test public void live() {}\n\
                   }\n";
        let cs = discover(src);
        assert!(cs[0].methods[0].disabled);
        assert!(!cs[0].methods[1].disabled);
    }
}
