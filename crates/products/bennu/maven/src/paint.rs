//! What a pom **says**, as spans the editor can colour.
//!
//! ## The problem
//!
//! An XML mode has three colours to give: one for tag names, one for attribute values, one for
//! comments. A pom has almost no attributes and almost no comments, so everything that carries
//! meaning — every coordinate, every scope, every version — arrives as element *text*, which is
//! the one thing the mode paints as nothing at all. The result is the wall of grey anybody who
//! has looked for `<scope>test</scope>` in a four-hundred-line pom has scrolled through: the
//! word is there, in the same colour as the two hundred words around it.
//!
//! ## The rule
//!
//! **Add, never subtract.** Every tag keeps the colour the XML mode gives it — this is IntelliJ's
//! arrangement and it is the right one: a tag is a tag, and a file where half of them are dimmed
//! reads as a file half of which has been switched off. What the skeleton and the repeated blocks
//! get is **weight**, in that same colour, so the file can be scrolled by shape without any of it
//! losing legibility.
//!
//! The colours are spent on the **values**, which had none at all: a name, a version, a word from
//! a closed vocabulary, a property. That is the half of the file the mode was silent about, and
//! the half where the reading is actually hard.
//!
//! (An earlier version muted the coordinate labels to let their values read. It works on paper and
//! it is horrible on screen — the eye reads dimmed text as disabled, and `<groupId>` is not
//! disabled.)
//!
//! ## What it needs
//!
//! The document, and nothing else. No repository, no effective pom, no index — so the colours are
//! there on the first frame, on a buffer that does not parse yet, and on a pom in `~/.m2` that
//! belongs to no project at all.

use bennu_ext::prelude::ExtHighlight;
use bennu_xml::prelude::TagKind;

use crate::doc::Doc;

// ── Kinds ────────────────────────────────────────────────────────────────────
//
// Namespaced `maven.`, like every other extension's. The frontend maps a kind it does not know
// to a neutral class rather than dropping it, so adding one here is not a breaking change.

/// `<dependencies>`, `<build>`, `<profiles>` — the skeleton you scroll to.
pub const TAG_SECTION: &str = "maven.tag.section";
/// `<dependency>`, `<plugin>`, `<execution>` — one repeated unit inside a section.
pub const TAG_ITEM: &str = "maven.tag.item";
/// A property **declared** in `<properties>`, where the tag name is the data.
pub const PROPERTY: &str = "maven.property";

pub const GROUP: &str = "maven.group";
pub const ARTIFACT: &str = "maven.artifact";
pub const VERSION: &str = "maven.version";
pub const SCOPE: &str = "maven.scope";
pub const PACKAGING: &str = "maven.packaging";
pub const PHASE: &str = "maven.phase";
pub const GOAL: &str = "maven.goal";
pub const MODULE: &str = "maven.module";
/// The whole `${…}`, tinted so the substitution reads as one thing.
pub const PLACEHOLDER: &str = "maven.placeholder";
/// The name inside it — the part that has to be right.
pub const PLACEHOLDER_NAME: &str = "maven.placeholder-name";

/// The containers. Emphasised because they are what a reader navigates by.
const SECTION: &[&str] = &[
    "project", "modules", "properties", "dependencyManagement", "dependencies", "build",
    "pluginManagement", "plugins", "profiles", "executions", "exclusions", "extensions",
    "repositories", "pluginRepositories", "resources", "testResources", "reporting",
    "distributionManagement", "configuration", "goals", "licenses", "developers", "activation",
];

/// The repeated unit — the thing a section holds many of.
const ITEM: &[&str] = &[
    "parent", "dependency", "plugin", "exclusion", "extension", "execution", "profile",
    "repository", "pluginRepository", "resource", "testResource",
];

fn span(start: usize, end: usize, kind: &str) -> ExtHighlight {
    ExtHighlight { start, end, kind: kind.to_string() }
}

/// Every span worth colouring in this pom.
///
/// Empty for a document whose root is not `<project>`: a `.pom` in the repository is one, a
/// `pom.xml` is one, and anything else reaching here is a file we were wrong about.
pub fn highlights(doc: &Doc<'_>) -> Vec<ExtHighlight> {
    if doc.root().map(|r| doc.name(r)) != Some("project") {
        return Vec::new();
    }
    let tags = &doc.scan.tags;
    let close = closers(doc);
    let mut out = Vec::new();
    // The ancestors of the tag being looked at, innermost last. Maintained by the same
    // push-on-open / pop-on-close discipline `closers` uses, so the two never disagree about
    // where an unmatched close tag left the document.
    let mut path: Vec<&str> = Vec::new();

    for (i, tag) in tags.iter().enumerate() {
        if tag.kind == TagKind::Close {
            path.pop();
            continue;
        }
        let name = tag.local();
        // A leaf is an element whose close tag is the very next tag — which is exactly "it holds
        // no elements", read off the pairing instead of re-derived from the text.
        let leaf = tag.kind == TagKind::SelfClose || close[i] == Some(i + 1);

        if let Some(kind) = tag_kind(name, &path, leaf) {
            out.push(span(tag.name_start, tag.name_end, kind));
            if let Some(c) = close[i] {
                out.push(span(tags[c].name_start, tags[c].name_end, kind));
            }
        }

        if leaf {
            if let Some(c) = close[i] {
                let (start, end) = (tag.end, tags[c].start);
                if start <= end && end <= doc.source.len() {
                    value(doc, name, &path, start, end, &mut out);
                }
            }
        }
        if tag.kind == TagKind::Open {
            path.push(name);
        }
    }
    out
}

/// The close tag of each opening tag, paired in one pass.
///
/// One pass rather than [`Doc::close_of`] per element: that walk is linear, so asking it for every
/// tag is quadratic on a document where one traversal answers for all of them. A reactor's root pom
/// is a few thousand tags and this runs on a keystroke.
fn closers(doc: &Doc<'_>) -> Vec<Option<usize>> {
    let mut out = vec![None; doc.scan.tags.len()];
    let mut open: Vec<usize> = Vec::new();
    for (j, tag) in doc.scan.tags.iter().enumerate() {
        match tag.kind {
            TagKind::Open => open.push(j),
            TagKind::Close => {
                if let Some(o) = open.pop() {
                    out[o] = Some(j);
                }
            }
            TagKind::SelfClose => {}
        }
    }
    out
}

/// What role this element's *name* plays, or `None` to leave it the colour the XML mode gave it.
fn tag_kind(name: &str, path: &[&str], leaf: bool) -> Option<&'static str> {
    // Inside `<properties>` the tag name IS the data — the one place in a pom where that is true,
    // and the reason a property declaration is invisible in an ordinary XML mode.
    if path.last() == Some(&"properties") {
        return Some(PROPERTY);
    }
    if ITEM.contains(&name) {
        return Some(TAG_ITEM);
    }
    // `!leaf` because a section is a container by definition; an empty `<properties/>` is not one
    // worth shouting about.
    if !leaf && SECTION.contains(&name) {
        return Some(TAG_SECTION);
    }
    // Everything else keeps the colour it had. A `<groupId>` is a tag like any other, and the one
    // thing worth saying about it is written between it and its close tag.
    None
}

/// Colour the text of a leaf element, `start..end` being the raw span between its tags.
fn value(
    doc: &Doc<'_>,
    name: &str,
    path: &[&str],
    start: usize,
    end: usize,
    out: &mut Vec<ExtHighlight>,
) {
    let raw = &doc.source[start..end];
    let text = raw.trim();
    if text.is_empty() {
        return;
    }
    let at = start + (raw.len() - raw.trim_start().len());

    // A value written as a property is coloured as the **reference** it is, not as the kind of
    // value it will become — the whole point of the colour being that this is not the value yet.
    // Checked for every leaf and not only the coordinate ones: `<finalName>${project.build}</…>`
    // is the same substitution and the same risk of a name nobody defines.
    if text.contains("${") {
        placeholders(text, at, out);
        return;
    }

    let kind = match name {
        "groupId" => GROUP,
        "artifactId" => ARTIFACT,
        "version" => VERSION,
        "scope" => SCOPE,
        "packaging" | "type" => PACKAGING,
        "phase" => PHASE,
        // Both of these are a name written where an element of a list goes, so they are only
        // themselves under the list that holds them — a `<goal>` inside a plugin's own
        // `<configuration>` is that plugin's parameter and means whatever it says it does.
        "goal" if path.last() == Some(&"goals") => GOAL,
        "module" if path.last() == Some(&"modules") => MODULE,
        _ => return,
    };
    out.push(span(at, at + text.len(), kind));
}

/// Every `${…}` in `text`, whose first byte is at `base`.
fn placeholders(text: &str, base: usize, out: &mut Vec<ExtHighlight>) {
    let mut from = 0usize;
    while let Some(rel) = text[from..].find("${") {
        let open = from + rel;
        // An unterminated `${` is a value being typed. Nothing after it is a placeholder yet, and
        // guessing where it ends would paint the rest of the line.
        let Some(close) = text[open + 2..].find('}') else { break };
        let end = open + 2 + close + 1;
        out.push(span(base + open, base + end, PLACEHOLDER));
        if end - 1 > open + 2 {
            out.push(span(base + open + 2, base + end - 1, PLACEHOLDER_NAME));
        }
        from = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(source: &str) -> Vec<(String, String)> {
        let doc = Doc::new(source);
        highlights(&doc)
            .into_iter()
            .map(|h| (h.kind, source[h.start..h.end].to_string()))
            .collect()
    }

    fn kinds_of(source: &str, text: &str) -> Vec<String> {
        painted(source).into_iter().filter(|(_, t)| t == text).map(|(k, _)| k).collect()
    }

    const POM: &str = r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.acme</groupId>
    <artifactId>platform</artifactId>
    <version>3.1.0</version>
  </parent>
  <artifactId>orders</artifactId>
  <packaging>jar</packaging>
  <properties>
    <spring.version>5.3.20</spring.version>
  </properties>
  <modules>
    <module>core</module>
  </modules>
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-core</artifactId>
      <version>${spring.version}</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#;

    #[test]
    fn a_document_that_is_not_a_pom_is_left_alone() {
        assert!(painted("<beans><bean id=\"a\"/></beans>").is_empty());
    }

    #[test]
    fn the_sections_and_the_repeated_units_are_told_apart() {
        let marks = painted(POM);
        let section = |n: &str| marks.iter().filter(|(k, t)| k == TAG_SECTION && t == n).count();
        let item = |n: &str| marks.iter().filter(|(k, t)| k == TAG_ITEM && t == n).count();
        // Open and close both, or the two halves of one element would read as two things.
        assert_eq!(section("dependencies"), 2);
        assert_eq!(section("modules"), 2);
        assert_eq!(item("dependency"), 2);
        assert_eq!(item("parent"), 2);
    }

    /// The rule the grey version got wrong: a coordinate's LABEL keeps the tag colour it had, and
    /// only its value is coloured. Dimmed text reads as disabled, and `<groupId>` is not disabled.
    #[test]
    fn a_coordinate_label_keeps_the_colour_it_had_and_its_value_reads() {
        assert_eq!(kinds_of(POM, "groupId"), Vec::<String>::new());
        assert_eq!(kinds_of(POM, "version"), Vec::<String>::new());
        assert_eq!(kinds_of(POM, "org.springframework"), vec![GROUP]);
        assert_eq!(kinds_of(POM, "spring-core"), vec![ARTIFACT]);
        assert_eq!(kinds_of(POM, "3.1.0"), vec![VERSION]);
        assert_eq!(kinds_of(POM, "test"), vec![SCOPE]);
        assert_eq!(kinds_of(POM, "jar"), vec![PACKAGING]);
        assert_eq!(kinds_of(POM, "core"), vec![MODULE]);
    }

    #[test]
    fn a_declared_property_is_named_by_its_tag() {
        // Twice — the open tag and the close tag — and nowhere else: the same name inside the
        // `${…}` below is a *reference*, which is a different thing to know.
        assert_eq!(kinds_of(POM, "spring.version"), vec![PROPERTY, PROPERTY, PLACEHOLDER_NAME]);
    }

    #[test]
    fn a_version_written_as_a_property_is_coloured_as_the_reference_it_is() {
        let marks = painted(POM);
        assert!(marks.contains(&(PLACEHOLDER.to_string(), "${spring.version}".to_string())));
        // …and NOT as a version: it is not one yet, and saying otherwise is the mistake this
        // whole colour exists to make visible.
        assert!(!marks.iter().any(|(k, t)| k == VERSION && t.contains('$')));
    }

    #[test]
    fn a_placeholder_still_being_typed_paints_nothing_after_it() {
        let marks = painted("<project><version>${spring.</version></project>");
        assert!(!marks.iter().any(|(k, _)| k == PLACEHOLDER || k == PLACEHOLDER_NAME));
    }

    #[test]
    fn a_phase_and_a_goal_are_coloured_where_they_mean_what_they_say() {
        let src = r#"<project><build><plugins><plugin>
          <executions><execution>
            <phase>process-classes</phase>
            <goals><goal>shade</goal></goals>
          </execution></executions>
          <configuration><goal>whatever</goal></configuration>
        </plugin></plugins></build></project>"#;
        assert_eq!(kinds_of(src, "process-classes"), vec![PHASE]);
        assert_eq!(kinds_of(src, "shade"), vec![GOAL]);
        // A plugin's own parameter that happens to be spelled `goal` is that plugin's business.
        assert_eq!(kinds_of(src, "whatever"), Vec::<String>::new());
    }

    #[test]
    fn an_element_that_is_not_closed_does_not_drag_the_rest_of_the_file_into_it() {
        // The state a pom is in while it is being written. Everything after the unclosed tag must
        // still be addressed at its own depth, or the colours walk down the file.
        let src = "<project><dependencies><dependency><groupId>com.acme</groupId>";
        assert_eq!(kinds_of(src, "com.acme"), vec![GROUP]);
    }

    #[test]
    fn a_self_closing_element_holds_nothing_and_says_so() {
        // `<relativePath/>` is the shape in every pom that inherits from a released parent. It
        // must not be read as opening a block, or everything after it is addressed one level too
        // deep and the colours walk down the file.
        let src = "<project><parent><relativePath/><artifactId>p</artifactId></parent></project>";
        assert_eq!(kinds_of(src, "p"), vec![ARTIFACT]);
        assert_eq!(kinds_of(src, "parent"), vec![TAG_ITEM, TAG_ITEM]);
    }

    #[test]
    fn every_span_is_a_real_slice_of_the_source() {
        let doc = Doc::new(POM);
        for h in highlights(&doc) {
            assert!(h.start < h.end, "{h:?}");
            assert!(POM.is_char_boundary(h.start) && POM.is_char_boundary(h.end), "{h:?}");
        }
    }
}
