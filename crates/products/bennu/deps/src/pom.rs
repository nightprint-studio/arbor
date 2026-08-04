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

use bennu_xml::prelude::{scan, Scan, TagKind};

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
    /// The `<profile>` id this sits under, empty for a plain `<project><dependencies>` entry.
    pub profile: String,
    /// Byte offset of the `<dependency>` tag, and its 1-based line.
    pub offset: usize,
    pub line: u32,
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
    let doc = Doc { source, scan: scan(source) };
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
        pom.dependencies = doc.dependencies_in(deps, "");
    }
    if let Some(dm) = doc.child(project, "dependencyManagement") {
        if let Some(deps) = doc.child(dm, "dependencies") {
            pom.managed = doc.dependencies_in(deps, "");
        }
    }
    // Profile dependencies, each carrying the id of the profile that would switch it on.
    if let Some(profiles) = doc.child(project, "profiles") {
        for profile in doc.children(profiles).into_iter().filter(|c| doc.name(*c) == "profile") {
            let id = doc.child_text(profile, "id");
            let label = if id.is_empty() { "profile".to_string() } else { id };
            if let Some(deps) = doc.child(profile, "dependencies") {
                pom.dependencies.extend(doc.dependencies_in(deps, &label));
            }
        }
    }

    pom
}

// ── The element walk ─────────────────────────────────────────────────────────

/// A scanned document, addressed by tag index.
///
/// Elements are identified by the index of their opening tag, which is all a caller ever needs:
/// from it come the children, the text and the byte span, and it is stable for the life of the
/// scan.
struct Doc<'a> {
    source: &'a str,
    scan: Scan,
}

impl<'a> Doc<'a> {
    /// The document element, when there is one.
    fn root(&self) -> Option<usize> {
        self.scan.tags.iter().position(|t| t.kind == TagKind::Open)
    }

    fn name(&self, i: usize) -> &str {
        self.scan.tags[i].local()
    }

    /// The direct children of the element opened at `i`, in document order.
    ///
    /// Depth-counted rather than name-matched, so a `<dependencies>` inside a
    /// `<dependencyManagement>` is never mistaken for the project's own — which is the entire
    /// reason this module walks structure instead of scanning for tags.
    fn children(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        for (j, t) in self.scan.tags.iter().enumerate().skip(i + 1) {
            match t.kind {
                TagKind::Open => {
                    if depth == 0 {
                        out.push(j);
                    }
                    depth += 1;
                }
                TagKind::SelfClose => {
                    if depth == 0 {
                        out.push(j);
                    }
                }
                TagKind::Close => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
            }
        }
        out
    }

    fn child(&self, i: usize, name: &str) -> Option<usize> {
        self.children(i).into_iter().find(|c| self.name(*c) == name)
    }

    /// The text content of the element opened at `i`, trimmed. Empty for a self-closing element or
    /// one that holds other elements.
    fn text(&self, i: usize) -> String {
        let Some(close) = self.close_of(i) else { return String::new() };
        let (start, end) = (self.scan.tags[i].end, self.scan.tags[close].start);
        if start > end || end > self.source.len() {
            return String::new();
        }
        let text = self.source[start..end].trim();
        if text.contains('<') {
            return String::new();
        }
        text.to_string()
    }

    fn child_text(&self, i: usize, name: &str) -> String {
        self.child(i, name).map(|c| self.text(c)).unwrap_or_default()
    }

    /// The index of the close tag that ends the element opened at `i`.
    fn close_of(&self, i: usize) -> Option<usize> {
        if self.scan.tags[i].kind != TagKind::Open {
            return None;
        }
        let mut depth = 0usize;
        for (j, t) in self.scan.tags.iter().enumerate().skip(i + 1) {
            match t.kind {
                TagKind::Open => depth += 1,
                TagKind::Close => {
                    if depth == 0 {
                        return Some(j);
                    }
                    depth -= 1;
                }
                TagKind::SelfClose => {}
            }
        }
        None
    }

    /// Every `<dependency>` directly inside the `<dependencies>` opened at `i`.
    fn dependencies_in(&self, i: usize, profile: &str) -> Vec<RawDependency> {
        self.children(i)
            .into_iter()
            .filter(|c| self.name(*c) == "dependency")
            .map(|c| self.dependency(c, profile))
            .filter(|d| !d.artifact_id.is_empty())
            .collect()
    }

    fn dependency(&self, i: usize, profile: &str) -> RawDependency {
        let tag = &self.scan.tags[i];
        RawDependency {
            group_id: self.child_text(i, "groupId"),
            artifact_id: self.child_text(i, "artifactId"),
            version: self.child_text(i, "version"),
            scope: self.child_text(i, "scope"),
            packaging: self.child_text(i, "type"),
            classifier: self.child_text(i, "classifier"),
            optional: self.child_text(i, "optional") == "true",
            profile: profile.to_string(),
            offset: tag.start,
            line: line_at(self.source, tag.start),
        }
    }
}

/// 1-based line of a byte offset.
fn line_at(source: &str, offset: usize) -> u32 {
    source[..offset.min(source.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
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
}
