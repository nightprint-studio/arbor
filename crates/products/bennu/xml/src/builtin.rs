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
//! [`POM_REQUIRED`] is the one thing here that IS a validation rule, and it is a separate list on
//! purpose: it is not read off the vocabulary, it is Maven's own documented set of obligations,
//! and it is written where its own reasoning can be read beside it.
//!
//! If the real schema *is* reachable (a project that vendors it, an `xsi:schemaLocation` pointing
//! at a local copy) that wins: [`crate::catalog::Catalog::grammar_for`] tries the real one first
//! and only falls back here.

use bennu_proto::prelude::Diagnostic;

use crate::grammar::{Child, Element, Grammar, GrammarKind};
use crate::scan::{local_name, Scan, TagKind};

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

/// The tag-library descriptor, whose grammar is unreachable for the same reason the POM's is.
///
/// A `.tld` names `web-jsptaglibrary_1_2.dtd` (or the 2.x XSD) at `java.sun.com`, and the copy
/// lives inside a servlet container's jars — which are `provided` scope and often not on the
/// classpath at all. So the file that *defines* a project's tag vocabulary was itself the one
/// XML file with no vocabulary.
///
/// Both generations in one table: the 1.1 spellings (`tlibversion`, `tagclass`, `bodycontent`)
/// sit beside the 1.2/2.x ones (`tlib-version`, `tag-class`, `body-content`), because both are in
/// use in the same project and a table that knew only the modern half would report the older
/// files as wrong.
const TAGLIB: &[(&str, &str, &str)] = &[
    (
        "taglib",
        "tlib-version tlibversion jsp-version jspversion short-name shortname uri info \
         display-name description icon small-icon large-icon validator listener tag tag-file \
         function taglib-extension",
        "The root of a tag library descriptor.",
    ),
    ("tlib-version", "", "The tag library's own version."),
    ("tlibversion", "", "The tag library's own version (JSP 1.1 spelling)."),
    ("jsp-version", "", "The JSP version the library requires."),
    ("jspversion", "", "The JSP version the library requires (JSP 1.1 spelling)."),
    ("short-name", "", "The prefix a page is expected to bind. A hint, not a rule."),
    ("shortname", "", "The prefix a page is expected to bind (JSP 1.1 spelling)."),
    (
        "uri",
        "",
        "The URI a page declares this library by. Absent in the older descriptors, which left the \
         binding to a `<taglib>` entry in web.xml.",
    ),
    ("info", "", "The library's documentation (JSP 1.1 spelling of description)."),
    ("display-name", "", ""),
    ("description", "", ""),
    ("icon", "small-icon large-icon", ""),
    ("small-icon", "", ""),
    ("large-icon", "", ""),
    ("validator", "validator-class init-param description", ""),
    ("validator-class", "", ""),
    ("init-param", "param-name param-value description", ""),
    ("param-name", "", ""),
    ("param-value", "", ""),
    ("listener", "listener-class description display-name icon", ""),
    ("listener-class", "", ""),
    (
        "tag",
        "name tag-class tagclass tei-class teiclass body-content bodycontent display-name \
         description info icon small-icon large-icon variable attribute dynamic-attributes \
         example tag-extension",
        "One tag the library declares.",
    ),
    ("name", "", "The tag's, attribute's or function's own name — what a page writes."),
    ("tag-class", "", "The class that implements the tag."),
    ("tagclass", "", "The class that implements the tag (JSP 1.1 spelling)."),
    ("tei-class", "", "The TagExtraInfo subclass, when the tag introduces scripting variables."),
    ("teiclass", "", "The TagExtraInfo subclass (JSP 1.1 spelling)."),
    (
        "body-content",
        "",
        "What may go between the tags: empty, JSP, scriptless, or tagdependent.",
    ),
    ("bodycontent", "", "What may go between the tags (JSP 1.1 spelling)."),
    ("dynamic-attributes", "", "true when the tag accepts attributes it does not declare."),
    ("example", "", ""),
    (
        "attribute",
        "name required rtexprvalue type description fragment deferred-value deferred-method",
        "One attribute of a tag.",
    ),
    ("required", "", "true / yes when the tag cannot be written without it."),
    (
        "rtexprvalue",
        "",
        "true when the attribute accepts a runtime expression rather than only a literal.",
    ),
    ("type", "", "The Java type the attribute takes."),
    ("fragment", "", "true when the attribute is a JSP fragment rather than a value."),
    ("deferred-value", "type", ""),
    ("deferred-method", "method-signature", ""),
    ("method-signature", "", ""),
    (
        "variable",
        "name-given name-from-attribute alias variable-class declare scope description",
        "A scripting variable the tag introduces into the page.",
    ),
    ("name-given", "", ""),
    ("name-from-attribute", "", ""),
    ("alias", "", ""),
    ("variable-class", "", ""),
    ("declare", "", ""),
    ("scope", "", "AT_BEGIN, AT_END or NESTED."),
    ("tag-file", "name path description display-name icon example", "A tag written as a .tag file."),
    ("path", "", "Where the .tag file is, web-app-relative."),
    (
        "function",
        "name function-class function-signature description display-name icon example \
         function-extension",
        "An EL function the library declares.",
    ),
    ("function-class", "", ""),
    ("function-signature", "", "The Java signature, return type included."),
    ("taglib-extension", "", ""),
    ("tag-extension", "", ""),
    ("function-extension", "", ""),
];

/// The built-in grammar for a document, or `None`.
pub fn grammar_for(scan: &Scan) -> Option<Grammar> {
    if is_pom(scan) {
        return Some(pom());
    }
    is_taglib(scan).then(taglib)
}

/// Whether this document is a tag library descriptor: a `<taglib>` root. Unlike `<project>`,
/// which is a name anything could use, a root element called `taglib` in a Java project is one
/// thing only.
fn is_taglib(scan: &Scan) -> bool {
    scan.tags.iter().find(|t| t.kind != TagKind::Close).is_some_and(|root| root.local() == "taglib")
}

/// The tag-library descriptor grammar.
pub fn taglib() -> Grammar {
    table("JSP tag library descriptor (built in)", "taglib", TAGLIB)
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
    table("Maven POM (built in)", "project", POM)
}

// ── What a POM cannot leave out ──────────────────────────────────────────────
//
// The vocabulary table above deliberately says nothing about cardinality (see the module docs),
// and for a schema in general that is the right call: a curated list is the wrong place to be
// confident about how many of something is legal.
//
// Maven's *required* fields are the exception, and for the same reason the vocabulary is here at
// all. They are few, they are documented, they have not moved since 4.0.0, and getting them wrong
// is not a style question — Maven refuses to build. `<dependency>` with no `<artifactId>` is not
// an opinion about the file, it is a file that does not work, and being told at deploy time is
// exactly what this crate exists to stop.
//
// Nothing conditional is guessed at. `<version>` is absent from every row: a dependency may take
// it from `<dependencyManagement>`, a plugin from `<pluginManagement>`, either from a parent this
// file cannot see — so a missing version is unknowable here, and an unknowable rule is not a rule.

/// `(element, required children, the sibling that excuses them)`.
///
/// The third column is what makes the `<project>` rows honest: `groupId` and `version` are
/// required *unless* the POM has a `<parent>`, which is most modules of most reactors. Without
/// it this check would fire on every one of them, which is the fastest way to teach someone to
/// ignore a whole diagnostic class.
const POM_REQUIRED: &[(&str, &str, &str)] = &[
    ("project", "modelVersion artifactId", ""),
    ("project", "groupId version", "parent"),
    ("parent", "groupId artifactId version", ""),
    ("dependency", "groupId artifactId", ""),
    ("exclusion", "groupId artifactId", ""),
    ("extension", "groupId artifactId", ""),
    // `groupId` is absent on purpose: it defaults to `org.apache.maven.plugins`, and the
    // overwhelming majority of `<plugin>` blocks in the wild rely on that.
    ("plugin", "artifactId", ""),
    ("repository", "id url", ""),
    ("pluginRepository", "id url", ""),
];

/// The fields this POM leaves out that Maven will not.
///
/// Keyed on the **document** rather than on whichever grammar answered for it: a project that
/// vendors the real `maven-4.0.0.xsd` gets the real grammar, and the rules below are no less true
/// there. Returns nothing for a document that is not a POM.
pub fn pom_diagnostics(scan: &Scan) -> Vec<Diagnostic> {
    if !is_pom(scan) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (tag, children) in scan.direct_children() {
        for (element, required, unless) in POM_REQUIRED {
            if *element != tag.local() {
                continue;
            }
            if !unless.is_empty() && children.iter().any(|c| local_name(c) == *unless) {
                continue;
            }
            for name in required.split_whitespace() {
                if children.iter().any(|c| local_name(c) == name) {
                    continue;
                }
                out.push(Diagnostic {
                    message: format!("`{}` requires `<{name}>`", tag.local()),
                    severity: "error".to_string(),
                    code: "pom.missing-element".to_string(),
                    start: tag.name_start,
                    end: tag.name_end,
                });
            }
        }
    }
    out
}

/// One of these tables as a [`Grammar`]. Shared by both built-ins so a third costs a table and
/// a name rather than a copy of this.
fn table(source: &str, root: &str, rows: &[(&str, &str, &str)]) -> Grammar {
    Grammar {
        source: source.to_string(),
        kind: Some(GrammarKind::Builtin),
        roots: vec![root.to_string()],
        elements: rows
            .iter()
            .map(|(name, children, doc)| Element {
                name: name.to_string(),
                // A curated table is a vocabulary, not a content model: it says which names are
                // legal here and takes no position on which of them a document must write. The
                // POM's own demands are the exception, and they live in `POM_REQUIRED` rather
                // than in these rows, because they are conditional in a way a flag cannot say.
                children: children
                    .split_whitespace()
                    .map(|n| Child { name: n.to_string(), required: false })
                    .collect(),
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

    fn pom_checks(src: &str) -> Vec<String> {
        pom_diagnostics(&scan(src)).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_dependency_without_coordinates_is_reported() {
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion><groupId>g</groupId>\
             <artifactId>a</artifactId><version>1</version>\
             <dependencies><dependency><version>2</version></dependency></dependencies></project>",
        );
        assert_eq!(out, ["`dependency` requires `<groupId>`", "`dependency` requires `<artifactId>`"]);
    }

    #[test]
    fn a_dependency_with_no_version_is_left_alone() {
        // It may come from `<dependencyManagement>` or from a parent this file cannot see, so a
        // missing version is unknowable here — and an unknowable rule is not a rule.
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion><groupId>g</groupId>\
             <artifactId>a</artifactId><version>1</version>\
             <dependencies><dependency><groupId>x</groupId><artifactId>y</artifactId></dependency>\
             </dependencies></project>",
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_parent_excuses_the_group_and_version() {
        // The commonest module of the commonest reactor. If this fired, the whole diagnostic
        // class would be noise on every multi-module project.
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion>\
             <parent><groupId>g</groupId><artifactId>p</artifactId><version>1</version></parent>\
             <artifactId>child</artifactId></project>",
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_root_pom_without_a_parent_must_say_who_it_is() {
        let out = pom_checks("<project><modelVersion>4.0.0</modelVersion><artifactId>a</artifactId></project>");
        assert_eq!(out, ["`project` requires `<groupId>`", "`project` requires `<version>`"]);
    }

    #[test]
    fn an_incomplete_parent_is_reported() {
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion>\
             <parent><groupId>g</groupId></parent><artifactId>c</artifactId></project>",
        );
        assert_eq!(out, ["`parent` requires `<artifactId>`", "`parent` requires `<version>`"]);
    }

    #[test]
    fn a_plugin_needs_only_its_artifact() {
        // `groupId` defaults to `org.apache.maven.plugins`, and nearly every `<plugin>` in the
        // wild relies on it — checking for it would report correct POMs as wrong.
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion><groupId>g</groupId>\
             <artifactId>a</artifactId><version>1</version>\
             <build><plugins><plugin><artifactId>maven-compiler-plugin</artifactId></plugin>\
             </plugins></build></project>",
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn the_same_name_at_two_depths_is_judged_separately() {
        // `groupId` inside `<parent>` does not satisfy `<project>`, and vice versa — the whole
        // reason the walk collects DIRECT children rather than everything underneath.
        let out = pom_checks(
            "<project><modelVersion>4.0.0</modelVersion>\
             <dependencies><dependency><groupId>g</groupId><artifactId>a</artifactId></dependency>\
             </dependencies></project>",
        );
        assert_eq!(
            out,
            [
                "`project` requires `<artifactId>`",
                "`project` requires `<groupId>`",
                "`project` requires `<version>`",
            ],
        );
    }

    #[test]
    fn a_document_that_is_not_a_pom_is_not_judged_by_maven_rules() {
        assert!(pom_checks("<project><other/></project>").is_empty());
        assert!(pom_checks("<taglib><tag/></taglib>").is_empty());
    }

    #[test]
    fn a_pom_being_typed_is_still_checked() {
        // No close tags at all. A check that waited for a well-formed file would be silent
        // exactly while the mistake is being written.
        let out = pom_checks("<project><modelVersion>4.0.0</modelVersion><dependencies><dependency>");
        assert!(out.iter().any(|m| m == "`dependency` requires `<artifactId>`"), "{out:?}");
    }

    #[test]
    fn the_nesting_people_actually_type_resolves() {
        let g = pom();
        assert!(g.element("project").unwrap().child_names().contains(&"dependencies"));
        assert_eq!(g.children_of("dependencies").len(), 1);
        let dep = g.element("dependency").unwrap();
        assert!(dep.child_names().contains(&"artifactId"));
        assert!(dep.child_names().contains(&"scope"));
        assert!(g.element("artifactId").unwrap().text, "a leaf holds text");
        // Every child named anywhere in the table is itself declared, or completion would offer
        // a name and then know nothing about it.
        for e in &g.elements {
            for c in e.child_names() {
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
    fn a_tld_is_recognised_by_its_root_and_reads_both_generations() {
        assert!(grammar_for(&scan("<taglib><tag><name>x</name></tag></taglib>")).is_some());
        assert!(grammar_for(&scan("<web-app/>")).is_none());

        let g = taglib();
        let tag = g.element("tag").expect("declares <tag>");
        // The 1.2 spelling and the 1.1 one, because both are open in the same project.
        assert!(tag.child_names().contains(&"tag-class"));
        assert!(tag.child_names().contains(&"tagclass"));
        assert!(g.element("attribute").unwrap().child_names().contains(&"rtexprvalue"));
        // Same rule as the POM's: a name offered must be a name the table knows.
        for e in &g.elements {
            for c in e.child_names() {
                assert!(g.element(c).is_some(), "`{c}` is a child of `{}` but is not declared", e.name);
            }
        }
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
                g.element(parent).unwrap().child_names().contains(&child),
                "{parent} → {child}",
            );
        }
    }
}
