//! Sealed hierarchies, judged inside one compilation unit (pure-AST, JLS §8.1.1.2 / §9.1.1.4).
//!
//! * a type extending a sealed type its `permits` clause does not list;
//! * a permitted subclass that is not `final`, `sealed` or `non-sealed` (records and enums are
//!   implicitly final);
//! * a sealed type with no `permits` clause and no subclass in its file — without the clause, only
//!   the file's own declarations can be its subclasses, so the file is the whole answer;
//! * a `non-sealed` type none of whose direct supertypes is sealed.
//!
//! Names are matched by simple name, and only when that is unambiguous: a name declared twice in the
//! file, or also imported, is left alone. A supertype declared in another file is never judged —
//! whether IT is sealed is not something this file can say.

use std::collections::{HashMap, HashSet};

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::{modifier_keywords, text};
use crate::support::supertypes;

const TYPE_DECLS: [&str; 4] = ["class_declaration", "interface_declaration", "enum_declaration", "record_declaration"];

/// Every sealed-hierarchy error in the file.
pub fn sealed_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let decls: Vec<Node> = nodes.iter().copied().filter(|n| TYPE_DECLS.contains(&n.kind())).collect();
    let sealing_used = decls.iter().any(|d| {
        let mods = modifier_keywords(*d, bytes);
        mods.contains(&"sealed") || mods.contains(&"non-sealed")
    });
    if !sealing_used {
        return Vec::new();
    }
    let mut by_name: HashMap<String, Option<Node>> = HashMap::new();
    for d in &decls {
        if let Some(name) = decl_name(*d, bytes) {
            by_name.entry(name).and_modify(|e| *e = None).or_insert(Some(*d));
        }
    }
    let imported = imported_simple_names(nodes, bytes);
    let declared_in_file = |written: &str| -> Option<Node> {
        let simple = simple_of(written);
        if imported.contains(simple) {
            return None;
        }
        by_name.get(simple).copied().flatten()
    };

    let mut out = Vec::new();
    let mut extended: HashSet<usize> = HashSet::new();
    for d in &decls {
        let Some(name_node) = d.child_by_field_name("name") else { continue };
        let Some(name) = text(name_node, bytes) else { continue };
        let mods = modifier_keywords(*d, bytes);
        let mut sealed_super = false;
        let mut every_super_read = true;
        for sup in supertypes::all(*d, bytes) {
            let Some(target) = declared_in_file(&sup.text) else {
                every_super_read = false;
                continue;
            };
            if !modifier_keywords(target, bytes).contains(&"sealed") {
                continue;
            }
            sealed_super = true;
            extended.insert(target.id());
            let sealed_name = simple_of(&sup.text);
            if let Some(permitted) = permits_of(target, bytes) {
                if !permitted.contains(&name) {
                    out.push(CheckId::IllegalInheritance.at(
                        sup.node,
                        format!("`{name}` is not listed in the `permits` clause of the sealed type `{sealed_name}`"),
                    ));
                    continue;
                }
            }
            let implicitly_final = matches!(d.kind(), "record_declaration" | "enum_declaration");
            let states_sealing = mods.iter().any(|m| matches!(*m, "final" | "sealed" | "non-sealed"));
            if !implicitly_final && !states_sealing {
                let expected = if d.kind() == "interface_declaration" {
                    "`sealed` or `non-sealed`"
                } else {
                    "`final`, `sealed` or `non-sealed`"
                };
                out.push(CheckId::IllegalInheritance.at(
                    name_node,
                    format!("`{name}` extends the sealed type `{sealed_name}` and must be declared {expected}"),
                ));
            }
        }
        if mods.contains(&"non-sealed") && !sealed_super && every_super_read {
            out.push(CheckId::IllegalInheritance.at(
                name_node,
                format!("`{name}` is `non-sealed`, but none of its direct supertypes is sealed"),
            ));
        }
    }
    for d in &decls {
        let mods = modifier_keywords(*d, bytes);
        // `sealed final` is already an illegal combination; its subclasses are not the problem.
        let contradictory = mods.contains(&"final") || mods.contains(&"non-sealed");
        if !mods.contains(&"sealed") || contradictory || permits_of(*d, bytes).is_some() || extended.contains(&d.id()) {
            continue;
        }
        if let (Some(name_node), Some(name)) = (d.child_by_field_name("name"), decl_name(*d, bytes)) {
            out.push(CheckId::IllegalInheritance.at(
                name_node,
                format!("Sealed type `{name}` has no `permits` clause and no subclass in this file"),
            ));
        }
    }
    out
}

fn decl_name(decl: Node, bytes: &[u8]) -> Option<String> {
    decl.child_by_field_name("name").and_then(|n| text(n, bytes))
}

/// `a.b.Outer.Inner<T>` → `Inner`.
fn simple_of(written: &str) -> &str {
    let raw = written.split('<').next().unwrap_or(written).trim();
    raw.rsplit('.').next().unwrap_or(raw).trim()
}

/// The simple names a `permits` clause lists, or `None` when the declaration has none.
fn permits_of(decl: Node, bytes: &[u8]) -> Option<Vec<String>> {
    let permits = decl.child_by_field_name("permits")?;
    let list = crate::support::nodes::child_of_kind(permits, "type_list")?;
    let mut c = list.walk();
    let names = list
        .named_children(&mut c)
        .filter_map(|t| text(t, bytes))
        .map(|t| simple_of(&t).to_string())
        .collect();
    Some(names)
}

/// The simple names single-type imports bring in — a supertype spelled like one of them is the
/// imported type, not a declaration of this file.
fn imported_simple_names(nodes: &[Node], bytes: &[u8]) -> HashSet<String> {
    nodes
        .iter()
        .filter(|n| n.kind() == "import_declaration")
        .filter_map(|n| n.utf8_text(bytes).ok())
        .filter(|t| !t.contains('*'))
        .filter_map(|t| t.trim_end_matches(';').trim().rsplit('.').next().map(|s| s.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errs(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        sealed_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_type_missing_from_permits_is_flagged() {
        let e = errs("class O { sealed interface V permits Car {} final class Car implements V {} final class Truck implements V {} }");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`Truck`"), "{e:?}");
    }

    #[test]
    fn a_permitted_subclass_must_state_its_sealing() {
        let e = errs("class O { sealed interface A permits Dog {} static class Dog implements A {} }");
        assert!(e.iter().any(|m| m.contains("must be declared")), "{e:?}");
        // Records and enums are implicitly final.
        assert!(errs("class O { sealed interface A {} record P(int x) implements A {} enum D implements A { N } }").is_empty());
    }

    #[test]
    fn a_sealed_type_without_subclasses_is_flagged() {
        assert!(errs("class O { sealed static class S {} }").iter().any(|m| m.contains("no subclass")));
        // With a permits clause the subclasses may live in other files.
        assert!(errs("sealed class S permits A, B {}").is_empty());
    }

    #[test]
    fn non_sealed_needs_a_sealed_supertype() {
        assert!(errs("class O { non-sealed static class N {} }").iter().any(|m| m.contains("non-sealed")));
        // A supertype declared elsewhere may well be sealed.
        assert!(errs("non-sealed class Bicycle extends Vehicle {}").is_empty());
    }

    #[test]
    fn a_legal_hierarchy_is_clean() {
        let src = "class O { sealed interface V permits Car, Truck, Bike {} static final class Car implements V {} \
                   static non-sealed class Truck implements V {} static class Pickup extends Truck {} \
                   abstract static sealed class Bike implements V permits Road {} static final class Road extends Bike {} }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }
}
