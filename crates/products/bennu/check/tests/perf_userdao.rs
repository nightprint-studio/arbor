//! Profiler for a `UserDAO`-shaped class: ~55 methods, each JDBC-heavy (locals typed as resolvable
//! `Connection`/`PreparedStatement`/`ResultSet`, dozens of `stat.setX(..)` / `res.getX(..)` calls,
//! try/catch/finally), plus a wall of `String` SQL constants. Unlike `perf_profile` (blind resolver),
//! here the resolver RESOLVES everything — every receiver has a type, so the resolver-backed checks
//! (unknown-member / arity / argument / cast) actually infer + walk, which is the real cost on a file
//! like `UserDAO.java` that the user measures at ~800 ms.
//!
//! Run: `cargo test -p bennu-check --test perf_userdao --release -- --nocapture`

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bennu_check::prelude::{check_file, check_file_resolved, FileContext};
use bennu_java::prelude::{
    extract_symbols_from_root, infer_node_type_cached, ClassFlags, ClassMembers, Import, InferCache,
    Member, TypeRef, TypeResolver,
};
use tree_sitter::{Node, Parser};

/// Models a REAL JDBC-style type: a DEEP superclass chain (`p/S0`→`p/S1`→…→`p/S5`→Object) plus two
/// interfaces, EACH with a long method list — like `PreparedStatement`→`Statement`→`Wrapper`→
/// `AutoCloseable`. Every simple name resolves to `p/S0`; every method returns `p/S0` so chains keep
/// resolving. This is the shape whose full hierarchy the arity/argument checks walk PER CALL.
struct ResolveAll {
    cache: RwLock<HashMap<String, Option<Arc<ClassMembers>>>>,
    classes: HashMap<String, Arc<ClassMembers>>,
}
impl ResolveAll {
    fn new() -> Self {
        let real_names = [
            "getConnection", "createStatement", "prepareStatement", "executeQuery", "executeUpdate",
            "setString", "setInt", "setDate", "setTimestamp", "setNull", "setLong", "next", "getString",
            "getInt", "getLong", "getDate", "getTimestamp", "close", "commit", "rollback",
            "setAutoCommit", "setTransactionIsolation", "isNotEmpty", "isEmpty", "equals",
            "equalsIgnoreCase", "toUpperCase", "substring", "indexOf", "length", "add", "getTime",
            "getInstance", "setTime", "getUsername", "getPassword", "getDelegateUser", "getCrc",
            "getSessionId", "getEmail", "name", "build", "getSha256", "size", "get", "put",
        ];
        // A class with `filler` filler methods + the real names, given `superclass` + `interfaces`.
        let make = |tag: &str, filler: usize, superclass: Option<&str>, interfaces: Vec<String>| {
            let mut methods: Vec<Member> = (0..filler)
                .map(|i| Member::method(format!("{tag}_m{i}"), TypeRef::simple("p/S0"), vec![TypeRef::simple("java/lang/Object")]))
                .collect();
            for n in real_names {
                methods.push(Member::method(n, TypeRef::simple("p/S0"), vec![TypeRef::simple("java/lang/Object")]));
            }
            Arc::new(ClassMembers {
                type_params: Vec::new(),
                superclass: superclass.map(str::to_string),
                interfaces,
                methods,
                fields: vec![Member::field("value", TypeRef::simple("int"))],
                flags: ClassFlags::default(),
            })
        };
        let mut classes = HashMap::new();
        // Long method lists + a 6-deep chain + two interfaces, modelling real JDBC types
        // (`ResultSet` alone declares ~190 methods; `Connection`/`PreparedStatement` ~50, over
        // `Statement`/`Wrapper`/`AutoCloseable`).
        classes.insert("p/S0".to_string(), make("S0", 150, Some("p/S1"), vec!["p/I0".into(), "p/I1".into()]));
        classes.insert("p/S1".to_string(), make("S1", 150, Some("p/S2"), vec![]));
        classes.insert("p/S2".to_string(), make("S2", 150, Some("p/S3"), vec![]));
        classes.insert("p/S3".to_string(), make("S3", 150, Some("p/S4"), vec![]));
        classes.insert("p/S4".to_string(), make("S4", 150, Some("p/S5"), vec![]));
        classes.insert("p/S5".to_string(), make("S5", 150, Some("java/lang/Object"), vec![]));
        classes.insert("p/I0".to_string(), make("I0", 100, None, vec![]));
        classes.insert("p/I1".to_string(), make("I1", 100, None, vec![]));
        classes.insert(
            "java/lang/Object".to_string(),
            Arc::new(ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![Member::method("equals", TypeRef::simple("boolean"), vec![TypeRef::simple("java/lang/Object")])],
                fields: Vec::new(),
                flags: ClassFlags::default(),
            }),
        );
        ResolveAll { cache: RwLock::new(HashMap::new()), classes }
    }
}
impl TypeResolver for ResolveAll {
    fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
        if let Some(hit) = self.cache.read().unwrap().get(binary) {
            return hit.clone();
        }
        let computed = self.classes.get(binary).cloned().or_else(|| self.classes.get("p/S0").cloned());
        self.cache.write().unwrap().insert(binary.to_string(), computed.clone());
        computed
    }
    fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
        if name == "Object" {
            Some("java/lang/Object".to_string())
        } else {
            Some("p/S0".to_string())
        }
    }
}

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

/// A `UserDAO`-shaped file: `methods` JDBC methods + `consts` SQL string constants.
fn userdao_like(methods: usize, consts: usize) -> String {
    let mut s = String::from("package com.agiletec.aps.system.services.user;\n");
    s.push_str("import java.sql.*;\nimport java.util.*;\nimport org.apache.commons.lang.StringUtils;\n");
    s.push_str("public class UserDAO extends AbstractDAO implements IUserDAO {\n");
    for m in 0..methods {
        s.push_str(&format!("  public java.util.List doThing{m}(String username) {{\n"));
        s.push_str("    Connection conn = null;\n    PreparedStatement stat = null;\n    ResultSet res = null;\n");
        s.push_str("    java.util.List users = null;\n");
        s.push_str("    try {\n");
        s.push_str("      conn = this.getConnection();\n");
        s.push_str(&format!("      stat = conn.prepareStatement(SQL{});\n", m % consts));
        for k in 0..8 {
            s.push_str(&format!("      stat.setString({}, username.toUpperCase());\n", k + 1));
        }
        s.push_str("      res = stat.executeQuery();\n");
        s.push_str("      while (res.next()) {\n");
        for k in 0..6 {
            s.push_str(&format!("        String v{k} = res.getString({});\n", k + 1));
        }
        s.push_str("        if (StringUtils.isNotEmpty(v0)) { users.add(v0.toUpperCase()); }\n");
        s.push_str("      }\n");
        s.push_str("    } catch (Throwable t) {\n");
        s.push_str("      processDaoException(t, \"err\", \"doThing\");\n");
        s.push_str("    } finally {\n      closeDaoResources(res, stat, conn);\n    }\n");
        s.push_str("    return users;\n  }\n");
    }
    for c in 0..consts {
        s.push_str(&format!("  private final String SQL{c} = \"SELECT a, b, c FROM t{c} WHERE x = ? AND y = ?\";\n"));
    }
    s.push_str("}\n");
    s
}

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
//     cargo test -p bennu-check --release --test perf_userdao -- --ignored --nocapture
#[ignore = "profiler — run explicitly, in release; see the note above"]
#[test]
fn profile_userdao_shaped_file() {
    let src = userdao_like(55, 40);
    let ctx = FileContext { java_major: Some(8), ..Default::default() };
    let resolver = ResolveAll::new();
    let reps = 5u32;

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let root = tree.root_node();
    let node_count = collect_nodes(root).len();
    let nodes = collect_nodes(root);
    let symbols = extract_symbols_from_root(&root, &src);

    eprintln!("\n════════ perf_userdao: {} lines, {} named nodes (resolver resolves ALL) ════════", src.lines().count(), node_count);

    macro_rules! resolved {
        ($name:literal, $cache:ident, $call:expr) => {{
            let (ms, n) = avg_ms(reps, || {
                let $cache = InferCache::new();
                $call.len()
            });
            eprintln!("    {:<32} {ms:>7.2} ms  ({n} diags)", $name);
        }};
    }
    eprintln!("  [resolver tier — fresh cache each rep]");
    resolved!("unknown_members_in", c, bennu_check::members::unknown_members_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("unknown_fields_in", c, bennu_check::fields::unknown_fields_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("arity_errors_in", c, bennu_check::arity::arity_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("argument_type_errors_in", c, bennu_check::arguments::argument_type_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("unresolved_types_in", _c, bennu_check::types::unresolved_types_in(&nodes, &src, &symbols, &resolver));
    resolved!("inheritance_errors_in", _c, bennu_check::inheritance::inheritance_errors_in(&nodes, &src, &symbols, &resolver));
    resolved!("missing_abstract_impls_in", _c, bennu_check::inheritance::missing_abstract_impls_in(&nodes, &src, &symbols, &resolver));
    resolved!("type_compat_errors_in", c, bennu_check::casts::type_compat_errors_in(root, &nodes, &src, &symbols, &resolver, &c));
    resolved!("functional_errors_in", _c, bennu_check::functional::functional_errors_in(&nodes, &src, &symbols, &resolver));

    // ── the fix, measured directly: per-call full hierarchy walk (what arity/argument USED to do,
    //    once per call site) vs the memoized `(type, method)` resolution (once per distinct pair). Both
    //    infer the receiver the same way; only the member-resolution differs.
    let bytes = src.as_bytes();
    let (old_ms, old_hits) = avg_ms(reps, || {
        let cache = InferCache::new(); // shared inference, but NO member-resolution memo
        let mut hits = 0usize;
        for &n in &nodes {
            if n.kind() != "method_invocation" {
                continue;
            }
            let (Some(obj), Some(name)) = (n.child_by_field_name("object"), n.child_by_field_name("name")) else { continue };
            let Some(ty) = infer_node_type_cached(&root, &src, &symbols, &obj, &resolver, &cache) else { continue };
            if ty.binary_name.is_empty() { continue; }
            let method = name.utf8_text(bytes).unwrap_or("");
            // The OLD path: a full non-short-circuiting hierarchy walk, per call site.
            bennu_check::walk::for_each_supertype(&resolver, &ty.binary_name, &mut |_b, cm| {
                for m in &cm.methods {
                    if m.name == method {
                        hits += 1;
                    }
                }
            });
        }
        hits
    });
    let (new_ms, new_hits) = avg_ms(reps, || {
        let cache = InferCache::new();
        let mut hits = 0usize;
        for &n in &nodes {
            if n.kind() != "method_invocation" {
                continue;
            }
            let (Some(obj), Some(name)) = (n.child_by_field_name("object"), n.child_by_field_name("name")) else { continue };
            let Some(ty) = infer_node_type_cached(&root, &src, &symbols, &obj, &resolver, &cache) else { continue };
            if ty.binary_name.is_empty() { continue; }
            let method = name.utf8_text(bytes).unwrap_or("");
            // The NEW path: memoized once per (type, method).
            hits += cache.resolve_methods(&resolver, &ty.binary_name, method).candidates.len();
        }
        hits
    });

    let (pure_ms, pure_n) = avg_ms(reps, || check_file(&src, &ctx).len());
    let (full_ms, full_n) = avg_ms(reps, || check_file_resolved(&src, &ctx, &resolver, true).len());
    eprintln!("  [member resolution: old per-call walk vs new memo]");
    eprintln!("    old (for_each_supertype/call)    {old_ms:>7.2} ms  ({old_hits} hits)");
    eprintln!("    new (resolve_methods memo)       {new_ms:>7.2} ms  ({new_hits} hits)");
    eprintln!("  [aggregate]");
    eprintln!("    check_file (pure-AST)            {pure_ms:>7.2} ms  ({pure_n} diags)");
    eprintln!("    check_file_resolved (full)       {full_ms:>7.2} ms  ({full_n} diags)");
    eprintln!("    ⇒ resolver tier delta            {:>7.2} ms", full_ms - pure_ms);
    eprintln!("═══════════════════════════════════════════════════════════════════════\n");

    assert!(node_count > 3000, "generator should produce a big tree ({node_count})");
}
