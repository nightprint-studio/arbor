//! The **TLD model** and its parser — what a tag library says about itself.
//!
//! A `.tld` is the one file that answers "what may I write inside `<s:…>`". It ships
//! inside the framework's own jar, it lists every tag with every attribute, it says
//! which of them are required, and it carries the prose the framework's website
//! reprints. An editor that does not read it is guessing at the vocabulary of the
//! language the page is mostly written in.
//!
//! Two TLD generations, one model. The 1.1/1.2 form is a DTD document
//! (`<taglib><tag><name>…`), the 2.0/2.1 form is namespaced XSD (`<taglib
//! xmlns="http://java.sun.com/xml/ns/j2ee">`), and the difference is entirely in the
//! envelope: the element names that matter are identical. So every lookup here goes
//! through the **local** name and the namespace is ignored, which is what lets one
//! parser read both without a version switch.
//!
//! Every declaration carries a byte offset into the file, because the point of
//! reading the file is being able to go there.

use std::ops::Range;

use roxmltree::{Document, Node, ParsingOptions};

/// One tag library, as its TLD describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Taglib {
    /// The canonical URI a page declares it by (`/struts-tags`). Empty in the older TLDs,
    /// which left the mapping to `web.xml` — see [`crate::catalog`] for how those resolve.
    pub uri: String,
    /// The suggested prefix (`s`, `c`), which is a hint and not a rule: a page may bind any.
    pub short_name: String,
    /// The library's own prose, for the hover on a directive.
    pub description: String,
    /// Where this library was read from — an absolute path. Also what go-to opens.
    pub source: String,
    pub tags: Vec<TagDecl>,
    pub functions: Vec<FunctionDecl>,
}

/// One `<tag>` (or `<tag-file>`) declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagDecl {
    /// The local name, without a prefix (`iterator`).
    pub name: String,
    pub description: String,
    /// `empty`, `scriptless`, `JSP`, `tagdependent` — what may go between the tags.
    pub body_content: String,
    /// The implementing class, or the `.tag` file's path for a tag file. Shown in the hover
    /// because on a legacy project it is often the only documentation there is.
    pub implementation: String,
    /// Byte offset of the declaration in its TLD.
    pub offset: usize,
    pub attrs: Vec<AttrDecl>,
    /// The tag declares `<dynamic-attributes>true`: it accepts attributes nobody wrote down,
    /// so an unknown one is legal and may not be reported.
    pub dynamic_attributes: bool,
    /// A **tag file** rather than a Java tag: its attributes live in the `.tag` file, so an
    /// empty attribute list here means *unknown*, not *none*, and nothing may be reported
    /// against it.
    pub tag_file: bool,
}

impl TagDecl {
    /// The attribute of this tag by name, if it declares one.
    pub fn attr(&self, name: &str) -> Option<&AttrDecl> {
        self.attrs.iter().find(|a| a.name == name)
    }

    /// Whether an unknown attribute on this tag can be reported at all. A tag declaring
    /// `<dynamic-attributes>true` accepts anything by design, and a tag file's list is
    /// unknown rather than empty.
    pub fn attrs_are_closed(&self) -> bool {
        !self.tag_file && !self.dynamic_attributes
    }
}

/// One `<attribute>` of a tag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttrDecl {
    pub name: String,
    pub description: String,
    pub required: bool,
    /// Accepts a runtime expression (`${…}` / `%{…}`) rather than only a literal.
    pub rtexprvalue: bool,
    /// The declared Java type, when the TLD states one.
    pub ty: String,
    /// Byte offset of the declaration in its TLD.
    pub offset: usize,
}

/// One EL `<function>` (`fn:length`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: String,
    pub signature: String,
    pub description: String,
    pub offset: usize,
}

impl Taglib {
    /// The tag this library declares under `name`, if any.
    pub fn tag(&self, name: &str) -> Option<&TagDecl> {
        self.tags.iter().find(|t| t.name == name)
    }
}

/// Read a TLD.
///
/// `source` is where it came from, kept on the model so a go-to has somewhere to land.
/// Returns `None` only when the file is not XML at all or has no `<taglib>` root — a
/// declaration it cannot make sense of is skipped, on the same principle as the DTD
/// parser next door: half a library is worth more than none, and the half that parsed is
/// the half the page is using.
pub fn parse_tld(text: &str, source: &str) -> Option<Taglib> {
    // `allow_dtd` is not optional here, it is the majority case: every 1.1/1.2 TLD opens with
    // `<!DOCTYPE taglib PUBLIC "-//Sun Microsystems…">`, and roxmltree rejects a document with a
    // DTD unless told otherwise. It never *fetches* the DTD — the flag only says "do not refuse".
    // Without it, exactly the older half of a legacy project's libraries parse to nothing, which
    // reads as "this library does not exist" everywhere downstream.
    let opts = ParsingOptions { allow_dtd: true, ..ParsingOptions::default() };
    let doc = Document::parse_with_options(text, opts).ok()?;
    let root = doc.root_element();
    if local(&root) != "taglib" {
        return None;
    }
    let mut lib = Taglib {
        uri: child_text(&root, "uri"),
        short_name: child_text(&root, "short-name"),
        description: description_of(&root),
        source: source.to_string(),
        ..Taglib::default()
    };
    for node in root.children().filter(Node::is_element) {
        match local(&node) {
            "tag" => lib.tags.push(parse_tag(&node, false)),
            "tag-file" => lib.tags.push(parse_tag(&node, true)),
            "function" => lib.functions.push(FunctionDecl {
                name: child_text(&node, "name"),
                signature: child_text(&node, "function-signature"),
                description: description_of(&node),
                offset: start_of(&node),
            }),
            _ => {}
        }
    }
    lib.tags.retain(|t| !t.name.is_empty());
    lib.functions.retain(|f| !f.name.is_empty());
    Some(lib)
}

fn parse_tag(node: &Node<'_, '_>, tag_file: bool) -> TagDecl {
    let implementation = if tag_file {
        child_text(node, "path")
    } else {
        child_text(node, "tag-class").or_else_text(|| child_text(node, "tagclass"))
    };
    TagDecl {
        name: child_text(node, "name"),
        description: description_of(node),
        body_content: child_text(node, "body-content").or_else_text(|| child_text(node, "bodycontent")),
        implementation,
        offset: start_of(node),
        dynamic_attributes: flag(&child_text(node, "dynamic-attributes")),
        attrs: node
            .children()
            .filter(Node::is_element)
            .filter(|c| local(c) == "attribute")
            .map(|c| AttrDecl {
                name: child_text(&c, "name"),
                description: description_of(&c),
                required: flag(&child_text(&c, "required")),
                rtexprvalue: flag(&child_text(&c, "rtexprvalue")),
                ty: child_text(&c, "type"),
                offset: start_of(&c),
            })
            .filter(|a| !a.name.is_empty())
            .collect(),
        tag_file,
    }
}

/// `true` / `yes` — the 1.1 TLDs say `yes`, the 2.x ones say `true`, and both appear in the
/// same project. Anything else (including absent) is false.
fn flag(text: &str) -> bool {
    matches!(text.trim().to_ascii_lowercase().as_str(), "true" | "yes")
}

/// The prose of a declaration. `<description>` is the 2.x spelling, `<info>` the 1.x one.
fn description_of(node: &Node<'_, '_>) -> String {
    let d = child_text(node, "description");
    if d.is_empty() { child_text(node, "info") } else { d }
}

/// The element's local name, namespace ignored — the one rule that lets a 1.2 TLD and a 2.1
/// TLD be read by the same code.
fn local<'a>(node: &Node<'a, '_>) -> &'a str {
    node.tag_name().name()
}

/// The trimmed text of the first child element with this local name, or empty.
fn child_text(node: &Node<'_, '_>, name: &str) -> String {
    node.children()
        .filter(Node::is_element)
        .find(|c| local(c) == name)
        .and_then(|c| c.text())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn start_of(node: &Node<'_, '_>) -> usize {
    let Range { start, .. } = node.range();
    start
}

/// "Use this unless it is empty" — the shape every legacy/modern element-name pair needs.
trait OrElseText {
    fn or_else_text(self, f: impl FnOnce() -> String) -> String;
}

impl OrElseText for String {
    fn or_else_text(self, f: impl FnOnce() -> String) -> String {
        if self.is_empty() { f() } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRUTS: &str = r#"<?xml version="1.0"?>
<!DOCTYPE taglib PUBLIC "-//Sun Microsystems, Inc.//DTD JSP Tag Library 1.2//EN" "http://java.sun.com/dtd/web-jsptaglibrary_1_2.dtd">
<taglib>
  <tlib-version>2.5</tlib-version>
  <short-name>s</short-name>
  <uri>/struts-tags</uri>
  <tag>
    <name>iterator</name>
    <tag-class>org.apache.struts2.views.jsp.IteratorTag</tag-class>
    <body-content>JSP</body-content>
    <info>Iterate over a value</info>
    <attribute>
      <name>value</name>
      <required>false</required>
      <rtexprvalue>true</rtexprvalue>
    </attribute>
    <attribute>
      <name>var</name>
      <required>yes</required>
      <type>java.lang.String</type>
    </attribute>
  </tag>
</taglib>"#;

    const JSTL21: &str = r#"<taglib xmlns="http://java.sun.com/xml/ns/j2ee" version="2.1">
  <uri>http://java.sun.com/jsp/jstl/core</uri>
  <tag>
    <name>if</name>
    <description>Conditional</description>
    <attribute><name>test</name><required>true</required></attribute>
  </tag>
  <tag-file><name>panel</name><path>/WEB-INF/tags/panel.tag</path></tag-file>
  <function>
    <name>length</name>
    <function-signature>int length(java.lang.Object)</function-signature>
  </function>
</taglib>"#;

    #[test]
    fn a_1_2_taglib_reads_its_tags_attributes_and_prose() {
        let lib = parse_tld(STRUTS, "/p/struts-tags.tld").expect("parses");
        assert_eq!(lib.uri, "/struts-tags");
        assert_eq!(lib.short_name, "s");
        let tag = lib.tag("iterator").expect("declares iterator");
        assert_eq!(tag.description, "Iterate over a value");
        assert_eq!(tag.implementation, "org.apache.struts2.views.jsp.IteratorTag");
        assert!(!tag.attr("value").expect("value").required);
        // `yes` is the 1.1 spelling of true, and both spellings live in the same project.
        assert!(tag.attr("var").expect("var").required);
        assert_eq!(tag.attr("var").expect("var").ty, "java.lang.String");
    }

    #[test]
    fn a_namespaced_2_1_taglib_reads_the_same_way() {
        let lib = parse_tld(JSTL21, "/p/c.tld").expect("parses");
        assert_eq!(lib.uri, "http://java.sun.com/jsp/jstl/core");
        assert!(lib.tag("if").expect("if").attr("test").expect("test").required);
        assert_eq!(lib.functions.len(), 1);
    }

    #[test]
    fn a_tag_file_is_a_tag_whose_attributes_are_unknown_rather_than_none() {
        let lib = parse_tld(JSTL21, "/p/c.tld").expect("parses");
        let panel = lib.tag("panel").expect("declares the tag file");
        assert!(panel.tag_file);
        assert!(panel.attrs.is_empty());
        // The distinction that keeps the checks honest: nothing may be reported against it.
        assert!(!panel.attrs_are_closed());
    }

    #[test]
    fn every_declaration_knows_where_it_is() {
        let lib = parse_tld(STRUTS, "/p/struts-tags.tld").expect("parses");
        let tag = lib.tag("iterator").expect("iterator");
        assert_eq!(&STRUTS[tag.offset..tag.offset + 5], "<tag>");
        let var = tag.attr("var").expect("var");
        assert!(STRUTS[var.offset..].starts_with("<attribute>"));
    }

    /// The regression that shipped: every 1.1/1.2 TLD opens with a `<!DOCTYPE>`, roxmltree
    /// refuses those by default, and the whole file came back as "no such library" — while
    /// Ctrl+click still found it, because the include resolver reaches the file by path and
    /// never asks whether it parsed.
    #[test]
    fn a_doctype_does_not_make_the_library_disappear() {
        assert!(STRUTS.contains("<!DOCTYPE"), "the fixture is the case under test");
        assert!(parse_tld(STRUTS, "/p/struts-tags.tld").is_some());
    }

    #[test]
    fn a_file_that_is_not_a_taglib_is_declined_rather_than_half_read() {
        assert!(parse_tld("<web-app><servlet/></web-app>", "/p/web.xml").is_none());
        assert!(parse_tld("not xml at all", "/p/x").is_none());
    }
}
