//! Perf smoke test: a big, realistically-shaped class (many methods, each with locals used many
//! times) must validate quickly. The per-file inference cache (scope-local + result memo), node-based
//! inference, single parse, and the shared node walk keep this roughly linear; before them it was
//! quadratic per method and took seconds on a 2.8k-line legacy class.

use std::sync::Arc;

use bennu_check::prelude::{check_file_resolved, FileContext};
use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef, TypeResolver};

/// A resolver that knows one library type `p/Foo` with `doThing()` + field `value`, memoizing the
/// `Arc` (like the real `IndexResolver`) so `members_of` is a refcount bump, not a re-allocation.
struct FooResolver {
    foo: Arc<ClassMembers>,
}
impl FooResolver {
    fn new() -> Self {
        Self {
            foo: Arc::new(ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![Member::method("doThing", TypeRef::simple("void"), Vec::new())],
                fields: vec![Member::field("value", TypeRef::simple("int"))],
                flags: ClassFlags::default(),
            }),
        }
    }
}
impl TypeResolver for FooResolver {
    fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
        (binary == "p/Foo").then(|| self.foo.clone())
    }
    fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
        (name == "Foo").then(|| "p/Foo".to_string())
    }
}

/// A realistically-shaped class: `methods` methods, each declaring `locals` `Foo` locals and using
/// each `uses` times (a call + a field read). Closer to a real legacy class than one giant method.
fn realistic_source(methods: usize, locals: usize, uses: usize) -> String {
    let mut s = String::from("package p;\npublic class Big {\n");
    for m in 0..methods {
        s.push_str(&format!("  void m{m}() {{\n"));
        for i in 0..locals {
            s.push_str(&format!("    Foo a{i} = new Foo();\n"));
        }
        for i in 0..locals {
            for _ in 0..uses {
                s.push_str(&format!("    a{i}.doThing();\n"));
                s.push_str(&format!("    int r = a{i}.value;\n"));
            }
        }
        s.push_str("  }\n");
    }
    s.push_str("}\n");
    s
}

#[test]
fn big_realistic_class_validates_quickly() {
    // ~25 methods × 12 locals × 4 uses ≈ 2.8k lines — matching the user's slow legacy class.
    let src = realistic_source(25, 12, 4);
    let ctx = FileContext { java_major: Some(8), ..Default::default() };
    let resolver = FooResolver::new();

    let t0 = std::time::Instant::now();
    let diags = check_file_resolved(&src, &ctx, &resolver, true);
    let elapsed = t0.elapsed();
    eprintln!(
        "perf_smoke: validated {} lines in {:?} ({} diagnostics)",
        src.lines().count(),
        elapsed,
        diags.len()
    );

    // Known members must not be flagged.
    assert!(
        !diags.iter().any(|d| d.message.contains("doThing") || d.message.contains("Cannot resolve field `value`")),
        "known members must not be flagged",
    );
    // No quadratic blowup: a ~2.8k-line class validates in well under a second (measured ~0.7s).
    // Generous ceiling so a loaded CI box isn't flaky — a quadratic regression would be many seconds.
    assert!(elapsed.as_millis() < 2500, "validation too slow (quadratic regression?): {elapsed:?}");
}
