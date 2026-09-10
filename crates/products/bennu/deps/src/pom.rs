//! Reading one `pom.xml` — structurally, not by grepping for tags.
//!
//! ## Why structure matters here in particular
//!
//! `<dependencyManagement>` contains a `<dependencies>` containing `<dependency>` elements that
//! look *identical* to the real ones and mean something completely different: they pin a version
//! for a dependency the module may not even have. A reader that scans for `<dependency>` blocks
//! reports a project's managed versions as its dependencies — a list twice too long, in which the
//! entries that are actually on the classpath cannot be told from the ones that are not.
//!
//! So this walks the element tree and answers by **path**. The tolerant scanner from `bennu-xml`
//! supplies the tags and their byte spans; the spans are not incidental, they are what lets every
//! row in the panel be a place the editor can jump to.
//!
//! ## What it does not do
//!
//! Entities are left as written (`&amp;` stays), because nothing read out of a pom here — a
//! coordinate, a version, a scope — has ever contained one. And a `<profile>`'s activation is not
//! evaluated: whether a profile is on depends on the JDK, the OS, a `-P` flag and a property, none
//! of which an editor knows. Profile dependencies are reported *and labelled* rather than silently
//! included or silently dropped.

use bennu_xml::prelude::Doc;

/// A pom's `<parent>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParentRef {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// `<relativePath>` as written. Maven's default is `../pom.xml`; an **explicitly empty** one
    /// means "do not look on disk, resolve it from the repository", which is a different thing and
    /// is why this is an `Option` rather than a defaulted string.
    pub relative_path: Option<String>,
}

/// A `<dependency>` exactly as the pom writes it — before properties, management or inheritance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawDependency {
    pub group_id: String,
    pub artifact_id: String,
    /// As written, `${…}` included. Empty when the pom declares none.
    pub version: String,
    pub scope: String,
    pub packaging: String,
    pub classifier: String,
    pub optional: bool,
    /// `<exclusions>` as `(groupId, artifactId)` pairs — what this dependency refuses to drag in.
    ///
    /// Part of the declaration rather than a detail of it: an exclusion is the difference between
    /// the classpath a build produces and the one a naive walk of the poms would, and a resolver
    /// that ignores them puts back exactly the jar the project went out of its way to remove. A
    /// wildcard (`*`) is kept as written — Maven reads it as "everything under this dependency".
    pub exclusions: Vec<(String, String)>,
    /// The `<profile>` id this sits under, empty for a plain `<project><dependencies>` entry.
    pub profile: String,
    /// Byte offset of the `<dependency>` tag, and its 1-based line.
    pub offset: usize,
    pub line: u32,
    /// Byte span of the **text inside** `<version>` — not the element, the value. `None` when the
    /// pom declares no version at all (a managed dependency), which is the case where there is
    /// nothing to replace and an offer to update would have nowhere to write.
    ///
    /// Trimmed the same way [`version`](Self::version) is, so `<version> 1.2 </version>` yields the
    /// span of `1.2` and replacing it leaves the surrounding whitespace alone.
    pub version_span: Option<(usize, usize)>,
}

impl RawDependency {
    pub fn coord(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }
}

/// One parsed pom.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pom {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    pub packaging: String,
    pub name: String,
    pub parent: Option<ParentRef>,
    pub modules: Vec<String>,
    pub properties: Vec<(String, String)>,
    /// `<project><dependencies>` plus every `<profile>`'s, in that order.
    pub dependencies: Vec<RawDependency>,
    /// `<project><dependencyManagement><dependencies>` — versions and scopes for *other* poms.
    pub managed: Vec<RawDependency>,
    /// `<distributionManagement><relocation>` — the coordinates this artifact **moved to**.
    ///
    /// An artifact that has been renamed keeps publishing at its old coordinates, as a pom with
    /// `<packaging>pom</packaging>` and no jar, whose only content is where to go instead. Maven
    /// follows it; anything that does not sees an artifact that resolves and then has no jar, and
    /// reports as missing something that is sitting on disk under its new name. Measured on
    /// `org.hibernate.orm:hibernate-jpamodelgen:7.4.5.Final`, which is a relocation to
    /// `hibernate-processor` — a rename Hibernate made in ORM 7.
    ///
    /// A field of its own rather than a `Coord`: this crate's `Coord` lives in `repo`, and a pom
    /// carries the three parts as written, any of which the relocation may leave out (an artifact
    /// that only changed groupId writes only the groupId).
    pub relocation: Option<Relocation>,
}

/// Where an artifact moved to. Each part is empty when the relocation does not change it — Maven
/// reads an omitted part as "the same as before".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relocation {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
}

impl Pom {
    /// A property this pom declares.
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// The pom's own version, falling back to its parent's — which is how the overwhelming
    /// majority of module poms are written (no `<version>` at all, inherited from the parent), and
    /// therefore what `${project.version}` has to expand to there.
    pub fn effective_version(&self) -> &str {
        if !self.version.is_empty() {
            return &self.version;
        }
        self.parent.as_ref().map(|p| p.version.as_str()).unwrap_or_default()
    }

    /// Likewise for the groupId: a module usually declares only its artifactId.
    pub fn effective_group(&self) -> &str {
        if !self.group_id.is_empty() {
            return &self.group_id;
        }
        self.parent.as_ref().map(|p| p.group_id.as_str()).unwrap_or_default()
    }

    /// Display name — `<name>` when the pom bothers, else the artifactId.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.artifact_id
        } else {
            &self.name
        }
    }
}

/// Parse a pom. Never fails: a pom this cannot make sense of yields empty fields, which every
/// consumer already has to handle (a module with no dependencies is an ordinary thing).
pub fn parse(source: &str) -> Pom {
    let doc = Doc::new(source);
    let Some(project) = doc.root() else { return Pom::default() };

    let mut pom = Pom {
        group_id: doc.child_text(project, "groupId"),
        artifact_id: doc.child_text(project, "artifactId"),
        version: doc.child_text(project, "version"),
        packaging: doc.child_text(project, "packaging"),
        name: doc.child_text(project, "name"),
        ..Pom::default()
    };
    if pom.packaging.is_empty() {
        pom.packaging = "jar".to_string();
    }

    if let Some(parent) = doc.child(project, "parent") {
        pom.parent = Some(ParentRef {
            group_id: doc.child_text(parent, "groupId"),
            artifact_id: doc.child_text(parent, "artifactId"),
            version: doc.child_text(parent, "version"),
            relative_path: doc.child(parent, "relativePath").map(|i| doc.text(i)),
        });
    }

    if let Some(modules) = doc.child(project, "modules") {
        pom.modules = doc
            .children(modules)
            .into_iter()
            .filter(|c| doc.name(*c) == "module")
            .map(|c| doc.text(c))
            .filter(|m| !m.is_empty())
            .collect();
    }

    if let Some(props) = doc.child(project, "properties") {
        pom.properties =
            doc.children(props).into_iter().map(|c| (doc.name(c).to_string(), doc.text(c))).collect();
    }

    if let Some(deps) = doc.child(project, "dependencies") {
        pom.dependencies = dependencies_in(&doc, deps, "");
    }
    if let Some(dm) = doc.child(project, "dependencyManagement") {
        if let Some(deps) = doc.child(dm, "dependencies") {
            pom.managed = dependencies_in(&doc, deps, "");
        }
    }
    if let Some(dm) = doc.child(project, "distributionManagement") {
        if let Some(reloc) = doc.child(dm, "relocation") {
            pom.relocation = Some(Relocation {
                group_id: doc.child_text(reloc, "groupId"),
                artifact_id: doc.child_text(reloc, "artifactId"),
                version: doc.child_text(reloc, "version"),
            });
        }
    }
    // Profile dependencies, each carrying the id of the profile that would switch it on.
    if let Some(profiles) = doc.child(project, "profiles") {
        for profile in doc.children(profiles).into_iter().filter(|c| doc.name(*c) == "profile") {
            let id = doc.child_text(profile, "id");
            let label = if id.is_empty() { "profile".to_string() } else { id };
            if let Some(deps) = doc.child(profile, "dependencies") {
                pom.dependencies.extend(dependencies_in(&doc, deps, &label));
            }
        }
    }

    pom
}

// ── The pom-specific reads ───────────────────────────────────────────────────
//
// The generic element walk lives in `bennu_xml::prelude::Doc` — it is the same walk anything that
// reads (or rewrites) a configuration file needs, and a second copy of it here would be a second
// place for "what counts as this element's text" to be decided.

/// Every `<dependency>` directly inside the `<dependencies>` opened at `i`.
fn dependencies_in(doc: &Doc<'_>, i: usize, profile: &str) -> Vec<RawDependency> {
    doc.children(i)
        .into_iter()
        .filter(|c| doc.name(*c) == "dependency")
        .map(|c| dependency(doc, c, profile))
        .filter(|d| !d.artifact_id.is_empty())
        .collect()
}

fn dependency(doc: &Doc<'_>, i: usize, profile: &str) -> RawDependency {
    let start = doc.scan().tags[i].start;
    RawDependency {
        group_id: doc.child_text(i, "groupId"),
        artifact_id: doc.child_text(i, "artifactId"),
        version: doc.child_text(i, "version"),
        scope: doc.child_text(i, "scope"),
        packaging: doc.child_text(i, "type"),
        classifier: doc.child_text(i, "classifier"),
        optional: doc.child_text(i, "optional") == "true",
        exclusions: exclusions_of(doc, i),
        profile: profile.to_string(),
        offset: start,
        line: doc.line_at(start),
        version_span: doc.child(i, "version").and_then(|c| doc.text_span(c)),
    }
}

/// The `<exclusions>` of the dependency opened at `i`.
fn exclusions_of(doc: &Doc<'_>, i: usize) -> Vec<(String, String)> {
    let Some(list) = doc.child(i, "exclusions") else { return Vec::new() };
    doc.children(list)
        .into_iter()
        .filter(|c| doc.name(*c) == "exclusion")
        .map(|c| (doc.child_text(c, "groupId"), doc.child_text(c, "artifactId")))
        .filter(|(_, a)| !a.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.acme</groupId>
    <artifactId>portale-parent</artifactId>
    <version>2.4.0</version>
    <relativePath>../pom.xml</relativePath>
  </parent>
  <artifactId>portale-web</artifactId>
  <packaging>war</packaging>
  <name>Portale Web</name>
  <properties>
    <spring.version>5.3.27</spring.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>org.springframework</groupId>
        <artifactId>spring-web</artifactId>
        <version>${spring.version}</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>org.apache.struts</groupId>
      <artifactId>struts2-core</artifactId>
      <version>2.5.30</version>
      <exclusions>
        <exclusion>
          <groupId>commons-logging</groupId>
          <artifactId>commons-logging</artifactId>
        </exclusion>
      </exclusions>
    </dependency>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-web</artifactId>
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
      <scope>test</scope>
      <optional>true</optional>
    </dependency>
  </dependencies>
  <profiles>
    <profile>
      <id>oracle</id>
      <dependencies>
        <dependency>
          <groupId>com.oracle</groupId>
          <artifactId>ojdbc8</artifactId>
          <version>19.3</version>
        </dependency>
      </dependencies>
    </profile>
  </profiles>
</project>"#;

    #[test]
    fn the_projects_own_identity_comes_from_the_project_element_not_the_first_tag_that_matches() {
        let pom = parse(POM);
        assert_eq!(pom.artifact_id, "portale-web", "not the parent's, and not a dependency's");
        assert_eq!(pom.packaging, "war");
        assert_eq!(pom.display_name(), "Portale Web");
        // Inherited coordinates: the module declares neither.
        assert_eq!(pom.effective_group(), "com.acme");
        assert_eq!(pom.effective_version(), "2.4.0");
        let parent = pom.parent.unwrap();
        assert_eq!(parent.artifact_id, "portale-parent");
        assert_eq!(parent.relative_path.as_deref(), Some("../pom.xml"));
    }

    /// The bug a tag-scanning reader has by construction: `<dependencyManagement>` holds
    /// `<dependency>` elements that are not dependencies.
    #[test]
    fn managed_versions_are_not_dependencies() {
        let pom = parse(POM);
        let coords: Vec<String> = pom.dependencies.iter().map(|d| d.coord()).collect();
        assert_eq!(
            coords,
            [
                "org.apache.struts:struts2-core",
                "org.springframework:spring-web",
                "junit:junit",
                "com.oracle:ojdbc8",
            ],
        );
        assert_eq!(pom.managed.len(), 1);
        assert_eq!(pom.managed[0].version, "${spring.version}", "left as written, resolved later");
    }

    /// An `<exclusion>` is a `<groupId>` + `<artifactId>` inside a dependency, and reading the
    /// "first groupId in the block" finds the wrong one whenever the pom orders them that way.
    #[test]
    fn an_exclusion_does_not_become_the_dependencys_coordinate() {
        let pom = parse(POM);
        assert_eq!(pom.dependencies[0].coord(), "org.apache.struts:struts2-core");
    }

    #[test]
    fn the_fields_a_row_shows_are_all_read() {
        let pom = parse(POM);
        let junit = &pom.dependencies[2];
        assert_eq!(junit.scope, "test");
        assert!(junit.optional);
        assert_eq!(pom.dependencies[1].version, "", "declared without one — management answers it");
        assert_eq!(pom.property("spring.version"), Some("5.3.27"));
    }

    #[test]
    fn a_profiles_dependencies_are_reported_and_labelled() {
        let pom = parse(POM);
        let ojdbc = pom.dependencies.iter().find(|d| d.artifact_id == "ojdbc8").unwrap();
        assert_eq!(ojdbc.profile, "oracle");
        assert!(pom.dependencies[0].profile.is_empty(), "an ordinary dependency carries none");
    }

    #[test]
    fn every_dependency_knows_where_it_is_written() {
        let pom = parse(POM);
        let d = &pom.dependencies[0];
        assert!(POM[d.offset..].starts_with("<dependency>"));
        assert_eq!(POM[..d.offset].lines().count() as u32, d.line);
    }

    #[test]
    fn modules_are_read_and_a_single_module_pom_has_none() {
        let pom = parse(
            "<project><artifactId>root</artifactId><modules><module>core</module>\
             <module>web</module></modules></project>",
        );
        assert_eq!(pom.modules, ["core", "web"]);
        assert!(parse("<project><artifactId>solo</artifactId></project>").modules.is_empty());
    }

    /// Never fails, whatever it is handed — the panel's degradation is "this module lists
    /// nothing", never an error dialog.
    #[test]
    fn nonsense_yields_an_empty_pom_rather_than_a_panic() {
        assert_eq!(parse(""), Pom::default());
        assert_eq!(parse("not xml at all").artifact_id, "");
        // Unterminated markup is the state a pom is in while it is being edited.
        assert_eq!(parse("<project><artifactId>half").artifact_id, "");
        assert!(parse("<project><dependencies><dependency><artifactId>x").dependencies.is_empty());
    }

    #[test]
    fn an_empty_relative_path_is_told_apart_from_an_absent_one() {
        let disabled = parse("<project><parent><relativePath></relativePath></parent></project>");
        assert_eq!(disabled.parent.unwrap().relative_path.as_deref(), Some(""));
        let absent = parse("<project><parent><artifactId>p</artifactId></parent></project>");
        assert_eq!(absent.parent.unwrap().relative_path, None);
    }
    #[test]
    fn the_version_span_is_the_value_and_not_the_element() {
        let src = "<project><dependencies><dependency>\
<groupId>g</groupId><artifactId>a</artifactId><version>1.2.3</version>\
</dependency></dependencies></project>";
        let pom = parse(src);
        let (s, e) = pom.dependencies[0].version_span.expect("a literal version has a span");
        assert_eq!(&src[s..e], "1.2.3");
    }

    #[test]
    fn a_padded_version_spans_only_what_was_read() {
        // The two must agree: a span wider than the text is a replacement that eats the whitespace
        // the file was formatted with.
        let src = "<project><dependencies><dependency>\
<artifactId>a</artifactId><version>\n      4.0.1\n    </version>\
</dependency></dependencies></project>";
        let pom = parse(src);
        let d = &pom.dependencies[0];
        let (s, e) = d.version_span.unwrap();
        assert_eq!(&src[s..e], "4.0.1");
        assert_eq!(d.version, "4.0.1");
    }

    #[test]
    fn a_managed_dependency_has_no_version_to_replace() {
        let src = "<project><dependencies><dependency>\
<groupId>g</groupId><artifactId>a</artifactId>\
</dependency></dependencies></project>";
        assert_eq!(parse(src).dependencies[0].version_span, None);
    }

}
