//! The Surefire `<test>` — reading what a pom pins, and rewriting it into something steerable.
//!
//! ## The defect this exists for
//!
//! Maven gives a plugin parameter **written in the pom** precedence over the user property that
//! names it. So a pom containing
//!
//! ```xml
//! <configuration><test>TestSuite</test></configuration>
//! ```
//!
//! runs `TestSuite` however the command line is written: `-Dtest=OrderTest` is read and discarded.
//! Measured on Surefire 3.5.6. Every "run this one test" button in every tool is inert on such a
//! project, and inert *silently* — the run starts, the output scrolls, and what comes back is the
//! whole suite.
//!
//! ## The rewrite
//!
//! ```xml
//! <properties><test>TestSuite</test></properties>
//! <configuration><test>${test}</test></configuration>
//! ```
//!
//! Same two facts, one indirection: a plain `mvn test` still runs the suite, because the property
//! defaults to it, and `-Dtest=OrderTest` now reaches the mojo, because a command-line property
//! beats a pom-declared one. Nothing about the build changes for anyone who was not trying to
//! select a test.
//!
//! `test` is the name to use whenever it is free, deliberately: it is the property Surefire already
//! documents, so the conversion leaves the project steerable from a bare terminal and from every
//! other IDE, not only from here. Any other name works too — the runner reads the name back and
//! sets *that* property — but it only helps the tool that knows to look.
//!
//! ## What it will not touch
//!
//! A value already written as a property, and a value **composed** of several (`${a}${b}`, or
//! `Pre${a}`). No single property steers a composed value, so claiming this can make it steerable
//! would be a guess — and the standing rule (docs §7) is to under-report rather than risk being
//! confidently wrong about somebody's build.

use bennu_xml::prelude::Doc;

use crate::edit::Edit;
use crate::write::set_property;

/// What a pom writes for Surefire's `<test>`, when it writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurefireTest {
    /// A literal class or pattern. Not steerable by any command line.
    Literal(String),
    /// A single `${property}`. Steerable by setting that property.
    Property(String),
}

/// What this pom pins Surefire's `<test>` to, if anything.
pub fn surefire_test_in(xml: &str) -> Option<SurefireTest> {
    let doc = Doc::new(xml);
    let value = doc.text(first_test_element(&doc)?);
    classify(&value)
}

/// Every literal `<test>` this pom writes, as byte spans of the value — what a conversion replaces.
fn literal_tests(doc: &Doc<'_>) -> Vec<(usize, String)> {
    test_elements(doc)
        .into_iter()
        .filter_map(|i| {
            let (start, end) = doc.text_span(i)?;
            match classify(&doc.source()[start..end])? {
                SurefireTest::Literal(value) => Some((i, value)),
                SurefireTest::Property(_) => None,
            }
        })
        .collect()
}

/// What a rewrite of one pom comes to — reported before it is applied, because a button that
/// edits a build file has to be able to say what it will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteConversion {
    /// The property that will now steer the selection, and whose default keeps the suite running.
    pub property: String,
    /// The literal that was pinned, which becomes that property's default.
    pub suite: String,
    /// The edits, against the source they were computed from.
    pub edits: Vec<Edit>,
}

/// Rewrite every literal Surefire `<test>` in this pom as a property reference, defaulted to what
/// it says today. `None` for a pom that pins nothing.
///
/// Reported per pom rather than per project: the caller knows which files it is allowed to write,
/// and a conversion that spanned files could not be shown as a diff of one.
pub fn convert_pinned_suite(source: &str) -> Option<SuiteConversion> {
    let doc = Doc::new(source);
    let literals = literal_tests(&doc);
    let suite = literals.first()?.1.clone();
    let property = free_property_name(&doc, &suite);
    let mut edits = Vec::new();
    for (element, value) in &literals {
        // A pom with two different literals is not a shape anybody writes on purpose, and giving
        // each its own property would produce a build nobody can explain. The first one names the
        // property; the rest are left exactly as they are, and the caller still sees them.
        if value != &suite {
            continue;
        }
        if let Some((start, end)) = doc.text_span(*element) {
            edits.push(Edit::replace(start, end, format!("${{{property}}}")));
        }
    }
    if let Some(edit) = set_property(source, &property, &suite) {
        edits.push(edit);
    }
    Some(SuiteConversion { property, suite, edits })
}

/// `test` when the pom leaves it free, a qualified fallback when it does not.
///
/// The collision that matters is a `<properties><test>` already declaring something else: writing
/// ours would change a value the build already depends on. A pom that declares `test` as *this very
/// suite* is not a collision — it is the conversion already half-done.
fn free_property_name(doc: &Doc<'_>, suite: &str) -> String {
    let taken = |name: &str| -> bool {
        let Some(root) = doc.root() else { return false };
        let Some(props) = doc.child(root, "properties") else { return false };
        doc.children(props)
            .into_iter()
            .any(|c| doc.name(c) == name && doc.text(c) != suite)
    };
    if !taken("test") {
        return "test".to_string();
    }
    if !taken("surefire.test") {
        return "surefire.test".to_string();
    }
    (2..)
        .map(|n| format!("surefire.test.{n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| "surefire.test".to_string())
}

/// A value as one of the two things it can be, or `None` for one that is neither.
fn classify(value: &str) -> Option<SurefireTest> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(name) = single_property(value) {
        return Some(SurefireTest::Property(name));
    }
    // Anything else containing a `${` is a value built from several parts, and no single property
    // steers it. Nothing is claimed about it in either direction.
    if value.contains("${") {
        return None;
    }
    Some(SurefireTest::Literal(value.to_string()))
}

/// `${name}` and nothing else — not two run together, not one with a prefix.
fn single_property(value: &str) -> Option<String> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    if inner.is_empty() || inner.contains('$') || inner.contains('{') || inner.contains('}') {
        return None;
    }
    Some(inner.to_string())
}

/// The first `<test>` any Surefire configuration in this pom declares.
fn first_test_element(doc: &Doc<'_>) -> Option<usize> {
    test_elements(doc).into_iter().next()
}

/// Every `<test>` under a `maven-surefire-plugin`, wherever the pom declares one — `<build>`,
/// `<pluginManagement>`, or inside a `<profile>`.
///
/// Structural rather than a scan between `maven-surefire-plugin` and `</plugin>`: a pom whose
/// Surefire block contains an `<executions>` has its `<test>` one level deeper, and a `<test>`
/// belonging to some other plugin is not Surefire's however close it sits.
fn test_elements(doc: &Doc<'_>) -> Vec<usize> {
    let Some(root) = doc.root() else { return Vec::new() };
    let mut plugins = Vec::new();
    collect_plugins(doc, root, &mut plugins);
    plugins
        .into_iter()
        .filter(|p| doc.child_text(*p, "artifactId") == "maven-surefire-plugin")
        .filter_map(|p| configured_test(doc, p))
        .collect()
}

fn collect_plugins(doc: &Doc<'_>, i: usize, out: &mut Vec<usize>) {
    for child in doc.children(i) {
        if doc.name(child) == "plugin" {
            out.push(child);
        } else {
            collect_plugins(doc, child, out);
        }
    }
}

/// The `<test>` of a plugin's own `<configuration>`, or of one of its executions'.
fn configured_test(doc: &Doc<'_>, plugin: usize) -> Option<usize> {
    for child in doc.children(plugin) {
        match doc.name(child) {
            "configuration" => {
                if let Some(test) = doc.child(child, "test") {
                    return Some(test);
                }
            }
            "executions" => {
                for exec in doc.children(child) {
                    if let Some(test) =
                        doc.child(exec, "configuration").and_then(|c| doc.child(c, "test"))
                    {
                        return Some(test);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether any Surefire configuration in this pom pins `<forkCount>` to zero — tests in Maven's own
/// JVM, so there is no fork to put a debug agent on.
pub fn forkcount_zero_in(xml: &str) -> bool {
    let doc = Doc::new(xml);
    let Some(root) = doc.root() else { return false };
    let mut plugins = Vec::new();
    collect_plugins(&doc, root, &mut plugins);
    plugins
        .into_iter()
        .filter(|p| doc.child_text(*p, "artifactId") == "maven-surefire-plugin")
        .filter_map(|p| doc.child(p, "configuration"))
        .any(|c| doc.child_text(c, "forkCount").trim() == "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply;

    const PINNED: &str = r#"<project>
  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-surefire-plugin</artifactId>
        <version>3.5.6</version>
        <configuration><test>TestSuite</test></configuration>
      </plugin>
    </plugins>
  </build>
</project>"#;

    /// The reported case: the pom pins the selector, so `-Dtest=` is read and discarded and the
    /// whole suite runs whatever you clicked. Nothing on a command line can move it.
    #[test]
    fn a_literal_test_selector_is_a_literal() {
        assert_eq!(surefire_test_in(PINNED), Some(SurefireTest::Literal("TestSuite".into())));
    }

    /// The spelling that works out of the box: `${test}` is the property `-Dtest` already sets.
    #[test]
    fn the_test_property_is_recognised_as_a_property() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${test}</test>");
        assert_eq!(surefire_test_in(&xml), Some(SurefireTest::Property("test".into())));
    }

    /// And ANY property works, which is the answer to "can we keep the suite as the default?" —
    /// yes: the pom defaults it, so a plain `mvn test` runs the suite, and setting that property
    /// steers a single run. Measured on Surefire 3.5.6.
    #[test]
    fn any_property_name_is_recognised_and_can_be_driven() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${suite}</test>");
        assert_eq!(surefire_test_in(&xml), Some(SurefireTest::Property("suite".into())));
    }

    /// A value built from several parts is steered by no single property, and claiming it is
    /// pinned would be as much a guess as claiming it is not.
    #[test]
    fn a_composed_value_claims_nothing() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${a}${b}</test>");
        assert_eq!(surefire_test_in(&xml), None);
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>Pre${a}</test>");
        assert_eq!(surefire_test_in(&xml), None);
    }

    #[test]
    fn a_pom_that_does_not_configure_surefire_pins_nothing() {
        assert_eq!(surefire_test_in("<project><build><plugins></plugins></build></project>"), None);
    }

    /// A `<test>` outside the plugin is not Surefire's — and here, unlike a text scan bounded by
    /// `</plugin>`, it is not even in the same element.
    #[test]
    fn a_test_element_belonging_to_something_else_is_ignored() {
        let xml = PINNED.replace("<configuration><test>TestSuite</test></configuration>", "")
            + "<other><test>Nope</test></other>";
        assert_eq!(surefire_test_in(&xml), None);
    }

    /// The shape a text scan gets wrong in the other direction: Surefire configured per execution.
    #[test]
    fn a_test_inside_an_execution_is_still_surefires() {
        let xml = PINNED.replace(
            "<configuration><test>TestSuite</test></configuration>",
            "<executions><execution><configuration><test>TestSuite</test></configuration></execution></executions>",
        );
        assert_eq!(surefire_test_in(&xml), Some(SurefireTest::Literal("TestSuite".into())));
    }

    /// And a `<test>` under a **different** plugin is not Surefire's, however similar it looks.
    #[test]
    fn another_plugins_configuration_is_not_surefires() {
        let xml = PINNED.replace("maven-surefire-plugin", "maven-failsafe-plugin");
        assert_eq!(surefire_test_in(&xml), None);
    }

    #[test]
    fn a_zero_fork_count_is_recognised() {
        let xml = PINNED.replace("<configuration>", "<configuration><forkCount>0</forkCount>");
        assert!(forkcount_zero_in(&xml));
    }

    #[test]
    fn an_ordinary_fork_count_is_not_zero() {
        let xml = PINNED.replace("<configuration>", "<configuration><forkCount>1</forkCount>");
        assert!(!forkcount_zero_in(&xml));
        assert!(!forkcount_zero_in(PINNED));
    }

    // ── the conversion ───────────────────────────────────────────────────────

    #[test]
    fn the_conversion_keeps_the_suite_as_the_default() {
        let plan = convert_pinned_suite(PINNED).unwrap();
        assert_eq!(plan.property, "test");
        assert_eq!(plan.suite, "TestSuite");
        let out = apply(PINNED, &plan.edits);
        assert!(out.contains("<test>${test}</test>"));
        assert!(out.contains("<properties>"));
        assert!(out.contains("<test>TestSuite</test>"));
        // And the converted pom is no longer pinned — which is the whole point, and the property
        // that now steers it is the one Surefire already documents.
        assert_eq!(surefire_test_in(&out), Some(SurefireTest::Property("test".into())));
    }

    #[test]
    fn converting_twice_is_not_a_second_conversion() {
        let once = apply(PINNED, &convert_pinned_suite(PINNED).unwrap().edits);
        assert_eq!(convert_pinned_suite(&once), None);
    }

    /// A pom that already uses `test` for something else keeps it: ours goes somewhere free rather
    /// than quietly changing a value the build depends on.
    #[test]
    fn an_occupied_property_name_is_not_taken_over() {
        let xml = PINNED.replace(
            "<build>",
            "<properties><test>something.else</test></properties>\n  <build>",
        );
        let plan = convert_pinned_suite(&xml).unwrap();
        assert_eq!(plan.property, "surefire.test");
        let out = apply(&xml, &plan.edits);
        assert!(out.contains("<test>something.else</test>"));
        assert!(out.contains("<surefire.test>TestSuite</surefire.test>"));
    }

    #[test]
    fn a_pom_that_pins_nothing_has_nothing_to_convert() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${test}</test>");
        assert_eq!(convert_pinned_suite(&xml), None);
    }

    /// The file keeps everything the change is not about — comments included.
    #[test]
    fn the_rest_of_the_file_is_left_alone() {
        let xml = PINNED.replace("<build>", "<!-- keep me -->\n  <build>");
        let out = apply(&xml, &convert_pinned_suite(&xml).unwrap().edits);
        assert!(out.contains("<!-- keep me -->"));
        assert!(out.contains("<version>3.5.6</version>"));
    }
}
