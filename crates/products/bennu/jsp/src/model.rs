//! The **model** of a page — a JSP read in JSP's own vocabulary.
//!
//! The syntax-tree panel's second tab, and the two answer different questions. The parse shows
//! every node the grammar built, `<` and `"` included: *why did it read my file that way*. This
//! shows what Bennu **understood** — which library each tag comes from, which of its attributes
//! the TLD actually declares, which values are expressions and which are text — because that is
//! what completion, the checks and go-to reason over, and when the two disagree the second is the
//! one that explains a wrong answer.
//!
//! ## Three things the parse cannot tell you
//!
//! **Nesting.** The grammar is deliberately flat: `start_tag` and `end_tag` are siblings, which is
//! what keeps a page with unbalanced markup colouring correctly instead of collapsing into one
//! ERROR node. A reader wants the nesting anyway, so it is rebuilt here — tolerantly, by pairing
//! names on a stack. A close with no open, or an open a page never closes, is normal in a legacy
//! JSP and must not lose the rest of the file.
//!
//! **Which library a tag belongs to.** `<s:iterator>` is a name until the page's own `<%@ taglib
//! %>` line says what `s` is and the catalog says where that lives.
//!
//! **What is an expression.** `value="%{codice}"` and `value="Codice"` are the same shape to a
//! grammar and opposite things to a reader.
//!
//! ## What is left out, deliberately
//!
//! Text. A page is mostly prose and markup, and a model listing every run of it would bury the
//! twenty rows that carry meaning under two thousand that do not. The parse tab has them all.
//!
//! ## Resolution is optional
//!
//! `catalog: None` yields the same tree with the library columns empty — which is what a unit
//! test can build, and also what an unindexed project honestly deserves.

use crate::catalog::TaglibCatalog;
use crate::directives::taglib_directives;
use crate::tld::{TagDecl, Taglib};

use tree_sitter::Node;

/// One row of the model.
///
/// The same three columns as the Java model tree, so one panel draws both: what it **is**, the
/// **role** it plays, and its **name** with whatever it resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JspModelNode {
    /// What it is — `taglib`, `tag`, `attribute`, `expression`, `scriptlet`.
    pub kind: String,
    /// The accent column: the library a tag came from, the flavour of a value, the declared type
    /// of an attribute. `None` when the row has nothing more to say than its name.
    pub field: Option<String>,
    /// The name, and what it resolved to.
    pub text: Option<String>,
    /// Byte range in the source — every row selects its own bytes.
    pub span: (usize, usize),
    pub children: Vec<JspModelNode>,
}

impl JspModelNode {
    fn new(kind: &str, span: (usize, usize)) -> Self {
        Self { kind: kind.to_string(), field: None, text: None, span, children: Vec::new() }
    }

    fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// What the page declares, so a tag can be told which library it belongs to.
///
/// Built once per page rather than per tag: a legacy JSP has five directives and two thousand
/// tags, and a linear scan per tag is the kind of accident that only shows up on the big files.
struct Libraries<'a> {
    /// `prefix` → the `uri` the page wrote.
    declared: Vec<(String, String)>,
    catalog: Option<&'a TaglibCatalog>,
}

impl<'a> Libraries<'a> {
    fn of(source: &str, catalog: Option<&'a TaglibCatalog>) -> Self {
        let declared = taglib_directives(source)
            .into_iter()
            .map(|d| (d.prefix, d.uri))
            .collect();
        Self { declared, catalog }
    }

    fn uri_of(&self, prefix: &str) -> Option<&str> {
        self.declared.iter().find(|(p, _)| p.as_str() == prefix).map(|(_, u)| u.as_str())
    }

    fn taglib_of(&self, prefix: &str) -> Option<&Taglib> {
        let uri = self.uri_of(prefix)?;
        self.catalog?.resolve(uri).map(|t| t.as_ref())
    }
}

/// The model of `source`. `catalog` supplies the TLDs when the project has them.
pub fn model(source: &str, catalog: Option<&TaglibCatalog>) -> JspModelNode {
    let mut root = JspModelNode::new("page", (0, source.len()));
    let Some(tree) = bennu_jsp_grammar::prelude::parse_jsp(source) else { return root };
    let libs = Libraries::of(source, catalog);

    // The open tags still waiting for their close, innermost last. A row is pushed onto the
    // stack's top rather than onto the root, which is what turns a flat sibling list into a tree.
    let mut open: Vec<(String, JspModelNode)> = Vec::new();
    let mut cursor = tree.walk();
    for node in tree.root_node().children(&mut cursor) {
        visit(node, source, &libs, &mut root, &mut open);
    }
    // Whatever a page never closed still belongs to the reader — dropped rows would be a model
    // that quietly disagrees with the file.
    while let Some((_, node)) = open.pop() {
        push(&mut root, &mut open, node);
    }
    root
}

/// Add `node` under the innermost open tag, or under the page when there is none.
fn push(root: &mut JspModelNode, open: &mut [(String, JspModelNode)], node: JspModelNode) {
    match open.last_mut() {
        Some((_, parent)) => parent.children.push(node),
        None => root.children.push(node),
    }
}

fn visit(
    node: Node<'_>,
    source: &str,
    libs: &Libraries<'_>,
    root: &mut JspModelNode,
    open: &mut Vec<(String, JspModelNode)>,
) {
    let span = (node.start_byte(), node.end_byte());
    match node.kind() {
        "jsp_directive" => {
            let row = directive_row(node, source, libs);
            push(root, open, row);
        }
        "start_tag" => {
            let name = tag_name(node, source).unwrap_or_default();
            let row = tag_row(node, source, libs, "tag");
            open.push((name, row));
        }
        "self_closing_tag" => {
            let row = tag_row(node, source, libs, "tag");
            push(root, open, row);
        }
        "end_tag" => {
            let name = tag_name(node, source).unwrap_or_default();
            close_tag(&name, root, open);
        }
        // `<script>` / `<style>` are containers in the grammar, so their body arrives as a child
        // rather than as a sibling — one row for the element, one for what is inside it.
        "script_element" | "style_element" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                visit(child, source, libs, root, open);
            }
        }
        "script_tag" | "style_tag" => {}
        "script_content" => {
            push(root, open, JspModelNode::new("script", span).with_field("JavaScript"));
        }
        "style_content" => {
            push(root, open, JspModelNode::new("style", span).with_field("CSS"));
        }
        "jsp_scriptlet" => push(root, open, JspModelNode::new("scriptlet", span).with_field("Java")),
        "jsp_declaration" => {
            push(root, open, JspModelNode::new("declaration", span).with_field("Java"));
        }
        "jsp_expression" => {
            let row = JspModelNode::new("expression", span)
                .with_field("Java")
                .with_text(inner_of(source, span, "<%=", "%>"));
            push(root, open, row);
        }
        "el_expression" => push(root, open, expression_row(source, span, "EL")),
        "ognl_expression" => push(root, open, expression_row(source, span, "OGNL")),
        "jsp_comment" | "html_comment" => push(root, open, JspModelNode::new("comment", span)),
        // Text, whitespace, doctype, cdata, stray: see the module doc.
        _ => {}
    }
}

/// Close the innermost open tag named `name`.
///
/// The innermost **matching** one, not simply the innermost: a page that closes `</td>` while a
/// `<b>` is still open is closing the `td`, and treating the close as the `b`'s would nest the
/// rest of the table inside a bold. When nothing matches — a close with no open at all — the row
/// is dropped rather than closing something arbitrary.
fn close_tag(name: &str, root: &mut JspModelNode, open: &mut Vec<(String, JspModelNode)>) {
    let Some(at) = open.iter().rposition(|(n, _)| n.eq_ignore_ascii_case(name)) else { return };
    while open.len() > at {
        let (_, node) = open.pop().expect("the position exists");
        push(root, open, node);
    }
}

fn expression_row(source: &str, span: (usize, usize), flavour: &str) -> JspModelNode {
    let body = source
        .get(span.0..span.1)
        .unwrap_or_default()
        .trim_start_matches(['$', '#', '%'])
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    JspModelNode::new("expression", span).with_field(flavour).with_text(body)
}

fn inner_of(source: &str, span: (usize, usize), open: &str, close: &str) -> String {
    source
        .get(span.0..span.1)
        .unwrap_or_default()
        .trim_start_matches(open)
        .trim_end_matches(close)
        .trim()
        .to_string()
}

// ── directives ──────────────────────────────────────────────────────────────────

/// `<%@ taglib %>` gets its own kind — it is the legend for every namespaced tag below it, and a
/// reader scanning for "where does `wp:` come from" should not have to read past `page` and
/// `include` to find it.
fn directive_row(node: Node<'_>, source: &str, libs: &Libraries<'_>) -> JspModelNode {
    let span = (node.start_byte(), node.end_byte());
    let text = source.get(span.0..span.1).unwrap_or_default();
    let body = text.trim_start_matches("<%@").trim_end_matches("%>").trim();
    let name = body.split_whitespace().next().unwrap_or("").to_ascii_lowercase();

    match name.as_str() {
        "taglib" => {
            let prefix = directive_attr(body, "prefix").unwrap_or_default();
            let uri = directive_attr(body, "uri")
                .or_else(|| directive_attr(body, "tagdir"))
                .unwrap_or_default();
            // What the page asked for, and what it got. The second half is the whole reason a
            // taglib row is worth a line: `uri="aps-core.tld"` resolving to a file inside a jar
            // is the fact that decides whether every `<wp:…>` below it can be checked at all.
            let resolved = match libs.catalog.and_then(|c| c.resolve(&uri)) {
                Some(lib) => format!(" → {}", lib.source),
                None if libs.catalog.is_some() => " → no library with this uri".to_string(),
                None => String::new(),
            };
            JspModelNode::new("taglib", span)
                .with_field(prefix.clone())
                .with_text(format!("{prefix} : {uri}{resolved}"))
        }
        "include" => JspModelNode::new("include", span)
            .with_field("static")
            .with_text(directive_attr(body, "file").unwrap_or_default()),
        _ => JspModelNode::new("directive", span).with_field(name).with_text(
            body.split_once(char::is_whitespace).map(|(_, rest)| rest.trim()).unwrap_or(""),
        ),
    }
}

/// The value of `name="…"` inside a directive body. Tolerant of both quote styles, and of the
/// spaces a legacy page puts around the `=`.
///
/// The name has to be a **whole word** followed by an `=`: `<%@ taglib prefix="uri" …%>` would
/// otherwise answer the question "what is the uri" with the letters it found inside the prefix.
fn directive_attr(body: &str, name: &str) -> Option<String> {
    let mut from = 0usize;
    while let Some(rel) = body[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before_is_boundary =
            at == 0 || body[..at].chars().next_back().is_some_and(|c| !is_name_char(c));
        if !before_is_boundary {
            continue;
        }
        let rest = body[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        let rest = &rest[quote.len_utf8()..];
        if let Some(end) = rest.find(quote) {
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == ':'
}

// ── tags ────────────────────────────────────────────────────────────────────────

/// Whether `prefix` is one the JSP specification defines rather than one a page declares.
///
/// `<jsp:include>`, `<jsp:useBean>`, `<jsp:param>` need no `<%@ taglib %>` — the container knows
/// them, and reporting them as undeclared is the model calling the one vocabulary every page can
/// rely on a mistake. The rest of the list is reserved by the specification for the same reason;
/// a page cannot bind those prefixes to anything of its own either.
fn is_standard_prefix(prefix: &str) -> bool {
    matches!(prefix, "jsp" | "jspx" | "java" | "javax" | "servlet" | "sun" | "sunw")
}

fn tag_name(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    // Bound to a local rather than left as the tail expression: the iterator borrows `cursor`,
    // and a temporary in tail position is dropped *after* the function's locals — which the
    // borrow checker refuses. The same shape the rest of the workspace's tree walks use.
    let name = node
        .children(&mut cursor)
        .find(|c| c.kind() == "tag_name")
        .and_then(|c| source.get(c.start_byte()..c.end_byte()))
        .map(str::to_string);
    name
}

fn tag_row(node: Node<'_>, source: &str, libs: &Libraries<'_>, kind: &str) -> JspModelNode {
    let span = (node.start_byte(), node.end_byte());
    let qname = tag_name(node, source).unwrap_or_default();
    let (prefix, local) = match qname.split_once(':') {
        Some((p, l)) => (Some(p.to_string()), l.to_string()),
        None => (None, qname.clone()),
    };

    // Four states, and they must read as four different things: plain HTML (no prefix at all),
    // the **standard actions**, a prefix the page declared, and a prefix nobody declared — the
    // last being the single most common reason a taglib "stops working", and worth seeing rather
    // than inferring.
    let (field, decl) = match &prefix {
        None => (None, None),
        Some(p) if is_standard_prefix(p) => (Some("JSP standard actions".to_string()), None),
        Some(p) => match libs.uri_of(p) {
            None => (Some(format!("undeclared prefix '{p}'")), None),
            Some(uri) => {
                let lib = libs.taglib_of(p);
                let decl = lib.and_then(|l| l.tag(&local));
                let note = match (lib, decl) {
                    (Some(_), None) => format!("{uri} — no such tag"),
                    _ => uri.to_string(),
                };
                (Some(note), decl)
            }
        },
    };

    let mut row = JspModelNode::new(kind, span).with_text(qname);
    row.field = field;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute" {
            row.children.push(attribute_row(child, source, decl));
        }
    }
    row
}

/// One attribute: its name, its value, and the two things that are not in the text — whether the
/// value is an **expression**, and what the TLD says the attribute is.
fn attribute_row(node: Node<'_>, source: &str, tag: Option<&TagDecl>) -> JspModelNode {
    let span = (node.start_byte(), node.end_byte());
    let mut name = String::new();
    let mut value: Option<String> = None;
    let mut flavour: Option<&str> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "attribute_name" => {
                name = source.get(child.start_byte()..child.end_byte()).unwrap_or("").to_string();
            }
            "quoted_value_double" | "quoted_value_single" | "unquoted_value" => {
                let raw = source.get(child.start_byte()..child.end_byte()).unwrap_or("");
                value = Some(raw.trim_matches(['"', '\'']).to_string());
                let mut inner = child.walk();
                for part in child.children(&mut inner) {
                    match part.kind() {
                        "el_expression" => flavour = Some("EL"),
                        "ognl_expression" => flavour = Some("OGNL"),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // The declared type is the useful half of what a TLD knows about an attribute — `required`
    // belongs to the check that enforces it, not to a row someone is reading to orient themselves.
    let declared = tag.and_then(|t| t.attr(&name));
    let field = match (flavour, declared, tag) {
        (Some(f), Some(a), _) if !a.ty.is_empty() => Some(format!("{f} : {}", short_type(&a.ty))),
        (Some(f), _, _) => Some(f.to_string()),
        (None, Some(a), _) if !a.ty.is_empty() => Some(short_type(&a.ty).to_string()),
        (None, None, Some(t)) if t.attrs_are_closed() => Some("not declared".to_string()),
        _ => None,
    };

    let text = match value {
        Some(v) => format!("{name} = {v}"),
        None => name,
    };
    let mut row = JspModelNode::new("attribute", span).with_text(text);
    row.field = field;
    row
}

/// `java.lang.String` → `String`. A model row is read at a glance and the package never carries
/// the meaning.
fn short_type(ty: &str) -> &str {
    ty.rsplit('.').next().unwrap_or(ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = concat!(
        "<%@ page contentType=\"text/html\" pageEncoding=\"UTF-8\" %>\n",
        "<%@ taglib prefix=\"s\" uri=\"/struts-tags\" %>\n",
        "<table>\n",
        "  <s:iterator var=\"riga\" value=\"comunicazioni.dati\">\n",
        "    <tr><td><s:property value=\"%{codice}\"/></td></tr>\n",
        "  </s:iterator>\n",
        "</table>\n",
    );

    fn find<'t>(node: &'t JspModelNode, kind: &str) -> Option<&'t JspModelNode> {
        if node.kind == kind {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, kind))
    }

    fn find_text<'t>(node: &'t JspModelNode, text: &str) -> Option<&'t JspModelNode> {
        if node.text.as_deref().is_some_and(|t| t.starts_with(text)) {
            return Some(node);
        }
        node.children.iter().find_map(|c| find_text(c, text))
    }

    /// **The whole point of the tab.** The grammar is flat; a reader is not.
    #[test]
    fn a_tag_body_nests_under_the_tag_that_opened_it() {
        let root = model(PAGE, None);
        let iterator = find_text(&root, "s:iterator").expect("the iterator");
        assert!(
            find_text(iterator, "s:property").is_some(),
            "the property is inside the iterator, not beside it"
        );
    }

    /// A close with no open, and an open with no close, are both ordinary in a legacy page. They
    /// must cost the rows below them nothing.
    #[test]
    fn unbalanced_markup_keeps_every_row() {
        let root = model("</div><br><s:if test=\"%{x}\"><p>text", None);
        assert!(find_text(&root, "s:if").is_some());
        assert!(find_text(&root, "p").is_some());
    }

    /// A page that closes an outer tag while an inner one is still open closes the outer — and
    /// takes the inner with it, rather than nesting the rest of the file inside it.
    #[test]
    fn closing_an_outer_tag_closes_what_is_still_open_inside_it() {
        let root = model("<table><b>x</table><hr/>", None);
        assert!(
            root.children.iter().any(|c| c.text.as_deref() == Some("hr")),
            "the <hr/> is back at the top level once the table closed, not nested inside the <b>"
        );
    }

    #[test]
    fn the_taglib_row_carries_the_prefix_and_the_uri() {
        let root = model(PAGE, None);
        let taglib = find(&root, "taglib").expect("the taglib row");
        assert_eq!(taglib.field.as_deref(), Some("s"));
        assert_eq!(taglib.text.as_deref(), Some("s : /struts-tags"));
    }

    /// The single most common reason a page's taglib "stops working", and the model says it in
    /// the one column a reader is already looking at.
    #[test]
    fn a_prefix_the_page_never_declared_says_so() {
        let root = model("<wp:currentPage param=\"code\"/>", None);
        let tag = find_text(&root, "wp:currentPage").expect("the tag");
        assert_eq!(tag.field.as_deref(), Some("undeclared prefix 'wp'"));
    }

    /// …and `jsp:` is not that. The standard actions need no directive — the container knows
    /// them — so calling them undeclared would flag the one vocabulary every page can rely on.
    #[test]
    fn the_standard_actions_need_no_directive() {
        let root = model("<jsp:include page=\"/WEB-INF/head.jsp\"/>", None);
        let tag = find_text(&root, "jsp:include").expect("the tag");
        assert_eq!(tag.field.as_deref(), Some("JSP standard actions"));
    }

    /// An expression and a piece of text are the same shape to a grammar and opposite things to
    /// a reader — so the flavour is a column, not something to squint at the quotes for.
    #[test]
    fn an_expression_valued_attribute_is_marked_as_one() {
        let root = model(PAGE, None);
        let property = find_text(&root, "s:property").expect("the property");
        let value = property.children.first().expect("its attribute");
        assert_eq!(value.text.as_deref(), Some("value = %{codice}"));
        assert_eq!(value.field.as_deref(), Some("OGNL"));
    }

    #[test]
    fn a_literal_valued_attribute_is_not() {
        let root = model(PAGE, None);
        let iterator = find_text(&root, "s:iterator").expect("the iterator");
        let var = iterator.children.first().expect("var=");
        assert_eq!(var.text.as_deref(), Some("var = riga"));
        assert_eq!(var.field, None, "a name is not an expression");
    }

    /// Text is the bulk of a page and none of its meaning — see the module doc.
    #[test]
    fn prose_does_not_reach_the_model() {
        let root = model("<p>Lorem ipsum dolor sit amet</p>", None);
        assert!(find(&root, "text").is_none());
    }

    #[test]
    fn every_row_selects_its_own_bytes() {
        let root = model(PAGE, None);
        let property = find_text(&root, "s:property").expect("the property");
        assert_eq!(&PAGE[property.span.0..property.span.1], "<s:property value=\"%{codice}\"/>");
    }

    #[test]
    fn a_directive_attribute_survives_spaces_around_the_equals() {
        assert_eq!(directive_attr("taglib prefix = 's' uri='/x'", "prefix").as_deref(), Some("s"));
        assert_eq!(directive_attr("taglib prefix=\"s\"", "uri"), None);
    }

    #[test]
    fn a_scriptlet_is_one_row_and_not_its_java() {
        let root = model("<% for (int i = 0; i < 10; i++) { out.print(i); } %>", None);
        let scriptlet = find(&root, "scriptlet").expect("the scriptlet");
        assert_eq!(scriptlet.field.as_deref(), Some("Java"));
        assert!(scriptlet.children.is_empty(), "Java in a page is not modelled here");
    }
}
