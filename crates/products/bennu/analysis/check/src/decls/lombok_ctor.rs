//! A hand-written constructor that repeats the one Lombok generates.
//!
//! `@RequiredArgsConstructor`, `@AllArgsConstructor` and `@NoArgsConstructor` — written explicitly —
//! generate their constructor **whatever else the class declares**, unlike the implicit constructor
//! of `@Data` / `@Value` / `@Builder`, which a hand-written one replaces. So a class that carries
//! `@RequiredArgsConstructor` and also spells out the constructor that annotation would write has two
//! constructors of one signature, and javac refuses it: `constructor X(…) is already defined`.
//!
//! The generated parameter list follows Lombok's rules: `@RequiredArgsConstructor` takes the
//! uninitialised `final` fields and the uninitialised `@NonNull` ones; `@AllArgsConstructor` every
//! instance field but an already-initialised `final`. Parameters are compared by their written type,
//! the way [`super::duplicates`] compares two hand-written ones, so only an exact repeat is reported.
//!
//! The report sits on the hand-written constructor's name: it is the line the reader is looking at, and
//! it names the annotation that makes it redundant.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::decls::duplicates::{normalize_type, param_types, type_var_erasures};
use crate::engine::check_id::CheckId;
use crate::support::lombok::{annotations_of, file_uses_lombok, imports_from_nodes};
use crate::support::nodes::has_keyword;

/// Which fields a Lombok constructor annotation takes.
#[derive(Clone, Copy)]
enum Takes {
    Nothing,
    Required,
    All,
}

fn takes(annotation: &str) -> Option<Takes> {
    match annotation {
        "NoArgsConstructor" => Some(Takes::Nothing),
        "RequiredArgsConstructor" => Some(Takes::Required),
        "AllArgsConstructor" => Some(Takes::All),
        _ => None,
    }
}

/// Flag each hand-written constructor that repeats a Lombok-generated one.
pub fn lombok_constructor_clash_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let imports = imports_from_nodes(nodes, bytes);
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "class_declaration") {
        // `@lombok.RequiredArgsConstructor` needs no import; anything else does.
        let lombok = file_uses_lombok(&imports);
        let asked: Vec<(Takes, &str)> = annotations_of(n, bytes)
            .iter()
            .filter(|a| lombok || a.written.starts_with("lombok."))
            // `access = AccessLevel.NONE` generates nothing, so nothing can clash.
            .filter(|a| !a.args.is_some_and(|args| args.contains("NONE")))
            .filter_map(|a| takes(a.simple).map(|t| (t, a.simple)))
            .collect();
        if asked.is_empty() {
            continue;
        }
        let Some(body) = n.child_by_field_name("body") else { continue };
        let fields = instance_fields(body, bytes);
        let mut c = body.walk();
        for ctor in body.named_children(&mut c).filter(|m| m.kind() == "constructor_declaration") {
            let Some(name) = ctor.child_by_field_name("name") else { continue };
            let written = param_types(ctor, bytes, &type_var_erasures(ctor, bytes));
            let clash = asked.iter().find(|(takes, _)| generated(&fields, *takes) == written);
            if let Some((_, annotation)) = clash {
                out.push(Diagnostic {
                    message: format!(
                        "Duplicate constructor: `@{annotation}` already generates a constructor with this signature"
                    ),
                    severity: CheckId::DuplicateMethod.severity().to_string(),
                    code: CheckId::DuplicateMethod.code().to_string(),
                    start: name.start_byte(),
                    end: name.end_byte(),
                });
            }
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// One instance field, as far as the generated constructor cares.
struct Field {
    type_text: String,
    is_final: bool,
    initialised: bool,
    non_null: bool,
}

fn instance_fields(body: Node, bytes: &[u8]) -> Vec<Field> {
    let mut out = Vec::new();
    let mut c = body.walk();
    for m in body.named_children(&mut c).filter(|m| m.kind() == "field_declaration") {
        if has_keyword(m, bytes, "static") {
            continue;
        }
        let Some(type_text) = m.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) else {
            continue;
        };
        let is_final = has_keyword(m, bytes, "final");
        let non_null = annotations_of(m, bytes).iter().any(|a| a.simple == "NonNull");
        let mut dc = m.walk();
        for d in m.named_children(&mut dc).filter(|d| d.kind() == "variable_declarator") {
            out.push(Field {
                type_text: normalize_type(type_text),
                is_final,
                initialised: d.child_by_field_name("value").is_some(),
                non_null,
            });
        }
    }
    out
}

fn generated(fields: &[Field], takes: Takes) -> Vec<String> {
    fields
        .iter()
        .filter(|f| match takes {
            Takes::Nothing => false,
            Takes::Required => !f.initialised && (f.is_final || f.non_null),
            Takes::All => !(f.is_final && f.initialised),
        })
        .map(|f| f.type_text.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clashes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        lombok_constructor_clash_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    const HEAD: &str = "import lombok.RequiredArgsConstructor;\nimport lombok.AllArgsConstructor;\nimport lombok.NoArgsConstructor;\nimport lombok.Data;\nimport lombok.NonNull;\n";

    #[test]
    fn a_constructor_repeating_required_args_is_flagged() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor\nclass S {{\n  private final Repo repo;\n  private final Mapper mapper;\n  private String note;\n  S(Repo repo, Mapper mapper) {{ this.repo = repo; this.mapper = mapper; }}\n}}"
        );
        let found = clashes(&src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("@RequiredArgsConstructor"), "{found:?}");
    }

    #[test]
    fn a_different_signature_is_a_legal_overload() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor\nclass S {{\n  private final Repo repo;\n  S(Repo repo, int extra) {{ this.repo = repo; }}\n}}"
        );
        assert!(clashes(&src).is_empty());
    }

    #[test]
    fn parameter_order_matters() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor\nclass S {{\n  private final A a;\n  private final B b;\n  S(B b, A a) {{ }}\n}}"
        );
        assert!(clashes(&src).is_empty());
    }

    #[test]
    fn an_initialised_final_is_not_a_required_parameter() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor\nclass S {{\n  private final A a;\n  private final List<String> names = new ArrayList<>();\n  S(A a) {{ this.a = a; }}\n}}"
        );
        assert_eq!(clashes(&src).len(), 1);
    }

    #[test]
    fn a_non_null_field_is_required_too() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor\nclass S {{\n  private final A a;\n  @NonNull private String name;\n  S(A a, String name) {{ }}\n}}"
        );
        assert_eq!(clashes(&src).len(), 1);
    }

    #[test]
    fn all_args_takes_every_field_but_an_initialised_final() {
        let src = format!(
            "{HEAD}@AllArgsConstructor\nclass S {{\n  private int n;\n  private final String s = \"x\";\n  private static int count;\n  S(int n) {{ this.n = n; }}\n}}"
        );
        assert_eq!(clashes(&src).len(), 1);
    }

    #[test]
    fn a_hand_written_no_args_beside_no_args_constructor_is_flagged() {
        let src = format!("{HEAD}@NoArgsConstructor\nclass S {{\n  S() {{ }}\n}}");
        assert_eq!(clashes(&src).len(), 1);
    }

    #[test]
    fn data_is_replaced_by_a_hand_written_constructor_not_duplicated() {
        let src = format!("{HEAD}@Data\nclass S {{\n  private final A a;\n  S(A a) {{ this.a = a; }}\n}}");
        assert!(clashes(&src).is_empty());
    }

    #[test]
    fn access_none_generates_nothing() {
        let src = format!(
            "{HEAD}@RequiredArgsConstructor(access = AccessLevel.NONE)\nclass S {{\n  private final A a;\n  S(A a) {{ this.a = a; }}\n}}"
        );
        assert!(clashes(&src).is_empty());
    }

    #[test]
    fn without_a_lombok_import_the_annotation_is_not_lombok() {
        let src = "@RequiredArgsConstructor\nclass S {\n  private final A a;\n  S(A a) { this.a = a; }\n}";
        assert!(clashes(src).is_empty());
    }

    #[test]
    fn a_qualified_annotation_needs_no_import() {
        let src = "@lombok.RequiredArgsConstructor\nclass S {\n  private final A a;\n  S(A a) { this.a = a; }\n}";
        assert_eq!(clashes(src).len(), 1);
    }
}
