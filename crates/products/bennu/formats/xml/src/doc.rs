//! The element tree over a [`Scan`] — addressing an XML document by structure instead of by tag.
//!
//! ## Why this is not "just search for the tag"
//!
//! The scanner reports a flat list of tags with their byte spans, which is the right primitive and
//! the wrong interface. A pom's `<dependencyManagement>` contains a `<dependencies>` containing
//! `<dependency>` elements that look **identical** to the real ones and mean something completely
//! different; a `<test>` inside the Surefire plugin and a `<test>` inside a profile are two
//! unrelated values. Anything that answers by name rather than by path gets both wrong, and gets
//! them wrong quietly — a list twice too long is still a list.
//!
//! So every lookup here is *depth-counted*: [`children`](Doc::children) walks to the matching close
//! tag and yields only what sits directly inside, and nothing crosses an element boundary.
//!
//! ## Reading and writing are the same walk
//!
//! Every accessor comes in two forms — the value ([`text`](Doc::text)) and the byte span it
//! occupies ([`text_span`](Doc::text_span)) — because a reader and a writer that locate a value
//! separately drift, and the drift lands an edit one space to the left of what was read. Same
//! trimming rule, one implementation, so `<version> 1.2 </version>` yields the span of `1.2` and
//! replacing it leaves the whitespace alone.
//!
//! Tolerant like the scanner under it: a malformed document yields fewer answers, never an error.
//! Nothing here parses entities — `&amp;` stays as written, which is what every caller so far
//! wants, since a coordinate, a scope and a module name have never contained one.

use crate::scan::{scan, Scan, TagKind};

/// A scanned document, addressed by tag index.
///
/// Elements are identified by the index of their **opening tag**, which is all a caller ever needs:
/// from it come the children, the text and the byte spans, and it stays valid for the life of the
/// `Doc`.
pub struct Doc<'a> {
    source: &'a str,
    scan: Scan,
}

impl<'a> Doc<'a> {
    /// Scan `source` and address it as a tree.
    pub fn new(source: &'a str) -> Self {
        Self { source, scan: scan(source) }
    }

    /// The text this document was built from.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The underlying scan, for a caller that needs the tags themselves.
    pub fn scan(&self) -> &Scan {
        &self.scan
    }

    /// The document element, when there is one.
    pub fn root(&self) -> Option<usize> {
        self.scan.tags.iter().position(|t| t.kind == TagKind::Open)
    }

    /// The local name of the element opened at `i` — namespace prefix stripped.
    pub fn name(&self, i: usize) -> &str {
        self.scan.tags.get(i).map(|t| t.local()).unwrap_or_default()
    }

    /// The direct children of the element opened at `i`, in document order.
    pub fn children(&self, i: usize) -> Vec<usize> {
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

    /// The first direct child of `i` named `name`.
    pub fn child(&self, i: usize, name: &str) -> Option<usize> {
        self.children(i).into_iter().find(|c| self.name(*c) == name)
    }

    /// The text content of the element opened at `i`, trimmed. Empty for a self-closing element or
    /// one that holds other elements.
    pub fn text(&self, i: usize) -> String {
        self.text_span(i).map(|(s, e)| self.source[s..e].to_string()).unwrap_or_default()
    }

    /// [`text`](Self::text) of the first child named `name`.
    pub fn child_text(&self, i: usize, name: &str) -> String {
        self.child(i, name).map(|c| self.text(c)).unwrap_or_default()
    }

    /// The byte span of the trimmed text content of the element opened at `i`, or `None` when it
    /// has none — the writable half of [`text`](Self::text), for a caller replacing a value rather
    /// than reading one.
    pub fn text_span(&self, i: usize) -> Option<(usize, usize)> {
        let (start, end) = self.inner_span(i)?;
        let raw = &self.source[start..end];
        // An element holding other elements has no text of its own, and a span covering its
        // children is not something any caller would want to overwrite.
        if raw.contains('<') {
            return None;
        }
        let lead = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some((start + lead, start + lead + trimmed.len()))
    }

    /// The byte span **between** the open and close tags of the element opened at `i` — its
    /// content, whitespace and child elements included. `None` for a self-closing or unclosed one.
    ///
    /// This is where an insertion goes: appending a `<module>` to a `<modules>` means writing just
    /// before the end of this span.
    pub fn inner_span(&self, i: usize) -> Option<(usize, usize)> {
        let close = self.close_of(i)?;
        let (start, end) = (self.scan.tags[i].end, self.scan.tags[close].start);
        (start <= end && end <= self.source.len()).then_some((start, end))
    }

    /// The byte span of the whole element opened at `i`, open tag through close tag. For a
    /// self-closing element, the tag itself.
    pub fn element_span(&self, i: usize) -> Option<(usize, usize)> {
        let tag = self.scan.tags.get(i)?;
        match tag.kind {
            TagKind::SelfClose => Some((tag.start, tag.end)),
            TagKind::Open => self.close_of(i).map(|c| (tag.start, self.scan.tags[c].end)),
            TagKind::Close => None,
        }
    }

    /// The index of the close tag that ends the element opened at `i`.
    pub fn close_of(&self, i: usize) -> Option<usize> {
        if self.scan.tags.get(i)?.kind != TagKind::Open {
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

    /// The whitespace the line containing `offset` begins with — what a new sibling has to be
    /// written with to look like it was always there.
    pub fn indent_at(&self, offset: usize) -> &'a str {
        let offset = offset.min(self.source.len());
        let line_start = self.source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line = &self.source[line_start..offset];
        &line[..line.len() - line.trim_start().len()]
    }

    /// 1-based line of a byte offset.
    pub fn line_at(&self, offset: usize) -> u32 {
        self.source[..offset.min(self.source.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<project>
  <artifactId>web</artifactId>
  <modules>
    <module>core</module>
    <module>api</module>
  </modules>
  <dependencyManagement>
    <dependencies>
      <dependency><artifactId>managed</artifactId></dependency>
    </dependencies>
  </dependencyManagement>
  <empty/>
  <blank>   </blank>
</project>"#;

    fn doc() -> Doc<'static> {
        Doc::new(XML)
    }

    #[test]
    fn children_do_not_cross_an_element_boundary() {
        let d = doc();
        let root = d.root().unwrap();
        let names: Vec<&str> = d.children(root).into_iter().map(|c| d.name(c)).collect();
        // `<dependencies>` is a *grandchild*, and the managed `<dependency>` deeper still — the
        // whole reason this walks structure rather than scanning for tags.
        assert_eq!(names, ["artifactId", "modules", "dependencyManagement", "empty", "blank"]);
    }

    #[test]
    fn text_is_trimmed_and_its_span_agrees_with_it() {
        let d = doc();
        let root = d.root().unwrap();
        let artifact = d.child(root, "artifactId").unwrap();
        assert_eq!(d.text(artifact), "web");
        let (s, e) = d.text_span(artifact).unwrap();
        assert_eq!(&XML[s..e], "web");
    }

    #[test]
    fn an_element_holding_elements_has_no_text_to_overwrite() {
        let d = doc();
        let root = d.root().unwrap();
        let modules = d.child(root, "modules").unwrap();
        assert_eq!(d.text(modules), "");
        assert_eq!(d.text_span(modules), None);
        // Its inner span is still there — that is where an insertion goes.
        let (s, e) = d.inner_span(modules).unwrap();
        assert!(XML[s..e].contains("<module>core</module>"));
    }

    #[test]
    fn self_closing_and_blank_elements_have_no_text_span() {
        let d = doc();
        let root = d.root().unwrap();
        assert_eq!(d.text_span(d.child(root, "empty").unwrap()), None);
        assert_eq!(d.text_span(d.child(root, "blank").unwrap()), None);
    }

    #[test]
    fn indent_is_the_leading_whitespace_of_the_line() {
        let d = doc();
        let root = d.root().unwrap();
        let modules = d.child(root, "modules").unwrap();
        let start = d.scan().tags[modules].start;
        assert_eq!(d.indent_at(start), "  ");
    }
}
