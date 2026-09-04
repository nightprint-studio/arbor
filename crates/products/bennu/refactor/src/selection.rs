//! Finding the thing a refactoring is about: the expression under the caret, the statements a
//! selection covers, the method around them.
//!
//! ## Why this is not a byte scanner
//!
//! The intentions next door ([`bennu_intentions`]) are string-aware scans, and for what they do —
//! flip an `equals`, parameterise a log call — that is the right size of tool. These are not: what
//! *is* a statement, where a method's body starts, whether a `return` escapes the selection, and
//! which locals a block reads are all structural questions, and a scanner that answers them by
//! counting braces answers them wrongly on the first `String s = "}";` it meets.
//!
//! So everything here is read off the tree-sitter parse the rest of the editor already uses.
//!
//! ## Snapping, and why a refactoring refuses instead of guessing
//!
//! A user's selection almost never lands on a syntax boundary — it starts mid-indentation and ends
//! after a newline. Snapping it **outward to whole statements** is the one accommodation made here,
//! because it is unambiguous: every statement the selection touches, in full. What is not
//! accommodated is a selection covering *half* an expression, or statements from two different
//! blocks. Those have no honest interpretation, and picking one produces code that compiles and
//! means something else.
//!
//! [`bennu_intentions`]: https://docs.rs/bennu-intentions

use tree_sitter::Node;

/// Node kinds that are a statement in a block.
const STATEMENTS: &[&str] = &[
    "expression_statement",
    "local_variable_declaration",
    "if_statement",
    "while_statement",
    "for_statement",
    "enhanced_for_statement",
    "do_statement",
    "switch_expression",
    "try_statement",
    "try_with_resources_statement",
    "synchronized_statement",
    "return_statement",
    "throw_statement",
    "break_statement",
    "continue_statement",
    "yield_statement",
    "labeled_statement",
    "block",
    "local_variable_declaration_statement",
    "assert_statement",
    "explicit_constructor_invocation",
    ";",
];

/// Node kinds that carry a value — what "extract this" can be pointed at.
const EXPRESSIONS: &[&str] = &[
    "binary_expression",
    "unary_expression",
    "instanceof_expression",
    "ternary_expression",
    "method_invocation",
    "object_creation_expression",
    "array_creation_expression",
    "array_access",
    "field_access",
    "cast_expression",
    "parenthesized_expression",
    "lambda_expression",
    "switch_expression",
    "identifier",
    "scoped_identifier",
    "this",
    "class_literal",
    "string_literal",
    "character_literal",
    "decimal_integer_literal",
    "hex_integer_literal",
    "octal_integer_literal",
    "binary_integer_literal",
    "decimal_floating_point_literal",
    "hex_floating_point_literal",
    "true",
    "false",
    "null_literal",
    "template_expression",
];

/// The bodies a refactoring can work inside.
pub const CALLABLES: &[&str] =
    &["method_declaration", "constructor_declaration", "compact_constructor_declaration"];

/// The declarations a member can be added to.
pub const TYPE_DECLS: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Whether this node is a body statements can be lifted out of or inserted into.
///
/// A constructor's body is a `constructor_body` and not a `block`, which is the kind of grammar
/// detail that turns into "extract variable does not work in constructors" and stays there.
pub fn is_block(node: &Node<'_>) -> bool {
    matches!(node.kind(), "block" | "constructor_body")
}

/// Whether a declared type is one the compiler infers rather than one the source states.
///
/// Re-exported from `bennu-java`, which is where the inference engine already had to know: two
/// answers to "is this a written type" is how a `val` ends up refused by one refactoring and
/// mangled by the next.
pub use bennu_java::prelude::is_inferred_type;

pub fn is_statement(node: &Node<'_>) -> bool {
    STATEMENTS.contains(&node.kind())
}

pub fn is_expression(node: &Node<'_>) -> bool {
    EXPRESSIONS.contains(&node.kind())
}

/// The smallest named node containing `offset`.
pub fn node_at<'t>(root: Node<'t>, offset: usize) -> Option<Node<'t>> {
    root.named_descendant_for_byte_range(offset, offset)
}

/// The identifier a caret is on — **including the one it sits immediately after**.
///
/// A caret is between two characters, so `count|` and `|count` are the same place to a user, and
/// clicking at the end of a word (or arrow-keying to it) is the commonest way to land on a name.
/// [`node_at`] answers a *range* question and at a token boundary gives the enclosing node instead
/// of the token to the left — which is the right answer for "what would I extract here" and the
/// wrong one for "which name am I on". So the two questions get two functions.
pub fn identifier_at<'t>(root: Node<'t>, offset: usize) -> Option<Node<'t>> {
    if let Some(here) = node_at(root, offset).filter(|n| n.kind() == "identifier") {
        return Some(here);
    }
    let before = offset.checked_sub(1).and_then(|b| node_at(root, b))?;
    (before.kind() == "identifier" && before.end_byte() == offset).then_some(before)
}

/// The smallest named node covering `[start, end)`.
pub fn node_covering<'t>(root: Node<'t>, start: usize, end: usize) -> Option<Node<'t>> {
    root.named_descendant_for_byte_range(start, end.max(start))
}

/// The nearest ancestor (or `node` itself) of one of `kinds`.
pub fn enclosing<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if kinds.contains(&n.kind()) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// The method or constructor a node sits in.
pub fn enclosing_callable<'t>(node: Node<'t>) -> Option<Node<'t>> {
    enclosing(node, CALLABLES)
}

/// The type declaration a node sits in.
pub fn enclosing_type<'t>(node: Node<'t>) -> Option<Node<'t>> {
    enclosing(node, TYPE_DECLS)
}

/// The expression a refactoring should act on for this caret or selection.
///
/// With a **selection**, the node must cover it exactly (whitespace trimmed) — a range that ends in
/// the middle of an expression is refused rather than widened, because widening changes what is
/// being extracted without saying so.
///
/// With a **caret**, the innermost expression it is inside, then walked up out of the positions
/// where a bare name is not a value the user means: the *name* half of `foo.bar()` and of `a.b` are
/// syntactically identifiers and are not things to extract on their own.
pub fn expression_for<'t>(
    root: Node<'t>,
    source: &str,
    start: usize,
    end: usize,
) -> Option<Node<'t>> {
    let (start, end) = trim_range(source, start, end);
    if end > start {
        let node = node_covering(root, start, end)?;
        let exact = node.start_byte() == start && node.end_byte() == end;
        return (exact && is_expression(&node)).then_some(node);
    }
    let mut node = enclosing(node_at(root, start)?, EXPRESSIONS)?;
    while let Some(parent) = node.parent() {
        if !is_expression(&parent) {
            break;
        }
        // Only climb out of the positions where the child is a *name*, not a value: the method name
        // of an invocation, the field name of an access. Everything else — an operand, an argument
        // — is a perfectly good thing to extract, and climbing past it would offer the whole
        // enclosing expression when the user pointed at a part of it.
        let is_name_of = matches!(parent.kind(), "method_invocation" | "field_access")
            && parent.child_by_field_name("name").map(|n| n.id()) == Some(node.id());
        if !is_name_of && !is_bare_name(&node) {
            break;
        }
        node = parent;
    }
    // A caret that could climb no further than a bare name has nothing to offer: `count` already
    // has a name, and a menu row saying so on every caret in every identifier is noise. A bare name
    // that was *selected* still reaches the refusal, which is where saying it is worth something.
    (!is_bare_name(&node)).then_some(node)
}

/// A name is not a value worth extracting — it already has the name that extracting would give it.
fn is_bare_name(node: &Node<'_>) -> bool {
    matches!(node.kind(), "identifier" | "scoped_identifier" | "this")
}

/// The consecutive statements a selection covers, snapped outward to whole ones.
///
/// `None` when the selection is empty, touches nothing that is a statement, or spans two different
/// blocks — the last being the case with no honest interpretation (see the module docs).
pub fn statements_for<'t>(
    root: Node<'t>,
    source: &str,
    start: usize,
    end: usize,
) -> Option<Vec<Node<'t>>> {
    let (start, end) = trim_range(source, start, end);
    if end <= start {
        return None;
    }
    let first = enclosing(node_at(root, start)?, STATEMENTS)?;
    // `end` is exclusive, so a selection ending exactly at a statement's `}` must look one byte
    // back or it lands on whatever follows.
    let last = enclosing(node_at(root, end.saturating_sub(1))?, STATEMENTS)?;
    let parent = first.parent()?;
    // The end of a selection often lands INSIDE a nested statement — the `}` of the loop a label
    // wraps, the last line of an `if` body — so walk it out to whichever sibling of `first` it sits
    // in. Without this, selecting a labelled loop whole reads as "two different blocks" and the
    // whole gesture answers nothing.
    let mut last = last;
    while last.parent().is_some_and(|p| p.id() != parent.id()) {
        let Some(up) = last.parent() else { break };
        last = up;
    }
    if last.parent()?.id() != parent.id() {
        return None; // two different blocks
    }
    // …and it has to BE a block. The arm of a `switch` rule (`case X -> "text";`) holds an
    // expression, not a statement, and walking up from inside it lands on the whole `switch` —
    // which then gets "extracted" into a method whose call is not a statement at all. A selection
    // that is not a run of statements in a block is not an extract-method gesture.
    if !is_block(&parent) {
        return None;
    }
    let mut out = Vec::new();
    let mut cursor = parent.walk();
    let mut inside = false;
    for child in parent.named_children(&mut cursor) {
        if child.id() == first.id() {
            inside = true;
        }
        if inside && is_statement(&child) {
            out.push(child);
        }
        if child.id() == last.id() {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Trim whitespace off both ends of a byte range, so a selection made by dragging over a line does
/// not carry its indentation into the answer.
pub fn trim_range(source: &str, start: usize, end: usize) -> (usize, usize) {
    let end = end.min(source.len());
    let start = start.min(end);
    let slice = &source[start..end];
    let lead = slice.len() - slice.trim_start().len();
    let trail = slice.len() - slice.trim_end().len();
    (start + lead, end - trail)
}

/// Every `identifier` node under `node`, in source order.
pub fn identifiers<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect_kind(node, "identifier", &mut out);
    out
}

fn collect_kind<'t>(node: Node<'t>, kind: &str, out: &mut Vec<Node<'t>>) {
    if node.kind() == kind {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_kind(child, kind, out);
    }
}

/// Every node of `kind` under `node`.
pub fn descendants<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect_kind(node, kind, &mut out);
    out
}

/// Whether `node` carries the `static` modifier.
pub fn is_static(node: &Node<'_>, source: &str) -> bool {
    let mut cursor = node.walk();
    // Bound rather than returned directly: the iterator borrows `cursor`, and as a tail expression
    // it would outlive it. Same shape as a mutex guard on a function's last line.
    let is_static = node
        .named_children(&mut cursor)
        .any(|c| c.kind() == "modifiers" && text(&c, source).contains("static"));
    is_static
}

/// The source text of a node.
pub fn text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}

/// The indentation of the line `offset` sits on — what a generated statement is written with so it
/// lands in the surrounding code rather than at column zero.
pub fn indent_at(source: &str, offset: usize) -> String {
    let line_start = source[..offset.min(source.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// The newline this file is written with, so a generated line does not mix them.
///
/// A buffer the editor holds is normalised to `\n`, but a plan is also applied to text read
/// straight off disk in the tests and by any other caller — and a `\r\n` file gaining a lone `\n`
/// is the kind of diff that turns a two-line refactoring into a whole-file change.
pub fn newline(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    const SRC: &str = r#"class A {
    int total(int n) {
        int base = n * 2;
        int sum = base + compute(n);
        return sum;
    }
}"#;

    fn root(source: &str) -> tree_sitter::Tree {
        parse_java(source).expect("java parses")
    }

    #[test]
    fn a_caret_finds_the_expression_it_is_inside() {
        let tree = root(SRC);
        let at = SRC.find("n * 2").unwrap() + 1;
        let node = expression_for(tree.root_node(), SRC, at, at).unwrap();
        assert_eq!(text(&node, SRC), "n * 2");
    }

    /// The name half of a call is not a value; pointing at it means the call.
    #[test]
    fn a_caret_on_a_method_name_means_the_call() {
        let tree = root(SRC);
        let at = SRC.find("compute(n)").unwrap() + 2;
        let node = expression_for(tree.root_node(), SRC, at, at).unwrap();
        assert_eq!(text(&node, SRC), "compute(n)");
    }

    /// A selection that ends mid-expression is refused rather than widened — widening changes what
    /// is being extracted without saying so.
    /// A caret is between two characters, so the name it sits at the end of is the name it is on.
    #[test]
    fn a_caret_at_the_end_of_a_name_is_on_that_name() {
        let tree = root(SRC);
        let end = SRC.find("base + compute").unwrap() + "base".len();
        let node = identifier_at(tree.root_node(), end).expect("an identifier");
        assert_eq!(text(&node, SRC), "base");
    }

    #[test]
    fn a_selection_must_cover_an_expression_exactly() {
        let tree = root(SRC);
        let start = SRC.find("base + compute(n)").unwrap();
        assert!(expression_for(tree.root_node(), SRC, start, start + "base + comp".len()).is_none());
        let exact = expression_for(tree.root_node(), SRC, start, start + "base + compute(n)".len());
        assert_eq!(exact.map(|n| text(&n, SRC)), Some("base + compute(n)"));
    }

    #[test]
    fn a_selection_snaps_outward_to_whole_statements() {
        let tree = root(SRC);
        // Starts in the middle of the first declaration's indentation and ends mid-word.
        let start = SRC.find("int base").unwrap() - 2;
        let end = SRC.find("compute").unwrap() + 3;
        let stmts = statements_for(tree.root_node(), SRC, start, end).unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(text(&stmts[0], SRC).starts_with("int base"));
        assert!(text(&stmts[1], SRC).starts_with("int sum"));
    }

    #[test]
    fn a_selection_spanning_two_blocks_has_no_honest_reading() {
        let src = "class A { void a() { int x = 1; } void b() { int y = 2; } }";
        let tree = root(src);
        let start = src.find("int x").unwrap();
        let end = src.find("int y").unwrap() + 6;
        assert!(statements_for(tree.root_node(), src, start, end).is_none());
    }

    #[test]
    fn the_enclosing_callable_and_its_staticness_are_read_off_the_tree() {
        let src = "class A { static int f() { return 1; } }";
        let tree = root(src);
        let at = src.find("return").unwrap();
        let method = enclosing_callable(node_at(tree.root_node(), at).unwrap()).unwrap();
        assert_eq!(method.kind(), "method_declaration");
        assert!(is_static(&method, src));
    }

    #[test]
    fn indentation_is_read_off_the_line_a_statement_starts_on() {
        let src = "class A {\n    void f() {\n        int x = 1;\n    }\n}";
        assert_eq!(indent_at(src, src.find("int x").unwrap()), "        ");
        assert_eq!(newline(src), "\n");
        assert_eq!(newline("a\r\nb"), "\r\n");
    }
}
