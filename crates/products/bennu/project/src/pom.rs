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
    pub name: String,
    /// `<artifactId>` of the project itself (first one outside `<dependencies>`).
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

    // Project-level artifactId: the first `<artifactId>` NOT inside `<dependencies>`
    // / `<plugin>`. Cheap heuristic: take the first one before the `<dependencies>`
    // block, else the first overall.
    let deps_start = xml.find("<dependencies>").unwrap_or(xml.len());
    pom.artifact_id =
        tag_text(&xml[..deps_start], "artifactId").unwrap_or_default().to_string();

    pom.name = tag_text(xml, "name")
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

    #[test]
    fn extracts_modules() {
        let xml = r#"<project><artifactId>root</artifactId>
          <modules><module>core</module><module>web</module></modules></project>"#;
        let pom = parse(xml);
        assert_eq!(pom.modules, vec!["core".to_string(), "web".to_string()]);
    }
}
