//! A pom addressed as elements, over the tolerant scan.
//!
//! ## Why not the scan alone
//!
//! [`bennu_xml`]'s scan is a flat list of tags, which is the right shape for a buffer being typed
//! and the wrong one for every question asked of a pom. *Which* `<version>` is this — a
//! dependency's, the project's own, or one inside `<dependencyManagement>`? They are the same tag
//! name at four different places in the tree, they mean four different things, and a check that
//! cannot tell them apart is a check that underlines the project's own version because it is not in
//! anybody's repository.
//!
//! So this pairs each opening tag with its close, which buys three things at once: the **children**
//! of an element, the **text span** of a leaf (what a completion replaces and what a diagnostic
//! underlines), and the **ancestor path** of a caret (`project > dependencies > dependency >
//! artifactId`), which is the entire basis of knowing what to offer.
//!
//! Elements are identified by the index of their opening tag — stable for the life of the scan, and
//! all a caller ever needs.

use bennu_xml::prelude::{scan, Scan, TagKind};

/// A scanned pom, addressed by element.
pub struct Doc<'a> {
    pub source: &'a str,
    pub scan: Scan,
}

impl<'a> Doc<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, scan: scan(source) }
    }

    /// The document element (`<project>`), when there is one.
    pub fn root(&self) -> Option<usize> {
        self.scan.tags.iter().position(|t| t.kind == TagKind::Open)
    }

    /// The element's local name, prefix stripped.
    pub fn name(&self, i: usize) -> &str {
        self.scan.tags[i].local()
    }

    /// Byte offset of the element's `<`.
    pub fn start(&self, i: usize) -> usize {
        self.scan.tags[i].start
    }

    /// The direct children of the element opened at `i`, in document order.
    ///
    /// Depth-counted rather than name-matched, which is what keeps a `<dependencies>` inside
    /// `<dependencyManagement>` from being read as the project's own.
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

    pub fn child(&self, i: usize, name: &str) -> Option<usize> {
        self.children(i).into_iter().find(|c| self.name(*c) == name)
    }

    /// The span of an element's text content — quotes of nothing, the bytes between `>` and `</`.
    /// `None` for a self-closing element or one that holds other elements.
    pub fn text_span(&self, i: usize) -> Option<(usize, usize)> {
        let close = self.close_of(i)?;
        let (start, end) = (self.scan.tags[i].end, self.scan.tags[close].start);
        if start > end || end > self.source.len() {
            return None;
        }
        if self.source[start..end].contains('<') {
            return None;
        }
        Some((start, end))
    }

    /// The text content, trimmed.
    pub fn text(&self, i: usize) -> &str {
        self.text_span(i).map(|(s, e)| self.source[s..e].trim()).unwrap_or_default()
    }

    /// The text span with the surrounding whitespace trimmed off — the range a completion should
    /// replace, so accepting one does not eat the indentation of a hand-formatted pom.
    pub fn trimmed_span(&self, i: usize) -> Option<(usize, usize)> {
        let (start, end) = self.text_span(i)?;
        let raw = &self.source[start..end];
        let lead = raw.len() - raw.trim_start().len();
        let trail = raw.len() - raw.trim_end().len();
        Some((start + lead, end - trail))
    }

    pub fn child_text(&self, i: usize, name: &str) -> &str {
        self.child(i, name).map(|c| self.text(c)).unwrap_or_default()
    }

    /// The index of the close tag ending the element opened at `i`.
    pub fn close_of(&self, i: usize) -> Option<usize> {
        if self.scan.tags[i].kind != TagKind::Open {
            return None;
        }
        let mut depth = 0usize;
        for (j, t) in self.scan.tags.iter().enumerate().skip(i + 1) {
            match t.kind {
                TagKind::Open => depth += 1,
                TagKind::SelfClose => {}
                TagKind::Close => {
                    if depth == 0 {
                        return Some(j);
                    }
                    depth -= 1;
                }
            }
        }
        None
    }

    /// The chain of elements that contain `offset`, outermost first.
    ///
    /// A caret in the text of `<artifactId>` inside a `<dependency>` yields
    /// `[project, dependencies, dependency, artifactId]` — the ancestry every answer in this crate
    /// branches on.
    pub fn path_at(&self, offset: usize) -> Vec<usize> {
        let mut stack: Vec<usize> = Vec::new();
        for (i, tag) in self.scan.tags.iter().enumerate() {
            if tag.start >= offset {
                break;
            }
            match tag.kind {
                TagKind::Open => stack.push(i),
                TagKind::Close => {
                    // The caret sits inside the element that this tag closes only while it is
                    // before the `<` — which the loop guard has already established.
                    stack.pop();
                }
                TagKind::SelfClose => {}
            }
        }
        stack
    }

    /// The element names of [`Self::path_at`], for a readable match.
    pub fn path_names_at(&self, offset: usize) -> Vec<&str> {
        self.path_at(offset).into_iter().map(|i| self.name(i)).collect()
    }

    /// Every element with this name anywhere under `i`, at any depth.
    pub fn descendants(&self, i: usize, name: &str) -> Vec<usize> {
        let Some(close) = self.close_of(i) else { return Vec::new() };
        (i + 1..close).filter(|j| self.name(*j) == name && self.scan.tags[*j].kind == TagKind::Open).collect()
    }

    /// 1-based line of a byte offset.
    pub fn line_at(&self, offset: usize) -> u32 {
        self.source[..offset.min(self.source.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POM: &str = "<project>\n  <dependencies>\n    <dependency>\n      <groupId>org.x</groupId>\n\
                       <artifactId>core</artifactId>\n    </dependency>\n  </dependencies>\n</project>";

    #[test]
    fn a_caret_knows_its_ancestry() {
        let doc = Doc::new(POM);
        let at = POM.find("org.x").unwrap() + 2;
        assert_eq!(doc.path_names_at(at), ["project", "dependencies", "dependency", "groupId"]);
    }

    #[test]
    fn a_leaf_reports_the_span_a_completion_replaces() {
        let doc = Doc::new(POM);
        let at = POM.find("org.x").unwrap();
        let leaf = *doc.path_at(at + 2).last().unwrap();
        assert_eq!(doc.text(leaf), "org.x");
        let (s, e) = doc.trimmed_span(leaf).unwrap();
        assert_eq!(&POM[s..e], "org.x");
    }

    /// The distinction the whole module exists for: management's `<dependencies>` is not the
    /// project's.
    #[test]
    fn a_managed_block_is_not_the_projects_own() {
        let src = "<project><dependencyManagement><dependencies><dependency>\
                   <artifactId>a</artifactId></dependency></dependencies></dependencyManagement>\
                   <dependencies><dependency><artifactId>b</artifactId></dependency></dependencies></project>";
        let doc = Doc::new(src);
        let project = doc.root().unwrap();
        let own = doc.child(project, "dependencies").unwrap();
        let deps = doc.children(own);
        assert_eq!(deps.len(), 1);
        assert_eq!(doc.child_text(deps[0], "artifactId"), "b");
    }

    /// A buffer mid-edit has an unclosed tag and must still answer — the scan is tolerant and this
    /// must not undo that.
    #[test]
    fn an_unfinished_element_does_not_break_the_path() {
        let src = "<project><dependencies><dependency><artifactId>co";
        let doc = Doc::new(src);
        let names = doc.path_names_at(src.len());
        assert_eq!(names.last(), Some(&"artifactId"));
    }
}
