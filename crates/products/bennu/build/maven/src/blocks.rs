//! The coordinate-bearing blocks of a pom, found once and reused by every answer.
//!
//! A `<dependency>`, a `<parent>`, a `<plugin>` and an `<exclusion>` are the same five fields in
//! four places that mean different things, and every answer in this crate — the red underline, the
//! completion list, the hover card, the jump — starts by asking *which one is this, and what does it
//! already say*. Deriving that four times from the element tree is how the four end up disagreeing.
//!
//! Each field keeps its **span** as well as its text, because a diagnostic underlines the value and
//! not the element, and a completion replaces the value and not the indentation around it.

use crate::doc::Doc;
use crate::repo::Coord;

/// Where a coordinate sits, which is what decides how it is judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// `<project><dependencies>` — the ones that are actually on the classpath.
    Dependency,
    /// `<dependencyManagement>` — a version for a dependency this module may not even have. Not on
    /// any classpath, so "not resolved" is not a problem here in the way it is above.
    Managed,
    /// `<parent>`.
    Parent,
    /// `<build><plugins>` or `<pluginManagement>`. A plugin's groupId defaults to
    /// `org.apache.maven.plugins` and its version usually comes from the super-pom, so an absent
    /// one is normal rather than a mistake.
    Plugin,
    /// `<exclusions><exclusion>` — a `groupId:artifactId` with no version by definition.
    Exclusion,
    /// `<build><extensions>`.
    Extension,
}

impl BlockKind {
    /// Whether a missing artifact here means the compiler will not find a type. Only the real
    /// dependencies of the module do.
    pub fn is_classpath(self) -> bool {
        matches!(self, BlockKind::Dependency)
    }

    /// The groupId Maven assumes when the block does not write one.
    pub fn default_group(self) -> &'static str {
        match self {
            BlockKind::Plugin => "org.apache.maven.plugins",
            _ => "",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BlockKind::Dependency => "dependency",
            BlockKind::Managed => "managed dependency",
            BlockKind::Parent => "parent",
            BlockKind::Plugin => "plugin",
            BlockKind::Exclusion => "exclusion",
            BlockKind::Extension => "build extension",
        }
    }
}

/// One field of a block: what it says, and where it says it.
#[derive(Debug, Clone, Default)]
pub struct Field {
    /// The element index, when the field is written at all.
    pub element: Option<usize>,
    pub text: String,
    /// The span of the value, whitespace trimmed off — what a diagnostic underlines and a
    /// completion replaces.
    pub span: Option<(usize, usize)>,
}

impl Field {
    pub fn is_written(&self) -> bool {
        self.element.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The span to underline, falling back to `fallback` when the field is not written at all
    /// (there is nothing at the field's own position to point at).
    pub fn span_or(&self, fallback: (usize, usize)) -> (usize, usize) {
        match self.span {
            Some((s, e)) if e > s => (s, e),
            _ => fallback,
        }
    }
}

/// A coordinate as one place in the pom writes it.
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    /// The element index of the block itself.
    pub element: usize,
    /// The span of the block's opening tag — where a diagnostic lands when the field it is about
    /// was never written.
    pub tag: (usize, usize),
    pub group: Field,
    pub artifact: Field,
    pub version: Field,
    pub packaging: Field,
    pub classifier: Field,
    pub scope: Field,
    pub optional: Field,
    /// The `<profile>` id this sits under, empty for the ordinary case. A profile's dependency may
    /// legitimately be absent from a machine that never builds with it.
    pub profile: String,
}

impl Block {
    /// The coordinate as written, with the block's default groupId applied and nothing expanded.
    pub fn raw_coord(&self) -> Coord {
        let group = match self.group.text.is_empty() {
            true => self.kind.default_group().to_string(),
            false => self.group.text.clone(),
        };
        Coord {
            group_id: group,
            artifact_id: self.artifact.text.clone(),
            version: self.version.text.clone(),
            classifier: self.classifier.text.clone(),
            packaging: self.packaging.text.clone(),
        }
    }
}

/// Every coordinate-bearing block in the document, in order.
///
/// One pass with an element stack: the ancestry is what separates a `<dependency>` inside
/// `<dependencyManagement>` from a real one, and inside `<exclusions>` from either.
pub fn blocks(doc: &Doc<'_>) -> Vec<Block> {
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for (i, tag) in doc.scan.tags.iter().enumerate() {
        match tag.kind {
            // `<parent/>` and friends carry nothing to read.
            bennu_xml::prelude::TagKind::SelfClose => continue,
            bennu_xml::prelude::TagKind::Close => {
                stack.pop();
                continue;
            }
            bennu_xml::prelude::TagKind::Open => stack.push(i),
        }
        if let Some(kind) = kind_of(doc, &stack) {
            out.push(read_block(doc, kind, &stack));
        }
    }
    out
}

/// The block a caret is in, plus the element the caret itself sits in.
pub fn block_at(doc: &Doc<'_>, offset: usize) -> Option<(Block, usize)> {
    let path = doc.path_at(offset);
    let leaf = *path.last()?;
    // Innermost first: a caret inside an `<exclusion>` is in the exclusion, not in the dependency
    // that holds it.
    for depth in (0..path.len()).rev() {
        if let Some(kind) = kind_of(doc, &path[..=depth]) {
            return Some((read_block(doc, kind, &path[..=depth]), leaf));
        }
    }
    None
}

/// Which kind of block the element at the end of `ancestry` is, or `None` for anything else.
fn kind_of(doc: &Doc<'_>, ancestry: &[usize]) -> Option<BlockKind> {
    let name = doc.name(*ancestry.last()?);
    let under = |what: &str| ancestry.iter().any(|i| doc.name(*i) == what);
    match name {
        "exclusion" => Some(BlockKind::Exclusion),
        "dependency" if under("dependencyManagement") => Some(BlockKind::Managed),
        "dependency" => Some(BlockKind::Dependency),
        // `pluginManagement` sits under `<build>` like `<plugins>` does, and its entries are
        // judged the same way: they configure a plugin rather than put anything on a classpath.
        "plugin" => Some(BlockKind::Plugin),
        "extension" => Some(BlockKind::Extension),
        // Only the project's own parent — `<parent>` is not a name anything else in a pom uses,
        // but the check costs one comparison and rules out a future one.
        "parent" if ancestry.len() == 2 => Some(BlockKind::Parent),
        _ => None,
    }
}

fn read_block(doc: &Doc<'_>, kind: BlockKind, ancestry: &[usize]) -> Block {
    let element = *ancestry.last().expect("a block has an element");
    let tag = &doc.scan.tags[element];
    // The profile's own id, not just "there is one": a dependency that only exists under `-Pwas`
    // is legitimately absent from a machine that never builds with it, and the message says which.
    let profile = ancestry
        .iter()
        .find(|i| doc.name(**i) == "profile")
        .map(|i| match doc.child_text(*i, "id") {
            "" => "profile".to_string(),
            id => id.to_string(),
        })
        .unwrap_or_default();
    Block {
        kind,
        element,
        tag: (tag.start, tag.end.min(doc.source.len())),
        group: field(doc, element, "groupId"),
        artifact: field(doc, element, "artifactId"),
        version: field(doc, element, "version"),
        packaging: field(doc, element, "type"),
        classifier: field(doc, element, "classifier"),
        scope: field(doc, element, "scope"),
        optional: field(doc, element, "optional"),
        profile,
    }
}

fn field(doc: &Doc<'_>, block: usize, name: &str) -> Field {
    match doc.child(block, name) {
        Some(child) => Field {
            element: Some(child),
            text: doc.text(child).to_string(),
            span: doc.trimmed_span(child),
        },
        None => Field::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POM: &str = r#"<project>
  <parent><groupId>com.acme</groupId><artifactId>parent</artifactId><version>1.0</version></parent>
  <dependencyManagement><dependencies>
    <dependency><groupId>org.x</groupId><artifactId>managed</artifactId><version>2.0</version></dependency>
  </dependencies></dependencyManagement>
  <dependencies>
    <dependency><groupId>org.y</groupId><artifactId>real</artifactId><version>3.0</version>
      <exclusions><exclusion><groupId>org.z</groupId><artifactId>gone</artifactId></exclusion></exclusions>
    </dependency>
  </dependencies>
  <build><plugins><plugin><artifactId>maven-compiler-plugin</artifactId></plugin></plugins></build>
</project>"#;

    #[test]
    fn each_coordinate_is_classified_by_where_it_sits() {
        let doc = Doc::new(POM);
        let kinds: Vec<BlockKind> = blocks(&doc).iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Parent,
                BlockKind::Managed,
                BlockKind::Dependency,
                BlockKind::Exclusion,
                BlockKind::Plugin
            ]
        );
    }

    /// Only the real dependencies decide whether a type resolves — the distinction the red
    /// underline stands on.
    #[test]
    fn only_a_real_dependency_is_on_the_classpath() {
        let doc = Doc::new(POM);
        let on_classpath: Vec<String> = blocks(&doc)
            .iter()
            .filter(|b| b.kind.is_classpath())
            .map(|b| b.artifact.text.clone())
            .collect();
        assert_eq!(on_classpath, ["real"]);
    }

    /// A plugin with no groupId is `org.apache.maven.plugins`, which is how nearly every pom
    /// writes one — reading it as an empty group marks the whole build section as unresolved.
    #[test]
    fn a_plugin_without_a_group_gets_mavens_own() {
        let doc = Doc::new(POM);
        let plugin = blocks(&doc).into_iter().find(|b| b.kind == BlockKind::Plugin).unwrap();
        assert_eq!(plugin.raw_coord().ga(), "org.apache.maven.plugins:maven-compiler-plugin");
    }

    #[test]
    fn a_caret_finds_the_block_it_is_in() {
        let doc = Doc::new(POM);
        let at = POM.find("org.y").unwrap() + 2;
        let (block, leaf) = block_at(&doc, at).unwrap();
        assert_eq!(block.kind, BlockKind::Dependency);
        assert_eq!(doc.name(leaf), "groupId");
        assert_eq!(block.artifact.text, "real");
        let (s, e) = block.artifact.span.unwrap();
        assert_eq!(&POM[s..e], "real");
    }
}
