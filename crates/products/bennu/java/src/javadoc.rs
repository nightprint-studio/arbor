//! The `/** … */` block above a declaration, as the text a reader wants.
//!
//! One rule, in one place, because two readers of it already existed and a third was about to.
//! [`leading`] answers for a byte offset — *what is documented immediately above this?* — and
//! [`declarations`] answers for a whole file at once: every type, method and field it declares,
//! keyed the way a caller holding a binary name and a member name can look one up.
//!
//! The second is what makes a **library**'s documentation reachable. A `.class` carries no comments,
//! so the only place a dependency's Javadoc exists is its source — the `-sources.jar` behind
//! "Download sources" and the JDK's own `src.zip`, both of which Bennu already opens for go-to. A
//! file-at-once pass is the right shape there: reading one archive entry and parsing it once answers
//! every hover into that type, where a per-hover search would re-read the archive on every pointer
//! move.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

/// How much of a doc block is kept. Long enough for the sentence that says what a thing is and the
/// `@param`/`@return` lines under it; short enough that a tooltip stays a tooltip.
const MAX_DOC: usize = 600;

/// Extract and clean the `/** … */` block that ends immediately above the declaration starting at
/// `decl_start`.
///
/// Returns the joined, trimmed text — the `/**` and `*/` markers gone, the gutter `*` stripped from
/// each line, capped at [`MAX_DOC`] characters — or `None` when what sits above the declaration is
/// not a Javadoc block.
pub fn leading(source: &str, decl_start: usize) -> Option<String> {
    // Everything above the declaration. Only the whitespace/comment tail matters: a modifier
    // keyword (`public`) between the comment and the node cannot occur, because a declaration node
    // starts at its modifiers.
    let head = source.get(..decl_start)?;
    let trimmed = head.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    let open = trimmed.rfind("/**")?;
    let close = trimmed.len() - "*/".len();
    if open + "/**".len() > close {
        return None; // malformed / `/**/`
    }
    clean(&trimmed[open + "/**".len()..close])
}

/// The body of a doc block, with its gutter removed and its blank edges trimmed. `None` when
/// nothing is left.
fn clean(inner: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in inner.lines() {
        let mut l = raw.trim();
        // Strip a leading `*` (the Javadoc gutter) and one following space.
        if let Some(rest) = l.strip_prefix('*') {
            l = rest.strip_prefix(' ').unwrap_or(rest);
        }
        lines.push(l.to_string());
    }
    while lines.first().is_some_and(|s| s.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|s| s.is_empty()) {
        lines.pop();
    }
    let joined = lines.join("\n");
    let doc = joined.trim();
    (!doc.is_empty()).then(|| doc.chars().take(MAX_DOC).collect())
}

/// Every documented declaration in one Java source.
///
/// Built once per file and looked up many times — see the module docs on why that shape, not a
/// search per question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDocs {
    /// The doc on the file's **outermost** type. The one a hover on the type name wants, and the
    /// one a `-sources.jar` entry is named after.
    pub type_doc: Option<String>,
    /// Doc by `(method name, parameter count)`. A constructor is keyed under `<init>`, as the
    /// member model names it.
    ///
    /// Overloads that agree on arity are **dropped** rather than merged: nothing here can tell which
    /// one a call bound to, and the doc of the wrong overload is worse than none.
    pub methods: HashMap<(String, usize), String>,
    /// Doc by field name.
    pub fields: HashMap<String, String>,
    /// Doc by nested-type simple name — `Map.Entry`'s own block, reached from `Map$Entry`.
    pub types: HashMap<String, String>,
    /// Method names this file declares MORE THAN ONCE — whether or not each one is documented.
    ///
    /// The question `method` has to answer is *is this name overloaded*, and [`methods`] cannot
    /// answer it: a bucket dropped for an arity collision leaves no trace, and an overload with no
    /// doc block never enters the map at all. Both cases look identical to "one method of that
    /// name", which is the one shape the fallback is allowed to answer for.
    ///
    /// Names, not signatures, and file-wide: two same-named methods in two nested types of one file
    /// count as overloaded here. That is the same conservatism [`methods`] already has — it is keyed
    /// by name and arity with no owner — and it errs towards saying nothing.
    ///
    /// [`methods`]: FileDocs::methods
    pub overloaded: HashSet<String>,
}

impl FileDocs {
    pub fn is_empty(&self) -> bool {
        // `overloaded` is bookkeeping about what was declared, not documentation: a file of
        // undocumented overloads fills it and still documents nothing.
        self.type_doc.is_none()
            && self.methods.is_empty()
            && self.fields.is_empty()
            && self.types.is_empty()
    }

    /// The doc for a method, preferring the overload of `arity` and falling back to the only one of
    /// that name when the caller does not know the count (a hover on a declaration rather than a
    /// call). `None` when the name is overloaded and nothing narrows it.
    ///
    /// The fallback is worth having for one shape in particular: a lone **varargs** method, where
    /// the call site counts three arguments and the declaration has two parameters, so the arity
    /// asked for is not the arity declared. It must not survive contact with an overloaded name,
    /// though — answering `get(String)` with the block written above `get()` is not a near miss,
    /// it is the documentation of a different method.
    pub fn method(&self, name: &str, arity: Option<usize>) -> Option<&String> {
        if let Some(n) = arity {
            if let Some(doc) = self.methods.get(&(name.to_string(), n)) {
                return Some(doc);
            }
        }
        if self.overloaded.contains(name) {
            return None;
        }
        let mut hits = self.methods.iter().filter(|((n, _), _)| n == name);
        let only = hits.next()?;
        hits.next().is_none().then_some(only.1)
    }
}

/// Read every `/** … */` in `source` and key it by what it documents.
pub fn declarations(source: &str) -> FileDocs {
    let mut out = FileDocs::default();
    let Some(tree) = crate::grammar::parse_java(source) else { return out };
    let bytes = source.as_bytes();
    // Arity collisions are resolved by dropping both, so the pass records what it has seen rather
    // than overwriting — see `FileDocs::methods`.
    let mut ambiguous: HashSet<(String, usize)> = HashSet::new();
    // How many times each method name is DECLARED, documented or not — the only way to know that a
    // name is overloaded, since the map below keeps neither the collisions it drops nor the
    // declarations that carry no doc block.
    let mut declared: HashMap<String, usize> = HashMap::new();
    let mut outermost_seen = false;

    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        let doc = leading(source, n.start_byte());
        match n.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" | "annotation_type_declaration" => {
                let (Some(name), Some(doc)) = (child_name(&n, bytes), doc) else { continue };
                // Every type is keyed by its simple name, which is what a caller holding a binary
                // name has. `type_doc` is the convenience on top: the file's own type, for the
                // caller that asked about `Optional` and does not want to spell it twice. Depth is
                // read off the tree rather than tracked, because this walk is not in source order.
                if is_top_level(&n) && !outermost_seen {
                    outermost_seen = true;
                    out.type_doc = Some(doc.clone());
                }
                out.types.insert(name, doc);
            }
            "method_declaration" | "constructor_declaration" => {
                let name = if n.kind() == "constructor_declaration" {
                    "<init>".to_string()
                } else {
                    match child_name(&n, bytes) {
                        Some(nm) => nm,
                        None => continue,
                    }
                };
                let Some(arity) = parameter_count(&n) else { continue };
                *declared.entry(name.clone()).or_default() += 1;
                let Some(doc) = doc else { continue };
                let key = (name, arity);
                if ambiguous.contains(&key) {
                    continue;
                }
                if out.methods.insert(key.clone(), doc).is_some() {
                    out.methods.remove(&key);
                    ambiguous.insert(key);
                }
            }
            "field_declaration" => {
                let Some(doc) = doc else { continue };
                for name in declarator_names(&n, bytes) {
                    out.fields.entry(name).or_insert_with(|| doc.clone());
                }
            }
            _ => {}
        }
    }
    out.overloaded = declared
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|(name, _)| name)
        .collect();
    out
}

/// Whether a type declaration is the file's own rather than a member of another type.
fn is_top_level(n: &Node) -> bool {
    let mut cur = n.parent();
    while let Some(p) = cur {
        // A member type sits inside somebody's body (`class_body`, `interface_body`, `enum_body`)
        // and, above that, inside their declaration. Either is enough to say this is not the
        // file's own type.
        if p.kind().ends_with("_declaration") || p.kind().ends_with("_body") {
            return false;
        }
        cur = p.parent();
    }
    true
}

fn child_name(n: &Node, bytes: &[u8]) -> Option<String> {
    n.child_by_field_name("name")?.utf8_text(bytes).ok().map(str::to_string)
}

/// How many parameters a method/constructor declares. `None` when the node has no parameter list,
/// which means the tree is not shaped the way this expects and nothing should be recorded.
fn parameter_count(n: &Node) -> Option<usize> {
    let params = n.child_by_field_name("parameters")?;
    let mut c = params.walk();
    Some(
        params
            .named_children(&mut c)
            .filter(|p| matches!(p.kind(), "formal_parameter" | "spread_parameter"))
            .count(),
    )
}

/// The names declared by one `field_declaration` — `int a, b;` declares two.
fn declarator_names(n: &Node, bytes: &[u8]) -> Vec<String> {
    let mut c = n.walk();
    n.named_children(&mut c)
        .filter(|ch| ch.kind() == "variable_declarator")
        .filter_map(|ch| child_name(&ch, bytes))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
package com.acme;

/**
 * A box that holds one thing.
 *
 * @param <T> what it holds
 */
public class Box<T> {

    /** The thing. */
    private final T value;

    /** Undocumented arity twin. */
    public T get() { return value; }

    /** Two of them at this arity. */
    public T get(int i) { return value; }

    /** The other one. */
    public T get(String k) { return value; }

    /** A nested pair. */
    public static class Pair { }
}
"#;

    #[test]
    fn the_outermost_types_doc_is_the_files_doc() {
        let docs = declarations(SRC);
        assert!(docs.type_doc.as_deref().unwrap().starts_with("A box that holds one thing."));
        assert_eq!(docs.types.get("Pair").map(String::as_str), Some("A nested pair."));
    }

    #[test]
    fn a_field_is_keyed_by_its_name() {
        assert_eq!(declarations(SRC).fields.get("value").map(String::as_str), Some("The thing."));
    }

    #[test]
    fn overloads_that_agree_on_arity_are_dropped_rather_than_guessed_at() {
        let docs = declarations(SRC);
        assert_eq!(docs.method("get", Some(0)).map(String::as_str), Some("Undocumented arity twin."));
        assert_eq!(docs.method("get", Some(1)), None, "two one-argument overloads: no answer");
        assert_eq!(docs.method("get", None), None, "and none without an arity either");
    }

    /// The other half of "is this name overloaded": an overload with **no doc block** never enters
    /// the map, so a name with one documented and one undocumented declaration used to look like a
    /// name with exactly one method — and the hover answered every call of it with the block above
    /// the other one.
    #[test]
    fn an_undocumented_overload_still_makes_the_name_ambiguous() {
        let docs = declarations("class A { /** One. */ void run() {} void run(int n) {} }");
        assert_eq!(docs.method("run", Some(0)).map(String::as_str), Some("One."));
        assert_eq!(docs.method("run", Some(1)), None, "the undocumented overload has no doc");
        assert_eq!(docs.method("run", None), None, "and the name alone does not pick one");
    }

    /// The fallback that survives: a LONE varargs method, where the call counts more arguments than
    /// the declaration has parameters. Nothing else is named `join`, so there is nothing to confuse
    /// it with.
    #[test]
    fn a_lone_method_answers_for_an_arity_it_does_not_declare() {
        let docs = declarations("class A { /** Joins. */ String join(String sep, Object... parts) { return null; } }");
        assert_eq!(docs.method("join", Some(4)).map(String::as_str), Some("Joins."));
    }

    #[test]
    fn a_lone_method_answers_without_an_arity() {
        let docs = declarations("class A { /** Only one. */ void run() {} }");
        assert_eq!(docs.method("run", None).map(String::as_str), Some("Only one."));
    }

    #[test]
    fn a_line_comment_above_a_declaration_is_not_a_doc_block() {
        let docs = declarations("class A { // not javadoc\n void run() {} }");
        assert!(docs.methods.is_empty());
    }

    #[test]
    fn the_gutter_and_the_markers_are_stripped() {
        let src = "/**\n * First.\n *\n * @return nothing\n */\nclass A {}";
        assert_eq!(leading(src, src.find("class").unwrap()).unwrap(), "First.\n\n@return nothing");
    }
}
