//! Per-check profiler for a big, legacy-shaped Java class (the `InterceptorEncodedData` archetype:
//! ~120 `static final String` constants, one giant `switch` over them, and ~110 small getter methods
//! each casting an UNRESOLVED dependency type and chaining calls on it). Prints the wall time of every
//! individual check + the two aggregate tiers, so the bottleneck is measured, not guessed.
//!
//! Run with output: `cargo test -p bennu-check --test perf_profile -- --nocapture`
//!
//! The resolver here is BLIND (knows nothing) on purpose: it reproduces the user's real project where
//! the Maven dependencies aren't indexed, so every library type is unknown — the exact scenario whose
//! validation cost we want to break down.

use std::sync::Arc;
use std::time::Instant;

use bennu_check::prelude::{check_file, check_file_resolved, FileContext};
use bennu_java::prelude::{
    extract_symbols_from_root, ClassMembers, Import, InferCache, TypeResolver,
};
use tree_sitter::{Node, Parser};

/// Knows nothing — mirrors "dependencies not indexed": every dep type resolves to `None`, so the
/// member/field/arity checks bail early and the import/type checks probe-and-miss.
struct BlindResolver;
impl TypeResolver for BlindResolver {
    fn members_of(&self, _binary: &str) -> Option<Arc<ClassMembers>> {
        None
    }
    fn resolve_simple_name(&self, _name: &str, _imports: &[Import]) -> Option<String> {
        None
    }
}

/// Every named node pre-order (same order the resolver-tier checks iterate) — a local copy of the
/// crate-private `collect_nodes`.
fn collect_nodes(root: Node) -> Vec<Node> {
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

/// A file shaped like `InterceptorEncodedData`: `consts` string constants, a `switch` over all of
/// them, and `getters` small methods that cast an unresolved dep type and chain calls on it.
fn interceptor_like(consts: usize, getters: usize) -> String {
    let mut s = String::from("package it.foo.bar.aps.system;\n");
    // ~35 dependency imports (all unresolved by the blind resolver).
    for i in 0..35 {
        s.push_str(&format!("import com.dep.pkg{i}.Type{i};\n"));
    }
    s.push_str("public class Big extends AbstractInterceptor {\n");
    // The constant wall.
    for i in 0..consts {
        s.push_str(&format!("  public static final String LISTA_{i} = \"lista{i}\";\n"));
    }
    // The giant switch.
    s.push_str("  private static java.util.LinkedHashMap<String,String> getList(String type) {\n");
    s.push_str("    java.util.LinkedHashMap<String,String> lista = null;\n");
    s.push_str("    switch (type) {\n");
    for i in 0..consts {
        s.push_str(&format!("      case LISTA_{i}: lista = get{i}(); break;\n"));
    }
    s.push_str("      default: throw new RuntimeException(\"x\");\n");
    s.push_str("    }\n    return lista;\n  }\n");
    // The getter methods: cast an unresolved wrapper, chain calls on it, hashtable dance.
    for i in 0..getters {
        s.push_str(&format!(
            "  private static java.util.LinkedHashMap<String,String> get{i}() {{\n\
             \x20   java.util.Hashtable<String, java.util.LinkedHashMap<String,String>> hash = getHash();\n\
             \x20   java.util.LinkedHashMap<String,String> lista = null;\n\
             \x20   if (hash.containsKey(LISTA_{i})) {{\n\
             \x20     lista = hash.get(LISTA_{i});\n\
             \x20   }} else {{\n\
             \x20     WSGareAppaltoWrapper wrapper = (WSGareAppaltoWrapper) ApsWebApplicationUtils.getBean(K.WS, ServletActionContext.getRequest());\n\
             \x20     String xml = wrapper.getProxyWSGare().getElenco{i}();\n\
             \x20     lista = InterceptorEncodedData.parseXml(xml);\n\
             \x20     hash.put(LISTA_{i}, lista);\n\
             \x20   }}\n\
             \x20   return clonaHash(lista);\n\
             \x20 }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

/// Time a closure `reps` times, return average milliseconds.
fn avg_ms<F: FnMut() -> usize>(reps: u32, mut f: F) -> (f64, usize) {
    let t = Instant::now();
    let mut n = 0;
    for _ in 0..reps {
        n = f();
    }
    (t.elapsed().as_secs_f64() * 1000.0 / reps as f64, n)
}

// A PROFILER, not a regression guard: its assertions check that this file's own generator produced
// the shape it meant to, never that the code under test is fast. Left un-ignored it ran on every
// `cargo test`, in DEBUG — which is both the slowest way to run it and the least meaningful, since
// the numbers it prints only mean something optimised. Run it deliberately:
//
//     cargo test -p bennu-check --release --test perf_profile -- --ignored --nocapture
#[ignore = "profiler — run explicitly, in release; see the note above"]
#[test]
fn profile_big_interceptor() {
    let src = interceptor_like(120, 110);
    let ctx = FileContext { java_major: Some(8), ..Default::default() };
    let resolver = BlindResolver;
    let reps = 5u32;

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();

    // ── setup phases ──────────────────────────────────────────────────────────────────────────
    let (parse_ms, _) = avg_ms(reps, || {
        parser.parse(&src, None).unwrap().root_node().child_count()
    });
    let tree = parser.parse(&src, None).unwrap();
    let root = tree.root_node();
    let node_count = collect_nodes(root).len();

    let (collect_ms, _) = avg_ms(reps, || collect_nodes(root).len());
    let (symbols_ms, _) = avg_ms(reps, || extract_symbols_from_root(&root, &src).types.len());

    let nodes = collect_nodes(root);
    let symbols = extract_symbols_from_root(&root, &src);

    eprintln!("\n════════ perf_profile: {} lines, {} named nodes ════════", src.lines().count(), node_count);
    eprintln!("  [setup]");
    eprintln!("    parse                            {parse_ms:>7.2} ms");
    eprintln!("    collect_nodes                    {collect_ms:>7.2} ms");
    eprintln!("    extract_symbols_from_root        {symbols_ms:>7.2} ms");

    // ── pure-AST tier, per check ──────────────────────────────────────────────────────────────
    // The `*_nodes` checks iterate the SHARED pre-collected slice (the real `check_file` path — one
    // traversal for all). `syntax_errors` + the import checks stay `root`-based (anonymous nodes /
    // pruning / top-level-only), so they're shown against `root`.
    eprintln!("  [pure-AST tier — *_nodes iterate the shared slice]");
    macro_rules! pure {
        ($name:literal, $call:expr) => {{
            let (ms, n) = avg_ms(reps, || $call.len());
            eprintln!("    {:<32} {ms:>7.2} ms  ({n} diags)", $name);
        }};
    }
    pure!("syntax_errors", bennu_check::syntax::syntax_errors(root, &src));
    pure!("invalid_statements_nodes", bennu_check::statements::invalid_statements_nodes(&nodes, &src));
    pure!("missing_return_nodes", bennu_check::returns::missing_return_nodes(&nodes, &src));
    pure!("return_statement_errors_nodes", bennu_check::returns::return_statement_errors_nodes(&nodes, &src));
    pure!("switch_yield_errors_nodes", bennu_check::switches::switch_yield_errors_nodes(&nodes, &src));
    pure!("switch_selector_errors_nodes", bennu_check::switches::switch_selector_errors_nodes(&nodes, &src));
    pure!("duplicate_signatures_nodes", bennu_check::duplicates::duplicate_signatures_nodes(&nodes, &src));
    pure!("redeclaration_errors_nodes", bennu_check::redeclaration::redeclaration_errors_nodes(&nodes, &src));
    pure!("final_reassignment_errors_nodes", bennu_check::finals::final_reassignment_errors_nodes(&nodes, &src));
    pure!("unreachable_code_nodes", bennu_check::reachable::unreachable_code_nodes(&nodes, &src));
    pure!("declaration_errors_nodes", bennu_check::declarations::declaration_errors_nodes(&nodes, &src));
    pure!("annotation_errors_nodes", bennu_check::annotations::annotation_errors_nodes(&nodes, &src));
    pure!("lambda_capture_errors_nodes", bennu_check::lambdas::lambda_capture_errors_nodes(&nodes, &src));
    pure!("unused_imports (root)", bennu_check::imports::unused_imports(root, &src));
    pure!("duplicate_imports (root)", bennu_check::imports::duplicate_imports(root, &src));
    pure!("redundant_imports (root)", bennu_check::imports::redundant_imports(root, &src));
    pure!("version_errors_nodes", bennu_check::version::version_errors_nodes(root, &nodes, &src, 8));

    // ── resolver tier, per check (fresh InferCache each rep = cold cost) ───────────────────────
    eprintln!("  [resolver tier — blind resolver, fresh cache each rep]");
    macro_rules! resolved {
        ($name:literal, $cache:ident, $call:expr) => {{
            let (ms, n) = avg_ms(reps, || {
                let $cache = InferCache::new();
                $call.len()
            });
            eprintln!("    {:<32} {ms:>7.2} ms  ({n} diags)", $name);
        }};
    }
    resolved!("unresolved_imports", _c, bennu_check::imports::unresolved_imports(root, &src, &resolver, true));
    resolved!("unknown_members_in", c, bennu_check::members::unknown_members_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("unknown_fields_in", c, bennu_check::fields::unknown_fields_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("arity_errors_in", c, bennu_check::arity::arity_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("argument_type_errors_in", c, bennu_check::arguments::argument_type_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("unresolved_types_in", _c, bennu_check::types::unresolved_types_in(&nodes, &src, &symbols, &resolver));
    resolved!("inheritance_errors_in", _c, bennu_check::inheritance::inheritance_errors_in(&nodes, &src, &symbols, &resolver));
    resolved!("missing_abstract_impls_in", _c, bennu_check::inheritance::missing_abstract_impls_in(&nodes, &src, &symbols, &resolver));
    resolved!("type_compat_errors_in", c, bennu_check::casts::type_compat_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("functional_errors_in", _c, bennu_check::functional::functional_errors_in(&nodes, &src, &symbols, &resolver));
    resolved!("super_constructor_errors_in", _c, bennu_check::constructors::super_constructor_errors_in(&nodes, &src, &symbols, &resolver));
    resolved!("final_override_errors_in", _c, bennu_check::finals::final_override_errors_in(&nodes, &src, &symbols, &resolver));

    // ── realistic aggregates (what validate_java actually calls) ──────────────────────────────
    let (pure_total_ms, pure_n) = avg_ms(reps, || check_file(&src, &ctx).len());
    let (resolved_total_ms, resolved_n) = avg_ms(reps, || check_file_resolved(&src, &ctx, &resolver, true).len());
    eprintln!("  [aggregate — the real entry points]");
    eprintln!("    check_file (pure-AST only)       {pure_total_ms:>7.2} ms  ({pure_n} diags)");
    eprintln!("    check_file_resolved (both tiers) {resolved_total_ms:>7.2} ms  ({resolved_n} diags)");
    eprintln!("    ⇒ resolver tier delta            {:>7.2} ms", resolved_total_ms - pure_total_ms);
    eprintln!("════════════════════════════════════════════════════════════\n");

    // Not a benchmark assertion (timings vary by machine) — just guard the harness itself stays
    // wired: the file must parse to a real tree and both tiers must run.
    assert!(node_count > 10_000, "generator should produce a big tree ({node_count} nodes)");
    assert!(resolved_n >= pure_n, "the resolver tier only ever adds diagnostics");
}
