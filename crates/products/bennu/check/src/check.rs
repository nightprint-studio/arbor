//! The `check_file` aggregator — parse once, run every AST-level check, return the merged
//! diagnostics ordered by position.

use bennu_java::prelude::TypeResolver;
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Parser;

/// Cap on diagnostics returned for one file — a badly-broken buffer shouldn't paint the whole
/// gutter red (or flood the Problems panel). Ordered by position, so the cap keeps the earliest.
pub const MAX_DIAGNOSTICS: usize = 200;

/// Per-file context the source alone doesn't carry — the file's location + the project's language
/// level. All optional: a field left `None` skips the check that needs it (a scratch buffer with no
/// path / unknown JDK still gets every source-only check). A struct so new context-dependent checks
/// don't churn the `check_file` signature.
#[derive(Debug, Clone, Default)]
pub struct FileContext {
    /// The file's base name without `.java` (public-type / file-name check).
    pub file_stem: Option<String>,
    /// The package inferred from the file's location under its source root (package-mismatch check).
    /// Empty / default-package / non-source-root files leave this `None`.
    pub expected_package: Option<String>,
    /// The project's target major Java version (`8` for `1.8`) — drives the version-gated feature
    /// checks. `None` skips them.
    pub java_major: Option<u32>,
}

/// Validate one Java `source` with the no-resolver checks and return the merged diagnostics, ordered
/// by start offset and capped at [`MAX_DIAGNOSTICS`]. Never errors: an unparseable grammar handle or
/// a failed parse yields `[]`.
pub fn check_file(source: &str, ctx: &FileContext) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();

    let mut out = crate::syntax::syntax_errors(root, source);
    out.extend(crate::statements::invalid_statements(root, source));
    out.extend(crate::returns::missing_return(root, source));
    out.extend(crate::returns::return_statement_errors(root, source));
    out.extend(crate::switches::switch_yield_errors_in(root, source));
    out.extend(crate::switches::switch_selector_errors_in(root, source));
    out.extend(crate::duplicates::duplicate_signatures_in(root, source));
    out.extend(crate::declarations::declaration_errors(root, source));
    out.extend(crate::annotations::annotation_errors(root, source));
    out.extend(crate::lambdas::lambda_capture_errors(root, source));
    out.extend(crate::imports::unused_imports(root, source));
    out.extend(crate::imports::duplicate_imports(root, source));
    if let Some(stem) = &ctx.file_stem {
        out.extend(crate::naming::class_name_matches_file(root, source, stem));
        out.extend(crate::special_files::special_file_errors(root, source, stem));
    }
    if let Some(expected) = &ctx.expected_package {
        out.extend(crate::packaging::package_mismatch(root, source, expected));
    }
    if let Some(major) = ctx.java_major {
        out.extend(crate::version::version_errors(root, source, major));
    }
    out.sort_by_key(|d| d.start);
    out.truncate(MAX_DIAGNOSTICS);
    out
}

/// Like [`check_file`], plus the **resolver-backed** checks (currently: unknown members on an
/// inferred receiver). The pure-AST checks always run; the resolver checks run only when
/// `jdk_available` (otherwise every type resolves to "unknown" and they'd stay silent anyway).
pub fn check_file_resolved(
    source: &str,
    ctx: &FileContext,
    resolver: &dyn TypeResolver,
    jdk_available: bool,
) -> Vec<Diagnostic> {
    let mut out = check_file(source, ctx);
    if jdk_available {
        // ONE parse + ONE symbol extraction shared by every resolver-backed check. This is critical
        // for performance: the checks type many expressions per file, and re-parsing / re-extracting
        // per site made a large file's validation quadratic (it pegged bennu-be's CPU). The `_in`
        // variants reuse this `root` + `symbols` and the tree-reusing `infer_*_at` inference.
        let mut parser = Parser::new();
        if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_ok() {
            if let Some(tree) = parser.parse(source, None) {
                let root = tree.root_node();
                let symbols = bennu_java::prelude::extract_symbols(source);
                out.extend(crate::imports::unresolved_imports(root, source, resolver));
                out.extend(crate::members::unknown_members_in(root, source, &symbols, resolver));
                out.extend(crate::fields::unknown_fields_in(root, source, &symbols, resolver));
                out.extend(crate::arity::arity_errors_in(root, source, &symbols, resolver));
                out.extend(crate::arguments::argument_type_errors_in(root, source, &symbols, resolver));
                out.extend(crate::types::unresolved_types_in(root, source, &symbols, resolver));
                out.extend(crate::inheritance::inheritance_errors_in(root, source, &symbols, resolver));
                out.extend(crate::inheritance::missing_abstract_impls_in(root, source, &symbols, resolver));
                out.extend(crate::casts::type_compat_errors_in(root, source, &symbols, resolver));
                out.extend(crate::functional::functional_errors_in(root, source, &symbols, resolver));
                out.extend(crate::constructors::super_constructor_errors_in(root, source, &symbols, resolver));
            }
        }
    }
    out.sort_by_key(|d| d.start);
    out.truncate(MAX_DIAGNOSTICS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(stem: Option<&str>) -> FileContext {
        FileContext { file_stem: stem.map(str::to_string), ..Default::default() }
    }

    #[test]
    fn clean_file_yields_nothing() {
        let src = "package com.acme;\nimport java.util.List;\npublic class Foo {\n  List<String> xs;\n}\n";
        assert!(check_file(src, &ctx(Some("Foo"))).is_empty());
    }

    #[test]
    fn combines_syntax_and_unused_import_ordered() {
        // An unused import (line 2) + a broken statement (later) → both, ordered by position.
        let src = "package a;\nimport java.util.List;\nclass Foo { void run() { int x = ; } }\n";
        let diags = check_file(src, &FileContext::default());
        assert!(diags.len() >= 2, "expected the unused import + a syntax error ({:?})", diags);
        assert!(diags.windows(2).all(|w| w[0].start <= w[1].start));
        assert!(diags.iter().any(|d| d.severity == "warning" && d.message.contains("List")));
        assert!(diags.iter().any(|d| d.severity == "error"));
    }

    #[test]
    fn public_class_file_name_mismatch_is_flagged() {
        let diags = check_file("public class Foo {}\n", &ctx(Some("Bar")));
        assert!(diags.iter().any(|d| d.message.contains("Foo.java")), "{diags:?}");
    }

    #[test]
    fn package_mismatch_flows_through_context() {
        let src = "package com.acme.web;\npublic class Foo {}\n";
        let c = FileContext {
            file_stem: Some("Foo".to_string()),
            expected_package: Some("com.acme.model".to_string()),
            ..Default::default()
        };
        assert!(check_file(src, &c).iter().any(|d| d.message.contains("does not match")));
    }

    #[test]
    fn version_check_flows_through_context() {
        let src = "public record R(int x) {}\n";
        let c = FileContext { java_major: Some(8), ..Default::default() };
        assert!(check_file(src, &c).iter().any(|d| d.message.contains("Records")));
        // Same file on Java 17 → no version error.
        let c17 = FileContext { java_major: Some(17), ..Default::default() };
        assert!(check_file(src, &c17).iter().all(|d| !d.message.contains("Records")));
    }

    #[test]
    fn empty_source_is_safe() {
        assert!(check_file("", &FileContext::default()).is_empty());
    }
}
