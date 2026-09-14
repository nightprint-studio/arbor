//! One Java buffer, parsed once, and the questions every check in this crate asks of it.
//!
//! Recognising Mockito is a name-resolution problem before it is anything else. `when`, `verify`,
//! `any` and `eq` are ordinary identifiers: a project may keep a test helper called `when`, AssertJ
//! declares a `not`, Hamcrest an `any`. Nothing here is recognised by its spelling — every call is
//! resolved through the file's imports, with one stricter rule on top, see [`JavaFile::resolves`].

use bennu_facts::prelude::{scan_java, static_call_resolves_to, JavaFacts};
use bennu_java::prelude::{node_text, parse_java};
use tree_sitter::{Node, Tree};

use crate::owners::{LIBRARY, NEIGHBOURS};

pub(crate) struct JavaFile<'s> {
    pub source: &'s str,
    pub facts: JavaFacts,
    tree: Tree,
    /// The owners of every `import static X.*;` — the imports that can make a bare name ambiguous.
    static_on_demand: Vec<String>,
}

impl<'s> JavaFile<'s> {
    pub fn parse(source: &'s str) -> Option<Self> {
        let facts = scan_java("", source)?;
        // A cache hit: `scan_java` parsed the same text through the same cache a moment ago.
        let tree = parse_java(source)?;
        let static_on_demand = static_on_demand_owners(tree.root_node(), source);
        Some(Self { source, facts, tree, static_on_demand })
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Whether `name(…)` — or `qualifier.name(…)` — is certainly a static of one of `owners`.
    ///
    /// The import rule, plus one thing it does not ask: a bare name reached only through an on-demand
    /// import is certain only when **no other** static on-demand import could declare it too.
    /// `import static org.hamcrest.Matchers.*;` beside Mockito's makes `any()` a coin toss to a
    /// reader who has not memorised both classes, and a check that guessed would be wrong on exactly
    /// the files where the answer matters.
    pub fn resolves(&self, name: &str, qualifier: Option<&str>, owners: &[&str]) -> bool {
        if !static_call_resolves_to(name, qualifier, &self.facts, owners) {
            return false;
        }
        if qualifier.is_some() {
            return true;
        }
        // A single-static-import of the name beats every on-demand one, as it does for the compiler.
        let suffix = format!(".{name}");
        if self.facts.imports.iter().any(|i| i.ends_with(&suffix)) {
            return true;
        }
        self.static_on_demand.iter().all(|owner| cannot_declare(owner, name))
    }

    /// Whether `call` is a method invocation named `name` that resolves as a static of `owners`.
    pub fn is_static_call(&self, call: Node<'_>, name: &str, owners: &[&str]) -> bool {
        call.kind() == "method_invocation"
            && call_name(call, self.source) == name
            && static_qualifier(call, self.source).is_some_and(|q| self.resolves(name, q, owners))
    }

    /// Whether the file declares a method of this name — which hides any static import of it.
    pub fn declares_method(&self, name: &str) -> bool {
        self.facts
            .types
            .iter()
            .any(|t| t.methods.iter().any(|m| m.name == name && !m.is_constructor))
    }
}

/// Whether a static on-demand import of `owner` certainly does not bring a `name` of its own.
fn cannot_declare(owner: &str, name: &str) -> bool {
    LIBRARY.contains(&owner)
        || NEIGHBOURS
            .iter()
            .any(|(neighbour, declared)| *neighbour == owner && !declared.contains(&name))
}

/// The owners of the file's `import static X.*;` lines. Read from the tree, because the facts
/// normalise the `static` keyword away and a plain `import java.util.*;` must not count.
fn static_on_demand_owners(root: Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for import in root.named_children(&mut cursor).filter(|n| n.kind() == "import_declaration") {
        let (mut is_static, mut on_demand, mut owner) = (false, false, None);
        let mut parts = import.walk();
        for part in import.children(&mut parts) {
            match part.kind() {
                "static" => is_static = true,
                "asterisk" => on_demand = true,
                "identifier" | "scoped_identifier" => owner = Some(node_text(&part, source)),
                _ => {}
            }
        }
        if let (true, true, Some(owner)) = (is_static, on_demand, owner) {
            out.push(owner.to_string());
        }
    }
    out
}

/// The name a method invocation calls.
pub(crate) fn call_name<'s>(call: Node<'_>, source: &'s str) -> &'s str {
    call.child_by_field_name("name").map(|n| node_text(&n, source)).unwrap_or_default()
}

/// The arguments of a call, comments left out.
pub(crate) fn arguments(call: Node<'_>) -> Vec<Node<'_>> {
    let Some(list) = call.child_by_field_name("arguments") else { return Vec::new() };
    let mut cursor = list.walk();
    let args = list
        .named_children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .collect();
    args
}

/// How a call is qualified, when it can be a static call at all: `Some(None)` for a bare `when(…)`,
/// `Some(Some("Mockito"))` for `Mockito.when(…)`, and `None` for a call on a computed receiver
/// (`doReturn(1).when(…)`, `this.when(…)`), which is an instance call whatever it is named.
///
/// A lowercase `repo.when(…)` comes back qualified by `repo` — and then resolves to nothing, because
/// no import names a class `repo`. That is resolution doing the work, not this function guessing.
pub(crate) fn static_qualifier<'s>(call: Node<'_>, source: &'s str) -> Option<Option<&'s str>> {
    match call.child_by_field_name("object") {
        None => Some(None),
        Some(object) if is_dotted_name(object) => Some(Some(node_text(&object, source))),
        Some(_) => None,
    }
}

fn is_dotted_name(node: Node<'_>) -> bool {
    match node.kind() {
        "identifier" => true,
        "field_access" => {
            node.child_by_field_name("object").is_some_and(is_dotted_name)
                && node.child_by_field_name("field").is_some_and(|f| f.kind() == "identifier")
        }
        _ => false,
    }
}

/// An expression with its parentheses and casts peeled off — `(String) any()` is still `any()`.
pub(crate) fn unwrap_expression(node: Node<'_>) -> Node<'_> {
    let mut node = node;
    loop {
        let inner = match node.kind() {
            "parenthesized_expression" => node.named_child(0),
            "cast_expression" => node.child_by_field_name("value"),
            _ => None,
        };
        match inner {
            Some(inner) => node = inner,
            None => return node,
        }
    }
}

/// Every named node under `root`, `root` included. Not in source order.
pub(crate) fn descendants(root: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

/// Whether evaluating `node` calls a method anywhere.
pub(crate) fn contains_invocation(node: Node<'_>) -> bool {
    descendants(node).iter().any(|n| n.kind() == "method_invocation")
}

/// The variable an assignment's left-hand side writes: `x` in `x = …` and in `this.x = …`.
pub(crate) fn assigned_identifier(left: Node<'_>) -> Option<Node<'_>> {
    match left.kind() {
        "identifier" => Some(left),
        "field_access" => left.child_by_field_name("field"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owners::{ARGUMENT_MATCHERS, MOCKITO, STUBBING};

    const MATCHERS: &[&str] = &[ARGUMENT_MATCHERS, MOCKITO];

    #[test]
    fn a_static_on_demand_import_of_mockito_reaches_its_names() {
        let src = "import static org.mockito.Mockito.*;\nclass T {}";
        let file = JavaFile::parse(src).unwrap();
        assert!(file.resolves("when", None, STUBBING));
        assert!(file.resolves("any", None, MATCHERS));
    }

    /// Hamcrest declares an `any` too: beside its on-demand import a bare `any()` is not certain.
    #[test]
    fn an_unknown_static_on_demand_import_makes_a_bare_name_uncertain() {
        let src = "import static org.mockito.Mockito.*;\nimport static org.hamcrest.Matchers.*;\nclass T {}";
        let file = JavaFile::parse(src).unwrap();
        assert!(!file.resolves("any", None, MATCHERS));
    }

    /// The usual neighbours do not silence everything — only the names they really share.
    #[test]
    fn a_known_neighbour_only_takes_the_names_it_declares() {
        let src = "import static org.mockito.Mockito.*;\nimport static org.assertj.core.api.Assertions.*;\nimport static org.mockito.AdditionalMatchers.not;\nclass T {}";
        let file = JavaFile::parse(src).unwrap();
        assert!(file.resolves("when", None, STUBBING));
        assert!(file.resolves("not", None, &[crate::owners::ADDITIONAL_MATCHERS]), "a single import decides");
        let bare = "import static org.mockito.AdditionalMatchers.*;\nimport static org.assertj.core.api.Assertions.*;\nclass T {}";
        assert!(!JavaFile::parse(bare).unwrap().resolves("not", None, &[crate::owners::ADDITIONAL_MATCHERS]));
    }

    #[test]
    fn a_plain_on_demand_import_is_not_a_static_one() {
        let src = "import java.util.*;\nimport static org.mockito.Mockito.*;\nclass T {}";
        let file = JavaFile::parse(src).unwrap();
        assert!(file.resolves("any", None, MATCHERS));
    }
}
