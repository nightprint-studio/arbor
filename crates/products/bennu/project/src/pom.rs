//! Lightweight Maven `pom.xml` extraction.
//!
//! Phase-0 capability detection (and the project header) needs a *few* things out of
//! the pom: the display name, the `<modules>`, the declared dependency coordinates
//! (`groupId:artifactId`), the `<properties>` (for `project.build.sourceEncoding` and
//! `maven.compiler.*`), and the `maven-compiler-plugin` `<source>`/`<target>`. It
//! does **not** need a DOM.
//!
//! We extract by targeted tag scanning rather than pulling in an XML crate (none is
//! on the approved dep list — hard rule 7). This is intentionally shallow: it reads
//! the raw pom text, is tolerant of whitespace, and never fails on unexpected
//! structure (a field it can't find is simply absent). A real config-graph XML model
//! is `bennu-web`'s job later.

/// The slice of a pom Bennu reads in Phase 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pom {
    /// `<name>` (else `<artifactId>`, else empty — the caller falls back to the dir).
    /// The project's **own**, never one read out of a nested block — see [`root_child_text`].
    pub name: String,
    /// `<artifactId>` of the project itself — the direct child of `<project>`, not the
    /// `<parent>`'s and not a dependency's.
    pub artifact_id: String,
    /// The `<modules><module>` entries (empty for single-module).
    pub modules: Vec<String>,
    /// Declared dependency coordinates as `"groupId:artifactId"`, lowercased.
    pub dependencies: Vec<String>,
    /// `<properties>` as a flat key→value map (raw string values).
    pub properties: Vec<(String, String)>,
    /// `maven-compiler-plugin` `<release>` if present. Checked before source/target because
    /// `javac --release` overrides both.
    pub compiler_release: Option<String>,
    /// `maven-compiler-plugin` `<source>` if present.
    pub compiler_source: Option<String>,
    /// `maven-compiler-plugin` `<target>` if present.
    pub compiler_target: Option<String>,
    /// Whether a `<toolchains>` / `maven-toolchains-plugin` element is present.
    pub has_toolchains: bool,
}

/// The trimmed text of a **direct child** of the document's root element.
///
/// The reason this exists rather than [`tag_text`]: a child POM writes its parent's coordinates
/// first, so the first `<artifactId>` in the file is `<parent><artifactId>`. Read that way, every
/// module of a reactor is named after the reactor — and renaming a module's own artifactId
/// changes nothing on screen, because the name never came from it. `<name>` has the same problem
/// one step further out: `<organization>`, `<licenses>` and half the plugin configurations in the
/// wild each carry one.
///
/// Depth is the only thing that separates them, so this counts it. Still no DOM: comments, CDATA,
/// processing instructions and the prolog are skipped whole, and anything it cannot make sense of
/// simply ends the scan — a field it cannot find is absent, which is this module's contract.
fn root_child_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let mut depth = 0usize;
    let mut i = 0usize;
    while let Some(rel) = xml[i..].find('<') {
        let lt = i + rel;
        let rest = &xml[lt..];
        if let Some(skip) = inert_len(rest) {
            i = lt + skip;
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let inner = &rest[1..gt];
        i = lt + gt + 1;
        if inner.starts_with('/') {
            depth = depth.saturating_sub(1);
            continue;
        }
        // `<foo/>` opens and closes at once: it never becomes the enclosing element, and it can
        // hold no text worth returning.
        if inner.ends_with('/') {
            continue;
        }
        depth += 1;
        // `depth` is now the depth OF this element — 1 is the root, 2 is a direct child of it.
        if depth == 2 && inner.split(|c: char| c.is_whitespace()).next() == Some(tag) {
            let close = format!("</{tag}>");
            let end = xml[i..].find(&close)?;
            return Some(xml[i..i + end].trim());
        }
    }
    None
}

/// How far past `<` a comment, CDATA section, processing instruction or doctype runs — the spans
/// that carry no structure and must not move the depth. Ordered so `<!--` and `<![CDATA[` are
/// recognised before the bare `<!` that both start with.
fn inert_len(rest: &str) -> Option<usize> {
    for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>"), ("<!", ">")] {
        if rest.starts_with(open) {
            return Some(rest.find(close).map(|p| p + close.len()).unwrap_or(rest.len()));
        }
    }
    None
}

/// Extract the inner text of the first `<tag>…</tag>` in `xml`, trimmed. `None` when
/// the tag is absent.
fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim())
}

/// Extract the inner content (with tags) of the first `<tag>…</tag>` block.
fn tag_block<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find(&close)?;
    Some(&rest[..end])
}

/// Every `<tag>…</tag>` inner text in `xml`, in order.
fn all_tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = xml[cursor..].find(&open) {
        let s = cursor + rel + open.len();
        let Some(erel) = xml[s..].find(&close) else { break };
        out.push(xml[s..s + erel].trim().to_string());
        cursor = s + erel + close.len();
    }
    out
}

/// Parse the Phase-0 slice out of raw pom text.
pub fn parse(xml: &str) -> Pom {
    let mut pom = Pom::default();

    // The project's own identity, and only ever its own: a direct child of `<project>`. A POM
    // that declares neither leaves both empty, and the caller falls back to the directory —
    // which is right, and is not the same wrong answer as its parent's name.
    pom.artifact_id = root_child_text(xml, "artifactId").unwrap_or_default().to_string();

    pom.name = root_child_text(xml, "name")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| pom.artifact_id.clone());

    // Modules.
    if let Some(block) = tag_block(xml, "modules") {
        pom.modules = all_tag_texts(block, "module");
    }

    // Dependencies: each `<dependency>` block → groupId:artifactId.
    let mut cursor = 0usize;
    while let Some(rel) = xml[cursor..].find("<dependency>") {
        let s = cursor + rel;
        let Some(erel) = xml[s..].find("</dependency>") else { break };
        let block = &xml[s..s + erel];
        let g = tag_text(block, "groupId").unwrap_or_default();
        let a = tag_text(block, "artifactId").unwrap_or_default();
        if !a.is_empty() {
            pom.dependencies.push(format!("{g}:{a}").to_ascii_lowercase());
        }
        cursor = s + erel + "</dependency>".len();
    }

    // Properties: a flat key/value block.
    if let Some(block) = tag_block(xml, "properties") {
        pom.properties = parse_properties(block);
    }

    // maven-compiler-plugin source/target (best-effort: the first `<source>`/
    // `<target>` inside a `<configuration>`). Also honour `maven.compiler.*` props,
    // resolved by the `jdk` module — here we only surface the plugin values.
    pom.compiler_release = tag_text(xml, "release").map(|s| s.to_string());
    pom.compiler_source = tag_text(xml, "source").map(|s| s.to_string());
    pom.compiler_target = tag_text(xml, "target").map(|s| s.to_string());

    pom.has_toolchains =
        xml.contains("<toolchains>") || xml.contains("maven-toolchains-plugin");

    pom
}

/// Parse a `<properties>` inner block into key/value pairs. Each child element
/// `<key>value</key>` becomes `(key, value)`.
fn parse_properties(block: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let bytes = block.as_bytes();
    while cursor < block.len() {
        // Find the next opening tag.
        let Some(rel) = block[cursor..].find('<') else { break };
        let lt = cursor + rel;
        if bytes.get(lt + 1) == Some(&b'/') || bytes.get(lt + 1) == Some(&b'!') {
            cursor = lt + 1;
            continue;
        }
        let Some(gt_rel) = block[lt..].find('>') else { break };
        let gt = lt + gt_rel;
        let key = block[lt + 1..gt].trim();
        // Self-closing or malformed → skip.
        if key.is_empty() || key.ends_with('/') {
            cursor = gt + 1;
            continue;
        }
        let close = format!("</{key}>");
        let Some(crel) = block[gt + 1..].find(&close) else {
            cursor = gt + 1;
            continue;
        };
        let val = block[gt + 1..gt + 1 + crel].trim();
        out.push((key.to_string(), val.to_string()));
        cursor = gt + 1 + crel + close.len();
    }
    out
}

impl Pom {
    /// Look up a property value by key.
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Whether any declared dependency artifactId contains `needle` (case-insensitive
    /// substring on the `groupId:artifactId` coordinate). The Spike-D tier-A signal.
    pub fn has_dependency(&self, needle: &str) -> bool {
        let n = needle.to_ascii_lowercase();
        self.dependencies.iter().any(|d| d.contains(&n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_name_encoding_and_deps() {
        let xml = r#"
          <project>
            <artifactId>gestionale-web</artifactId>
            <name>Gestionale Web</name>
            <properties>
              <project.build.sourceEncoding>Cp1252</project.build.sourceEncoding>
              <maven.compiler.source>1.8</maven.compiler.source>
              <maven.compiler.target>1.8</maven.compiler.target>
            </properties>
            <dependencies>
              <dependency>
                <groupId>org.apache.struts</groupId>
                <artifactId>struts2-core</artifactId>
              </dependency>
              <dependency>
                <groupId>org.springframework</groupId>
                <artifactId>spring-jdbc</artifactId>
              </dependency>
            </dependencies>
          </project>
        "#;
        let pom = parse(xml);
        assert_eq!(pom.name, "Gestionale Web");
        assert_eq!(pom.artifact_id, "gestionale-web");
        assert_eq!(pom.property("project.build.sourceEncoding"), Some("Cp1252"));
        assert_eq!(pom.property("maven.compiler.source"), Some("1.8"));
        assert!(pom.has_dependency("struts2-core"));
        assert!(pom.has_dependency("spring-jdbc"));
        assert!(!pom.has_dependency("mybatis"));
    }

    /// The one that was wrong: a module POM lists its parent's coordinates first, so the first
    /// `<artifactId>` in the file belongs to the reactor. Reading that one names every module
    /// after its parent — and renaming the module's own artifactId then changes nothing at all,
    /// because the name was never coming from it.
    #[test]
    fn the_projects_own_artifact_id_wins_over_its_parents() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
          <project xmlns="http://maven.apache.org/POM/4.0.0">
            <modelVersion>4.0.0</modelVersion>
            <parent>
              <groupId>it.acme</groupId>
              <artifactId>acme-reactor</artifactId>
              <version>2.1.0</version>
            </parent>
            <artifactId>acme-web</artifactId>
          </project>"#;
        let pom = parse(xml);
        assert_eq!(pom.artifact_id, "acme-web");
        assert_eq!(pom.name, "acme-web", "and the display name follows it");
    }

    /// `<name>` is worse than `<artifactId>`, not better: a POM that does not declare one still
    /// has several further in, and the first of them is never the project's.
    #[test]
    fn a_name_nested_in_another_block_is_not_the_projects_name() {
        let xml = r#"<project>
            <parent><artifactId>reactor</artifactId></parent>
            <artifactId>acme-web</artifactId>
            <organization><name>Acme S.p.A.</name></organization>
            <licenses><license><name>Apache-2.0</name></license></licenses>
          </project>"#;
        let pom = parse(xml);
        assert_eq!(pom.name, "acme-web");
    }

    /// A comment holding what looks like markup must not move the depth — otherwise everything
    /// after it is read one level out.
    #[test]
    fn comments_and_self_closing_tags_do_not_shift_the_depth() {
        let xml = r#"<project>
            <!-- <artifactId>commented-out</artifactId> -->
            <parent><artifactId>reactor</artifactId><relativePath/></parent>
            <artifactId>acme-web</artifactId>
          </project>"#;
        assert_eq!(parse(xml).artifact_id, "acme-web");
    }

    /// A POM that declares neither leaves both empty rather than borrowing from somewhere: the
    /// caller falls back to the directory name, which is at least the module's own.
    #[test]
    fn a_pom_without_its_own_identity_says_so() {
        let xml = r#"<project><parent><artifactId>reactor</artifactId></parent></project>"#;
        let pom = parse(xml);
        assert!(pom.artifact_id.is_empty());
        assert!(pom.name.is_empty());
    }

    #[test]
    fn extracts_modules() {
        let xml = r#"<project><artifactId>root</artifactId>
          <modules><module>core</module><module>web</module></modules></project>"#;
        let pom = parse(xml);
        assert_eq!(pom.modules, vec!["core".to_string(), "web".to_string()]);
    }
}
