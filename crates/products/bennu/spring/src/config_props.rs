//! `@ConfigurationProperties` — working out the full key a field binds.
//!
//! ```java
//! @ConfigurationProperties(prefix = "app.http")
//! class HttpProperties {
//!     private Client client;            // app.http.client
//!     private Map<String, Endpoint> endpoints;   // app.http.endpoints.<key>
//! }
//! class Client { private Duration readTimeout; }  // app.http.client.read-timeout
//! ```
//!
//! `app.http.client.read-timeout` appears **nowhere** in that source. You assemble it every time
//! you need to write it in a yaml, from the prefix, the chain of field names between the field and
//! the root, and Spring's relaxed-binding rules — and the assembly is where the mistakes are,
//! silently, because a mistyped key simply never binds.
//!
//! So the walk goes the other way: from each root **down**, carrying the path.
//!
//! ## The four things that make it more than string concatenation
//!
//! - **Nesting.** A field whose type is another project class continues the path rather than
//!   ending it. The same class may be reached from two different roots — both paths are recorded,
//!   because picking one would be a guess.
//! - **Maps.** `Map<String, Endpoint> endpoints` binds `endpoints.<key>`, and `Endpoint`'s own
//!   fields hang below *that*. The key is written `<key>` rather than invented.
//! - **Collections.** `List<Server> servers` binds `servers[0]`, `servers[1]`, … — the index is
//!   shown as `[0]` because a concrete number is the only readable way to say "indexed here".
//! - **Renaming.** `@Name("read-timeout")` overrides the field name outright. Without it the
//!   canonical spelling is kebab-case ([`canonical_key_segment`]).
//!
//! ## Where it stops
//!
//! A cycle (`A` holding a `B` holding an `A`) and a depth beyond [`MAX_DEPTH`] end the descent.
//! A field whose type is not a scanned project class is a leaf — `String`, `Duration`, an enum,
//! anything from a jar. That under-reports the deepest keys in an unusual model and never invents
//! one, which is the direction this crate always errs in.
//!
//! [`canonical_key_segment`]: crate::model::canonical_key_segment

use std::collections::BTreeMap;

use crate::beans::JavaUnit;
use crate::model::{canonical_key_segment, simple_name, strip_generics, ConfigBinding};
use crate::scan::{FieldFacts, JavaFacts, TypeFacts};

/// How deep a nested-properties chain is followed. Well past any real configuration model; the
/// cap exists so a pathological (or cyclic-by-generics) graph cannot walk forever.
const MAX_DEPTH: usize = 8;

/// The container types whose *element* continues the path, and how the key reads at that point.
/// A map's key is unknown at compile time; a collection is addressed by index.
fn container_suffix(type_text: &str) -> Option<(&'static str, usize)> {
    let head = type_text.split('<').next().unwrap_or(type_text).trim();
    match simple_name(head) {
        "Map" | "HashMap" | "LinkedHashMap" | "TreeMap" | "SortedMap" | "Properties" => {
            // The VALUE type is the second argument.
            Some((".<key>", 1))
        }
        "List" | "ArrayList" | "Set" | "HashSet" | "LinkedHashSet" | "Collection" | "Iterable" => {
            Some(("[0]", 0))
        }
        _ if type_text.trim().ends_with("[]") => Some(("[0]", 0)),
        _ => None,
    }
}

/// The `n`-th generic argument of a written type, or `None`. Handles nesting by counting depth,
/// so `Map<String, List<Foo>>` yields `List<Foo>` for `n = 1`.
fn type_argument(type_text: &str, n: usize) -> Option<String> {
    let open = type_text.find('<')?;
    let inner = &type_text[open + 1..type_text.rfind('>')?];
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut found = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if found == n {
                    return Some(inner[start..i].trim().to_string());
                }
                found += 1;
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    (found == n).then(|| inner[start..].trim().to_string())
}

/// Every `@ConfigurationProperties`-bound field in the scan, with its full key.
pub fn bindings(units: &[JavaUnit]) -> Vec<ConfigBinding> {
    // Every scanned type, by FQCN and by simple name — a nested-properties field is usually
    // written with a simple type name, and resolving it is what continues the path.
    let mut by_fqcn: BTreeMap<&str, (&TypeFacts, &JavaFacts)> = BTreeMap::new();
    let mut by_simple: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for u in units {
        for t in &u.facts.types {
            by_fqcn.insert(t.fqcn.as_str(), (t, &u.facts));
            by_simple.entry(t.name.as_str()).or_default().push(t.fqcn.as_str());
        }
    }

    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            let Some(ann) = crate::known::find(&t.annotations, &u.facts, "ConfigurationProperties")
            else {
                continue;
            };
            // The prefix is `value` or `prefix` — aliases for each other.
            let prefix = ann
                .value()
                .map(|s| s.value.clone())
                .or_else(|| {
                    ann.strings_for("prefix").next().map(|s| s.value.clone())
                })
                .unwrap_or_default();
            let mut visited = vec![t.fqcn.as_str()];
            walk(t, &u.facts, &prefix, &prefix, &by_fqcn, &by_simple, 0, &mut visited, &mut out);
        }
    }
    out
}

/// Record `owner`'s fields under `path`, descending into the ones that are themselves
/// properties objects.
#[allow(clippy::too_many_arguments)]
fn walk<'a>(
    owner: &'a TypeFacts,
    facts: &JavaFacts,
    path: &str,
    root_prefix: &str,
    by_fqcn: &BTreeMap<&'a str, (&'a TypeFacts, &'a JavaFacts)>,
    by_simple: &BTreeMap<&'a str, Vec<&'a str>>,
    depth: usize,
    visited: &mut Vec<&'a str>,
    out: &mut Vec<ConfigBinding>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for f in &owner.fields {
        if f.is_static {
            continue;
        }
        let key = key_of(f, facts);
        if key.is_empty() {
            continue;
        }
        let full = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
        out.push(ConfigBinding {
            owner_fqcn: owner.fqcn.clone(),
            field: f.name.clone(),
            path: full.clone(),
            type_text: f.type_text.clone(),
            root_prefix: root_prefix.to_string(),
            file: facts.file.clone(),
            offset: f.name_offset,
        });

        // Does the path continue below this field? A container passes through to its element
        // type, carrying `.<key>` / `[0]`; anything else continues only if it is a project type.
        let (nested_path, element) = match container_suffix(&f.type_text) {
            Some((suffix, arg)) => match type_argument(&f.type_text, arg) {
                Some(inner) => (format!("{full}{suffix}"), inner),
                None => continue, // a raw `Map` / `List` says nothing about its element
            },
            None => (full.clone(), strip_generics(&f.type_text)),
        };
        let Some(next) = resolve(&element, facts, by_fqcn, by_simple) else { continue };
        let (next_type, next_facts) = by_fqcn[next];
        if visited.contains(&next) {
            continue; // a cycle in the model — the path below it would never end
        }
        visited.push(next);
        walk(
            next_type, next_facts, &nested_path, root_prefix, by_fqcn, by_simple, depth + 1,
            visited, out,
        );
        visited.pop();
    }
}

/// The key segment a field binds: `@Name("…")` when written, else the canonical kebab-case
/// spelling of the field name.
fn key_of(f: &FieldFacts, facts: &JavaFacts) -> String {
    if let Some(name) = crate::known::find(&f.annotations, facts, "Name").and_then(|a| a.value()) {
        if !name.value.is_empty() {
            return name.value.clone();
        }
    }
    canonical_key_segment(&f.name)
}

/// Resolve a written type name to a scanned type's FQCN — through the file's imports first, then
/// by a UNIQUE simple-name match. Ambiguity yields `None`: two candidate classes mean the path
/// below is unknowable, and stopping is better than descending into the wrong one.
fn resolve<'a>(
    written: &str,
    facts: &JavaFacts,
    by_fqcn: &BTreeMap<&'a str, (&'a TypeFacts, &'a JavaFacts)>,
    by_simple: &BTreeMap<&'a str, Vec<&'a str>>,
) -> Option<&'a str> {
    let resolved = crate::beans::resolve_type(written, facts);
    if let Some((fqcn, _)) = by_fqcn.get_key_value(resolved.as_str()) {
        return Some(*fqcn);
    }
    match by_simple.get(simple_name(&resolved)) {
        Some(fqcns) if fqcns.len() == 1 => Some(fqcns[0]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    const IMPORTS: &str = "import org.springframework.boot.context.properties.*; import org.springframework.boot.context.properties.bind.*;";

    fn unit(name: &str, src: &str) -> JavaUnit {
        let text = match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        };
        JavaUnit { facts: scan_java(&format!("/p/{name}.java"), &text).unwrap(), text }
    }

    fn paths(units: &[JavaUnit]) -> Vec<String> {
        let mut p: Vec<String> = bindings(units).into_iter().map(|b| b.path).collect();
        p.sort();
        p
    }

    #[test]
    fn a_flat_root_prefixes_its_fields() {
        let p = paths(&[unit(
            "A",
            "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private String name; private int maxPoolSize; }\n",
        )]);
        assert_eq!(p, ["app.max-pool-size", "app.name"], "canonical kebab-case");
    }

    #[test]
    fn the_bare_value_form_is_the_prefix_too() {
        let p = paths(&[unit(
            "A",
            "package p;\n@ConfigurationProperties(\"app\")\nclass A { private String name; }\n",
        )]);
        assert_eq!(p, ["app.name"]);
    }

    #[test]
    fn a_nested_object_continues_the_path() {
        let units = vec![
            unit(
                "Http",
                "package p;\n@ConfigurationProperties(prefix = \"app.http\")\nclass Http { private Client client; }\n",
            ),
            unit("Client", "package p;\nclass Client { private String readTimeout; }\n"),
        ];
        let p = paths(&units);
        assert_eq!(p, ["app.http.client", "app.http.client.read-timeout"]);
    }

    #[test]
    fn a_map_binds_its_key_and_its_value_object() {
        let units = vec![
            unit(
                "Root",
                "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass Root { private Map<String, Endpoint> endpoints; }\n",
            ),
            unit("Endpoint", "package p;\nclass Endpoint { private String url; }\n"),
        ];
        let p = paths(&units);
        assert_eq!(p, ["app.endpoints", "app.endpoints.<key>.url"]);
    }

    #[test]
    fn a_collection_is_addressed_by_index() {
        let units = vec![
            unit(
                "Root",
                "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass Root { private List<Server> servers; }\n",
            ),
            unit("Server", "package p;\nclass Server { private int port; }\n"),
        ];
        let p = paths(&units);
        assert_eq!(p, ["app.servers", "app.servers[0].port"]);
    }

    #[test]
    fn a_name_annotation_overrides_the_field_name() {
        let p = paths(&[unit(
            "A",
            "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { @Name(\"read-timeout-ms\") private int readTimeout; }\n",
        )]);
        assert_eq!(p, ["app.read-timeout-ms"]);
    }

    #[test]
    fn a_leaf_type_ends_the_path() {
        // `Duration` is not a scanned class, so nothing hangs below it — and nothing is invented.
        let p = paths(&[unit(
            "A",
            "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private java.time.Duration timeout; }\n",
        )]);
        assert_eq!(p, ["app.timeout"]);
    }

    #[test]
    fn a_cycle_terminates() {
        let units = vec![
            unit(
                "A",
                "package p;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private B b; }\n",
            ),
            unit("B", "package p;\nclass B { private A a; private String leaf; }\n"),
        ];
        let p = paths(&units);
        assert_eq!(p, ["app.b", "app.b.a", "app.b.leaf"], "the second A is not descended into");
    }

    #[test]
    fn a_type_reached_from_two_roots_records_both_paths() {
        let units = vec![
            unit(
                "One",
                "package p;\n@ConfigurationProperties(prefix = \"one\")\nclass One { private Shared shared; }\n",
            ),
            unit(
                "Two",
                "package p;\n@ConfigurationProperties(prefix = \"two\")\nclass Two { private Shared shared; }\n",
            ),
            unit("Shared", "package p;\nclass Shared { private String url; }\n"),
        ];
        let all = bindings(&units);
        let urls: Vec<_> =
            all.iter().filter(|b| b.field == "url").map(|b| b.path.as_str()).collect();
        assert_eq!(urls.len(), 2, "both are true; picking one would be a guess");
        assert!(urls.contains(&"one.shared.url"));
        assert!(urls.contains(&"two.shared.url"));
    }

    #[test]
    fn a_class_without_the_annotation_binds_nothing() {
        assert!(bindings(&[unit("A", "package p;\nclass A { private String name; }\n")]).is_empty());
    }

    #[test]
    fn a_same_named_annotation_from_elsewhere_binds_nothing() {
        let src = "package p;\nimport com.acme.ConfigurationProperties;\n@ConfigurationProperties(prefix = \"app\")\nclass A { private String name; }\n";
        let u = JavaUnit { facts: scan_java("/p/A.java", src).unwrap(), text: src.to_string() };
        assert!(bindings(&[u]).is_empty());
    }

    #[test]
    fn generic_arguments_are_split_at_the_top_level_only() {
        assert_eq!(type_argument("Map<String, List<Foo>>", 1).as_deref(), Some("List<Foo>"));
        assert_eq!(type_argument("Map<String, Foo>", 0).as_deref(), Some("String"));
        assert_eq!(type_argument("List<Foo>", 0).as_deref(), Some("Foo"));
        assert_eq!(type_argument("Map", 1), None, "a raw type has no arguments");
        assert_eq!(type_argument("List<Foo>", 1), None);
    }
}
