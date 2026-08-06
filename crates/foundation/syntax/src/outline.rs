//! The tree as data — what a syntax-tree panel draws.
//!
//! ## Every node, or the panel lies
//!
//! It would be tempting to show only *named* nodes: the anonymous ones are the
//! commas and the keywords, and a tree without them reads more like an outline.
//! But the panel exists to answer "why did the parser read it that way", and the
//! answer is very often a comma that landed somewhere unexpected. So both are
//! reported and the caller decides what to draw — [`OutlineOptions::named_only`]
//! is there for the caller who wants the tidy version.
//!
//! ## Limits are part of the contract
//!
//! A 40 000-line install script has a few hundred thousand nodes, and a panel
//! that tries to hand all of them to a frontend at once is a frozen window. The
//! outline therefore takes a **budget** and says whether it hit it, rather than
//! silently returning a partial tree that looks complete —
//! [`SyntaxTree::truncated`] is the difference between "this file has no more
//! structure" and "I stopped looking".

use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Node, Parser};

use crate::error::SyntaxError;
use crate::range::ByteRange;

/// How much of the tree to walk, and how much of the source to carry back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineOptions {
    /// Deepest level to descend to. The root is 0. `None` is "all of it".
    pub max_depth: Option<usize>,
    /// Stop after this many nodes. `None` is "all of them" and is a promise the
    /// caller is making about the file, not one this crate can keep for them.
    pub max_nodes: Option<usize>,
    /// Skip anonymous nodes — the punctuation and the keywords.
    pub named_only: bool,
    /// Carry each node's own text, truncated to this many **characters**. `None`
    /// carries none, which is what a caller that already has the source wants:
    /// it can slice the range itself and the payload stays small.
    pub text_preview: Option<usize>,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            // A limit rather than none: the default has to be safe for the file
            // somebody actually opens, not for the file in the test.
            max_nodes: Some(20_000),
            named_only: false,
            text_preview: Some(60),
        }
    }
}

/// A node whose **text is itself source**, to be parsed and spliced in.
///
/// Some grammars hand back an island as a single token: PostgreSQL's `$$ … $$`
/// routine body is one string as far as the SQL grammar is concerned, and a JSP's
/// scriptlet is one blob to an HTML one. A tree that stops there is not wrong,
/// but it is useless exactly where the interesting code is — an update script's
/// real work happens inside that body.
///
/// So the caller declares the islands. `kind` and `parents` say which nodes are
/// islands (the parents matter: `$$ … $$` in a `WHERE` clause is an ordinary
/// string literal, and re-parsing it would invent structure nobody wrote), and
/// `inner` says which part of the node's text is the source — the delimiters are
/// not.
pub struct Injection {
    /// Tree-sitter kind of the node that holds the island.
    pub kind: String,
    /// Parent kinds that make it one. Empty means "anywhere", which is almost
    /// never what a caller wants.
    pub parents: Vec<String>,
    /// The island inside the node's own text, as a range into it. `None` when this
    /// particular node is not one after all.
    ///
    /// A plain function pointer rather than a closure: the delimiters differ per
    /// language (`$$`, `$body$`, `<% %>`) and only the caller can strip them, but
    /// nothing here needs to capture state to do it.
    pub inner: fn(&str) -> Option<std::ops::Range<usize>>,
    /// The language to read the island with. Usually — but not always — the same
    /// one that produced the outer tree.
    pub language: Language,
}

impl std::fmt::Debug for Injection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Injection").field("kind", &self.kind).field("parents", &self.parents).finish()
    }
}

impl Injection {
    fn applies(&self, kind: &str, parent: Option<&str>) -> bool {
        if self.kind != kind {
            return false;
        }
        self.parents.is_empty() || parent.is_some_and(|p| self.parents.iter().any(|k| k == p))
    }
}

/// One node of the syntax tree.
///
/// Produced by [`outline`] from a parse — but also, deliberately, constructible by hand: a
/// product that derives a **semantic** model from its parse (declarations, symbols) can express
/// it in this same shape and get the panel that draws trees for free, rather than growing a
/// second one. Every field below is meaningful for such a tree; [`SyntaxNode::synthesized`]
/// exists only for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxNode {
    /// Tree-sitter's kind. For an anonymous node this **is** the literal text
    /// (`","`, `"INSERT"`), which is why the panel needs no separate label.
    pub kind: String,
    /// The field this node fills in its parent (`name`, `body`, `condition`), when
    /// the grammar gives it one. The single most useful column in the panel: it
    /// is the difference between "an identifier" and "the table being written to".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Named nodes are the grammar's own concepts; anonymous ones are its literal
    /// tokens.
    pub named: bool,
    /// This node is an error node, or a token the parser had to invent to recover.
    /// Drawn in the panel as what it is — the fastest way to see why a file reads
    /// oddly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub error: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
    pub range: ByteRange,
    /// One-based line of the node's start, for the panel's own label. Columns are
    /// deliberately absent: a click selects a **byte range**, and a column would
    /// be a second, weaker way of saying the same thing.
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SyntaxNode>,
    /// This node has children that were not walked — the depth or the node budget
    /// ran out here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub elided: bool,
    /// This node's children come from a **second parse** of its own text — see
    /// [`Injection`]. Worth showing: it is the difference between "the grammar
    /// reads this" and "we read this separately because the grammar would not".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub injected: bool,
    /// **Nothing in the file says this.** Never set by [`outline`] — a parse tree is
    /// by definition all source. It is for a derived tree, where some of the model
    /// is written by the language rather than by the author: a Java record's
    /// accessors, a Lombok getter.
    ///
    /// Such a node's `range` points at whatever *declares* it, so selecting it still
    /// goes somewhere true; the flag is what stops a panel from claiming those bytes
    /// are the member.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub synthesized: bool,
}

/// A whole file's tree, with what it cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxTree {
    pub root: SyntaxNode,
    /// Nodes actually reported.
    pub node_count: usize,
    /// The walk stopped early. The panel says so rather than implying the file
    /// simply ends here.
    pub truncated: bool,
    /// The parser recovered from at least one error somewhere in the file.
    pub has_errors: bool,
}

/// Parse `source` with `language` and describe the tree.
///
/// Never panics and never returns a partial tree pretending to be whole: a file
/// that will not parse still has a tree — Tree-sitter always produces one — and
/// its error nodes are exactly what somebody opening this panel wants to see.
pub fn outline(
    language: &Language,
    source: &str,
    options: &OutlineOptions,
) -> Result<SyntaxTree, SyntaxError> {
    outline_with(language, source, options, &[])
}

/// The same, descending into the islands the caller declares.
pub fn outline_with(
    language: &Language,
    source: &str,
    options: &OutlineOptions,
    injections: &[Injection],
) -> Result<SyntaxTree, SyntaxError> {
    let tree = parse(language, source)?;

    let mut walk = Walk { source, options, injections, count: 0, truncated: false };
    let root = walk.node(tree.root_node(), None, 0);
    Ok(SyntaxTree {
        has_errors: tree.root_node().has_error(),
        node_count: walk.count,
        truncated: walk.truncated,
        root,
    })
}

fn parse(language: &Language, source: &str) -> Result<tree_sitter::Tree, SyntaxError> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|e| SyntaxError::Language(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| SyntaxError::Language("the parser produced no tree".to_string()))
}

struct Walk<'a> {
    source: &'a str,
    options: &'a OutlineOptions,
    injections: &'a [Injection],
    count: usize,
    truncated: bool,
}

impl Walk<'_> {
    fn node(&mut self, node: Node<'_>, parent_kind: Option<&str>, depth: usize) -> SyntaxNode {
        self.count += 1;
        let range = ByteRange::new(node.start_byte(), node.end_byte());
        let mut out = SyntaxNode {
            kind: node.kind().to_string(),
            field: None,
            named: node.is_named(),
            error: node.is_error() || node.kind() == "ERROR",
            missing: node.is_missing(),
            range,
            line: node.start_position().row + 1,
            text: self.preview(range),
            children: Vec::new(),
            elided: false,
            injected: false,
            // A parse tree is all source, by definition — see the field's doc.
            synthesized: false,
        };

        let deep_enough = self.options.max_depth.is_some_and(|max| depth >= max);
        if deep_enough {
            out.elided = node.child_count() > 0;
            return out;
        }

        // An island first: a node that holds source is a leaf to the grammar, so
        // there is nothing below it to walk and everything below it to read.
        if let Some(island) = self.island(node, parent_kind, depth) {
            out.children = island;
            out.injected = true;
            return out;
        }

        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return out;
        }
        loop {
            let child = cursor.node();
            let wanted = !self.options.named_only || child.is_named();
            if wanted {
                if self.over_budget() {
                    out.elided = true;
                    self.truncated = true;
                    break;
                }
                let mut described = self.node(child, Some(node.kind()), depth + 1);
                described.field = cursor.field_name().map(str::to_string);
                out.children.push(described);
            } else if !self.options.named_only {
                // Unreachable, kept explicit: `wanted` is false only when
                // `named_only` is on.
            } else {
                // An anonymous node the caller asked not to see. Its *children*
                // are not skipped with it — an anonymous node has none in any
                // Tree-sitter grammar, so there is nothing to lose here.
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        out
    }

    fn over_budget(&self) -> bool {
        self.options.max_nodes.is_some_and(|max| self.count >= max)
    }

    /// Parse this node's own text, when the caller declared it an island, and
    /// return the sub-tree's children with every offset moved into the outer
    /// source.
    ///
    /// The sub-root itself is dropped: it is the grammar's file wrapper
    /// (`source_file`, `program`) and would appear in the panel as a node the
    /// author never wrote.
    fn island(
        &mut self,
        node: Node<'_>,
        parent_kind: Option<&str>,
        depth: usize,
    ) -> Option<Vec<SyntaxNode>> {
        let injection =
            self.injections.iter().find(|i| i.applies(node.kind(), parent_kind))?;
        let text = self.source.get(node.start_byte()..node.end_byte())?;
        let inner = (injection.inner)(text)?;
        let body = text.get(inner.clone())?;
        if body.trim().is_empty() {
            return None;
        }
        let tree = parse(&injection.language, body).ok()?;

        // Where the island sits in the outer world.
        let offset = node.start_byte() + inner.start;
        let line_offset = self.source.get(..offset)?.matches('\n').count();

        let mut inner_walk =
            Walk { source: body, options: self.options, injections: self.injections, count: 0, truncated: false };
        let root = inner_walk.node(tree.root_node(), None, depth + 1);
        self.count += inner_walk.count;
        self.truncated |= inner_walk.truncated;

        let mut children = root.children;
        for child in &mut children {
            shift(child, offset, line_offset);
        }
        Some(children)
    }

    fn preview(&self, range: ByteRange) -> Option<String> {
        let limit = self.options.text_preview?;
        let text = range.slice(self.source)?;
        // One line: a preview that wraps is a preview that ruins the row height
        // it sits in. The ellipsis is the honest signal that there is more.
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.chars().count() <= limit {
            return Some(flat);
        }
        Some(flat.chars().take(limit).collect::<String>() + "…")
    }
}

/// Move a sub-parse's node — and everything under it — into the outer source.
///
/// Both coordinates, and forgetting the second is the bug that makes a panel look
/// almost right: the ranges select correctly and every line number below the
/// island is wrong, because the sub-parse started counting again from one.
fn shift(node: &mut SyntaxNode, bytes: usize, lines: usize) {
    node.range.start += bytes;
    node.range.end += bytes;
    node.line += lines;
    for child in &mut node.children {
        shift(child, bytes, lines);
    }
}

/// The smallest node whose range contains `at` — what a click in the editor
/// should select in the panel.
///
/// Walks the real tree rather than the outline, so it is correct even where the
/// outline was truncated: "reveal this node" must not depend on whether the panel
/// happened to have walked that far.
pub fn node_path_at(
    language: &Language,
    source: &str,
    at: usize,
) -> Result<Vec<ByteRange>, SyntaxError> {
    node_path_at_with(language, source, at, &[])
}

/// The same, descending into islands.
///
/// It must do this as well as [`outline_with`], and for a sharper reason: the
/// panel's answer to "reveal what the cursor is in" comes from here, so without
/// it the caret inside a routine body reveals the body and stops — which reads as
/// the panel refusing to open rather than as a limit.
pub fn node_path_at_with(
    language: &Language,
    source: &str,
    at: usize,
    injections: &[Injection],
) -> Result<Vec<ByteRange>, SyntaxError> {
    let tree = parse(language, source)?;
    let mut path = vec![];
    descend(tree.root_node(), None, source, at, 0, injections, &mut path);
    Ok(path)
}

/// Walk down to `at`, recording every node on the way.
///
/// `offset` is where this (sub-)source sits in the outer one, so every range
/// pushed is in the coordinates the caller handed in — never the island's own.
fn descend(
    root: Node<'_>,
    parent_kind: Option<&str>,
    source: &str,
    at: usize,
    offset: usize,
    injections: &[Injection],
    path: &mut Vec<ByteRange>,
) {
    let mut node = root;
    let mut parent = parent_kind.map(str::to_string);
    loop {
        path.push(ByteRange::new(offset + node.start_byte(), offset + node.end_byte()));

        // An island: continue in its own text, with the offsets moved.
        if let Some(injection) =
            injections.iter().find(|i| i.applies(node.kind(), parent.as_deref()))
        {
            if let Some(inner) = source
                .get(node.start_byte()..node.end_byte())
                .and_then(|text| (injection.inner)(text).map(|r| (text, r)))
            {
                let (text, range) = inner;
                let island_offset = offset + node.start_byte() + range.start;
                if let Some(body) = text.get(range) {
                    if at >= island_offset && at < island_offset + body.len() {
                        if let Ok(tree) = parse(&injection.language, body) {
                            descend(
                                tree.root_node(),
                                None,
                                body,
                                at - island_offset,
                                island_offset,
                                injections,
                                path,
                            );
                        }
                    }
                }
            }
            return;
        }

        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return;
        }
        let mut descended = false;
        loop {
            let child = cursor.node();
            if child.start_byte() <= at && at < child.end_byte() {
                parent = Some(node.kind().to_string());
                node = child;
                descended = true;
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        if !descended {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn java() -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    const SOURCE: &str = "class Scheda { int codice = 7; }";

    #[test]
    fn the_tree_carries_fields_ranges_and_the_line() {
        let tree = outline(&java(), SOURCE, &OutlineOptions::default()).expect("outlines");
        assert_eq!(tree.root.kind, "program");
        assert!(!tree.has_errors);

        let class = &tree.root.children[0];
        assert_eq!(class.kind, "class_declaration");
        assert_eq!(class.range.slice(SOURCE), Some(SOURCE));
        assert_eq!(class.line, 1);

        // The field name is the column that makes the panel worth having: this
        // identifier is not just an identifier, it is the class's *name*.
        let name = class.children.iter().find(|c| c.field.as_deref() == Some("name"));
        assert_eq!(name.expect("a named field").text.as_deref(), Some("Scheda"));
    }

    #[test]
    fn anonymous_nodes_are_reported_unless_they_are_not_wanted() {
        let all = outline(&java(), SOURCE, &OutlineOptions::default()).expect("outlines");
        let class = &all.root.children[0];
        // `class` the keyword is a node of its own, and its kind IS its text.
        assert!(class.children.iter().any(|c| !c.named && c.kind == "class"));

        let named = OutlineOptions { named_only: true, ..OutlineOptions::default() };
        let tidy = outline(&java(), SOURCE, &named).expect("outlines");
        assert!(tidy.root.children[0].children.iter().all(|c| c.named));
    }

    #[test]
    fn a_depth_limit_says_it_elided_rather_than_pretending_the_node_is_a_leaf() {
        // The distinction the panel's expand arrow depends on. Without it a
        // truncated tree draws as a finished one.
        let shallow = OutlineOptions { max_depth: Some(1), ..OutlineOptions::default() };
        let tree = outline(&java(), SOURCE, &shallow).expect("outlines");
        let class = &tree.root.children[0];
        assert!(class.children.is_empty());
        assert!(class.elided, "it has children — they were just not walked");
    }

    #[test]
    fn a_node_budget_truncates_and_admits_it() {
        let tight = OutlineOptions { max_nodes: Some(4), ..OutlineOptions::default() };
        let tree = outline(&java(), SOURCE, &tight).expect("outlines");
        assert!(tree.truncated);
        assert!(tree.node_count <= 5, "{} nodes", tree.node_count);
    }

    #[test]
    fn a_file_that_will_not_parse_still_has_a_tree_with_its_errors_in_it() {
        // The case the panel is most useful for, so it must not be the case it
        // refuses. Tree-sitter always produces a tree; the errors are nodes.
        let broken = "class Scheda { int = ; }";
        let tree = outline(&java(), broken, &OutlineOptions::default()).expect("outlines anyway");
        assert!(tree.has_errors);
        fn any_error(node: &SyntaxNode) -> bool {
            node.error || node.missing || node.children.iter().any(any_error)
        }
        assert!(any_error(&tree.root));
    }

    #[test]
    fn the_path_to_an_offset_runs_root_to_leaf_and_narrows_all_the_way() {
        // What "select the node under the cursor" needs: every step contains the
        // next, and the last one is the smallest thing holding that byte.
        let at = SOURCE.find("codice").expect("present");
        let path = node_path_at(&java(), SOURCE, at).expect("walks");
        assert!(path.len() > 2);
        for pair in path.windows(2) {
            assert!(pair[0].contains(&pair[1]), "{:?} does not contain {:?}", pair[0], pair[1]);
        }
        assert_eq!(path.last().unwrap().slice(SOURCE), Some("codice"));
    }

    // ── Islands ───────────────────────────────────────────────────────────────

    /// The inside of a `"…"`, as a range into the token's own text.
    fn between_quotes(text: &str) -> Option<std::ops::Range<usize>> {
        (text.len() >= 2 && text.starts_with('"') && text.ends_with('"'))
            .then(|| 1..text.len() - 1)
    }

    fn quoted_java() -> Vec<Injection> {
        vec![Injection {
            kind: "string_literal".to_string(),
            parents: vec!["variable_declarator".to_string()],
            inner: between_quotes,
            language: java(),
        }]
    }

    fn find<'a>(node: &'a SyntaxNode, kind: &str) -> Option<&'a SyntaxNode> {
        if node.kind == kind {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, kind))
    }

    #[test]
    fn an_island_is_parsed_and_lands_in_the_outer_coordinates() {
        // The failure this exists to prevent, in the shape it took on SQL: a
        // grammar hands an island back as ONE token, so the tree stops exactly
        // where the interesting code is.
        let source = "class A {\n  String s = \"class B {}\";\n}";
        let tree = outline_with(&java(), source, &OutlineOptions::default(), &quoted_java())
            .expect("outlines");

        let literal = find(&tree.root, "string_literal").expect("the island");
        assert!(literal.injected, "its children come from a second parse");
        assert!(!literal.children.is_empty(), "the grammar alone gives it none");

        // Bytes: the range must slice the OUTER source to the island's own text.
        let inner = literal.children.first().expect("a child");
        assert_eq!(inner.range.slice(source), Some("class B {}"));

        // Lines: the sub-parse counts from one, and forgetting to shift is the bug
        // that makes a panel look almost right — the ranges select correctly and
        // every line below the island is wrong.
        assert_eq!(inner.line, 2);
    }

    #[test]
    fn a_node_of_the_right_kind_in_the_wrong_place_is_not_an_island() {
        // Load-bearing: the same token elsewhere is an ordinary string, and
        // re-parsing one would invent structure nobody wrote. A tree that says
        // something false is worse than one that stops.
        let source = "class A { void m() { f(\"class B {}\"); } }";
        let tree = outline_with(&java(), source, &OutlineOptions::default(), &quoted_java())
            .expect("outlines");
        let literal = find(&tree.root, "string_literal").expect("present");
        assert!(!literal.injected);
        // It keeps whatever the grammar itself puts inside a literal — what it must
        // NOT have is the structure a second parse would have invented.
        assert!(find(literal, "class_declaration").is_none(), "it was re-parsed as Java");
    }

    #[test]
    fn the_path_to_an_offset_descends_into_an_island() {
        // `reveal what the cursor is in` comes from here, so a path that stopped
        // at the island's edge would read as the panel refusing to open.
        let source = "class A {\n  String s = \"class Scheda {}\";\n}";
        let at = source.find("Scheda").expect("present");
        let path = node_path_at_with(&java(), source, at, &quoted_java()).expect("walks");

        for pair in path.windows(2) {
            assert!(pair[0].contains(&pair[1]), "{:?} does not contain {:?}", pair[0], pair[1]);
        }
        assert_eq!(path.last().and_then(|r| r.slice(source)), Some("Scheda"));
    }

    #[test]
    fn an_empty_island_is_left_alone() {
        let source = "class A {\n  String s = \"\";\n}";
        let tree = outline_with(&java(), source, &OutlineOptions::default(), &quoted_java())
            .expect("outlines");
        let literal = find(&tree.root, "string_literal").expect("present");
        assert!(!literal.injected, "there is nothing in it to parse");
    }

    #[test]
    fn the_preview_is_one_line_and_says_when_it_cut() {
        let wrapped = "class A {\n  int x = 1;\n}";
        let tree = outline(&java(), wrapped, &OutlineOptions { text_preview: Some(12), ..Default::default() })
            .expect("outlines");
        let text = tree.root.text.as_deref().expect("a preview");
        assert!(!text.contains('\n'));
        assert!(text.ends_with('…'), "{text}");
    }
}
