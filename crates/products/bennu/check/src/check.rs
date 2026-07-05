//! The `check_file` aggregator — parse once, run every AST-level check, return the merged
//! diagnostics ordered by position.

use bennu_java::prelude::TypeResolver;
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Parser;

/// Cap on diagnostics returned for one file — a badly-broken buffer shouldn't paint the whole
/// gutter red (or flood the Problems panel). Ordered by position, so the cap keeps the earliest.
pub const MAX_DIAGNOSTICS: usize = 200;

/// Every node of the tree in the SAME order a check's `stack.pop()` DFS visits them (children pushed
/// then the node recorded), so a check can iterate this flat slice instead of re-walking the tree and
/// get byte-for-byte identical behaviour. Collecting once and sharing it across the resolver-backed
/// checks turns their ~dozen independent tree walks — the dominant per-file cost on a big file — into
/// one walk plus cheap slice passes.
pub(crate) fn collect_nodes(root: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut nodes = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        nodes.push(n);
    }
    nodes
}

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
    check_file_in(root, &collect_nodes(root), source, ctx)
}

/// The pure-AST checks over an ALREADY-parsed `root` + its pre-collected node list — so both tiers
/// share ONE parse AND ONE tree traversal. Every check that just visits nodes by kind iterates the
/// flat `nodes` slice instead of re-walking the tree (a dozen independent DFS walks were the dominant
/// per-file cost on a big legacy class). The few that need anonymous nodes / subtree pruning /
/// top-level-only scans (`syntax_errors`, the import checks) stay `root`-based.
pub fn check_file_in(
    root: tree_sitter::Node,
    nodes: &[tree_sitter::Node],
    source: &str,
    ctx: &FileContext,
) -> Vec<Diagnostic> {
    let mut out = crate::syntax::syntax_errors(root, source);
    out.extend(crate::statements::invalid_statements_nodes(nodes, source));
    out.extend(crate::returns::missing_return_nodes(nodes, source));
    out.extend(crate::returns::return_statement_errors_nodes(nodes, source));
    out.extend(crate::switches::switch_yield_errors_nodes(nodes, source));
    out.extend(crate::switches::switch_selector_errors_nodes(nodes, source));
    out.extend(crate::duplicates::duplicate_signatures_nodes(nodes, source));
    out.extend(crate::redeclaration::redeclaration_errors_nodes(nodes, source));
    out.extend(crate::finals::final_reassignment_errors_nodes(nodes, source));
    out.extend(crate::reachable::unreachable_code_nodes(nodes, source));
    out.extend(crate::declarations::declaration_errors_nodes(nodes, source));
    out.extend(crate::annotations::annotation_errors_nodes(nodes, source));
    out.extend(crate::lambdas::lambda_capture_errors_nodes(nodes, source));
    out.extend(crate::ctor_checks::ctor_check_errors_nodes(nodes, source));
    out.extend(crate::expr_lint::expr_lint_warnings_nodes(nodes, source));
    out.extend(crate::switch_flow::switch_flow_warnings_nodes(nodes, source));
    out.extend(crate::generics_syntax::generics_syntax_errors_nodes(nodes, source));
    out.extend(crate::erasure_clash::erasure_clash_errors_nodes(nodes, source));
    out.extend(crate::iface_dup::iface_dup_errors_nodes(nodes, source));
    out.extend(crate::init_checks::init_check_errors_nodes(nodes, source));
    out.extend(crate::func_iface::func_iface_errors_nodes(nodes, source));
    out.extend(crate::switch_dup::switch_dup_errors_nodes(nodes, source));
    out.extend(crate::ctor_recursion::ctor_recursion_errors_nodes(nodes, source));
    out.extend(crate::method_body::method_body_errors_nodes(nodes, source));
    out.extend(crate::record_ctor::record_ctor_errors_nodes(nodes, source));
    out.extend(crate::imports::unused_imports(root, source));
    out.extend(crate::imports::duplicate_imports(root, source));
    out.extend(crate::imports::redundant_imports(root, source));
    out.extend(crate::import_clash::import_clash_errors(root, source));
    if let Some(stem) = &ctx.file_stem {
        out.extend(crate::naming::class_name_matches_file(root, source, stem));
        out.extend(crate::special_files::special_file_errors(root, source, stem));
    }
    if let Some(expected) = &ctx.expected_package {
        out.extend(crate::packaging::package_mismatch(root, source, expected));
    }
    if let Some(major) = ctx.java_major {
        out.extend(crate::version::version_errors_nodes(root, nodes, source, major));
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
    // Opt-in per-check timing (env `BENNU_PROFILE`): logs the breakdown for any file over a threshold,
    // so the slowest files reveal WHERE the time goes on real project types — zero cost when unset.
    let prof = profiling_enabled();
    let t_total = prof.then(std::time::Instant::now);
    let mut times: Vec<(&str, std::time::Duration)> = Vec::new();

    // ONE parse for BOTH tiers. The pure-AST checks and the resolver-backed checks run over the same
    // tree; a second parse (and a third from `extract_symbols`) per file was pure waste — costly on a
    // 2.8k-line class validated across a whole project.
    let t_parse = prof.then(std::time::Instant::now);
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return check_file(source, ctx);
    }
    let Some(tree) = parser.parse(source, None) else {
        return check_file(source, ctx);
    };
    let root = tree.root_node();
    if let Some(t) = t_parse {
        times.push(("parse", t.elapsed()));
    }

    // Collect every node ONCE (one DFS) and share the slice across BOTH tiers: the pure-AST checks
    // iterate it (no per-check re-walk) and the resolver-backed checks iterate it too. One traversal
    // feeds every check that visits nodes by kind.
    let t_collect = prof.then(std::time::Instant::now);
    let nodes = collect_nodes(root);
    if let Some(t) = t_collect {
        times.push(("collect_nodes", t.elapsed()));
    }
    let t_pure = prof.then(std::time::Instant::now);
    let mut out = check_file_in(root, &nodes, source, ctx);
    if let Some(t) = t_pure {
        times.push(("pure-AST", t.elapsed()));
    }
    if jdk_available {
        // ONE symbol extraction + ONE shared inference cache for every resolver-backed check. The
        // cache memoizes each site's inferred type (so the unknown-member / arity / argument / cast
        // checks that infer the SAME site pay once), each scope's locals, each type-text resolution,
        // and each `(type, method)` member lookup — turning quadratic per-file validation roughly linear.
        let t_sym = prof.then(std::time::Instant::now);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, source);
        if let Some(t) = t_sym {
            times.push(("extract_symbols", t.elapsed()));
        }
        let cache = bennu_java::prelude::InferCache::new();
        // Run a resolver-backed check, timing it only when profiling is on. A macro (not a fn/closure)
        // so the `Instant` is captured BEFORE the check call — with a closure the call is an argument,
        // evaluated first, and every check mis-measured as ~0.
        macro_rules! timed {
            ($label:expr, $call:expr) => {{
                let __t = prof.then(std::time::Instant::now);
                let __r = $call;
                if let Some(__t) = __t {
                    times.push(($label, __t.elapsed()));
                }
                out.extend(__r);
            }};
        }
        timed!("unresolved_imports", crate::imports::unresolved_imports(root, source, resolver));
        timed!("unknown_members", crate::members::unknown_members_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("unknown_fields", crate::fields::unknown_fields_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("arity", crate::arity::arity_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("argument_type", crate::arguments::argument_type_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("unresolved_types", crate::types::unresolved_types_in(&nodes, source, &symbols, resolver));
        timed!("undefined_var", crate::undefined_var::undefined_var_errors_in(root, &nodes, source, &symbols, resolver));
        timed!("inheritance", crate::inheritance::inheritance_errors_in(&nodes, source, &symbols, resolver));
        timed!("missing_abstract", crate::inheritance::missing_abstract_impls_in(&nodes, source, &symbols, resolver));
        timed!("type_compat", crate::casts::type_compat_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("functional", crate::functional::functional_errors_in(&nodes, source, &symbols, resolver));
        timed!("super_constructor", crate::constructors::super_constructor_errors_in(&nodes, source, &symbols, resolver));
        timed!("final_override", crate::finals::final_override_errors_in(&nodes, source, &symbols, resolver));
        timed!("inherit_cycle", crate::inherit_cycle::inherit_cycle_errors_in(&nodes, source, &symbols, resolver));
        timed!("exceptions", crate::exceptions::exception_errors_in(&nodes, source, &symbols, resolver));
        timed!("enum_switch", crate::enum_switch::enum_switch_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("super_method", crate::super_method::super_method_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("condition_type", crate::condition_type::condition_type_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("type_use", crate::type_use::type_use_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("narrowing", crate::narrowing::narrowing_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("checked_throw", crate::checked_throw::checked_throw_errors_in(&nodes, source, &symbols, resolver));
        timed!("checked_call", crate::checked_call::checked_call_errors_in(root, &nodes, source, &symbols, resolver, &cache));
        timed!("throws_widen", crate::throws_widen::throws_widen_errors_in(&nodes, source, &symbols, resolver));
        timed!("static_access", crate::static_access::static_access_errors_in(root, &nodes, source, &symbols, resolver));
        timed!("visibility", crate::visibility::visibility_errors_in(root, &nodes, source, &symbols, resolver, &cache));
    }
    if let Some(t) = t_total {
        log_profile(ctx, t.elapsed(), &times);
    }
    out.sort_by_key(|d| d.start);
    out.truncate(MAX_DIAGNOSTICS);
    out
}

/// Whether opt-in per-check profiling is enabled (env `BENNU_PROFILE` set). Read once.
fn profiling_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BENNU_PROFILE").is_some())
}

/// Log the per-check timing breakdown for a file whose validation crossed the reporting threshold, so
/// a project run surfaces its slow files with WHICH check dominated (real resolver, real types).
fn log_profile(ctx: &FileContext, total: std::time::Duration, times: &[(&str, std::time::Duration)]) {
    const REPORT_OVER_MS: u128 = 60;
    if total.as_millis() < REPORT_OVER_MS {
        return;
    }
    let file = ctx.file_stem.as_deref().unwrap_or("?");
    let mut line = format!("bennu-profile {file}: {}ms total", total.as_millis());
    let mut sorted: Vec<&(&str, std::time::Duration)> = times.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    for (name, dur) in sorted {
        line.push_str(&format!("  {name}={:.1}", dur.as_secs_f64() * 1000.0));
    }
    eprintln!("{line}");
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

    #[test]
    fn misplaced_super_call_is_reported() {
        // A `super()` that isn't the first statement is a grammar error (the parser only accepts an
        // explicit constructor call at the head of the body), so the syntax pass flags it — the case
        // is never silently missed even without a dedicated check.
        let src = "class X { X() { int a = 1; super(); } }";
        assert!(
            check_file(src, &FileContext::default()).iter().any(|d| d.severity == "error"),
            "misplaced super() must surface as an error",
        );
    }

    #[test]
    fn unreachable_and_redeclaration_flow_through() {
        let dead = check_file("class C { int m() { return 1; int x = 2; } }", &FileContext::default());
        assert!(dead.iter().any(|d| d.message.contains("Unreachable")), "{dead:?}");
        let dup = check_file("class C { int a; int a; }", &FileContext::default());
        assert!(dup.iter().any(|d| d.message.contains("Duplicate field")), "{dup:?}");
    }
}
