//! The one grammar that has to ship with the editor.
//!
//! ## Why exactly one
//!
//! Every schema in [`crate::catalog`] resolves because somebody's jar contains it. The Maven POM
//! is the exception, and it is the exception in the worst possible place: `pom.xml` is the most
//! opened XML file in any Java project, its schema lives at `maven.apache.org` and **nothing on
//! the build path ships a copy** — the jars a POM resolves are the project's dependencies, not
//! Maven's own model.
//!
//! So the choice is between shipping a table and leaving the single most-edited XML file in the
//! ecosystem with no completion at all. The precedent already exists in this codebase: Spring's
//! curated property table (`bennu-spring`'s `builtin_meta`) is the same trade for the same
//! reason — a documented vocabulary that is stable, small, and otherwise unreachable.
//!
//! ## What it is not
//!
//! A validator. This table says what may go where; it deliberately says nothing about
//! cardinality, ordering or types, because a curated table is exactly the wrong place to be
//! confident. Everything built on it is completion, ghost text and hover — the things that are
//! useful when right and invisible when incomplete — and the only check it feeds is "this
//! element is not one the POM has", which a curated table can answer honestly.
//!
//! If the real schema *is* reachable (a project that vendors it, an `xsi:schemaLocation` pointing
//! at a local copy) that wins: [`crate::catalog::Catalog::grammar_for`] tries the real one first
//! and only falls back here.

use crate::grammar::{Element, Grammar, GrammarKind};
use crate::scan::{Scan, TagKind};

/// `(element, children, documentation)`. Children are space-separated; an empty list means the
/// element holds text.
///
/// Written flat rather than as a tree so a row is one line and adding one is one line — the
/// nesting is expressed by the names, which is also how the POM reference reads.
const POM: &[(&str, &str, &str)] = &[
    (
        "project",
        "modelVersion parent groupId artifactId version packaging name description url \
         inceptionYear organization licenses developers contributors mailingLists prerequisites \
         modules scm issueManagement ciManagement distributionManagement properties \
         dependencyManagement dependencies repositories pluginRepositories build reporting \
         profiles",
        "The root of a Maven project.",
    ),
    ("modelVersion", "", "The POM model version. Always 4.0.0."),
    ("parent", "groupId artifactId version relativePath", "The POM this one inherits from."),
    ("relativePath", "", "Where the parent POM is, relative to this file. Empty disables the lookup."),
    ("groupId", "", "The group the artifact belongs to, usually a reversed domain name."),
    ("artifactId", "", "The artifact's own name."),
    ("version", "", "The artifact's version."),
    ("packaging", "", "What the build produces: jar, war, ear, pom, maven-plugin."),
    ("name", "", "A human-readable name."),
    ("description", "", "A short description."),
    ("url", "", "The project's home page."),
    ("inceptionYear", "", "The year the project started."),
    ("organization", "name url", "The organization behind the project."),
    ("licenses", "license", ""),
    ("license", "name url distribution comments", ""),
    ("distribution", "", "How the artifact is distributed: repo or manual."),
    ("comments", "", ""),
    ("developers", "developer", ""),
    ("developer", "id name email url organization organizationUrl roles timezone properties", ""),
    ("contributors", "contributor", ""),
    ("contributor", "name email url organization organizationUrl roles timezone properties", ""),
    ("roles", "role", ""),
    ("role", "", ""),
    ("id", "", ""),
    ("email", "", ""),
    ("organizationUrl", "", ""),
    ("timezone", "", ""),
    ("mailingLists", "mailingList", ""),
    ("mailingList", "name subscribe unsubscribe post archive otherArchives", ""),
    ("subscribe", "", ""),
    ("unsubscribe", "", ""),
    ("post", "", ""),
    ("archive", "", ""),
    ("otherArchives", "otherArchive", ""),
    ("otherArchive", "", ""),
    ("prerequisites", "maven", ""),
    ("maven", "", "The minimum Maven version this project builds with."),
    ("modules", "module", "The child projects built with this one."),
    ("module", "", "A directory containing a child POM."),
    ("scm", "connection developerConnection tag url", "Where the source lives."),
    ("connection", "", "A read-only SCM URL."),
    ("developerConnection", "", "A read-write SCM URL."),
    ("tag", "", "The tag this release was cut from."),
    ("issueManagement", "system url", ""),
    ("ciManagement", "system url notifiers", ""),
    ("system", "", ""),
    ("notifiers", "notifier", ""),
    ("notifier", "type sendOnError sendOnFailure sendOnSuccess sendOnWarning address configuration", ""),
    ("distributionManagement", "repository snapshotRepository site downloadUrl relocation status", ""),
    ("site", "id name url", ""),
    ("downloadUrl", "", ""),
    ("relocation", "groupId artifactId version message", ""),
    ("message", "", ""),
    ("status", "", ""),
    ("properties", "", "User-defined properties, addressable as ${name}. Any element name is legal here."),
    ("dependencyManagement", "dependencies", "Versions and scopes inherited by child modules, without adding the dependencies themselves."),
    ("dependencies", "dependency", ""),
    (
        "dependency",
        "groupId artifactId version type classifier scope systemPath exclusions optional",
        "A library this project needs.",
    ),
    ("type", "", "The dependency's packaging. Defaults to jar."),
    ("classifier", "", "Distinguishes artifacts built from the same POM (sources, javadoc)."),
    (
        "scope",
        "",
        "When the dependency is on the classpath: compile, provided, runtime, test, system, import.",
    ),
    ("systemPath", "", "For the system scope only: an absolute path to the jar."),
    ("optional", "", "Whether the dependency is left out of what depends on this project."),
    ("exclusions", "exclusion", "Transitive dependencies to leave out."),
    ("exclusion", "groupId artifactId", ""),
    ("repositories", "repository", ""),
    ("pluginRepositories", "pluginRepository", ""),
    ("repository", "id name url layout releases snapshots uniqueVersion", ""),
    ("snapshotRepository", "id name url layout releases snapshots uniqueVersion", ""),
    ("pluginRepository", "id name url layout releases snapshots", ""),
    ("layout", "", ""),
    ("releases", "enabled updatePolicy checksumPolicy", ""),
    ("snapshots", "enabled updatePolicy checksumPolicy", ""),
    ("enabled", "", ""),
    ("updatePolicy", "", "always, daily, interval:N, or never."),
    ("checksumPolicy", "", "fail, warn or ignore."),
    ("uniqueVersion", "", ""),
    (
        "build",
        "defaultGoal directory finalName sourceDirectory scriptSourceDirectory \
         testSourceDirectory outputDirectory testOutputDirectory resources testResources \
         filters plugins pluginManagement extensions",
        "",
    ),
    ("defaultGoal", "", "The goal run when none is named on the command line."),
    ("directory", "", ""),
    ("finalName", "", "The name of the produced artifact, without its extension."),
    ("sourceDirectory", "", ""),
    ("scriptSourceDirectory", "", ""),
    ("testSourceDirectory", "", ""),
    ("outputDirectory", "", ""),
    ("testOutputDirectory", "", ""),
    ("filters", "filter", ""),
    ("filter", "", ""),
    ("resources", "resource", ""),
    ("testResources", "testResource", ""),
    ("resource", "targetPath filtering directory includes excludes", ""),
    ("testResource", "targetPath filtering directory includes excludes", ""),
    ("targetPath", "", ""),
    ("filtering", "", "Whether ${…} in these files is substituted at build time."),
    ("includes", "include", ""),
    ("include", "", ""),
    ("excludes", "exclude", ""),
    ("exclude", "", ""),
    ("extensions", "extension", ""),
    ("extension", "groupId artifactId version", ""),
    ("pluginManagement", "plugins", "Plugin versions and configuration inherited by child modules."),
    ("plugins", "plugin", ""),
    (
        "plugin",
        "groupId artifactId version extensions executions dependencies goals inherited \
         configuration",
        "A plugin bound into this build.",
    ),
    ("executions", "execution", ""),
    ("execution", "id phase goals inherited configuration", ""),
    ("phase", "", "The lifecycle phase this execution binds to."),
    ("goals", "goal", ""),
    ("goal", "", ""),
    (
        "configuration",
        "",
        "Plugin configuration. Whatever the plugin declares is legal here, so nothing inside is \
         checked.",
    ),
    ("inherited", "", "Whether child modules inherit this."),
    ("reporting", "excludeDefaults outputDirectory plugins", ""),
    ("excludeDefaults", "", ""),
    ("profiles", "profile", "Alternative build configurations, activated by condition or by -P."),
    (
        "profile",
        "id activation build modules repositories pluginRepositories dependencies \
         dependencyManagement distributionManagement properties reporting",
        "",
    ),
    ("activation", "activeByDefault jdk os property file", "When this profile switches itself on."),
    ("activeByDefault", "", ""),
    ("jdk", "", "A JDK version or range."),
    ("os", "name family arch version", ""),
    ("family", "", ""),
    ("arch", "", ""),
    ("property", "name value", ""),
    ("value", "", ""),
    ("file", "missing exists", ""),
    ("missing", "", ""),
    ("exists", "", ""),
    ("address", "", ""),
    ("sendOnError", "", ""),
    ("sendOnFailure", "", ""),
    ("sendOnSuccess", "", ""),
    ("sendOnWarning", "", ""),
];

/// Elements whose content is user-defined and therefore never wrong.
///
/// `<properties>` holds whatever the project chose to call its properties, and `<configuration>`
/// holds whatever the plugin under it declares. Both would otherwise be a permanent source of
/// false "unknown element" reports, which is the one thing that would make this table worse than
/// having none.
const OPEN: &[&str] = &["properties", "configuration"];

/// The built-in grammar for a document, or `None`.
pub fn grammar_for(scan: &Scan) -> Option<Grammar> {
    is_pom(scan).then(pom)
}

/// Whether this document is a Maven POM.
///
/// Two signals, either of which is enough and both of which are things only a POM has: the Maven
/// namespace on the root, or a `<modelVersion>` directly inside a `<project>`. The second matters
/// because a POM with no namespace declaration at all is legal and common.
fn is_pom(scan: &Scan) -> bool {
    let Some(root) = scan.tags.iter().find(|t| t.kind != TagKind::Close) else { return false };
    if root.local() != "project" {
        return false;
    }
    root.attrs.iter().any(|a| a.value.contains("maven.apache.org"))
        || scan.tags.iter().any(|t| t.local() == "modelVersion")
}

/// The Maven POM grammar.
pub fn pom() -> Grammar {
    Grammar {
        source: "Maven POM (built in)".to_string(),
        kind: Some(GrammarKind::Builtin),
        roots: vec!["project".to_string()],
        elements: POM
            .iter()
            .map(|(name, children, doc)| Element {
                name: name.to_string(),
                children: children.split_whitespace().map(str::to_string).collect(),
                attributes: Vec::new(),
                text: children.is_empty(),
                open: OPEN.contains(name),
                doc: doc.to_string(),
                decl: Default::default(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    #[test]
    fn a_pom_is_recognised_by_its_namespace_or_by_its_model_version() {
        let by_ns = scan(r#"<project xmlns="http://maven.apache.org/POM/4.0.0"><a/></project>"#);
        assert!(grammar_for(&by_ns).is_some());

        let bare = scan("<project><modelVersion>4.0.0</modelVersion></project>");
        assert!(grammar_for(&bare).is_some(), "a POM with no namespace is legal and common");

        assert!(grammar_for(&scan("<project><other/></project>")).is_none());
        assert!(grammar_for(&scan("<struts/>")).is_none());
    }

    #[test]
    fn the_nesting_people_actually_type_resolves() {
        let g = pom();
        assert!(g.element("project").unwrap().children.contains(&"dependencies".to_string()));
        assert_eq!(g.children_of("dependencies").len(), 1);
        let dep = g.element("dependency").unwrap();
        assert!(dep.children.contains(&"artifactId".to_string()));
        assert!(dep.children.contains(&"scope".to_string()));
        assert!(g.element("artifactId").unwrap().text, "a leaf holds text");
        // Every child named anywhere in the table is itself declared, or completion would offer
        // a name and then know nothing about it.
        for e in &g.elements {
            for c in &e.children {
                assert!(g.element(c).is_some(), "`{c}` is a child of `{}` but is not declared", e.name);
            }
        }
    }

    /// Otherwise the table would be a permanent source of false reports, which would make it
    /// worse than having none.
    #[test]
    fn user_defined_content_is_open() {
        let g = pom();
        assert!(g.element("properties").unwrap().open);
        assert!(g.element("configuration").unwrap().open);
        assert!(!g.element("dependencies").unwrap().open);
    }

    #[test]
    fn the_build_and_plugin_chain_is_complete_enough_to_write_a_plugin_with() {
        let g = pom();
        for (parent, child) in [
            ("project", "build"),
            ("build", "plugins"),
            ("plugins", "plugin"),
            ("plugin", "executions"),
            ("executions", "execution"),
            ("execution", "goals"),
            ("goals", "goal"),
        ] {
            assert!(
                g.element(parent).unwrap().children.contains(&child.to_string()),
                "{parent} → {child}",
            );
        }
    }
}
