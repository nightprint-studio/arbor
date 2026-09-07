//! Initialization diagnostics (pure-AST): two definitely-illegal cases that need no type resolution.
//!
//!   1. **Blank `final` field never initialized** — a `final` field (instance or static) declared with
//!      NO initializer and assigned NOWHERE in its class → it can never be given a value, so it can
//!      never compile. SOUND by construction: we don't do flow analysis, so *any* assignment to the
//!      field name anywhere in the class body (a constructor, a method, a static initializer, even a
//!      single `if` branch) means "possibly initialized" and we SKIP. Only a `final`-without-initializer
//!      field with zero textual assignments to its name is flagged.
//!
//!   2. **`var` / Lombok `val` that can't infer from a lambda or method reference** — Java can't infer a
//!      local's type from a bare lambda / method reference (there's no target type), so `var f = () -> 1;`
//!      and `var g = String::valueOf;` don't compile. Flagged only when the declared type is literally
//!      `var` (or, on a Lombok file, `val`) AND the initializer is a `lambda_expression` /
//!      `method_reference`. Any other initializer is fine and never flagged.
//!
//! PARAMOUNT: never a false positive. When in doubt, skip.

use std::collections::HashSet;

use bennu_lombok::prelude::{initializes_blank_finals, ParsedImport};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::{has_keyword};

/// Public entry over a `root` node: extracts the file's Lombok-`val` import once (needed by check 2's
/// gate), then delegates to the slice core. Mirrors the `*_in` / `*_nodes` split of the sibling checks
/// so the `check_file` aggregator can share its single traversal.
pub fn init_check_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    init_check_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
///
/// The Lombok import is detected by scanning the collected nodes for top-level `import_declaration`s,
/// so this core needs only the flat node slice (no separate `root` argument).
pub fn init_check_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let imports = crate::lombok::imports_from_nodes(nodes, bytes);
    let lombok_val = crate::lombok::imports_keyword("val", &imports);
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // Check 1 is scoped to one type body: assignments inside it are gathered, then the type's
            // own blank-final fields are flagged if unassigned.
            "class_declaration" | "enum_declaration" => {
                check_uninitialized_final_fields(n, bytes, &imports, &mut out)
            }
            // Check 2 is per local declaration.
            "local_variable_declaration" => check_uninferrable_var(n, bytes, lombok_val, &mut out),
            _ => {}
        }
    }
    out
}

// ── check 1: blank final field never initialized ─────────────────────────────

/// Flag every `final` field of type `n` that has no declarator initializer AND whose name is assigned
/// nowhere in the type's own body. Skips as soon as *any* assignment to that name appears — no flow
/// analysis, so any assignment means "possibly initialized".
fn check_uninitialized_final_fields(
    n: Node,
    bytes: &[u8],
    imports: &[ParsedImport],
    out: &mut Vec<Diagnostic>,
) {
    // Lombok generates a constructor that initializes the `final` (and `@NonNull`) fields at COMPILE
    // time — there's no textual assignment in source, so without this the blank-final check would
    // falsely flag every final field of a `@Data` / `@Value` / `@Builder` / `@AllArgsConstructor`
    // class. Which annotations those are is `bennu-lombok`'s to know, and the gate it applies is
    // "Lombok is genuinely in use": the file imports it, or the annotation is written fully-qualified
    // (`@lombok.Data`). Without that, `@Data` is the project's OWN annotation (no generated ctor) and
    // the final fields really are uninitialized, so the report stands.
    if crate::lombok::has_lombok_annotation(n, bytes, imports, |a| {
        initializes_blank_finals(a.simple, a.args)
    }) {
        return;
    }
    let Some(body) = n.child_by_field_name("body") else { return };

    // (field name → name node) for each blank final candidate declared directly in this body. A field
    // WITH an initializer is never a candidate (it's already assigned).
    let mut candidates: Vec<(String, Node)> = Vec::new();
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "field_declaration" || !has_keyword(m, bytes, "final") {
            continue;
        }
        let mut dc = m.walk();
        for d in m.named_children(&mut dc) {
            if d.kind() != "variable_declarator" {
                continue;
            }
            // A declarator `= value` means it's already initialized → not a candidate.
            if d.child_by_field_name("value").is_some() {
                continue;
            }
            if let Some(name_node) = d.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(bytes) {
                    candidates.push((name.to_string(), name_node));
                }
            }
        }
    }
    if candidates.is_empty() {
        return;
    }

    // Every identifier that appears as an assignment / update target ANYWHERE in this type body —
    // bare `x = …`, `this.x = …`, `X.x = …`, `x++`, `this.x += …`. We collect the *field name* end of
    // any such target and, being conservative, DON'T cross into nested type bodies for the candidate
    // set (they were collected from this body only) but DO gather assignment names across the whole
    // subtree: an assignment to `x` in an inner class through the outer name is unusual, and counting it
    // only ever *suppresses* a report — never a false positive, which is the invariant that matters.
    let assigned = collect_assigned_names(body, bytes);

    for (name, name_node) in candidates {
        if assigned.contains(&name) {
            continue; // assigned somewhere → possibly initialized → skip
        }
        out.push(err(format!("Blank final field `{name}` is never initialized"), name_node));
    }
}

/// The set of identifier names that appear as an assignment / update target anywhere under `body`.
/// Covers `x = …` (bare identifier LHS), `this.x = …` / `X.x = …` (field-access LHS — the `.field`
/// name), and `x++` / `this.x++` update targets. Any of these suppresses a blank-final report, so
/// over-collecting is safe; under-collecting would risk a false positive, so we err toward gathering.
fn collect_assigned_names(body: Node, bytes: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack: Vec<Node> = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(node) = stack.pop() {
        let target = match node.kind() {
            "assignment_expression" => node.child_by_field_name("left"),
            "update_expression" => update_operand(node),
            _ => None,
        };
        if let Some(t) = target {
            if let Some(name) = assigned_target_name(t, bytes) {
                names.insert(name);
            }
        }
        let mut cc = node.walk();
        for ch in node.named_children(&mut cc) {
            stack.push(ch);
        }
    }
    names
}

/// The variable/field name an assignment / update target refers to, if it's a plain identifier or a
/// `object.field` access. `x` → `x`; `this.x` / `SomeClass.x` / `a.b.x` → `x` (the trailing field).
/// Returns `None` for an array-index or other complex LHS (those can't name a field, so ignoring them
/// only ever avoids over-suppression, never causes a false positive).
fn assigned_target_name(target: Node, bytes: &[u8]) -> Option<String> {
    match target.kind() {
        "identifier" => target.utf8_text(bytes).ok().map(str::to_string),
        // `object.field` — the assigned member is the `field` child (its trailing identifier).
        "field_access" => target
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(bytes).ok())
            .map(str::to_string),
        _ => None,
    }
}

/// The operand node of an `update_expression` (`x++`, `--x`, `this.x++`) — its single named child.
fn update_operand(update: Node) -> Option<Node> {
    let mut c = update.walk();
    for ch in update.named_children(&mut c) {
        return Some(ch);
    }
    None
}

// ── check 2: var / val that can't infer from a lambda or method reference ─────

/// If `decl` is a `var` (or Lombok `val`) local whose initializer is a lambda / method reference,
/// flag it — Java can't infer a type without a target type.
fn check_uninferrable_var(decl: Node, bytes: &[u8], lombok_val: bool, out: &mut Vec<Diagnostic>) {
    // The declared type must be literally `var` (always) or `val` (only on a Lombok file). `version.rs`
    // reads the `type` field's text for exactly this; we reuse that shape.
    let Some(ty) = decl.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) else {
        return;
    };
    let is_inference_keyword = ty == "var" || (lombok_val && ty == "val");
    if !is_inference_keyword {
        return;
    }

    // Each declarator's initializer: a bare lambda / method reference can't be inferred.
    let mut c = decl.walk();
    for d in decl.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(value) = d.child_by_field_name("value") else { continue };
        let msg = match value.kind() {
            "lambda_expression" => {
                "Cannot infer type for `var`: a lambda expression needs an explicit target type"
            }
            "method_reference" => {
                "Cannot infer type for `var`: a method reference needs an explicit target type"
            }
            _ => continue, // any other initializer infers fine → never flag
        };
        out.push(err(msg.to_string(), value));
    }
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: crate::check_id::CheckId::DefiniteAssignment.severity().to_string(),
        code: crate::check_id::CheckId::DefiniteAssignment.code().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        init_check_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    // ── check 1: blank final field never initialized ─────────────────────────

    #[test]
    fn blank_final_field_never_assigned_is_flagged() {
        let d = errs("class C { final int x; void m() { int y = 1; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Blank final field `x` is never initialized"), "{d:?}");
    }

    #[test]
    fn blank_static_final_field_never_assigned_is_flagged() {
        let d = errs("class C { static final int X; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`X`"), "{d:?}");
    }

    #[test]
    fn final_field_with_inline_initializer_is_not_flagged() {
        // Has `= 1` → already initialized, never a candidate.
        assert!(errs("class C { final int x = 1; }").is_empty());
    }

    #[test]
    fn final_field_assigned_in_constructor_is_not_flagged() {
        assert!(errs("class C { final int x; C() { this.x = 1; } }").is_empty());
        // Bare-name assignment in the ctor also suppresses.
        assert!(errs("class C { final int x; C() { x = 1; } }").is_empty());
    }

    #[test]
    fn final_field_assigned_in_one_if_branch_is_not_flagged() {
        // Any assignment ⇒ skip (we don't do flow analysis).
        let src = "class C { final int x; C(boolean b) { if (b) { this.x = 1; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn final_field_assigned_in_static_initializer_is_not_flagged() {
        let src = "class C { static final int X; static { X = 1; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_final_uninitialized_field_is_not_flagged() {
        assert!(errs("class C { int x; }").is_empty());
    }

    #[test]
    fn lombok_constructor_annotations_suppress_blank_final() {
        // Lombok generates a constructor that initializes the final fields — no source assignment
        // exists, so the check must NOT flag them (when Lombok is actually imported).
        for ann in [
            "@RequiredArgsConstructor",
            "@AllArgsConstructor",
            "@Data",
            "@Value",
            "@Builder",
            "@SuperBuilder",
            "@Builder(toBuilder = true)",
            "@NoArgsConstructor(force = true)",
        ] {
            let src = format!(
                "import lombok.*;\n{ann}\nclass C {{ private final int x; private final String y; }}"
            );
            assert!(errs(&src).is_empty(), "{ann} must suppress blank-final: {:?}", errs(&src));
        }
        // Fully-qualified annotation name counts even without an import.
        let fq = "@lombok.RequiredArgsConstructor\nclass C { private final int x; }";
        assert!(errs(fq).is_empty(), "{:?}", errs(fq));
    }

    #[test]
    fn builder_with_stacked_annotations_suppresses_blank_final() {
        // The real-world shape: `@Builder` alongside accessor-only annotations, with `@NonNull` on the
        // fields. `@Builder` alone carries the generated ctor → nothing must be flagged.
        let src = "import lombok.*;\n\
             @Accessors(fluent = true)\n@Getter\n@Builder\n\
             public class QueryResult {\n\
                 @NonNull private final String query;\n\
                 @NonNull private final java.util.List<Object> parameters;\n\
                 private final java.util.Map<String, Object> namedParameters;\n\
             }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn no_args_constructor_without_force_still_flags() {
        // `@NoArgsConstructor` without `force = true` doesn't initialize the finals (it doesn't even
        // compile) → the blank-final report stands.
        let d = errs("import lombok.*;\n@NoArgsConstructor\nclass C { private final int x; }");
        assert_eq!(d.len(), 1, "plain @NoArgsConstructor must not suppress: {d:?}");
    }

    #[test]
    fn blank_final_not_suppressed_without_lombok_import() {
        // A bare `@Data` with NO Lombok import is the project's own annotation → no generated ctor →
        // the blank-final field is still flagged (the "only if Lombok is a dependency" gate).
        let d = errs("@Data\nclass C { private final int x; }");
        assert_eq!(d.len(), 1, "unimported @Data must not suppress: {d:?}");
        assert!(d[0].contains("`x`"), "{d:?}");
    }

    #[test]
    fn blank_final_still_flagged_without_lombok_ctor_annotation() {
        // A different (non-constructor) Lombok annotation doesn't generate a ctor → still flagged.
        let d = errs("import lombok.*;\n@Getter\nclass C { private final int x; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`x`"), "{d:?}");
    }

    #[test]
    fn qualified_assignment_suppresses_static_final() {
        // `C.X = 1;` is a field-access LHS whose trailing name is `X` → counted as assigned.
        let src = "class C { static final int X; static void init() { C.X = 1; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    // ── check 2: var / val that can't infer from a lambda or method reference ──

    #[test]
    fn var_from_lambda_is_flagged() {
        let d = errs("class C { void m() { var f = () -> 1; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("lambda expression needs an explicit target type"), "{d:?}");
    }

    #[test]
    fn var_from_method_reference_is_flagged() {
        let d = errs("class C { void m() { var g = String::valueOf; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("method reference needs an explicit target type"), "{d:?}");
    }

    #[test]
    fn var_from_literal_is_not_flagged() {
        assert!(errs("class C { void m() { var x = 5; } }").is_empty());
        assert!(errs("class C { void m() { var s = \"x\"; } }").is_empty());
    }

    #[test]
    fn var_from_constructor_is_not_flagged() {
        let src = "class C { void m() { var list = new java.util.ArrayList<String>(); } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn explicit_type_from_lambda_is_not_flagged() {
        // A real target type infers fine — only `var`/`val` are affected.
        let src = "class C { void m() { java.util.function.Supplier<Integer> f = () -> 1; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn lombok_val_from_lambda_is_flagged_only_with_import() {
        let with_import =
            "import lombok.val;\nclass C { void m() { val f = () -> 1; } }";
        let d = errs(with_import);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("lambda expression"), "{d:?}");
        // Wildcard Lombok import also enables `val`.
        let wildcard = "import lombok.*;\nclass C { void m() { val g = String::valueOf; } }";
        assert_eq!(errs(wildcard).len(), 1, "{:?}", errs(wildcard));
        // Without a Lombok import, `val` isn't the inference keyword (it's an ordinary type name) →
        // never flagged, so we can't false-positive on a class literally named `val`.
        let no_import = "class C { void m() { val f = () -> 1; } }";
        assert!(errs(no_import).is_empty(), "{:?}", errs(no_import));
    }

    #[test]
    fn lombok_val_from_literal_is_not_flagged() {
        let src = "import lombok.val;\nclass C { void m() { val x = 5; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }
}
