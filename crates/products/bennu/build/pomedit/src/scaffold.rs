//! Generating a new module: its `pom.xml`, and the directories Maven will look for its sources in.
//!
//! ## What "smart" means here
//!
//! Nearly everything a module pom could say, it should not. A module that declares its own
//! `<groupId>` and `<version>` is a module that will drift from its parent at the next release, and
//! the reason so many legacy reactors need a version bumped in eleven files is that eleven modules
//! each wrote one down. So the default is inheritance, and a coordinate is written **only** when it
//! genuinely differs from what the parent gives.
//!
//! The same rule decides `<relativePath>`: Maven's default is `../pom.xml`, so a module directly
//! under its parent says nothing, and a module nested deeper says exactly how much deeper.
//!
//! ## The directories are part of the module
//!
//! A pom without `src/main/java` is a module that builds and contains nothing, and the first thing
//! anybody does with a new module is put a class in it. Which directories those are follows the
//! packaging — a `war` also has a `webapp`, a `pom` aggregator has no sources at all — because that
//! is the one fact the packaging actually determines.

/// The parent a new module inherits from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentCoords {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    /// How to reach the parent's pom from the new module's directory — `../pom.xml` unless it is
    /// nested deeper. Empty means "leave it out", which is Maven's default.
    pub relative_path: String,
}

/// What to create.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleSpec {
    pub artifact_id: String,
    /// Written only when it differs from the parent's — empty means inherited.
    pub group_id: String,
    /// Likewise.
    pub version: String,
    /// `jar` · `war` · `pom`. `jar` is Maven's default and is written anyway, because a module
    /// whose packaging is invisible is one whose packaging gets changed by accident.
    pub packaging: String,
    /// `<name>`, omitted when empty.
    pub name: String,
    pub parent: Option<ParentCoords>,
}

/// The `<relativePath>` for a module `depth` directories below its parent. `../pom.xml` at depth 1,
/// which is what Maven assumes and therefore what a spec can leave out.
pub fn relative_path_for(depth: usize) -> String {
    if depth <= 1 {
        String::new()
    } else {
        format!("{}pom.xml", "../".repeat(depth))
    }
}

/// The text of the new module's `pom.xml`.
pub fn module_pom(spec: &ModuleSpec) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
"#,
    );

    if let Some(parent) = &spec.parent {
        out.push_str("\n  <parent>\n");
        out.push_str(&format!("    <groupId>{}</groupId>\n", parent.group_id));
        out.push_str(&format!("    <artifactId>{}</artifactId>\n", parent.artifact_id));
        out.push_str(&format!("    <version>{}</version>\n", parent.version));
        if !parent.relative_path.is_empty() {
            out.push_str(&format!("    <relativePath>{}</relativePath>\n", parent.relative_path));
        }
        out.push_str("  </parent>\n");
    }

    out.push('\n');
    if !spec.group_id.is_empty() {
        out.push_str(&format!("  <groupId>{}</groupId>\n", spec.group_id));
    }
    out.push_str(&format!("  <artifactId>{}</artifactId>\n", spec.artifact_id));
    if !spec.version.is_empty() {
        out.push_str(&format!("  <version>{}</version>\n", spec.version));
    }
    let packaging = if spec.packaging.is_empty() { "jar" } else { &spec.packaging };
    out.push_str(&format!("  <packaging>{packaging}</packaging>\n"));
    if !spec.name.is_empty() {
        out.push_str(&format!("  <name>{}</name>\n", spec.name));
    }

    out.push_str("</project>\n");
    out
}

/// The directories a module of this packaging is built out of, relative to its own root.
///
/// `resources` included: a module whose `src/main/resources` appears only when somebody needs it
/// is a module where the first properties file lands in `src/main/java` and is not on the
/// classpath — which costs an afternoon and looks like a build problem.
pub fn source_dirs(packaging: &str) -> Vec<&'static str> {
    match packaging {
        // An aggregator has no sources at all, and creating them invites somebody to fill them.
        "pom" => Vec::new(),
        "war" => vec![
            "src/main/java",
            "src/main/resources",
            "src/main/webapp/WEB-INF",
            "src/test/java",
            "src/test/resources",
        ],
        _ => vec!["src/main/java", "src/main/resources", "src/test/java", "src/test/resources"],
    }
}

/// Whether `name` can be a Maven artifactId **and** a directory name on every platform this runs
/// on. Deliberately stricter than Maven, which accepts more than a filesystem does.
pub fn valid_artifact_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with('.')
        && !name.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent() -> ParentCoords {
        ParentCoords {
            group_id: "com.acme".into(),
            artifact_id: "portale-parent".into(),
            version: "2.4.0".into(),
            relative_path: String::new(),
        }
    }

    #[test]
    fn a_module_inherits_rather_than_repeats() {
        let pom = module_pom(&ModuleSpec {
            artifact_id: "portale-core".into(),
            packaging: "jar".into(),
            parent: Some(parent()),
            ..Default::default()
        });
        assert!(pom.contains("<artifactId>portale-core</artifactId>"));
        // The coordinates the parent already gives are NOT written again.
        assert!(!pom.contains("<groupId>com.acme</groupId>\n\n"));
        assert_eq!(pom.matches("<version>").count(), 1, "only the parent's");
        assert!(!pom.contains("<relativePath>"), "../pom.xml is the default");
    }

    #[test]
    fn a_coordinate_that_really_differs_is_written() {
        let pom = module_pom(&ModuleSpec {
            artifact_id: "tools".into(),
            group_id: "com.acme.tools".into(),
            version: "1.0.0-SNAPSHOT".into(),
            packaging: "jar".into(),
            name: "Tools".into(),
            parent: Some(parent()),
        });
        assert!(pom.contains("<groupId>com.acme.tools</groupId>"));
        assert!(pom.contains("<version>1.0.0-SNAPSHOT</version>"));
        assert!(pom.contains("<name>Tools</name>"));
    }

    #[test]
    fn a_module_with_no_parent_carries_its_own_coordinates() {
        let pom = module_pom(&ModuleSpec {
            artifact_id: "standalone".into(),
            group_id: "com.acme".into(),
            version: "1.0.0".into(),
            packaging: "jar".into(),
            ..Default::default()
        });
        assert!(!pom.contains("<parent>"));
        assert!(pom.contains("<groupId>com.acme</groupId>"));
    }

    #[test]
    fn a_nested_module_says_how_far_up_its_parent_is() {
        assert_eq!(relative_path_for(1), "");
        assert_eq!(relative_path_for(2), "../../pom.xml");
        assert_eq!(relative_path_for(3), "../../../pom.xml");
    }

    #[test]
    fn packaging_decides_the_directories() {
        assert!(source_dirs("jar").contains(&"src/main/java"));
        assert!(source_dirs("war").contains(&"src/main/webapp/WEB-INF"));
        assert!(source_dirs("pom").is_empty());
    }

    #[test]
    fn an_artifact_id_has_to_be_a_directory_name_too() {
        assert!(valid_artifact_id("portale-core"));
        assert!(valid_artifact_id("core_2.13"));
        assert!(!valid_artifact_id(""));
        assert!(!valid_artifact_id("has space"));
        assert!(!valid_artifact_id("has/slash"));
        assert!(!valid_artifact_id(".hidden"));
    }

    /// What is generated has to be readable by what reads poms — the two halves of this workspace
    /// disagreeing about a pom neither of them can be wrong about is the bug this rules out.
    #[test]
    fn the_generated_pom_parses_as_the_module_it_describes() {
        let pom = module_pom(&ModuleSpec {
            artifact_id: "portale-core".into(),
            packaging: "war".into(),
            parent: Some(parent()),
            ..Default::default()
        });
        assert_eq!(crate::write::packaging_of(&pom), "war");
        let doc = bennu_xml::prelude::Doc::new(&pom);
        let root = doc.root().unwrap();
        assert_eq!(doc.child_text(root, "artifactId"), "portale-core");
        let parent_el = doc.child(root, "parent").unwrap();
        assert_eq!(doc.child_text(parent_el, "artifactId"), "portale-parent");
    }
}
