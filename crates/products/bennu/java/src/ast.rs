//! The **abstract** syntax tree: the parse read in Java's vocabulary, all the way down.
//!
//! ## What this is, and what it is not
//!
//! [`crate::symbols`] builds the **declaration model** — types, members, signatures — because
//! that is what an indexer needs, and it deliberately stops at the method's opening brace. This
//! goes the other way: it lowers the whole tree-sitter parse into an AST, bodies included,
//! statement by statement and expression by expression.
//!
//! It is **derived on demand and never stored**, which is what makes it safe to have beside the
//! declaration model rather than a second thing to keep in sync: it is computed from one parse of
//! one buffer, at the moment somebody asks to look at it, and thrown away after.
//!
//! ## What lowering actually removes, and adds
//!
//! A CST and an AST differ in four concrete ways, and all four are the point:
//!
//! | | CST (tree-sitter) | here |
//! |---|---|---|
//! | punctuation | `,` `(` `;` are nodes | gone |
//! | vocabulary | `method_invocation`, `local_variable_declaration` | `call`, `local variable` |
//! | wrappers | `expression_statement` wraps every call | unwrapped |
//! | roles | a child either has a grammar field or nothing | `condition`, `receiver`, `argument` |
//!
//! And one thing a parse tree cannot have at all: the **resolved type** of an expression, and
//! whether a bare name denotes a class or a value. Both come from the resolver, and both are
//! attached where they are known and simply absent where they are not — an AST that guessed
//! would be worse than one that admits it does not know.
//!
//! ## Nothing is dropped silently
//!
//! The lowering is a table over the grammar's node kinds, and a kind it has no entry for **keeps
//! its grammar name** and its children rather than disappearing. So the tree is never wrong, only
//! occasionally less pretty — which is the right failure for something that has to survive the
//! next Java version without being edited first.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::infer::{infer_node_type_cached, InferCache};
use crate::seam::TypeResolver;
use crate::symbols::{extract_symbols_from_root, FileSymbols, Span, TypeDecl};

/// One node of the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstNode {
    /// What it is, in Java's words — `class`, `method`, `if`, `call`, `local variable`.
    pub kind: String,
    /// The part it plays in its parent — `condition`, `then`, `receiver`, `argument`. The column
    /// that turns "an expression" into "the thing being tested".
    pub role: Option<String>,
    /// The name, the operator, the literal — whatever identifies this node among its siblings.
    pub label: Option<String>,
    /// `public static`, `private final` — for a declaration. Kept apart from the label rather
    /// than rendered into it, because on a list of members it is the column you scan.
    pub modifiers: Option<String>,
    /// The resolved static type, dotted, when the resolver could tell. `None` is *unknown*.
    pub type_name: Option<String>,
    /// This name resolves to a **class**, not to a value — the static-versus-instance distinction
    /// the shape alone cannot carry.
    pub names_a_type: bool,
    /// Nothing in the file says this: a record's accessor, its canonical constructor. Its span is
    /// the declaration that owes it.
    pub synthesized: bool,
    pub span: Span,
    pub children: Vec<AstNode>,
}

impl AstNode {
    fn new(kind: impl Into<String>, span: Span) -> Self {
        AstNode {
            kind: kind.into(),
            role: None,
            label: None,
            modifiers: None,
            type_name: None,
            names_a_type: false,
            synthesized: false,
            span,
            children: Vec::new(),
        }
    }

    fn labelled(mut self, label: Option<String>) -> Self {
        self.label = label;
        self
    }

    fn with_role(mut self, role: &str) -> Self {
        self.role = Some(role.to_string());
        self
    }
}

/// Lower `source` into an AST.
///
/// `resolver` is optional and only adds: without one the tree is complete and untyped, with one
/// every expression that can be typed carries its type and every bare name says whether it is a
/// class or a value. That is the difference between "what the parser built" and "what Bennu
/// understood", and it is why the panel showing this is worth having beside the parse.
pub fn lower(source: &str, resolver: Option<&dyn TypeResolver>) -> AstNode {
    let Some(tree) = crate::grammar::parse_java(source) else {
        return AstNode::new(
            "compilation unit",
            Span {
                start: 0,
                end: source.len(),
            },
        );
    };
    let root = tree.root_node();
    let symbols = extract_symbols_from_root(&root, source);

    let lowering = Lowering {
        source,
        symbols: &symbols,
        resolver,
        cache: InferCache::new(),
        root,
    };
    lowering.file(root)
}

struct Lowering<'a> {
    source: &'a str,
    symbols: &'a FileSymbols,
    resolver: Option<&'a dyn TypeResolver>,
    cache: InferCache,
    root: Node<'a>,
}

impl<'a> Lowering<'a> {
    fn file(&self, root: Node<'a>) -> AstNode {
        let mut out = AstNode::new("compilation unit", span(root));
        out.label = self.symbols.package.clone();
        out.children = self
            .named_children(root)
            .into_iter()
            .filter_map(|c| self.node(c, None))
            .collect();
        out
    }

    /// Lower one node, or `None` when it carries nothing (a modifiers list, a comment).
    fn node(&self, node: Node<'a>, role: Option<&str>) -> Option<AstNode> {
        let kind = node.kind();

        // Wrappers that exist for the grammar and mean nothing to a reader. Unwrapping them is
        // half of what makes this an AST rather than a tidier parse tree.
        if matches!(kind, "expression_statement" | "parenthesized_expression") {
            let inner = self.named_children(node).into_iter().next()?;
            return self.node(inner, role);
        }
        // Consumed by whoever asked for them, or of no interest at all.
        if matches!(
            kind,
            "modifiers" | "comment" | "line_comment" | "block_comment"
        ) {
            return None;
        }

        // Two families, and the split is load-bearing: the first build their own children
        // (roles, parameters, declarators), the second are shells whose children are simply
        // everything named inside them. Filling the first family's children generically would
        // duplicate them — a `call` would get its receiver twice, once as a role and once as a
        // raw child.
        let mut out = match self.composed(node, kind) {
            Some(built) => built,
            None => {
                let mut shell = self.shell(node, kind);
                shell.children = self.plain_children(node);
                shell
            }
        };

        out.role = role.map(str::to_string);
        self.annotate(node, &mut out);
        Some(out)
    }

    /// The kinds that assemble their own children.
    fn composed(&self, node: Node<'a>, kind: &str) -> Option<AstNode> {
        Some(match kind {
            "import_declaration" => self.import(node),
            "class_declaration" => self.type_decl(node, "class"),
            "interface_declaration" => self.type_decl(node, "interface"),
            "enum_declaration" => self.type_decl(node, "enum"),
            "record_declaration" => self.type_decl(node, "record"),
            "annotation_type_declaration" => self.type_decl(node, "annotation"),
            "method_declaration" => self.callable(node, "method"),
            "constructor_declaration" | "compact_constructor_declaration" => {
                self.callable(node, "constructor")
            }
            "annotation_type_element_declaration" => self.callable(node, "element"),
            "field_declaration" | "constant_declaration" => self.variables(node, "field"),
            "local_variable_declaration" => self.variables(node, "local variable"),
            "formal_parameter"
            | "spread_parameter"
            | "receiver_parameter"
            | "catch_formal_parameter" => self.parameter(node),
            "method_invocation" => self.call(node),

            "if_statement" => self.roles(
                node,
                "if",
                &[
                    ("condition", "condition"),
                    ("consequence", "then"),
                    ("alternative", "else"),
                ],
            ),
            "while_statement" => self.roles(
                node,
                "while",
                &[("condition", "condition"), ("body", "body")],
            ),
            "do_statement" => {
                self.roles(node, "do", &[("body", "body"), ("condition", "condition")])
            }
            "for_statement" => self.roles(
                node,
                "for",
                &[
                    ("init", "init"),
                    ("condition", "condition"),
                    ("update", "update"),
                    ("body", "body"),
                ],
            ),
            "enhanced_for_statement" => self.for_each(node),
            "switch_expression" => {
                self.roles(node, "switch", &[("condition", "on"), ("body", "body")])
            }

            "field_access" => self
                .roles(node, "field access", &[("object", "receiver")])
                .labelled(self.field_text(node, "field")),
            "array_access" => self.roles(node, "index", &[("array", "array"), ("index", "index")]),
            "assignment_expression" => self
                .roles(node, "assign", &[("left", "target"), ("right", "value")])
                .labelled(self.operator(node)),
            "binary_expression" => self
                .roles(node, "binary", &[("left", "left"), ("right", "right")])
                .labelled(self.operator(node)),
            "ternary_expression" => self.roles(
                node,
                "ternary",
                &[
                    ("condition", "condition"),
                    ("consequence", "then"),
                    ("alternative", "else"),
                ],
            ),
            "cast_expression" => self
                .roles(node, "cast", &[("value", "value")])
                .labelled(self.field_text(node, "type")),
            "instanceof_expression" => self
                .roles(node, "instanceof", &[("left", "value")])
                .labelled(self.field_text(node, "right")),
            "lambda_expression" => self.roles(
                node,
                "lambda",
                &[("parameters", "parameters"), ("body", "body")],
            ),

            _ => return None,
        })
    }

    /// The kinds whose children are just what is inside them, in order.
    fn shell(&self, node: Node<'a>, kind: &str) -> AstNode {
        let text = || self.text(node);
        match kind {
            "package_declaration" => {
                AstNode::new("package", span(node)).labelled(self.symbols.package.clone())
            }
            "enum_constant" => {
                AstNode::new("constant", span(node)).labelled(self.field_text(node, "name"))
            }
            "static_initializer" => AstNode::new("static initializer", span(node)),
            "annotation" | "marker_annotation" => {
                AstNode::new("annotation use", span(node)).labelled(self.field_text(node, "name"))
            }

            "block" => AstNode::new("block", span(node)),
            "return_statement" => AstNode::new("return", span(node)),
            "throw_statement" => AstNode::new("throw", span(node)),
            "yield_statement" => AstNode::new("yield", span(node)),
            "break_statement" => {
                AstNode::new("break", span(node)).labelled(self.first_identifier(node))
            }
            "continue_statement" => {
                AstNode::new("continue", span(node)).labelled(self.first_identifier(node))
            }
            "try_statement" | "try_with_resources_statement" => AstNode::new("try", span(node)),
            "catch_clause" => AstNode::new("catch", span(node)),
            "finally_clause" => AstNode::new("finally", span(node)),
            "synchronized_statement" => AstNode::new("synchronized", span(node)),
            "labeled_statement" => {
                AstNode::new("label", span(node)).labelled(self.first_identifier(node))
            }
            "assert_statement" => AstNode::new("assert", span(node)),
            "switch_block_statement_group" | "switch_rule" => AstNode::new("case", span(node)),

            "object_creation_expression" => {
                AstNode::new("new", span(node)).labelled(self.field_text(node, "type"))
            }
            "array_creation_expression" => AstNode::new("new array", span(node)),
            "unary_expression" | "update_expression" => {
                AstNode::new("unary", span(node)).labelled(self.operator(node))
            }
            "method_reference" => {
                AstNode::new("method reference", span(node)).labelled(Some(text()))
            }
            "identifier" | "type_identifier" | "scoped_identifier" => {
                AstNode::new("name", span(node)).labelled(Some(text()))
            }
            "this" => AstNode::new("this", span(node)),
            "super" => AstNode::new("super", span(node)),
            "string_literal" | "text_block" => {
                AstNode::new("string", span(node)).labelled(Some(text()))
            }
            "character_literal" => AstNode::new("char", span(node)).labelled(Some(text())),
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal"
            | "decimal_floating_point_literal"
            | "hex_floating_point_literal" => {
                AstNode::new("number", span(node)).labelled(Some(text()))
            }
            "true" | "false" => AstNode::new("boolean", span(node)).labelled(Some(text())),
            "null_literal" => AstNode::new("null", span(node)),

            // ── the honest fallback ─────────────────────────────────────────────
            // A kind with no entry keeps its grammar name (spaced out, so it reads as prose
            // rather than as an internal symbol) and its children. Never wrong, only occasionally
            // less pretty — which is the failure a table over someone else's grammar should have.
            other => AstNode::new(other.replace('_', " "), span(node)),
        }
    }

    /// Children with no role of their own — everything named, lowered in order.
    fn plain_children(&self, node: Node<'a>) -> Vec<AstNode> {
        self.named_children(node)
            .into_iter()
            .filter_map(|c| self.node(c, None))
            .collect()
    }

    /// Lower the children named by `fields` under the roles given, then everything else in order.
    ///
    /// The half of "abstract" that *adds* rather than removes: a reader looking at an `if` wants
    /// to know which child is the condition, and the grammar's field names are the only place
    /// that is written down.
    fn roles(&self, node: Node<'a>, kind: &str, fields: &[(&str, &str)]) -> AstNode {
        let mut out = AstNode::new(kind, span(node));
        let mut claimed: HashSet<usize> = HashSet::new();
        for (field, role) in fields {
            if let Some(child) = node.child_by_field_name(field) {
                claimed.insert(child.id());
                if let Some(lowered) = self.node(child, Some(role)) {
                    out.children.push(lowered);
                }
            }
        }
        for child in self.named_children(node) {
            if claimed.contains(&child.id()) {
                continue;
            }
            if let Some(lowered) = self.node(child, None) {
                out.children.push(lowered);
            }
        }
        out
    }

    fn import(&self, node: Node<'a>) -> AstNode {
        let text = self.text(node);
        let mut out = AstNode::new("import", span(node));
        out.label = Some(
            text.trim_start_matches("import")
                .trim()
                .trim_start_matches("static")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string(),
        );
        if text.contains("static") {
            out.role = Some("static".to_string());
        }
        // Its path is one token to a reader; the grammar's `scoped_identifier` spine is not.
        out.children = Vec::new();
        out
    }

    /// A type declaration: its own name (fully qualified, which only the declaration model knows),
    /// The names of a `record`'s components, or empty for any other kind of type.
    fn record_component_names(&self, node: Node<'a>) -> Vec<String> {
        if node.kind() != "record_declaration" {
            return Vec::new();
        }
        let Some(params) = node.child_by_field_name("parameters") else {
            return Vec::new();
        };
        let mut cursor = params.walk();
        let names = params
            .named_children(&mut cursor)
            .filter_map(|p| p.child_by_field_name("name"))
            .map(|n| self.text(n))
            .collect();
        names
    }

    /// its supertypes as rows, then its members — plus the members the *language* writes.
    fn type_decl(&self, node: Node<'a>, kind: &str) -> AstNode {
        let mut out = AstNode::new(kind, span(node));
        let declared = self.type_named_at(span(node));
        out.label = declared
            .map(|d| d.fqn.clone())
            .or_else(|| self.field_text(node, "name"));
        out.modifiers = self.modifiers(node);

        for (field, role) in [
            ("type_parameters", "type parameters"),
            ("superclass", "extends"),
            ("interfaces", "implements"),
            ("body", "body"),
        ] {
            if let Some(child) = node.child_by_field_name(field) {
                if field == "body" {
                    // A body is not a row of its own — its members are the type's children.
                    out.children.extend(self.plain_children(child));
                } else if let Some(lowered) = self.node(child, Some(role)) {
                    out.children.push(lowered);
                }
            }
        }
        // In front of what they decorate, which is where they are written.
        let annotations: Vec<AstNode> = self
            .named_children(node)
            .into_iter()
            .filter(|c| matches!(c.kind(), "annotation" | "marker_annotation"))
            .filter_map(|c| self.node(c, Some("annotation")))
            .collect();
        out.children.splice(0..0, annotations);

        // The members nobody wrote. They are genuinely part of what Bennu understands — a record
        // `Point` really does have `x()` — so leaving them out would make the tree disagree with
        // completion. Marked, and pointed at the declaration that owes them.
        //
        // "Synthesized" is not the same as "has no span", even though a missing span implies it. A
        // record's accessor and backing field are synthesized AND carry the span of the component
        // in the header — because that is where a rename must edit and where go-to must land. This
        // panel wants the first fact; rename and navigation want the second. Asking the record for
        // its component names keeps the two apart without a flag on every symbol in the product.
        let components = self.record_component_names(node);
        let is_synthesized = |name: &str, span: &Option<Span>| {
            span.is_none() || components.iter().any(|c| c == name)
        };
        if let Some(declared) = declared {
            for method in declared
                .methods
                .iter()
                .filter(|m| is_synthesized(&m.name, &m.span))
            {
                let mut row = AstNode::new("method", span(node));
                row.label = Some(format!("{}({})", method.name, method.params.len()));
                row.synthesized = true;
                out.children.push(row);
            }
            for field in declared
                .fields
                .iter()
                .filter(|f| is_synthesized(&f.name, &f.span))
            {
                let mut row = AstNode::new("field", span(node));
                row.label = Some(format!("{}: {}", field.name, field.type_text));
                row.synthesized = true;
                out.children.push(row);
            }
        }
        out
    }

    /// The [`TypeDecl`] the declaration model built for the declaration at `span`.
    ///
    /// Matched by span, which cannot drift: both come from the same parse of the same buffer.
    fn type_named_at(&self, span: Span) -> Option<&'a TypeDecl> {
        self.symbols.types.iter().find(|t| t.span == Some(span))
    }

    fn callable(&self, node: Node<'a>, kind: &str) -> AstNode {
        let mut out = AstNode::new(kind, span(node));
        out.label = self.field_text(node, "name");
        out.modifiers = self.modifiers(node);
        for child in self.named_children(node) {
            if matches!(child.kind(), "annotation" | "marker_annotation") {
                if let Some(lowered) = self.node(child, Some("annotation")) {
                    out.children.push(lowered);
                }
            }
        }
        // The signature as **rows**, not as a string: the return type, each parameter with its
        // own name and type, what it throws. A one-line summary reads well and cannot be
        // clicked, filtered or navigated to.
        if let Some(returns) = node.child_by_field_name("type") {
            out.children.push(
                AstNode::new("type", span(returns))
                    .labelled(Some(self.text(returns)))
                    .with_role("returns"),
            );
        }
        if let Some(params) = node.child_by_field_name("parameters") {
            for param in self.named_children(params) {
                if let Some(lowered) = self.node(param, Some("parameter")) {
                    out.children.push(lowered);
                }
            }
        }
        for child in self.named_children(node) {
            if child.kind() == "throws" {
                for thrown in self.named_children(child) {
                    out.children.push(
                        AstNode::new("type", span(thrown))
                            .labelled(Some(self.text(thrown)))
                            .with_role("throws"),
                    );
                }
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            if let Some(lowered) = self.node(body, Some("body")) {
                out.children.push(lowered);
            }
        }
        out
    }

    fn parameter(&self, node: Node<'a>) -> AstNode {
        let name = self.field_text(node, "name").unwrap_or_default();
        let ty = self.field_text(node, "type").unwrap_or_default();
        AstNode::new("parameter", span(node)).labelled(Some(format!("{name}: {ty}")))
    }

    /// `int a = 1, b = 2;` is two declarations wearing one type — so it is two rows.
    fn variables(&self, node: Node<'a>, kind: &str) -> AstNode {
        let ty = self.field_text(node, "type").unwrap_or_default();
        // The modifiers are on the declaration, the names are on the declarators — so each row
        // has to be handed what it does not carry itself.
        let modifiers = self.modifiers(node);
        let declarators: Vec<Node> = self
            .named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "variable_declarator")
            .collect();

        // The common case is one, and wrapping it in a group row would add a level to every
        // field in the project to serve the rare case.
        if declarators.len() == 1 {
            return self.declarator(declarators[0], kind, &ty, modifiers);
        }
        let mut out = AstNode::new(format!("{kind}s"), span(node)).labelled(Some(ty.clone()));
        out.modifiers = modifiers.clone();
        out.children = declarators
            .iter()
            .map(|d| self.declarator(*d, kind, &ty, modifiers.clone()))
            .collect();
        out
    }

    fn declarator(
        &self,
        node: Node<'a>,
        kind: &str,
        ty: &str,
        modifiers: Option<String>,
    ) -> AstNode {
        let name = self.field_text(node, "name").unwrap_or_default();
        let mut out = AstNode::new(kind, span(node)).labelled(Some(format!("{name}: {ty}")));
        out.modifiers = modifiers;
        if let Some(value) = node.child_by_field_name("value") {
            if let Some(lowered) = self.node(value, Some("value")) {
                out.children.push(lowered);
            }
        }
        out
    }

    fn call(&self, node: Node<'a>) -> AstNode {
        let mut out = AstNode::new("call", span(node));
        out.label = self.field_text(node, "name");
        if let Some(object) = node.child_by_field_name("object") {
            if let Some(lowered) = self.node(object, Some("receiver")) {
                out.children.push(lowered);
            }
        }
        if let Some(args) = node.child_by_field_name("arguments") {
            // `argument_list` is a grammar node, not a concept — its children are the arguments.
            for arg in self.named_children(args) {
                if let Some(lowered) = self.node(arg, Some("argument")) {
                    out.children.push(lowered);
                }
            }
        }
        out
    }

    /// The two facts a parse tree cannot hold.
    ///
    /// Attached where the resolver can say and simply absent where it cannot — an AST that
    /// guessed a type would be worse than one that admits it does not know, because everything
    /// downstream would then be reasoning from an invented fact.
    fn annotate(&self, node: Node<'a>, out: &mut AstNode) {
        let Some(resolver) = self.resolver else {
            return;
        };
        if !is_expression(node.kind()) {
            return;
        }
        if let Some(ty) = infer_node_type_cached(
            &self.root,
            self.source,
            self.symbols,
            &node,
            resolver,
            &self.cache,
        ) {
            out.type_name = Some(ty.binary_name.replace('/', "."));
            return;
        }
        // No type, but it might be a **class name** — `Files.copy(a, b)`. That is the whole
        // static-versus-instance distinction, and it is invisible in the shape.
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "scoped_identifier"
        ) {
            let text = self.text(node);
            if let Some(binary) = resolver.resolve_simple_name(&text, &self.symbols.imports) {
                out.type_name = Some(binary.replace('/', "."));
                out.names_a_type = true;
            }
        }
    }

    // ── reading the tree ────────────────────────────────────────────────────────

    /// Every named child, materialised.
    ///
    /// Bound to a local before returning, never returned as the tail expression: a tree-sitter
    /// iterator borrows the cursor, and a temporary in tail position is dropped *after* the
    /// block's locals — so `node.named_children(&mut cursor).collect()` outlives its own cursor
    /// and will not compile.
    fn named_children(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
        children
    }

    fn text(&self, node: Node<'a>) -> String {
        self.source
            .get(node.start_byte()..node.end_byte())
            .unwrap_or_default()
            .to_string()
    }

    fn field_text(&self, node: Node<'a>, field: &str) -> Option<String> {
        node.child_by_field_name(field).map(|n| self.text(n))
    }

    /// `public static` — the keyword tokens of a declaration's `modifiers` node.
    ///
    /// They are **anonymous** nodes, which is why the parse tree buries them: to tree-sitter
    /// `static` is punctuation. To a reader scanning a class's members it is the first thing on
    /// the line.
    fn modifiers(&self, node: Node<'a>) -> Option<String> {
        let mut cursor = node.walk();
        let mods = node
            .children(&mut cursor)
            .find(|c| c.kind() == "modifiers")?;
        let mut inner = mods.walk();
        let words: Vec<String> = mods
            .children(&mut inner)
            .filter(|c| !c.is_named())
            .map(|c| self.text(c))
            .collect();
        let joined = (!words.is_empty()).then(|| words.join(" "));
        joined
    }

    /// The operator token of a binary / unary / assignment — an anonymous node, and the one
    /// anonymous node worth keeping, because it *is* what the expression does.
    fn operator(&self, node: Node<'a>) -> Option<String> {
        if let Some(operator) = node.child_by_field_name("operator") {
            return Some(self.text(operator));
        }
        // The grammar does not always give it a field — `++i` has none. The first anonymous
        // child is it. Bound to a local for the reason spelled out on `named_children`.
        let mut cursor = node.walk();
        let found = node.children(&mut cursor).find(|c| !c.is_named());
        found.map(|c| self.text(c))
    }

    fn first_identifier(&self, node: Node<'a>) -> Option<String> {
        self.named_children(node)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| self.text(c))
    }

    fn for_each(&self, node: Node<'a>) -> AstNode {
        let name = self.field_text(node, "name").unwrap_or_default();
        let ty = self.field_text(node, "type").unwrap_or_default();
        let mut out = self.roles(node, "for each", &[("value", "in"), ("body", "body")]);
        out.label = Some(format!("{name}: {ty}"));
        out
    }
}

/// Whether a grammar kind is an expression worth asking the resolver about. Over-inclusion costs
/// a cache lookup that answers `None`; under-inclusion silently drops a type somebody wanted.
fn is_expression(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "scoped_identifier"
            | "field_access"
            | "method_invocation"
            | "object_creation_expression"
            | "array_access"
            | "cast_expression"
            | "binary_expression"
            | "ternary_expression"
            | "assignment_expression"
            | "string_literal"
            | "text_block"
            | "character_literal"
            | "decimal_integer_literal"
            | "decimal_floating_point_literal"
            | "true"
            | "false"
            | "this"
    )
}

fn span(node: Node<'_>) -> Span {
    Span {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ast(source: &str) -> AstNode {
        lower(source, None)
    }

    fn find<'t>(node: &'t AstNode, kind: &str) -> Option<&'t AstNode> {
        if node.kind == kind {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, kind))
    }

    /// **The whole reason this module exists.** The declaration model stops at the opening brace;
    /// this does not.
    #[test]
    fn a_method_body_is_in_the_tree() {
        let root = ast("class A { void m() { if (x) { log.debug(\"hi\"); } } }");
        let branch = find(&root, "if").expect("the if");
        let call = find(branch, "call").expect("the call inside it");
        assert_eq!(call.label.as_deref(), Some("debug"));
    }

    /// Roles are the half of "abstract" that adds: which child is the condition is written down
    /// in the grammar's fields and nowhere else.
    #[test]
    fn the_parts_of_a_statement_are_named() {
        let root = ast("class A { void m() { if (a) b(); else c(); } }");
        let branch = find(&root, "if").expect("the if");
        let roles: Vec<&str> = branch
            .children
            .iter()
            .filter_map(|c| c.role.as_deref())
            .collect();
        assert_eq!(roles, ["condition", "then", "else"]);
    }

    /// A wrapper that exists for the grammar and means nothing to a reader is unwrapped — a call
    /// statement is a call, not an `expression_statement` containing one.
    #[test]
    fn grammar_wrappers_are_gone() {
        let root = ast("class A { void m() { f((1 + 2)); } }");
        assert!(find(&root, "expression statement").is_none());
        assert!(find(&root, "parenthesized expression").is_none());
        let call = find(&root, "call").expect("the call");
        assert_eq!(call.children[0].role.as_deref(), Some("argument"));
        assert_eq!(call.children[0].kind, "binary");
    }

    /// A signature is rows, not a string: each parameter is its own node, with its own span, so
    /// it can be clicked, filtered and revealed like anything else.
    #[test]
    fn a_signature_is_structured_rather_than_rendered() {
        let root = ast("class A { List<Order> findAll(int page, String q) throws SQLException { return null; } }");
        let method = find(&root, "method").expect("the method");
        let by_role = |role: &str| -> Vec<&str> {
            method
                .children
                .iter()
                .filter(|c| c.role.as_deref() == Some(role))
                .filter_map(|c| c.label.as_deref())
                .collect()
        };
        assert_eq!(by_role("returns"), ["List<Order>"]);
        assert_eq!(by_role("parameter"), ["page: int", "q: String"]);
        assert_eq!(by_role("throws"), ["SQLException"]);
        assert!(method
            .children
            .iter()
            .any(|c| c.role.as_deref() == Some("body")));
    }

    /// The receiver is a child with a role, not part of the call's name — which is what lets the
    /// resolver's answer hang off it.
    #[test]
    fn a_call_separates_its_receiver_from_its_arguments() {
        let root = ast("class A { void m() { svc.place(o, 1); } }");
        let call = find(&root, "call").expect("the call");
        assert_eq!(call.label.as_deref(), Some("place"));
        assert_eq!(call.children[0].role.as_deref(), Some("receiver"));
        assert_eq!(call.children[0].label.as_deref(), Some("svc"));
        assert_eq!(
            call.children
                .iter()
                .filter(|c| c.role.as_deref() == Some("argument"))
                .count(),
            2
        );
    }

    #[test]
    fn one_declaration_per_declarator() {
        let root = ast("class A { void m() { int a = 1, b = 2; } }");
        let labels: Vec<&str> = collect(&root, "local variable")
            .iter()
            .filter_map(|n| n.label.as_deref())
            .collect();
        assert_eq!(labels, ["a: int", "b: int"]);
    }

    fn collect<'t>(node: &'t AstNode, kind: &str) -> Vec<&'t AstNode> {
        let mut out = Vec::new();
        if node.kind == kind {
            out.push(node);
        }
        for child in &node.children {
            out.extend(collect(child, kind));
        }
        out
    }

    /// To tree-sitter `static` is punctuation; to a reader scanning a class's members it is the
    /// first thing on the line — so it gets its own column rather than being buried.
    #[test]
    fn a_declaration_carries_its_modifiers_apart_from_its_name() {
        let root =
            ast("public abstract class A { private final int x = 1; public static void m() {} }");
        assert_eq!(
            find(&root, "class").and_then(|c| c.modifiers.as_deref()),
            Some("public abstract")
        );
        assert_eq!(
            find(&root, "field").and_then(|c| c.modifiers.as_deref()),
            Some("private final")
        );
        assert_eq!(
            find(&root, "method").and_then(|c| c.modifiers.as_deref()),
            Some("public static")
        );
    }

    #[test]
    fn a_type_carries_its_fully_qualified_name() {
        let root = ast("package com.acme;\nclass OrderDao {}");
        assert_eq!(
            find(&root, "class").and_then(|c| c.label.as_deref()),
            Some("com.acme.OrderDao")
        );
        assert_eq!(root.label.as_deref(), Some("com.acme"));
    }

    /// A record's accessors are part of what Bennu understands even though nobody wrote them, so
    /// leaving them out would make the tree disagree with completion.
    #[test]
    fn language_written_members_are_present_and_marked() {
        let root = ast("record Point(int x, int y) {}");
        let generated: Vec<&str> = collect(&root, "method")
            .into_iter()
            .filter(|m| m.synthesized)
            .filter_map(|m| m.label.as_deref())
            .collect();
        assert!(
            generated.iter().any(|l| l.starts_with("x(")),
            "{generated:?}"
        );
        assert!(
            generated.iter().any(|l| l.starts_with("toString(")),
            "{generated:?}"
        );
    }

    /// The failure mode a table over someone else's grammar must have: a construct it has never
    /// heard of keeps its children rather than swallowing them, and reads as prose rather than as
    /// an internal symbol.
    #[test]
    fn an_unmapped_construct_keeps_its_children_and_reads_as_words() {
        let root = ast("class A { void m() { synchronized (lock) { f(); } } }");
        assert!(
            find(&root, "call").is_some(),
            "nothing is swallowed on the way down"
        );
        let mut every = Vec::new();
        walk(&root, &mut every);
        assert!(
            every.iter().all(|n| !n.kind.contains('_')),
            "{:?}",
            every.iter().map(|n| &n.kind).collect::<Vec<_>>(),
        );
    }

    fn walk<'t>(node: &'t AstNode, out: &mut Vec<&'t AstNode>) {
        out.push(node);
        for child in &node.children {
            walk(child, out);
        }
    }

    #[test]
    fn every_node_spans_its_own_text() {
        let source = "class A { void m() { log.debug(\"hi\"); } }";
        let root = ast(source);
        let call = find(&root, "call").expect("the call");
        assert_eq!(&source[call.span.start..call.span.end], "log.debug(\"hi\")");
        let literal = find(call, "string").expect("the literal");
        assert_eq!(&source[literal.span.start..literal.span.end], "\"hi\"");
    }

    /// Untyped is a complete tree, not a broken one: without a resolver every node is there and
    /// none of them claims a type.
    #[test]
    fn without_a_resolver_the_tree_is_complete_and_untyped() {
        let root = ast("class A { void m() { svc.place(o); } }");
        assert!(find(&root, "call").is_some());
        assert!(collect(&root, "name").iter().all(|n| n.type_name.is_none()));
    }
}
